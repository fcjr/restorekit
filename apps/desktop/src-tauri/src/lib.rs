mod commands;
#[cfg(feature = "history")]
mod history;
mod restores;
mod settings;
mod worker;
#[cfg(target_os = "macos")]
mod elevate;

pub use worker::maybe_run as maybe_run_restore_worker;
#[cfg(target_os = "windows")]
mod winusb;

#[cfg(target_os = "windows")]
pub use winusb::install_winusb_headless;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(restores::Restores::new());

    // The history commands only exist when the feature is compiled in;
    // generate_handler! needs the full list at each call, so branch on it.
    #[cfg(feature = "history")]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::host_can_trigger,
        commands::history_enabled,
        commands::manual_instructions,
        commands::list_devices,
        commands::trigger_dfu,
        commands::reboot_target,
        commands::list_dongles,
        commands::dongle_dfu,
        commands::dongle_reboot,
        commands::dongle_fw_check,
        commands::dongle_fw_update,
        commands::helper_status,
        commands::approve_helper,
        commands::setup_driver,
        commands::focus_app,
        commands::resolve_firmware,
        commands::download_firmware,
        commands::restore,
        commands::cache_info,
        commands::clear_cache,
        commands::export_devices_csv,
        commands::open_apple_configurator,
        settings::get_settings,
        settings::set_auto_dfu,
        restores::enqueue_restore,
        restores::cancel_restore,
        restores::restart_restore,
        restores::clear_restore_job,
        restores::list_restore_jobs,
        history::history_list,
        history::record_capture,
        history::history_clear,
        history::export_history_csv,
        history::record_seen_devices,
        history::list_seen_devices,
        history::export_seen_csv,
        history::serial_qr_svg,
    ]);
    #[cfg(not(feature = "history"))]
    let builder = builder.invoke_handler(tauri::generate_handler![
        commands::host_can_trigger,
        commands::history_enabled,
        commands::manual_instructions,
        commands::list_devices,
        commands::trigger_dfu,
        commands::reboot_target,
        commands::list_dongles,
        commands::dongle_dfu,
        commands::dongle_reboot,
        commands::dongle_fw_check,
        commands::dongle_fw_update,
        commands::helper_status,
        commands::approve_helper,
        commands::setup_driver,
        commands::focus_app,
        commands::resolve_firmware,
        commands::download_firmware,
        commands::restore,
        commands::cache_info,
        commands::clear_cache,
        commands::export_devices_csv,
        commands::open_apple_configurator,
        settings::get_settings,
        settings::set_auto_dfu,
        restores::enqueue_restore,
        restores::cancel_restore,
        restores::restart_restore,
        restores::clear_restore_job,
        restores::list_restore_jobs,
    ]);

    builder
        .run(tauri::generate_context!())
        .expect("error while running the RestoreKit app");
}
