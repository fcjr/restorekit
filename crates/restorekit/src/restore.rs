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

/// Total attempts for a restore that fails on a transient transport error (a
/// dropped USB write/read mid-transfer). Non-transport failures never retry.
const RESTORE_ATTEMPTS: u32 = 3;

/// Pause between attempts, giving the target time to settle back into a
/// re-detectable state (recovery/DFU) after a failed restore.
const RESTORE_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(5);

/// Whether a failed restore is worth retrying: the log tail indicates the
/// device connection dropped mid-transfer rather than a real restore error
/// (bad firmware, TSS rejection, ...).
fn is_transient_failure(err: &Error) -> bool {
    const MARKERS: &[&str] = &[
        // libimobiledevice property-list-service short write (e.g. -256).
        "Failed to send data",
        // restored read side dropped.
        "Could not read data",
        "Unable to receive message from FDR",
        "usb_bulk_transfer",
        // The target dropped off USB during the DFU→recovery reboot while the
        // stage-1 (iBEC/RestoreDCP) components were going over — almost always
        // recovers on another attempt once the device settles.
        "Unable to place device into recovery mode from DFU mode",
        "Unable to send iBoot stage 1 components",
    ];
    match err {
        Error::RestoreFailed { log_tail, .. } => MARKERS.iter().any(|m| log_tail.contains(m)),
        _ => false,
    }
}

/// Run a restore against the DFU device with the given ECID, streaming progress.
///
/// This links libidevicerestore statically; there is no external binary.
/// Transient transport failures (dropped USB writes) are retried a bounded
/// number of times, re-running the restore from device detection.
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
    // can reach USB devices (restore mode) without an external daemon — unless a
    // parent process already started a shared one (parallel restores), which we
    // then reuse via the inherited USBMUXD_SOCKET_ADDRESS.
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let _usbmuxd = if std::env::var_os("RESTOREKIT_SHARED_USBMUXD").is_some() {
        None
    } else {
        Some(crate::usbmuxd::UsbmuxdGuard::start(progress)?)
    };

    // On Windows the Mac's restore-mode interface is claimed by Apple's driver,
    // which libusb can't open; spawn an elevated watcher (one UAC) that forces
    // our WinUSB onto it when it appears. Held for the duration of the restore.
    #[cfg(target_os = "windows")]
    let _restore_watcher = crate::driver::spawn_restore_mode_watcher();

    let mut attempt = 1;
    loop {
        match restore_attempt(ipsw, ecid, cache_dir, mode, verbose, progress) {
            Ok(()) => {
                progress(Event::Done);
                return Ok(());
            }
            Err(e) if attempt < RESTORE_ATTEMPTS && is_transient_failure(&e) => {
                progress(Event::RestoreRetrying {
                    attempt,
                    max_attempts: RESTORE_ATTEMPTS,
                    message: e.to_string(),
                });
                std::thread::sleep(RESTORE_RETRY_DELAY);
                attempt += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// One full pass of the restore: spawn the worker, pump progress, join.
fn restore_attempt(
    ipsw: &Path,
    ecid: u64,
    cache_dir: Option<&Path>,
    mode: Mode,
    verbose: bool,
    progress: ProgressFn,
) -> Result<()> {
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

    // idevicerestore's tagged progress bars (the "Uploading [====]" bars from
    // dfu.c/recovery.c) print straight to stdout unless we override their sink.
    // That raw output corrupts --json and interleaves with our progress UI, so
    // discard them; step-level progress still comes through the step callback.
    sys::suppress_tagged_progress();

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

                    // Stream each log line as an event for a live log window. The
                    // sink is cleared before the worker's own sender drops, so the
                    // caller's `rx` loop still terminates.
                    let tx_log = tx.clone();
                    sys::set_log_sink(Some(Box::new(move |level, line| {
                        let _ = tx_log.send(Event::LogLine {
                            level,
                            line: line.to_string(),
                        });
                    })));
                    let rc = sys::idevicerestore_start(client);
                    sys::set_log_sink(None);
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
    })
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

    #[test]
    fn transient_failures_are_classified() {
        let transport = Error::RestoreFailed {
            status: -11,
            log_tail: "_restore_send_file_data: Failed to send data (-256)\n\
                       ipsw_extract_send: send failed"
                .into(),
        };
        assert!(is_transient_failure(&transport));

        let real = Error::RestoreFailed {
            status: -1,
            log_tail: "unable to get SHSH blobs for this device".into(),
        };
        assert!(!is_transient_failure(&real));

        assert!(!is_transient_failure(&Error::WaitTimeout));
    }
}
