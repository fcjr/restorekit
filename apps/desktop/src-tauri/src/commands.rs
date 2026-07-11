use restorekit::device::{self, parse_serial, Device};
use restorekit::dongle;
use restorekit::restore::Mode;
use restorekit::{dfu, firmware, restore, DfuOutcome, DfuVia, DongleStatus, Firmware};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// How long to wait for a dongle-triggered DFU to be observed on this host.
const DONGLE_DFU_TIMEOUT: Duration = Duration::from_secs(60);

/// One connected Apple device and the USB mode it's in. ECID is a hex string
/// (a u64 as a JSON number would lose precision in JS); `serial` is the raw DFU
/// string so the backend can re-derive the exact ECID without a float round-trip.
#[derive(Serialize)]
pub struct DeviceView {
    /// "dfu" | "recovery" | "wtf" | "restore" | "other"
    pub mode: String,
    pub name: String,
    pub identifier: Option<String>,
    pub chip: String,
    pub board: String,
    pub ecid: String,
    pub srtg: Option<String>,
    pub serial: String,
    /// The captured hardware serial number (recovery/booted), if any. Distinct
    /// from the raw `serial` DFU string above.
    pub serial_number: Option<String>,
    /// Whether restorekit can restore a device in this mode (DFU only).
    pub restorable: bool,
    /// The host port this device is on (macOS): its firmware location name and
    /// whether it's the DFU-capable port. `None` when undeterminable.
    pub port: Option<restorekit::Port>,
    /// Whether the OS driver lets restorekit open this device. Always true on
    /// macOS/Linux; on Windows, false until WinUSB is bound (see `setup_driver`).
    pub driver_ready: bool,
    /// How this device reaches the host: "direct" | "dongle" | "hub".
    pub connection: String,
    /// The dongle id when the device is reached through one; `None` otherwise.
    /// The UI routes DFU/reboot over this dongle instead of the host trigger.
    pub via_dongle: Option<String>,
    /// Whether the host's own USB-PD trigger can put this device into DFU — only
    /// when it's cabled straight to the host's DFU port (not via a dongle/hub).
    pub host_dfu_capable: bool,
}

fn view(d: Device) -> DeviceView {
    let conn = dongle::connection_for(&d);
    let host_dfu_capable = conn.host_reachable() && d.port.as_ref().is_some_and(|p| p.dfu);
    let via_dongle = conn.dongle().map(str::to_string);
    let connection = conn.kind().to_string();
    DeviceView {
        connection,
        via_dongle,
        host_dfu_capable,
        restorable: d.restorable(),
        driver_ready: d.driver_ready,
        mode: d.mode.to_string(),
        name: d.display_name(),
        identifier: d.identifier().map(str::to_string),
        chip: d
            .identity
            .as_ref()
            .map(|i| format!("CPID:{:04x}", i.cpid))
            .unwrap_or_default(),
        board: d
            .identity
            .as_ref()
            .map(|i| format!("BDID:{:02x}", i.bdid))
            .unwrap_or_default(),
        ecid: d.ecid_hex().unwrap_or_default(),
        srtg: d.identity.as_ref().and_then(|i| i.srtg.clone()),
        serial_number: d.srnm.clone(),
        serial: d.serial,
        port: d.port,
    }
}

/// Every Apple device currently on the USB bus, with its mode. Devices in a
/// restore mode (DFU/recovery/restore) carry a full identity; anything else is
/// shown by its USB product name so the user still sees what's connected.
#[tauri::command]
pub fn list_devices() -> Result<Vec<DeviceView>, String> {
    let mut devices = device::list().map_err(|e| e.to_string())?;
    // Fill in booted Macs' model/ECID/name (cached per serial, so the poll
    // only pays for the network name lookup once per machine).
    device::identify(&mut devices);
    Ok(devices.into_iter().map(view).collect())
}

#[tauri::command]
pub fn host_can_trigger() -> bool {
    dfu::host_can_trigger_dfu()
}

