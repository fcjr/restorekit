use qrcode::render::svg;
use qrcode::QrCode;
use restorekit::history::{self, HistoryEntry};

#[tauri::command]
pub fn history_list() -> Result<Vec<HistoryEntry>, String> {
    history::list().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn record_capture(entry: HistoryEntry) -> Result<(), String> {
    history::record(&entry).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn history_clear() -> Result<(), String> {
    history::clear().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_history_csv(path: String) -> Result<(), String> {
    history::export_csv(std::path::Path::new(&path)).map_err(|e| e.to_string())
}

/// Render `text` as a QR code and return it as a standalone SVG string. Black on
/// white so it scans regardless of the app theme.
#[tauri::command]
pub fn serial_qr_svg(text: String) -> Result<String, String> {
    let code = QrCode::new(text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(code
        .render()
        .min_dimensions(220, 220)
        .quiet_zone(true)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build())
}
