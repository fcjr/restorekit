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

/// A connected Apple device that needs WinUSB bound before restorekit can use it.
#[derive(Debug, Clone)]
pub struct Target {
    pub pid: u16,
    /// Human-readable label (product string, or a mode id as a fallback).
    pub name: String,
}

/// Enumerate connected Apple devices in DFU, recovery, or restore mode.
pub fn connected_targets() -> Result<Vec<Target>> {
    use nusb::MaybeFuture;
    let devices = nusb::list_devices()
        .wait()
        .map_err(|e| Error::Usb(e.to_string()))?;
    Ok(devices
        .filter(|d| d.vendor_id() == APPLE_VID && is_target_pid(d.product_id()))
        .map(|d| {
            let pid = d.product_id();
            let name = d
                .product_string()
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Apple device (mode {pid:#06x})"));
            Target { pid, name }
        })
        .collect())
}

/// Bind the WinUSB driver to every connected Apple DFU/recovery/restore device,
/// so libusb can open it. Returns the number of devices set up.
///
/// Must run elevated (the CLI handles that).
pub fn install_winusb(progress: ProgressFn) -> Result<usize> {
    let targets = connected_targets()?;
    if targets.is_empty() {
        return Err(Error::DriverInstall(
            "no Apple device in DFU or recovery mode is connected — put the target into DFU \
             (see `restorekit status`) and re-run"
                .into(),
        ));
    }

    progress(Event::DriverSetupStarting);

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
    let output = Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-WorkDir")
        .arg(&work)
        .output()
        .map_err(|e| Error::DriverInstall(format!("failed to run PowerShell: {e}")));

    let _ = std::fs::remove_dir_all(&work);
    let _ = std::fs::remove_file(&script);

    let output = output?;
    if !output.status.success() {
        let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if detail.is_empty() {
            detail = String::from_utf8_lossy(&output.stdout).trim().to_string();
        }
        return Err(Error::DriverInstall(format!(
            "WinUSB setup did not complete: {detail}"
        )));
    }

    for t in &targets {
        progress(Event::DriverBound {
            name: t.name.clone(),
        });
    }
    Ok(targets.len())
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
        models.push_str(&format!(
            "%DeviceName% = WINUSB_Install, USB\\VID_{APPLE_VID:04X}&PID_{pid:04X}\n"
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
            "%DeviceName% = WINUSB_Install, USB\\VID_{APPLE_VID:04X}&PID_{pid:04X}&RESTORE_MODE&MI_00\n"
        ));
    }
    format!(
        r#"; Generated by RestoreKit - binds WinUSB to an Apple device so libusb can access it.
[Version]
Signature   = "$Windows NT$"
Class       = USBDevice
ClassGuid   = {{88BAE032-5A81-49f0-BC3D-A4FF138216D6}}
Provider    = %ProviderName%
CatalogFile = restorekit_winusb.cat
DriverVer   = 01/01/2024,1.0.0.0

[Manufacturer]
%ProviderName% = Standard,NTamd64

[Standard.NTamd64]
{models}
[WINUSB_Install.NT]
Include = winusb.inf
Needs   = WINUSB.NT

[WINUSB_Install.NT.Services]
Include = winusb.inf
Needs   = WINUSB.NT.Services

[Strings]
ProviderName = "RestoreKit"
DeviceName   = "Apple Mobile Device (RestoreKit WinUSB)"
"#
    )
}

/// PowerShell that signs and installs the WinUSB package for `$WorkDir`.
///
/// Reuses a stable `CN=RestoreKit WinUSB` code-signing cert (creating it once),
/// builds + signs a catalog over the INF(s), trusts the cert, binds the driver,
/// then verifies WinUSB actually attached — exiting non-zero if not.
const WINUSB_SETUP_PS1: &str = r#"param([Parameter(Mandatory=$true)][string]$WorkDir)
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
$cer = Join-Path $WorkDir 'restorekit.cer'
Export-Certificate -Cert $cert -FilePath $cer | Out-Null
Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\Root | Out-Null
Import-Certificate -FilePath $cer -CertStoreLocation Cert:\LocalMachine\TrustedPublisher | Out-Null
Get-ChildItem $WorkDir -Filter '*.inf' | ForEach-Object {
    pnputil /add-driver $_.FullName /install | Out-Host
}
$svcs = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue |
    Where-Object { $_.InstanceId -match 'VID_05AC' } |
    ForEach-Object { (Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName 'DEVPKEY_Device_Service' -ErrorAction SilentlyContinue).Data }
if ($svcs -contains 'WINUSB' -or $svcs -contains 'winusb') { exit 0 }
Write-Error 'WinUSB was not bound to any connected Apple device'
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

/// The elevated watcher worker, invoked when this binary is rerun with
/// [`RESTORE_WATCH_ARG`]. Runs the PowerShell force-bind helper as a child
/// (inheriting this process's elevation) and returns when it exits — i.e. when
/// the restore removes the liveness file or the device leaves after being bound.
pub fn run_restore_mode_watch_worker(liveness: &std::path::Path) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

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
const RESTORE_WATCH_PS1: &str = r#"param([string]$LivenessFile)
$ErrorActionPreference = 'SilentlyContinue'
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
# Our staged WinUSB package (has the RESTORE_MODE interface entries).
$inf = (Get-ChildItem C:\Windows\INF\oem*.inf -ErrorAction SilentlyContinue | Where-Object {
    $c = Get-Content $_.FullName -Raw -ErrorAction SilentlyContinue
    $c -match 'RestoreKit' -and $c -match 'RESTORE_MODE'
} | Select-Object -First 1).FullName
if (-not $inf) { exit 1 }
$leaf = Split-Path $inf -Leaf
$deadline = (Get-Date).AddMinutes(45)
$bound = $false
while ((Get-Date) -lt $deadline) {
    if ($LivenessFile -and -not (Test-Path $LivenessFile)) { break }
    $dev = Get-PnpDevice -PresentOnly -ErrorAction SilentlyContinue |
        Where-Object { $_.InstanceId -match 'VID_05AC&PID_12[0-9A-Fa-f]{2}&RESTORE_MODE&MI_00' } |
        Select-Object -First 1
    if ($dev) {
        $infNow = (Get-PnpDeviceProperty -InstanceId $dev.InstanceId -KeyName DEVPKEY_Device_DriverInfPath -ErrorAction SilentlyContinue).Data
        if ($infNow -ne $leaf) {
            $hwid = ($dev.InstanceId -replace '\\[^\\]+$', '')
            $reboot = $false
            [RkNewDev]::UpdateDriverForPlugAndPlayDevices([IntPtr]::Zero, $hwid, $inf, $FLAGS, [ref]$reboot) | Out-Null
        }
        $bound = $true
    } elseif ($bound) {
        break
    }
    Start-Sleep -Milliseconds 350
}
"#;
