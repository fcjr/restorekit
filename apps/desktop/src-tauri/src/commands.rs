use restorekit::restore::Mode;
use restorekit::{dfu, firmware, restore, DfuDevice, Firmware};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::elevate;

/// Display-oriented view of a detected device. ECID is a hex **string** (a u64
/// serialized as a JSON number would lose precision in JS), and `serial` is the
/// raw DFU string the frontend hands back so the backend can re-derive the exact
/// ECID without any float round-trip.
#[derive(Serialize)]
pub struct DeviceView {
    pub name: String,
    pub identifier: Option<String>,
    pub chip: String,
    pub board: String,
    pub ecid: String,
    pub srtg: Option<String>,
    pub serial: String,
}

impl From<DfuDevice> for DeviceView {
    fn from(d: DfuDevice) -> Self {
        DeviceView {
            name: d.display_name(),
            identifier: d.identifier().map(str::to_string),
            chip: format!("CPID:{:04x}", d.cpid),
            board: format!("BDID:{:02x}", d.bdid),
            ecid: d.ecid_hex(),
            srtg: d.srtg.clone(),
            serial: d.serial.clone(),
        }
    }
}

#[tauri::command]
pub fn host_can_trigger() -> bool {
    dfu::host_can_trigger_dfu()
}

#[tauri::command]
pub fn manual_instructions() -> String {
    dfu::manual_dfu_instructions().to_string()
}

#[tauri::command]
pub fn list_devices() -> Result<Vec<DeviceView>, String> {
    dfu::list()
        .map(|v| v.into_iter().map(DeviceView::from).collect())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn cache_dir() -> Result<String, String> {
    firmware::default_cache_dir()
        .map(|p| p.display().to_string())
        .map_err(|e| e.to_string())
}

/// Trigger DFU via the elevated helper (admin prompt), then wait for the device.
#[tauri::command]
pub async fn trigger_dfu() -> Result<DeviceView, String> {
    tauri::async_runtime::spawn_blocking(|| {
        elevate::run_helper("dfu")?;
        dfu::wait_for_dfu(std::time::Duration::from_secs(30))
            .map(DeviceView::from)
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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

/// Restore (or revive) the device, emitting `progress` events. The UI confirms
/// the erase before calling this. `serial` is the raw DFU string; the exact ECID
/// is parsed from it here.
#[tauri::command]
pub async fn restore(
    app: AppHandle,
    ipsw: String,
    serial: String,
    revive: bool,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let (_, _, ecid, _) =
            dfu::parse_serial(&serial).ok_or_else(|| "could not parse device serial".to_string())?;
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
