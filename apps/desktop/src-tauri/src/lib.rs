mod commands;
mod elevate;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::host_can_trigger,
            commands::manual_instructions,
            commands::list_devices,
            commands::cache_dir,
            commands::trigger_dfu,
            commands::resolve_firmware,
            commands::download_firmware,
            commands::restore,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the restorekit app");
}
