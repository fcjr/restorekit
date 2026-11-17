//! WinUSB driver setup for Apple DFU/recovery devices (Windows only).
//!
//! On Windows, libusb (and `nusb`) can only *open* a device that has a
//! libusb-class driver bound to it. A Mac in DFU/recovery enumerates under
//! Apple's vendor id but Windows binds no usable driver, so restorekit can't
//! talk to it until **WinUSB** is bound. (Detection still works without a
//! driver — that's plain enumeration — only the restore needs WinUSB.)
//!
//! Windows won't install a driver package from an unsigned INF, so we do what
//! Zadig/libwdi do: generate a self-signed code-signing certificate, build and
//! sign a catalog for a WinUSB INF, trust the certificate, then bind the driver
//! with `pnputil`. That whole dance is scripted through PowerShell (present on
//! every supported Windows) rather than reimplementing the crypto here.
//!
//! Installing a driver and trusting a certificate need administrator rights;
//! the caller (the CLI's `setup-driver` command) elevates first.

use std::io::Write;
use std::process::Command;

use restorekit_dongle_proto as proto;

use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

/// Apple's USB vendor id.
pub const APPLE_VID: u16 = 0x05ac;

/// The DFU / recovery / WTF / KIS product ids the C stack (libirecovery) drives
/// — the modes a Mac passes through while being restored.
///
/// Deliberately NOT Apple's normal-mode range (0x129x): on Windows, binding
/// WinUSB *replaces* a device's driver, so covering normal mode would hijack a
/// plugged-in iPhone/iPad from Apple's own driver. Every id here is only ever an
/// Apple device in recovery/DFU, never a normal one — so staging them all up
/// front is safe and lets a restore hop DFU -> recovery without a re-run.
pub const DRIVER_PIDS: &[u16] = &[
    0x1222, // WTF
    0x1227, // DFU
    0x1280, 0x1281, 0x1282, 0x1283, // recovery 1-4
    0xf014, // port DFU
    0x1881, // KIS
];

/// Apple's normal-mode product-id range (0x1290–0x12af). A Mac in **restore
/// mode** enumerates as a composite device with one of these ids plus a
/// `RESTORE_MODE` qualifier; we bind WinUSB to that qualified interface only (see
/// [`winusb_inf`]), never the bare id — so a normal iPhone/iPad is untouched.
const RESTORE_MODE_PIDS: std::ops::RangeInclusive<u16> = 0x1290..=0x12af;

/// Does `pid` name a mode restorekit binds WinUSB for?
pub fn is_target_pid(pid: u16) -> bool {
    DRIVER_PIDS.contains(&pid)
}

/// What to tell Windows on ARM users, where restorekit's own driver package
/// can't be installed.
pub const ARM64_HINT: &str = "Windows on ARM only loads Microsoft-signed driver packages, so \
     restorekit can't install its own USB driver here. Install Apple Devices from the Microsoft \
     Store (or iTunes) instead and retry: restorekit then uses Apple's drivers and Apple Mobile \
     Device Service, and needs no driver of its own";

/// Whether Windows itself runs on ARM64, including when this x64 build runs
/// under emulation there.
#[cfg(target_os = "windows")]
pub fn native_arm64() -> bool {
    use windows_sys::Win32::System::SystemInformation::IMAGE_FILE_MACHINE_ARM64;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, IsWow64Process2};
    let (mut process, mut native) = (0u16, 0u16);
    let ok = unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, &mut native) != 0 };
    ok && native == IMAGE_FILE_MACHINE_ARM64
}

/// Whether our WinUSB package is in the driver store (the restore-mode watcher
/// binds the newest one).
#[cfg(target_os = "windows")]
pub fn winusb_package_staged() -> bool {
    let Some(windir) = std::env::var_os("windir") else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(std::path::Path::new(&windir).join("INF")) else {
        return false;
    };
    entries.flatten().any(|e| {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        name.starts_with("oem")
            && name.ends_with(".inf")
            && std::fs::read_to_string(e.path())
                .is_ok_and(|c| c.contains("RestoreKit") && c.contains("RESTORE_MODE"))
    })
}

