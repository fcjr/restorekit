//! Embedded usbmuxd server (Linux + Windows).
//!
//! Runs the usbmuxd event loop on a background thread so `idevicerestore` (via
//! libusbmuxd) can reach USB devices for the restore-mode phase without an
//! external usbmuxd daemon. On Linux it listens on a private Unix socket; on
//! Windows on TCP `127.0.0.1:27015`, which is libusbmuxd's default there — so no
//! client-side configuration is needed.

use std::ffi::CString;
use std::thread::{self, JoinHandle};

use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

/// RAII guard that starts the embedded usbmuxd server and tears it down on drop.
pub struct UsbmuxdGuard {
    /// The Unix socket file to clean up (Linux only).
    #[cfg(target_os = "linux")]
    socket_path: std::path::PathBuf,
    thread: Option<JoinHandle<()>>,
}

impl UsbmuxdGuard {
    /// Start the embedded usbmuxd server and point libusbmuxd at it.
    pub fn start(progress: ProgressFn) -> Result<Self> {
        progress(Event::UsbmuxdStarting);

        // What we hand the C server (Linux: the socket path; Windows: ignored —
        // it binds a fixed loopback port).
        #[cfg(target_os = "linux")]
        let socket_path = {
            let path = std::path::PathBuf::from(format!(
                "/tmp/restorekit-usbmuxd-{}.sock",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&path);
            path
        };
        #[cfg(target_os = "linux")]
        let start_arg = socket_path.to_string_lossy().into_owned();
        #[cfg(target_os = "windows")]
        let start_arg = String::from("127.0.0.1:27015");

        let path_c = CString::new(start_arg.as_bytes())
            .map_err(|_| Error::UsbmuxdFailed("socket path contains a NUL byte".into()))?;
        restorekit_sys::usbmuxd_start(&path_c).map_err(|rc| {
            Error::UsbmuxdFailed(format!("restorekit_usbmuxd_start returned {rc}"))
        })?;

        let thread = thread::Builder::new()
            .name("usbmuxd".into())
            .spawn(restorekit_sys::usbmuxd_run)
            .map_err(|e| Error::UsbmuxdFailed(format!("failed to spawn usbmuxd thread: {e}")))?;

        // Point libusbmuxd (linked into idevicerestore) at our server. On Windows
        // its default is already TCP 127.0.0.1:27015, so nothing to set.
        #[cfg(target_os = "linux")]
        unsafe {
            std::env::set_var(
                "USBMUXD_SOCKET_ADDRESS",
                format!("UNIX:{}", socket_path.display()),
            );
        }

        // Give the listener a moment to come up.
        #[cfg(target_os = "linux")]
        {
            for _ in 0..50 {
                if socket_path.exists() {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
        #[cfg(target_os = "windows")]
        std::thread::sleep(std::time::Duration::from_millis(200));

        Ok(Self {
            #[cfg(target_os = "linux")]
            socket_path,
            thread: Some(thread),
        })
    }
}

impl Drop for UsbmuxdGuard {
    fn drop(&mut self) {
        // Signal the event loop to exit, then wait for the thread.
        restorekit_sys::usbmuxd_stop();
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
        restorekit_sys::usbmuxd_cleanup();

        #[cfg(target_os = "linux")]
        {
            let _ = std::fs::remove_file(&self.socket_path);
            unsafe {
                std::env::remove_var("USBMUXD_SOCKET_ADDRESS");
            }
        }
    }
}
