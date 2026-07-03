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

/// Does `pid` name a mode restorekit talks to over libusb — DFU, recovery, or
/// restore? Mirrors the product-id ranges in `udev/51-restorekit.rules`.
pub fn is_target_pid(pid: u16) -> bool {
    matches!(pid,
        0x1222 | 0x1227 | 0x1338 | 0x8600
        | 0x1280..=0x1283
        | 0x1290..=0x12af
        | 0x1901..=0x1905
    )
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

    let mut pids: Vec<u16> = targets.iter().map(|t| t.pid).collect();
    pids.sort_unstable();
    pids.dedup();
    for pid in &pids {
        let inf_path = work.join(format!("restorekit_winusb_{pid:04x}.inf"));
        write_file(&inf_path, winusb_inf(*pid).as_bytes())?;
    }
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

/// A minimal WinUSB device INF binding the inbox `winusb.sys` to the Apple
/// device with product id `pid`.
///
/// Uses the documented `Include=winusb.inf` / `Needs=WINUSB.NT` form, which
/// relies on the in-box WinUSB driver (Windows 10+), so no redistributable
/// driver binaries ship with restorekit. `CatalogFile` names the signed catalog
/// the setup script produces.
fn winusb_inf(pid: u16) -> String {
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
%DeviceName% = WINUSB_Install, USB\VID_{APPLE_VID:04X}&PID_{pid:04X}

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