/// A connected RecoverKit dongle: its id, live status, and the Mac (if any)
/// cabled to it and visible on USB here.
#[derive(Serialize)]
pub struct DongleView {
    pub serial: String,
    pub product: String,
    /// Firmware version the dongle reports; `None` if it couldn't be read.
    pub fw_version: Option<String>,
    /// Live PD status, if the vendor interface could be read.
    pub status: Option<DongleStatus>,
    /// The Mac cabled to this dongle, if its USB data reaches this host.
    pub target: Option<DeviceView>,
}

fn dongle_view(d: restorekit::Dongle) -> DongleView {
    // Best-effort: reading status claims the vendor interface; the topology
    // lookup enumerates USB. Either may fail without failing the whole list.
    let fw_version = d.fw_version().ok();
    let status = d.status().ok();
    let target = d.attached_device().ok().flatten().map(view);
    DongleView {
        serial: d.serial,
        product: d.product,
        fw_version,
        status,
        target,
    }
}

/// Every connected RecoverKit dongle, with live status and its cabled Mac.
/// Dongle DFU is plain USB — no root, no helper, works on any host OS.
#[tauri::command]
pub fn list_dongles() -> Result<Vec<DongleView>, String> {
    let dongles = dongle::list().map_err(|e| e.to_string())?;
    Ok(dongles.into_iter().map(dongle_view).collect())
}