/// A connected Apple device that needs WinUSB bound before restorekit can use it.
#[derive(Debug, Clone)]
pub struct Target {
    pub pid: u16,
    /// Human-readable label (product string, or a mode id as a fallback).
    pub name: String,
}

/// Enumerate connected Apple devices in DFU, recovery, or restore mode.
pub fn connected_targets() -> Result<Vec<Target>> {
    Ok(present_instance_ids()?
        .iter()
        .filter_map(|id| target_pid_from_instance_id(id))
        .map(|pid| Target {
            pid,
            name: format!("Apple device (mode {pid:#06x})"),
        })
        .collect())
}

/// How many RecoverKit dongles are connected (by their vendor interface).
fn connected_dongles() -> Result<usize> {
    Ok(present_instance_ids()?
        .iter()
        .filter(|id| is_dongle_interface(id))
        .count())
}

/// The PnP hardware id of a dongle's vendor interface.
fn dongle_interface_hwid() -> String {
    format!(
        "USB\\VID_{:04X}&PID_{:04X}&MI_{:02X}",
        proto::VID,
        proto::PID,
        proto::VENDOR_IFACE
    )
}

fn is_dongle_interface(instance_id: &str) -> bool {
    instance_id
        .to_ascii_uppercase()
        .starts_with(&format!("{}\\", dongle_interface_hwid()))
}

/// Instance ids of every present device.
fn present_instance_ids() -> Result<Vec<String>> {
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW,
        SetupDiGetDeviceInstanceIdW, DIGCF_ALLCLASSES, DIGCF_PRESENT, SP_DEVINFO_DATA,
    };
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

    unsafe {
        let devices = SetupDiGetClassDevsW(
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null_mut(),
            DIGCF_ALLCLASSES | DIGCF_PRESENT,
        );
        if devices == INVALID_HANDLE_VALUE as isize {
            return Err(Error::DriverInstall(
                "could not enumerate Windows devices".into(),
            ));
        }

        let mut ids = Vec::new();
        let mut index = 0;
        loop {
            let mut info = SP_DEVINFO_DATA {
                cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };
            if SetupDiEnumDeviceInfo(devices, index, &mut info) == 0 {
                break;
            }
            index += 1;

            let mut instance_id = [0u16; 512];
            let mut required = 0;
            if SetupDiGetDeviceInstanceIdW(
                devices,
                &info,
                instance_id.as_mut_ptr(),
                instance_id.len() as u32,
                &mut required,
            ) == 0
            {
                continue;
            }
            let len = instance_id
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(required as usize);
            ids.push(String::from_utf16_lossy(&instance_id[..len]));
        }
        SetupDiDestroyDeviceInfoList(devices);
        Ok(ids)
    }
}

fn target_pid_from_instance_id(instance_id: &str) -> Option<u16> {
    let upper = instance_id.to_ascii_uppercase();
    let marker = format!("VID_{APPLE_VID:04X}&PID_");
    let start = upper.find(&marker)? + marker.len();
    let pid = u16::from_str_radix(upper.get(start..start + 4)?, 16).ok()?;
    is_target_pid(pid).then_some(pid)
}

/// Bind the WinUSB driver to every connected Apple DFU/recovery/restore device
/// and RecoverKit dongle, so libusb and nusb can open them. Returns the number
/// of devices set up.
///
/// Must run elevated (the CLI handles that).
pub fn install_winusb(progress: ProgressFn) -> Result<usize> {
    let targets = connected_targets()?;
    let dongles = connected_dongles()?;
    if targets.is_empty() && dongles == 0 {
        return Err(Error::DriverInstall(
            "no Apple device in DFU or recovery mode and no RecoverKit dongle is connected — \
             plug in the dongle or put the target into DFU (see `restorekit status`) and re-run"
                .into(),
        ));
    }

    progress(Event::DriverSetupStarting);
    run_setup_script(false).map_err(|e| match e {
        Error::DriverInstall(detail) if native_arm64() => {
            Error::DriverInstall(format!("{ARM64_HINT}\n\n{detail}"))
        }
        e => e,
    })?;

    for t in &targets {
        progress(Event::DriverBound {
            name: t.name.clone(),
        });
    }
    for _ in 0..dongles {
        progress(Event::DriverBound {
            name: "RecoverKit dongle".into(),
        });
    }
    Ok(targets.len() + dongles)
}

