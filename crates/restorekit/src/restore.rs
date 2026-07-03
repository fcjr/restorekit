use std::ffi::{c_void, CString};
use std::path::Path;
use std::sync::Mutex;

use restorekit_sys as sys;

use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

/// How to restore the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Full restore, erasing all data (`FLAG_ERASE`).
    Erase,
    /// Update-style restore that preserves user data ("revive").
    Revive,
}

/// Human name for an idevicerestore progress step.
fn step_name(step: i32) -> &'static str {
    match step {
        sys::RESTORE_STEP_DETECT => "detecting device",
        sys::RESTORE_STEP_PREPARE => "preparing",
        sys::RESTORE_STEP_UPLOAD_FS => "uploading filesystem",
        sys::RESTORE_STEP_VERIFY_FS => "verifying filesystem",
        sys::RESTORE_STEP_FLASH_FW => "flashing firmware",
        sys::RESTORE_STEP_FLASH_BB => "flashing baseband",
        sys::RESTORE_STEP_FUD => "flashing firmware updater",
        sys::RESTORE_STEP_UPLOAD_IMG => "uploading image",
        _ => "restoring",
    }
}

static RESTORE_LOCK: Mutex<()> = Mutex::new(());

/// idevicerestore's restore path is stack-hungry (big local buffers, deep call
/// chains). That fits comfortably in the 8 MB main-thread stack on macOS/Linux
/// but overflows Windows' ~1 MB default, so we always run it on a thread with a
/// generous stack — and the same on every platform for consistency.
const RESTORE_STACK: usize = 64 * 1024 * 1024;

/// Bridges the C progress callback to a channel back to the caller's thread.
/// libidevicerestore runs on the restore worker thread; the caller's `progress`
/// closure (which may not be `Send`) stays on the calling thread.
unsafe extern "C" fn progress_trampoline(step: i32, step_progress: f64, userdata: *mut c_void) {
    if userdata.is_null() {
        return;
    }
    let tx = &*(userdata as *const std::sync::mpsc::Sender<Event>);
    let _ = tx.send(Event::RestoreStep {
        step: step.max(0) as u32,
        name: step_name(step).to_string(),
        progress: step_progress as f32,
    });
}

/// Run a restore against the DFU device with the given ECID, streaming progress.
///
/// This links libidevicerestore statically; there is no external binary.
pub fn restore(
    ipsw: &Path,
    ecid: u64,
    cache_dir: Option<&Path>,
    mode: Mode,
    verbose: bool,
    progress: ProgressFn,
) -> Result<()> {
    let _guard = RESTORE_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // On Linux and Windows, start the embedded usbmuxd server so idevicerestore
    // can reach USB devices (restore mode) without an external daemon.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let _usbmuxd = crate::usbmuxd::UsbmuxdGuard::start(progress)?;

    // Route idevicerestore's logging through our capture sink (rather than its
    // default stdout dump) so it doesn't stomp on the progress UI and so we can
    // surface the real error text on failure. `verbose` also echoes to stderr.
    sys::install_log_capture(verbose);
    sys::set_log_level(if verbose {
        sys::LL_DEBUG
    } else {
        sys::LL_WARNING
    });

    // Initialize idevicerestore's global progress mutex up front — some of its
    // progress functions lock it without a lazy-init guard, which crashes on
    // Windows (a CRITICAL_SECTION) if hit before the first `set_progress`.
    sys::init_progress();

    // Owned copies to hand to the worker thread.
    let ipsw = ipsw.to_path_buf();
    let cache_dir = cache_dir.map(Path::to_path_buf);
    let (tx, rx) = std::sync::mpsc::channel::<Event>();

    let worker = std::thread::Builder::new()
        .name("restore".into())
        .stack_size(RESTORE_STACK)
        .spawn(move || -> Result<()> {
            let ipsw_c = CString::new(ipsw.as_os_str().to_string_lossy().as_bytes())
                .map_err(|_| Error::Download("ipsw path contains a NUL byte".into()))?;
            let cache_c = cache_dir
                .map(|d| CString::new(d.as_os_str().to_string_lossy().as_bytes()))
                .transpose()
                .map_err(|_| Error::Download("cache path contains a NUL byte".into()))?;

            unsafe {
                let client = sys::idevicerestore_client_new();
                if client.is_null() {
                    return Err(Error::RestoreFailed {
                        status: -1,
                        log_tail: "idevicerestore_client_new returned null".into(),
                    });
                }
                // Ensure the client is always freed, even on early return.
                #[allow(clippy::redundant_closure_call)]
                let result = (|| {
                    sys::idevicerestore_set_ecid(client, ecid);
                    let flags = match mode {
                        Mode::Erase => sys::FLAG_ERASE,
                        Mode::Revive => 0,
                    };
                    sys::idevicerestore_set_flags(client, flags);
                    sys::idevicerestore_set_ipsw(client, ipsw_c.as_ptr());
                    if let Some(cache) = &cache_c {
                        sys::idevicerestore_set_cache_path(client, cache.as_ptr());
                    }
                    sys::idevicerestore_set_progress_callback(
                        client,
                        Some(progress_trampoline),
                        &tx as *const std::sync::mpsc::Sender<Event> as *mut c_void,
                    );

                    let rc = sys::idevicerestore_start(client);
                    if rc == 0 {
                        Ok(())
                    } else {
                        let tail = sys::error_tail(20);
                        let log_tail = if tail.is_empty() {
                            format!("idevicerestore_start returned {rc}")
                        } else {
                            tail
                        };
                        Err(Error::RestoreFailed {
                            status: rc,
                            log_tail,
                        })
                    }
                })();

                sys::idevicerestore_client_free(client);
                result
            }
        })
        .map_err(|e| Error::RestoreFailed {
            status: -1,
            log_tail: format!("failed to spawn restore thread: {e}"),
        })?;

    // Pump progress events on the calling thread until the worker drops its
    // sender (i.e. the restore has finished).
    for event in rx {
        progress(event);
    }

    worker.join().unwrap_or_else(|_| {
        Err(Error::RestoreFailed {
            status: -1,
            log_tail: "restore thread panicked".into(),
        })
    })?;

    progress(Event::Done);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn step_names_have_fallback() {
        assert_eq!(
            step_name(sys::RESTORE_STEP_UPLOAD_FS),
            "uploading filesystem"
        );
        assert_eq!(step_name(999), "restoring");
    }
}
