#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Windows: when the app self-elevates to install the WinUSB driver it
    // relaunches itself with this flag. Do that headless work and exit before
    // the GUI (and WebView2) ever start.
    #[cfg(target_os = "windows")]
    {
        let args: Vec<String> = std::env::args().collect();
        if args.iter().any(|a| a == "--install-winusb-driver") {
            let result_file = args
                .iter()
                .position(|a| a == "--result-file")
                .and_then(|i| args.get(i + 1))
                .map(std::path::PathBuf::from);
            std::process::exit(restorekit_desktop_lib::install_winusb_headless(result_file));
        }
        // Relaunched elevated to run the restore-mode driver watcher (the restore
        // engine spawns this so the UAC prompt shows the app, not PowerShell).
        if let Some(i) = args
            .iter()
            .position(|a| a == restorekit::driver::RESTORE_WATCH_ARG)
        {
            if let Some(liveness) = args.get(i + 1) {
                restorekit::driver::run_restore_mode_watch_worker(std::path::Path::new(liveness));
            }
            return;
        }
    }

    restorekit_desktop_lib::run()
}