/// Sign and stage our WinUSB package, then (unless `stage_only`) force it onto
/// every connected device it covers. Must run elevated.
fn run_setup_script(stage_only: bool) -> Result<()> {
    // A private work dir holding only the INF(s) — the catalog hashes this
    // directory, so nothing else may live in it. The script goes alongside.
    let base = std::env::temp_dir();
    let stamp = std::process::id();
    let work = base.join(format!("restorekit-winusb-{stamp}"));
    let script = base.join(format!("restorekit-winusb-{stamp}.ps1"));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(|e| Error::DriverInstall(e.to_string()))?;

    // One INF covering every DFU/recovery mode, staged in a single pnputil run,
    // so a restore that hops DFU -> recovery is covered without re-running.
    let inf_path = work.join("restorekit_winusb.inf");
    write_file(&inf_path, winusb_inf(DRIVER_PIDS).as_bytes())?;
    write_file(&script, WINUSB_SETUP_PS1.as_bytes())?;

    // CREATE_NO_WINDOW: don't flash a PowerShell console (matters for the GUI).
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = Command::new("powershell");
    cmd.creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-WorkDir")
        .arg(&work)
        .arg("-DeviceRegex")
        .arg(bind_regex());
    if stage_only {
        cmd.arg("-StageOnly");
    }
    let output = cmd
        .output()
        .map_err(|e| Error::DriverInstall(format!("failed to run PowerShell: {e}")));

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&script);

    let output = output?;
    if !output.status.success() {
        // pnputil's own output (on stdout) usually says why; keep both.
        let detail = [&output.stderr, &output.stdout]
            .iter()
            .map(|b| String::from_utf8_lossy(b).trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        return Err(Error::DriverInstall(format!(
            "WinUSB setup did not complete: {detail}"
        )));
    }
    Ok(())
}

/// PowerShell regex matching the instance ids the setup script binds: the
/// Apple modes in [`DRIVER_PIDS`] and the dongle's vendor interface.
fn bind_regex() -> String {
    let pids: Vec<String> = DRIVER_PIDS.iter().map(|p| format!("{p:04X}")).collect();
    format!(
        "^USB\\\\VID_{APPLE_VID:04X}&PID_({})\\\\|^{}\\\\",
        pids.join("|"),
        dongle_interface_hwid().replace('\\', "\\\\")
    )
}

