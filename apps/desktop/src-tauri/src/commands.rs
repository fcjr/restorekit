use nusb::MaybeFuture;
use restorekit::device;
use restorekit::dfu::{self, parse_serial};
use restorekit::restore::Mode;
use restorekit::{firmware, restore, Firmware};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

const APPLE_VID: u16 = 0x05ac;

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
    /// Whether restorekit can restore a device in this mode (DFU only).
    pub restorable: bool,
    /// Whether the OS driver lets restorekit open this device. Always true on
    /// macOS/Linux; on Windows, false until WinUSB is bound (see `setup_driver`).
    pub driver_ready: bool,
}

fn mode_for(pid: u16) -> &'static str {
    match pid {
        0x1227 => "dfu",
        0x1280 | 0x1281 => "recovery",
        0x1222 => "wtf",
        0x1338 | 0x1339 => "restore",
        _ => "other",
    }
}

/// Every Apple device currently on the USB bus, with its mode. Devices in a
/// restore mode (DFU/recovery/restore) carry a full identity; anything else is
/// shown by its USB product name so the user still sees what's connected.
#[tauri::command]
pub fn list_devices() -> Result<Vec<DeviceView>, String> {
    let devices = nusb::list_devices().wait().map_err(|e| e.to_string())?;
    let mut out = Vec::new();

    for info in devices {
        if info.vendor_id() != APPLE_VID {
            continue;
        }
        let serial = info.serial_number().unwrap_or("");
        let parsed = parse_serial(serial);
        let mode = mode_for(info.product_id());

        let (name, identifier, chip, board, ecid, srtg) = match parsed {
            Some((cpid, bdid, ecid, srtg)) => {
                let model = device::lookup(cpid, bdid);
                (
                    model
                        .map(|m| m.name.to_string())
                        .unwrap_or_else(|| format!("Unknown Mac (CPID:{cpid:04x})")),
                    model.map(|m| m.identifier.to_string()),
                    format!("CPID:{cpid:04x}"),
                    format!("BDID:{bdid:02x}"),
                    format!("0x{ecid:x}"),
                    srtg,
                )
            }
            None => (
                info.product_string().unwrap_or("Apple device").to_string(),
                None,
                String::new(),
                String::new(),
                String::new(),
                None,
            ),
        };

        out.push(DeviceView {
            restorable: mode == "dfu",
            driver_ready: driver_ready_for(&info, mode),
            mode: mode.to_string(),
            name,
            identifier,
            chip,
            board,
            ecid,
            srtg,
            serial: serial.to_string(),
        });
    }
    Ok(out)
}

/// Whether the OS driver lets restorekit open a device in `mode`. Only the
/// restore-family modes need WinUSB on Windows; never poke a normal device.
#[cfg(target_os = "windows")]
fn driver_ready_for(info: &nusb::DeviceInfo, mode: &str) -> bool {
    match mode {
        "dfu" | "recovery" | "wtf" | "restore" => crate::winusb::device_ready(info),
        _ => true,
    }
}
#[cfg(not(target_os = "windows"))]
fn driver_ready_for(_info: &nusb::DeviceInfo, _mode: &str) -> bool {
    true
}

#[tauri::command]
pub fn host_can_trigger() -> bool {
    dfu::host_can_trigger_dfu()
}

#[tauri::command]
pub fn manual_instructions() -> String {
    dfu::manual_dfu_instructions().to_string()
}

/// Trigger DFU on the cabled target via the elevated helper (Touch ID prompt).
/// macOS-only: electronic DFU entry needs an Apple Silicon Mac host.
#[tauri::command]
pub async fn trigger_dfu() -> Result<(), String> {
    #[cfg(not(target_os = "macos"))]
    return Err(NO_TRIGGER.into());

    #[cfg(target_os = "macos")]
    {
        tauri::async_runtime::spawn_blocking(|| crate::elevate::run_helper("dfu"))
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
        let (_, _, ecid, _) =
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
