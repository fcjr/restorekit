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

/// Bridges the C progress callback to the Rust closure. libidevicerestore is not
/// re-entrant, so restores are serialized and the active callback lives here.
struct ActiveProgress<'a> {
    callback: &'a mut dyn FnMut(Event),
}

static RESTORE_LOCK: Mutex<()> = Mutex::new(());

unsafe extern "C" fn progress_trampoline(step: i32, step_progress: f64, userdata: *mut c_void) {
    if userdata.is_null() {
        return;
    }
    let active = &mut *(userdata as *mut ActiveProgress);
    (active.callback)(Event::RestoreStep {
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

    // On Linux, start the embedded usbmuxd server so idevicerestore can
    // communicate with USB devices without an external daemon.
    #[cfg(target_os = "linux")]
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

    let ipsw_c = CString::new(ipsw.as_os_str().to_string_lossy().as_bytes())
        .map_err(|_| Error::Download("ipsw path contains a NUL byte".into()))?;
    let cache_c = cache_dir
        .map(|d| CString::new(d.as_os_str().to_string_lossy().as_bytes()))
        .transpose()
        .map_err(|_| Error::Download("cache path contains a NUL byte".into()))?;

    let mut active = ActiveProgress { callback: progress };

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
                &mut active as *mut ActiveProgress as *mut c_void,
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
    }?;

    (active.callback)(Event::Done);
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
