mod commands;
#[cfg(target_os = "macos")]
mod elevate;
#[cfg(target_os = "windows")]
mod winusb;

#[cfg(target_os = "windows")]
pub use winusb::install_winusb_headless;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![
            commands::host_can_trigger,
            commands::manual_instructions,
            commands::list_devices,
            commands::trigger_dfu,
            commands::reboot_target,
            commands::helper_status,
            commands::approve_helper,
            commands::setup_driver,
            commands::focus_app,
            commands::resolve_firmware,
            commands::download_firmware,
            commands::restore,
            commands::cache_info,
            commands::clear_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the RestoreKit app");
}
