//! Embedded usbmuxd server for Linux.
//!
//! Runs the usbmuxd event loop on a background thread with a private Unix
//! socket, so `idevicerestore` (via libusbmuxd) can talk to USB devices without
//! requiring an external usbmuxd daemon.

use std::ffi::CString;
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

/// RAII guard that starts the embedded usbmuxd server and tears it down on drop.
pub struct UsbmuxdGuard {
    socket_path: PathBuf,
    thread: Option<JoinHandle<()>>,
}

impl UsbmuxdGuard {
    /// Start the embedded usbmuxd server.
    ///
    /// 1. Creates a private Unix socket at `/tmp/restorekit-usbmuxd-<pid>.sock`
    /// 2. Initializes USB device tracking on the current thread (fast)
    /// 3. Spawns a background thread running the ppoll event loop
    /// 4. Sets `USBMUXD_SOCKET_ADDRESS` so libusbmuxd connects to our socket
    pub fn start(progress: ProgressFn) -> Result<Self> {
        let pid = std::process::id();
        let socket_path = PathBuf::from(format!("/tmp/restorekit-usbmuxd-{pid}.sock"));

        // Remove any stale socket from a previous crash.
        let _ = std::fs::remove_file(&socket_path);

        progress(Event::UsbmuxdStarting);

        let path_c = CString::new(socket_path.as_os_str().to_string_lossy().as_bytes())
            .map_err(|_| Error::UsbmuxdFailed("socket path contains a NUL byte".into()))?;

        restorekit_sys::usbmuxd_start(&path_c).map_err(|rc| {
            Error::UsbmuxdFailed(format!("restorekit_usbmuxd_start returned {rc}"))
        })?;

        let thread = thread::Builder::new()
            .name("usbmuxd".into())
            .spawn(|| {
                restorekit_sys::usbmuxd_run();
            })
            .map_err(|e| Error::UsbmuxdFailed(format!("failed to spawn usbmuxd thread: {e}")))?;

        // Tell libusbmuxd (the *client* library already linked into idevicerestore)
        // to connect to our private socket instead of the system one.
        unsafe {
            std::env::set_var(
                "USBMUXD_SOCKET_ADDRESS",
                format!("UNIX:{}", socket_path.display()),
            );
        }

        // Wait briefly for the socket to become connectable.
        for _ in 0..50 {
            if socket_path.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        Ok(Self {
            socket_path,
            thread: Some(thread),
        })
    }
}

impl Drop for UsbmuxdGuard {
    fn drop(&mut self) {
        // Signal the event loop to exit.
        restorekit_sys::usbmuxd_stop();

        // Wait for the thread to finish.
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }

        // Clean up USB state.
        restorekit_sys::usbmuxd_cleanup();

        // Remove the socket file (cleanup may have already done this).
        let _ = std::fs::remove_file(&self.socket_path);

        // Unset the env var so subsequent operations don't try the stale path.
        unsafe {
            std::env::remove_var("USBMUXD_SOCKET_ADDRESS");
        }
    }
}