/// A WinUSB device INF binding the inbox `winusb.sys` to every Apple product id
/// in `pids`.
///
/// Uses the documented `Include=winusb.inf` / `Needs=WINUSB.NT` form, which
/// relies on the in-box WinUSB driver (Windows 10+), so no redistributable
/// driver binaries ship with restorekit. `CatalogFile` names the signed catalog
/// the setup script produces.
fn winusb_inf(pids: &[u16]) -> String {
    let mut models = String::new();
    for pid in pids {
        let install = match *pid {
            0x1222 | 0x1227 => "WINUSB_DFU_Install",
            0x1280..=0x1283 => "WINUSB_IBOOT_Install",
            0xf014 => "WINUSB_PORTDFU_Install",
            0x1881 => "WINUSB_KIS_Install",
            _ => unreachable!("unsupported Apple mode PID"),
        };
        models.push_str(&format!(
            "%DeviceName% = {install}, USB\\VID_{APPLE_VID:04X}&PID_{pid:04X}\n"
        ));
    }
    // Restore mode (Apple Silicon): the device becomes a USB *composite* whose
    // data interface enumerates as `VID&PID&RESTORE_MODE&MI_00`. Apple's
    // `appleusb.inf` binds a WinUSB *variant* that libusb can't open
    // (`winusbx_open` → ERROR_NOT_SUPPORTED), so bind our plain `winusb.sys`
    // there too — a more specific, trusted match that wins on re-enumeration.
    // The `RESTORE_MODE` qualifier is restore-only, so this never hijacks a
    // normal-mode iPhone/iPad sharing a 0x129x product id.
    for pid in RESTORE_MODE_PIDS {
        models.push_str(&format!(
            "%DeviceName% = WINUSB_RESTORE_Install, USB\\VID_{APPLE_VID:04X}&PID_{pid:04X}&RESTORE_MODE&MI_00\n"
        ));
    }
    // The dongle's vendor interface. Its firmware asks Windows for WinUSB but,
    // through 0.1.3, gave no interface GUID, without which Windows never exposes
    // that interface to apps; binding it here supplies the GUID. More specific
    // than the generic WinUSB compatible-id match, so it wins that ranking.
    models.push_str(&format!(
        "%DongleName% = WINUSB_DONGLE_Install, {}\n",
        dongle_interface_hwid()
    ));
    format!(
        r#"; Generated by RestoreKit - binds WinUSB to an Apple device so libusb can access it.
[Version]
Signature   = "$Windows NT$"
Class       = USBDevice
ClassGuid   = {{88BAE032-5A81-49f0-BC3D-A4FF138216D6}}
Provider    = %ProviderName%
CatalogFile = restorekit_winusb.cat
DriverVer   = 01/01/2024,1.0.0.2

[Manufacturer]
%ProviderName% = Standard,NTamd64,NTarm64

[Standard.NTamd64]
{models}
[Standard.NTarm64]
{models}
[WINUSB_DFU_Install.NT]
Include = winusb.inf
Needs   = WINUSB.NT

[WINUSB_DFU_Install.NT.Services]
Include = winusb.inf
Needs   = WINUSB.NT.Services

[WINUSB_DFU_Install.NT.HW]
AddReg = WINUSB_DFU_AddReg

[WINUSB_DFU_AddReg]
HKR,,DeviceInterfaceGUIDs,0x10000,"{{B8085869-FEB9-404B-8CB1-1E5C14FA8C54}}"

[WINUSB_IBOOT_Install.NT]
Include = winusb.inf
Needs   = WINUSB.NT

[WINUSB_IBOOT_Install.NT.Services]
Include = winusb.inf
Needs   = WINUSB.NT.Services

[WINUSB_IBOOT_Install.NT.HW]
AddReg = WINUSB_IBOOT_AddReg

[WINUSB_IBOOT_AddReg]
HKR,,DeviceInterfaceGUIDs,0x10000,"{{ED82A167-D61A-4AF6-9AB6-11E52236C576}}"

[WINUSB_PORTDFU_Install.NT]
Include = winusb.inf
Needs   = WINUSB.NT

[WINUSB_PORTDFU_Install.NT.Services]
Include = winusb.inf
Needs   = WINUSB.NT.Services

[WINUSB_PORTDFU_Install.NT.HW]
AddReg = WINUSB_PORTDFU_AddReg

[WINUSB_PORTDFU_AddReg]
HKR,,DeviceInterfaceGUIDs,0x10000,"{{AF633FF1-1170-4CA6-AE9E-08D001421EAA}}"

[WINUSB_KIS_Install.NT]
Include = winusb.inf
Needs   = WINUSB.NT

[WINUSB_KIS_Install.NT.Services]
Include = winusb.inf
Needs   = WINUSB.NT.Services

[WINUSB_KIS_Install.NT.HW]
AddReg = WINUSB_KIS_AddReg

[WINUSB_KIS_AddReg]
HKR,,DeviceInterfaceGUIDs,0x10000,"{{B36F4137-F4EF-4BFC-A25A-C2410768EE37}}"

[WINUSB_RESTORE_Install.NT]
Include = winusb.inf
Needs   = WINUSB.NT

[WINUSB_RESTORE_Install.NT.Services]
Include = winusb.inf
Needs   = WINUSB.NT.Services

[WINUSB_RESTORE_Install.NT.HW]
AddReg = WINUSB_RESTORE_AddReg

[WINUSB_RESTORE_AddReg]
HKR,,DeviceInterfaceGUIDs,0x10000,"{{3F966BD9-FA04-4EC5-991C-D326973B5128}}"

[WINUSB_DONGLE_Install.NT]
Include = winusb.inf
Needs   = WINUSB.NT

[WINUSB_DONGLE_Install.NT.Services]
Include = winusb.inf
Needs   = WINUSB.NT.Services

[WINUSB_DONGLE_Install.NT.HW]
AddReg = WINUSB_DONGLE_AddReg

[WINUSB_DONGLE_AddReg]
HKR,,DeviceInterfaceGUIDs,0x10000,"{dongle_guid}"

[Strings]
ProviderName = "RestoreKit"
DeviceName   = "Apple Mobile Device (RestoreKit WinUSB)"
DongleName   = "RecoverKit Dongle (RestoreKit WinUSB)"
"#,
        dongle_guid = proto::WINUSB_INTERFACE_GUID
    )
}

