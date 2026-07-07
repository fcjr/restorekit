//! Persisted app settings, stored as `<config_dir>/settings.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Settings {
    /// Automatically trigger DFU on a detected booted/recovery Mac. Off by
    /// default; only acts when this host can trigger DFU and the helper is enabled.
    #[serde(default)]
    pub auto_dfu: bool,
}

fn settings_path() -> Result<PathBuf, String> {
    let cache = restorekit::firmware::default_cache_dir().map_err(|e| e.to_string())?;
    let dir = cache.parent().map(Path::to_path_buf).unwrap_or(cache);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

#[tauri::command]
pub fn get_settings() -> Result<Settings, String> {
    match std::fs::read_to_string(settings_path()?) {
        Ok(t) => Ok(serde_json::from_str(&t).unwrap_or_default()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub fn set_auto_dfu(enabled: bool) -> Result<(), String> {
    let mut settings = get_settings()?;
    settings.auto_dfu = enabled;
    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    std::fs::write(settings_path()?, json).map_err(|e| e.to_string())
}
