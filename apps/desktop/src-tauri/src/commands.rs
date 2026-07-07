use restorekit::device::{self, parse_serial, Device};
use restorekit::restore::Mode;
use restorekit::{dfu, firmware, restore, Firmware};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

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
}

fn view(d: Device) -> DeviceView {
    DeviceView {
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

/// Trigger DFU on the cabled target via the elevated helper (Touch ID prompt)
/// and return the Mac that newly entered DFU. macOS-only: electronic DFU entry
/// needs an Apple Silicon Mac host.
#[tauri::command]
pub async fn trigger_dfu() -> Result<DeviceView, String> {
    #[cfg(not(target_os = "macos"))]
    return Err(NO_TRIGGER.into());

    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(|| {
            // Subscribe before triggering so a Mac already sitting in DFU is
            // never mistaken for the one the trigger just rebooted.
            let watch = dfu::watch().map_err(|e| e.to_string())?;
            crate::elevate::run_helper("dfu")?;
            let device = watch
                .wait(std::time::Duration::from_secs(20))
                .map_err(|e| e.to_string())?;
            Ok(view(device))
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

/// Reboot the cabled target out of DFU via the elevated helper (macOS-only).
#[tauri::command]
pub async fn reboot_target() -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    return Err(NO_TRIGGER.into());

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