/// PowerShell that signs and installs the WinUSB package for `$WorkDir`.
///
/// Reuses a stable `CN=RestoreKit WinUSB` code-signing cert (creating it once),
/// builds + signs a catalog over the INF(s), trusts the cert and stages the
/// package. Unless `-StageOnly`, it then forces the package onto every present
/// device matching `-DeviceRegex` and verifies WinUSB attached, exiting
/// non-zero if not.
///
/// Forcing is needed because Windows Update installs Apple's own WHQL-signed
/// drivers (e.g. `applekis.inf` for 05ac:1881), which outrank our self-signed
/// package, so a plain `pnputil /install` leaves them in place.
const WINUSB_SETUP_PS1: &str = r#"param(
    [Parameter(Mandatory=$true)][string]$WorkDir,
    [string]$DeviceRegex = '^$',
    [switch]$StageOnly
)
$ErrorActionPreference = 'Stop'
$cat = Join-Path $WorkDir 'restorekit_winusb.cat'
$subject = 'CN=RestoreKit WinUSB'
$cert = Get-ChildItem Cert:\CurrentUser\My -ErrorAction SilentlyContinue |
    Where-Object { $_.Subject -eq $subject } | Select-Object -First 1
if (-not $cert) {
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $subject `
        -CertStoreLocation Cert:\CurrentUser\My -KeyUsage DigitalSignature `
        -TextExtension @('2.5.29.37={text}1.3.6.1.5.5.7.3.3')
}
New-FileCatalog -Path $WorkDir -CatalogFilePath $cat -CatalogVersion 2 | Out-Null
Set-AuthenticodeSignature -FilePath $cat -Certificate $cert | Out-Null
# Trust the cert machine-wide through X509Store: current Windows 11 refuses
# Import-Certificate into LocalMachine\TrustedPublisher, even elevated.
$pub = [Security.Cryptography.X509Certificates.X509Certificate2]::new($cert.RawData)
foreach ($name in 'Root', 'TrustedPublisher') {
    $store = [Security.Cryptography.X509Certificates.X509Store]::new($name, 'LocalMachine')
    $store.Open('ReadWrite')
    $store.Add($pub)
    $store.Close()
}
$infFile = Get-ChildItem $WorkDir -Filter '*.inf' | Select-Object -First 1
$log = pnputil /add-driver $infFile.FullName /install | Out-String
Write-Output $log
if ($log -match 'Failed to add driver package') {
    Write-Error "pnputil could not stage the driver package"
    exit 1
}
# The staged copy (oemNN.inf) pnputil just published.
$leaf = if ($log -match 'Published Name:\s+(oem\d+\.inf)') { $Matches[1] } else { $null }
if (-not $leaf) {
    Write-Error 'pnputil did not report a published driver package'
    exit 1
}
if ($StageOnly) { exit 0 }

Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class RkNewDev {
  [DllImport("newdev.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern bool UpdateDriverForPlugAndPlayDevices(
      IntPtr hwnd, string HardwareId, string FullInfPath, uint InstallFlags, out bool bReboot);
}
"@
$staged = Join-Path $env:windir "INF\$leaf"
$devices = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue |
    Where-Object { $_.InstanceId -match $DeviceRegex }
# Apple's own drivers (Windows Update, iTunes, Apple Devices) already work with
# restorekit, and moving a KIS Mac off AppleKIS strands it until it re-enters
# DFU, so only bind devices on no driver or someone else's.
function Test-AppleDriver($id) {
    $provider = (Get-PnpDeviceProperty -InstanceId $id -KeyName DEVPKEY_Device_DriverProvider -ErrorAction SilentlyContinue).Data
    return ($provider -match '^Apple')
}
foreach ($dev in $devices) {
    $infNow = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName DEVPKEY_Device_DriverInfPath -ErrorAction SilentlyContinue).Data
    if ($infNow -eq $leaf) { continue }
    if (Test-AppleDriver $dev.InstanceId) { continue }
    # INSTALLFLAG_FORCE (0x1) | INSTALLFLAG_NONINTERACTIVE (0x4).
    $hwid = $dev.InstanceId -replace '\\[^\\]+$', ''
    $reboot = $false
    if (-not [RkNewDev]::UpdateDriverForPlugAndPlayDevices([IntPtr]::Zero, $hwid, $staged, 0x5, [ref]$reboot)) {
        $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
        Write-Output ("could not bind {0} (was {1}): error 0x{2:X8}" -f $dev.InstanceId, $infNow, $code)
    }
}
$ready = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue |
    Where-Object { $_.InstanceId -match $DeviceRegex } |
    Where-Object {
        (Test-AppleDriver $_.InstanceId) -or
        (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName 'DEVPKEY_Device_Service' -ErrorAction SilentlyContinue).Data -eq 'WINUSB'
    }
