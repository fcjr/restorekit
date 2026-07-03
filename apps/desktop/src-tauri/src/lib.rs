mod commands;
mod elevate;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::host_can_trigger,
            commands::manual_instructions,
            commands::list_devices,
            commands::trigger_dfu,
            commands::reboot_target,
            commands::resolve_firmware,
            commands::download_firmware,
            commands::restore,
            commands::cache_info,
            commands::clear_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the RestoreKit app");
}