/// Put the Mac cabled to dongle `serial` into DFU mode (plain USB, no helper).
/// Uses the same routed flow as the CLI (CC-cycle + retries).
#[tauri::command]
pub async fn dongle_dfu(serial: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        dfu::trigger_dfu(DfuVia::Dongle(serial), DONGLE_DFU_TIMEOUT, &mut |_| {})
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Reboot the Mac cabled to dongle `serial` (plain USB, no helper). Uses the
/// same routed flow as the CLI, including the retry loop that boots a target
/// back out of DFU (the bootrom acts on the reboot VDM only intermittently).
#[tauri::command]
pub async fn dongle_reboot(serial: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        dfu::reboot(DfuVia::Dongle(serial), &mut |_| {}).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Firmware-update availability for one dongle, against the published
/// releases (network).
#[derive(Serialize)]
pub struct DongleFwCheck {
    /// The version the dongle reports; `None` when unreadable.
    pub current: Option<String>,
    /// The newest published release for its model; `None` when none exist.
    pub latest: Option<String>,
    /// Whether `latest` should be installed (an unreadable current counts).
    pub available: bool,
}

/// Check whether a newer firmware release is published for dongle `serial`.
#[tauri::command]
pub async fn dongle_fw_check(serial: String) -> Result<DongleFwCheck, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let d = dongle::find(restorekit::DongleTarget::Id(serial)).map_err(|e| e.to_string())?;
        let current = d.fw_version().ok();
        let latest = dongle::latest_firmware(d.model).map_err(|e| e.to_string())?;
        let available = latest
            .as_ref()
            .is_some_and(|r| r.newer_than(current.as_deref().unwrap_or("unknown")));
        Ok(DongleFwCheck {
            current,
            latest: latest.map(|r| r.version),
            available,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Streamed firmware-update progress, emitted as `dongle_fw_progress` events.
#[derive(Clone, Serialize)]
struct DongleFwProgress {
    serial: String,
    staged: usize,
    total: usize,
}

/// Download the latest published firmware and stream it onto dongle `serial`
/// over its vendor interface. The dongle verifies, reboots, and its
/// bootloader swaps the image in (rolling back if it fails to boot). Emits
/// `dongle_fw_progress` events; resolves with the version running afterward.
#[tauri::command]
pub async fn dongle_fw_update(app: AppHandle, serial: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let d = dongle::find(restorekit::DongleTarget::Id(serial)).map_err(|e| e.to_string())?;
        let release = dongle::latest_firmware(d.model)
            .map_err(|e| e.to_string())?
            .ok_or("no published firmware releases for this model")?;
        let image = release.download().map_err(|e| e.to_string())?;
        let handle = d.open().map_err(|e| e.to_string())?;
        handle
            .update(&image, |staged, total| {
                let _ = app.emit(
                    "dongle_fw_progress",
                    DongleFwProgress {
                        serial: d.serial.clone(),
                        staged,
                        total,
                    },
                );
            })
            .map_err(|e| e.to_string())?;
        // The claimed interface is stale once the dongle reboots to swap.
        drop(handle);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            std::thread::sleep(Duration::from_millis(500));
            if let Some(back) = dongle::list()
                .ok()
                .and_then(|ds| ds.into_iter().find(|x| x.serial == d.serial))
            {
                return Ok(back.fw_version().unwrap_or(release.version));
            }
            if std::time::Instant::now() >= deadline {
                return Err("the dongle did not come back after the update; \
                     its bootloader reverts a firmware that fails to boot"
                    .into());
            }
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Whether this build includes the serial-capture history feature.
#[tauri::command]
pub fn history_enabled() -> bool {
    cfg!(feature = "history")
}

/// Launch Apple Configurator (macOS only). Errors if it isn't installed.
#[tauri::command]
pub fn open_apple_configurator() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let status = std::process::Command::new("open")
            .args(["-a", "Apple Configurator"])
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("Apple Configurator isn't installed — get it free from the App Store.".into())
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Apple Configurator is only available on macOS".into())
    }
}

#[tauri::command]
pub fn manual_instructions() -> String {
    dfu::manual_dfu_instructions().to_string()
}

/// Trigger DFU on the target and return the Mac that newly entered DFU.
///
/// When `dongle` is set the target is behind that dongle, so DFU is routed over
/// it (plain USB — no helper, works on any host OS). Otherwise it uses the host's
/// electronic trigger via the elevated helper (Touch ID prompt), which needs an
/// Apple Silicon Mac host.
#[tauri::command]
pub async fn trigger_dfu(dongle: Option<String>) -> Result<DeviceView, String> {
    if let Some(serial) = dongle {
        return tauri::async_runtime::spawn_blocking(move || {
            match dfu::trigger_dfu(DfuVia::Dongle(serial), DONGLE_DFU_TIMEOUT, &mut |_| {}) {
                Ok(DfuOutcome::Entered(dev)) => Ok(view(dev)),
                Ok(DfuOutcome::Sent) => Err("DFU trigger sent over the dongle, but this Mac's \
                     USB data isn't cabled to this host to confirm it entered DFU."
                    .into()),
                Err(e) => Err(e.to_string()),
            }
        })
        .await
        .map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(NO_TRIGGER.into())
    }

    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            // Subscribe before triggering so a Mac already sitting in DFU is
            // never mistaken for the one the trigger just rebooted.
            let mut watch = dfu::watch().map_err(|e| e.to_string())?;
            // The target's boot ROM occasionally misses the DFU request and
            // boots normally (a timing race Apple Configurator hits too), so
            // re-send the trigger when DFU enumeration doesn't happen.
            let mut attempt = 1;
            loop {
                crate::elevate::run_helper("dfu")?;
                match watch.wait(std::time::Duration::from_secs(10)) {
                    Ok(device) => return Ok(view(device)),
                    Err(restorekit::Error::WaitTimeout) if attempt < 3 => attempt += 1,
                    Err(e) => return Err(e.to_string()),
                }
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Reboot the target back out of DFU. When `dongle` is set, routes over it using
/// the CLI's retry-until-it-leaves-DFU flow (plain USB, any host OS); otherwise
/// uses the elevated helper (macOS-only).
#[tauri::command]
pub async fn reboot_target(dongle: Option<String>) -> Result<(), String> {
    if let Some(serial) = dongle {
        return tauri::async_runtime::spawn_blocking(move || {
            dfu::reboot(DfuVia::Dongle(serial), &mut |_| {}).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?;
    }

    #[cfg(not(target_os = "macos"))]
    {
        Err(NO_TRIGGER.into())
    }

    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(|| crate::elevate::run_helper("reboot"))
            .await
            .map_err(|e| e.to_string())?
    }
}

#[cfg(not(target_os = "macos"))]
const NO_TRIGGER: &str = "Entering DFU electronically needs an Apple Silicon Mac host. \
    Put the target into DFU by hand and it will show up here.";

/// Registration state of the privileged helper daemon:
/// "enabled" | "requiresApproval" | "notRegistered" | "notFound" | "unavailable".
#[tauri::command]
pub fn helper_status() -> String {
    #[cfg(target_os = "macos")]
    {
        crate::elevate::status().to_string()
    }
    #[cfg(not(target_os = "macos"))]
    {
        "unavailable".to_string()
    }
}

/// Bring the app window to the foreground (used after the helper is approved,
/// so the app surfaces itself over System Settings).
#[tauri::command]
pub fn focus_app(window: tauri::WebviewWindow) -> Result<(), String> {
    let _ = window.unminimize();
    let _ = window.show();
    window.set_focus().map_err(|e| e.to_string())
}

/// Register the helper and open System Settings so the user can approve it.
#[tauri::command]
pub fn approve_helper() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        crate::elevate::approve()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("the privileged helper is only used on macOS".into())
    }
}

/// Bind the WinUSB driver (Windows) so restorekit can open the cabled Mac. Shows
/// a UAC prompt; blocks until it finishes.
#[tauri::command]
pub async fn setup_driver() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        tauri::async_runtime::spawn_blocking(crate::winusb::setup_driver)
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("WinUSB setup is only needed on Windows".into())
    }
}

#[tauri::command]
pub async fn resolve_firmware(
    identifier: String,
    os_version: Option<String>,
) -> Result<Firmware, String> {
    tauri::async_runtime::spawn_blocking(move || {
        firmware::resolve(&identifier, os_version.as_deref()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Download firmware into the cache, emitting `progress` events. Returns the path.
#[tauri::command]
pub async fn download_firmware(app: AppHandle, firmware: Firmware) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cache = restorekit::firmware::default_cache_dir().map_err(|e| e.to_string())?;
        let path = restorekit::firmware::download(&cache, &firmware, &mut |event| {
            let _ = app.emit("progress", &event);
        })
        .map_err(|e| e.to_string())?;
        Ok(path.display().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Restore (or revive) a device, emitting `progress` events. The UI confirms the
/// erase first. `serial` is the raw DFU string; the exact ECID is parsed from it.
#[tauri::command]
pub async fn restore(
    app: AppHandle,
    ipsw: String,
    serial: String,
    revive: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (_, _, ecid, _, _) =
            parse_serial(&serial).ok_or_else(|| "could not parse device serial".to_string())?;
        let cache = firmware::default_cache_dir().ok();
        let mode = if revive { Mode::Revive } else { Mode::Erase };
        restore::restore(
            std::path::Path::new(&ipsw),
            ecid,
            cache.as_deref(),
            mode,
            false,
            &mut |event| {
                let _ = app.emit("progress", &event);
            },
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Quote a CSV field when it contains a comma, quote, or newline.
fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Write the currently connected devices to `path` as CSV.
#[tauri::command]
pub fn export_devices_csv(path: String) -> Result<(), String> {
    let mut devices = device::list().map_err(|e| e.to_string())?;
    device::identify(&mut devices);
    let mut out = String::from("Serial,Model,Identifier,ECID,Mode,iBoot,Port,DFUPort\n");
    for d in &devices {
        let cols = [
            d.srnm.clone().unwrap_or_default(),
            d.display_name(),
            d.identifier().unwrap_or_default().to_string(),
            d.ecid_hex().unwrap_or_default(),
            d.mode.to_string(),
            d.srtg().unwrap_or_default().to_string(),
            d.port
                .as_ref()
                .and_then(|p| p.location.clone())
                .unwrap_or_default(),
            d.port
                .as_ref()
                .map(|p| if p.dfu { "yes" } else { "no" }.to_string())
                .unwrap_or_default(),
        ];
        out.push_str(&cols.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    std::fs::write(&path, out).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct CacheInfo {
    pub path: String,
    pub bytes: u64,
    pub count: usize,
}

#[tauri::command]
pub fn cache_info() -> Result<CacheInfo, String> {
    let dir = firmware::default_cache_dir().map_err(|e| e.to_string())?;
    let mut bytes = 0u64;
    let mut count = 0usize;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|e| e.to_str()) == Some("ipsw") {
                bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
                count += 1;
            }
        }
    }
    Ok(CacheInfo {
        path: dir.display().to_string(),
        bytes,
        count,
    })
}

#[tauri::command]
pub fn clear_cache() -> Result<(), String> {
    let dir = firmware::default_cache_dir().map_err(|e| e.to_string())?;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.ends_with(".ipsw") || name.ends_with(".ipsw.json") || name.ends_with(".partial")
            {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    Ok(())
}