if ($ready) { exit 0 }
Write-Error 'WinUSB was not bound to any connected device'
exit 1
"#;

fn write_file(path: &std::path::Path, bytes: &[u8]) -> Result<()> {
    let mut f = std::fs::File::create(path).map_err(|e| Error::DriverInstall(e.to_string()))?;
    f.write_all(bytes)
        .map_err(|e| Error::DriverInstall(e.to_string()))?;
    Ok(())
}

// ── Restore-mode driver watcher ──────────────────────────────────────────────

/// Internal argument that reruns *this* binary as the elevated restore-mode
/// driver watcher, followed by the liveness-file path. Both the CLI and the
/// desktop app recognize it in `main`, so the library can relaunch whichever
/// binary is running — and the UAC prompt then shows restorekit, not PowerShell.
pub const RESTORE_WATCH_ARG: &str = "--rk-bind-restore-mode";

/// RAII guard for the elevated restore-mode driver watcher. Dropping it removes
/// the liveness file, which is the watcher's signal to exit.
pub struct RestoreWatcherGuard {
    liveness: std::path::PathBuf,
}

impl Drop for RestoreWatcherGuard {
    fn drop(&mut self) {
        // Removing the liveness file tells the watcher to stop.
        let _ = std::fs::remove_file(&self.liveness);
    }
}

/// Spawn an elevated background watcher that force-binds our WinUSB to the Mac's
/// restore-mode interface the moment it appears.
///
/// In restore mode the Mac becomes a USB composite whose data interface
/// (`…&RESTORE_MODE&MI_00`) is claimed by Apple's `appleusb.inf`, whose WinUSB
/// variant libusb can't open (`winusbx_open` → ERROR_NOT_SUPPORTED). Apple's INF
/// lists that exact hardware id and is WHQL-signed, so we can't win by ranking;
/// instead we *force* our plain `winusb.sys` onto that one device instance
/// (`UpdateDriverForPlugAndPlayDevices` + `INSTALLFLAG_FORCE`), leaving
/// `appleusb.inf` in the store for iPhones. That needs admin and can only happen
/// while the device is in restore mode, so the restore relaunches this binary
/// elevated (one UAC) to run [`run_restore_mode_watch_worker`], which binds when
/// the device shows up.
///
/// Returns `None` if the watcher couldn't be launched (e.g. UAC declined) — the
/// restore still runs but will stall at "waiting for device to enter restore
/// mode". The returned guard ties the watcher's lifetime to the restore.
pub fn spawn_restore_mode_watcher() -> Option<RestoreWatcherGuard> {
    let liveness = std::env::temp_dir().join(format!(
        "restorekit-restore-live-{}.tmp",
        std::process::id()
    ));
    if std::fs::write(&liveness, b"1").is_err() {
        return None;
    }

    // Already admin (e.g. launched from an elevated shell or the app already
    // elevated)? Skip UAC entirely — run the watcher inline on a background
    // thread. Otherwise relaunch this binary elevated (one prompt, shown as
    // restorekit).
    if is_elevated() {
        let live = liveness.clone();
        std::thread::spawn(move || run_restore_mode_watch_worker(&live));
        return Some(RestoreWatcherGuard { liveness });
    }

    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => {
            let _ = std::fs::remove_file(&liveness);
            return None;
        }
    };
    let params = format!("{RESTORE_WATCH_ARG} \"{}\"", liveness.display());
    if launch_elevated_hidden(&exe.to_string_lossy(), &params).is_err() {
        let _ = std::fs::remove_file(&liveness);
        return None;
    }

    Some(RestoreWatcherGuard { liveness })
}

/// Is this process running with an elevated (administrator) token?
fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Where the restore-mode watcher logs what it did, for diagnosing a restore
/// that stalls at "waiting for device to enter restore mode".
pub fn restore_watch_log() -> std::path::PathBuf {
    std::env::temp_dir().join("restorekit-restore-watch.log")
}

/// The elevated watcher worker, invoked when this binary is rerun with
/// [`RESTORE_WATCH_ARG`]. Stages our WinUSB package (a restore can reach this
/// point without `setup-driver` ever having run: with Apple's own driver
/// installed, DFU and recovery work without it), then runs the PowerShell
/// force-bind helper as a child (inheriting this process's elevation) and
/// returns when it exits — i.e. when the restore removes the liveness file or
/// the device leaves after being bound.
pub fn run_restore_mode_watch_worker(liveness: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let log = restore_watch_log();
    let staged = match run_setup_script(true) {
        Ok(()) => "staged the WinUSB package".to_string(),
        Err(e) => format!("staging the WinUSB package failed: {e}"),
    };
    let _ = std::fs::write(&log, format!("{staged}\r\n"));

    let script = std::env::temp_dir().join(format!(
        "restorekit-restore-watch-{}.ps1",
        std::process::id()
    ));
    if write_file(&script, RESTORE_WATCH_PS1.as_bytes()).is_err() {
        return;
    }
    let _ = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-LivenessFile")
        .arg(liveness)
        .arg("-LogFile")
        .arg(&log)
        .status();
    let _ = std::fs::remove_file(&script);
}

/// Launch `exe params` elevated (UAC) with no visible window, and return without
/// waiting — the caller manages the process's lifetime out of band.
fn launch_elevated_hidden(exe: &str, params: &str) -> Result<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }
    let verb = wide("runas");
    let file = wide(exe);
    let par = wide(params);

    unsafe {
        let mut sei: SHELLEXECUTEINFOW = std::mem::zeroed();
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.lpParameters = par.as_ptr();
        sei.nShow = SW_HIDE;
        if ShellExecuteExW(&mut sei) == 0 {
            return Err(Error::DriverInstall(
                "could not start the restore-mode driver watcher (UAC declined?)".into(),
            ));
        }
    }
    Ok(())
}

/// PowerShell watcher: find our staged INF, then poll for the restore-mode
/// device and force our WinUSB onto it (keeping `appleusb.inf` in the store).
/// Exits when the restore signals completion (liveness file gone), when the
/// device leaves after being bound, or after a safety deadline.
const RESTORE_WATCH_PS1: &str = r#"param([string]$LivenessFile, [string]$LogFile)
$ErrorActionPreference = 'SilentlyContinue'
$script:last = ''
function Log($msg) {
    if ($LogFile -and $msg -ne $script:last) { Add-Content -Path $LogFile -Value ("{0:HH:mm:ss} {1}" -f (Get-Date), $msg) }
    $script:last = $msg
}
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class RkNewDev {
  [DllImport("newdev.dll", CharSet=CharSet.Unicode, SetLastError=true)]
  public static extern bool UpdateDriverForPlugAndPlayDevices(
      IntPtr hwnd, string HardwareId, string FullInfPath, uint InstallFlags, out bool bReboot);
}
"@
# INSTALLFLAG_FORCE (0x1): install even though Apple's WHQL driver outranks ours.
# INSTALLFLAG_NONINTERACTIVE (0x4): never prompt.
$FLAGS = 0x1 -bor 0x4
# Our newest staged WinUSB package (has the RESTORE_MODE interface entries).
$inf = (Get-ChildItem (Join-Path $env:windir 'INF\oem*.inf') -ErrorAction SilentlyContinue | Where-Object {
    $c = Get-Content $_.FullName -Raw -ErrorAction SilentlyContinue
    $c -match 'RestoreKit' -and $c -match 'RESTORE_MODE'
} | Sort-Object LastWriteTime -Descending | Select-Object -First 1).FullName
if (-not $inf) { Log 'no staged RestoreKit WinUSB package found'; exit 1 }
Log "using $inf"
$leaf = Split-Path $inf -Leaf
$deadline = (Get-Date).AddMinutes(45)
$bound = $false
while ((Get-Date) -lt $deadline) {
    if ($LivenessFile -and -not (Test-Path $LivenessFile)) { Log 'restore ended'; break }
    $dev = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue |
        Where-Object { $_.InstanceId -match 'VID_05AC&PID_12[0-9A-Fa-f]{2}&RESTORE_MODE&MI_00' } |
        Select-Object -First 1
    if ($dev) {
        $infNow = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName DEVPKEY_Device_DriverInfPath -ErrorAction SilentlyContinue).Data
        if ($infNow -ne $leaf) {
            $hwid = ($dev.InstanceId -replace '\\[^\\]+$', '')
            $reboot = $false
            $ok = [RkNewDev]::UpdateDriverForPlugAndPlayDevices([IntPtr]::Zero, $hwid, $inf, $FLAGS, [ref]$reboot)
            $code = [Runtime.InteropServices.Marshal]::GetLastWin32Error()
            Log ("{0} was on {1}; bind -> ok={2} error=0x{3:X8}" -f $dev.InstanceId, $infNow, $ok, $code)
        }
        $bound = $true
    } elseif ($bound) {
        Log 'restore-mode device left'
        break
    }
    Start-Sleep -Milliseconds 350
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setupapi_instance_ids_find_unbound_targets() {
        assert_eq!(
            target_pid_from_instance_id("USB\\VID_05AC&PID_1227\\000445E11462001E"),
            Some(0x1227)
        );
        assert_eq!(
            target_pid_from_instance_id("USB\\VID_05AC&PID_1290\\NORMAL_MODE"),
            None
        );
        assert_eq!(
            target_pid_from_instance_id("USB\\VID_1234&PID_1227\\OTHER"),
            None
        );
    }

    #[test]
    fn inf_supports_both_windows_architectures_and_apple_modes() {
        let inf = winusb_inf(DRIVER_PIDS);
        assert!(inf.contains("Standard,NTamd64,NTarm64"));
        assert!(inf.contains("[Standard.NTarm64]"));
        assert!(inf.contains("USB\\VID_05AC&PID_1227"));
        assert!(inf.contains("USB\\VID_05AC&PID_12AC&RESTORE_MODE&MI_00"));
        assert!(inf.contains("{B8085869-FEB9-404B-8CB1-1E5C14FA8C54}"));
        assert!(inf.contains("{ED82A167-D61A-4AF6-9AB6-11E52236C576}"));
        assert!(inf.contains("{AF633FF1-1170-4CA6-AE9E-08D001421EAA}"));
        assert!(inf.contains("{B36F4137-F4EF-4BFC-A25A-C2410768EE37}"));
        assert!(inf.contains("USB\\VID_16D0&PID_14F0&MI_04\n"));
        assert!(inf.contains(proto::WINUSB_INTERFACE_GUID));
    }

    #[test]
    fn dongle_interface_is_recognized() {
        assert!(is_dongle_interface(
            "USB\\VID_16D0&PID_14F0&MI_04\\6&2E2718E1&0&0004"
        ));
        assert!(!is_dongle_interface(
            "USB\\VID_16D0&PID_14F0&MI_00\\6&2E2718E1&0&0000"
        ));
        assert!(!is_dongle_interface("USB\\VID_16D0&PID_14F0\\DL-6BAF14C4"));
    }

    #[test]
    fn bind_regex_is_a_powershell_regex_over_instance_ids() {
        assert_eq!(
            bind_regex(),
            "^USB\\\\VID_05AC&PID_(1222|1227|1280|1281|1282|1283|F014|1881)\\\\\
             |^USB\\\\VID_16D0&PID_14F0&MI_04\\\\"
        );
    }
}
