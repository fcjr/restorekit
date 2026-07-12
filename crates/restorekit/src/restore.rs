use std::ffi::{c_void, CString};
use std::path::Path;
use std::sync::{Arc, Mutex};

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
    /// Destroy the encryption (media) key and stop — the effaceable wipe runs
    /// but the OS is not reinstalled, leaving the Mac wiped and OS-less. Fast
    /// key destruction for decommissioning; requires a later restore to be
    /// usable again (`FLAG_ERASE | FLAG_OBLITERATE_ONLY`).
    Obliterate,
}

/// Outcome of the encryption-key obliteration check for an erase restore.
///
/// On Apple Silicon and T2 the media key that encrypts all user data lives in
/// effaceable storage managed by the SEP; an erase restore reformats that
/// region, destroying the key and cryptographically shredding the old data. The
/// key itself is never readable, so the strongest signal available is the
/// device's own report in the restore log — we scan for it and classify here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Obliteration {
    /// Revive restore — no erase requested, so nothing is obliterated.
    NotApplicable,
    /// The device reported the effaceable storage was formatted (media key
    /// destroyed). `evidence` is the log line that confirmed it.
    Confirmed { evidence: String },
    /// The device reported the effaceable format failed. `evidence` is the log
    /// line. The old key may still be present — treat the wipe as unproven.
    Failed { evidence: String },
    /// The erase restore itself succeeded but no obliteration signal appeared in
    /// the device log. The signal may simply not cross to the host on this
    /// model/OS; re-run with `--verbose` to inspect the full device log.
    Unconfirmed,
}

impl Obliteration {
    /// Machine-readable status tag (matches [`Event::Obliteration`]).
    pub fn status(&self) -> &'static str {
        match self {
            Obliteration::NotApplicable => "not_applicable",
            Obliteration::Confirmed { .. } => "confirmed",
            Obliteration::Failed { .. } => "failed",
            Obliteration::Unconfirmed => "unconfirmed",
        }
    }

    /// The device log line that produced this verdict, if any.
    pub fn evidence(&self) -> Option<&str> {
        match self {
            Obliteration::Confirmed { evidence } | Obliteration::Failed { evidence } => {
                Some(evidence)
            }
            _ => None,
        }
    }
}

/// Accumulates obliteration signals seen in the restore log stream. Failure
/// dominates success; the first line of each kind is kept as evidence.
#[derive(Default)]
struct ObliterationScan {
    confirmed: Option<String>,
    failed: Option<String>,
}

impl ObliterationScan {
    fn observe(&mut self, line: &str) {
        let l = line.to_ascii_lowercase();
        // Primary signal, verified on a real M1 (MacBookPro17,1) erase restore:
        // the device forwards its effaceable-media-key wipe to the host as a
        // restore checkpoint, e.g.
        //   Checkpoint completed id: 0x61F (format_effaceable_storage) result=0
        // `result=0` is success; any non-zero code is a failed wipe. The
        // "started" line has no `result=` and is ignored. This crosses the USB
        // link; `restored_update`'s on-device syslog string does not.
        if l.contains("format_effaceable_storage") {
            if let Some(code) = l
                .split("result=")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
            {
                if code == "0" {
                    self.confirmed.get_or_insert_with(|| line.to_string());
                } else {
                    self.failed.get_or_insert_with(|| line.to_string());
                }
            }
            return;
        }
        // Fallback: `restored_update`'s textual report, in case some model/OS
        // surfaces it to the host instead of (or before) the checkpoint.
        const FAIL: &[&str] = &[
            "failed to format effaceable storage",
            "error formatting effaceable storage",
        ];
        const OK: &[&str] = &["effaceable storage formatted successfully"];
        if self.failed.is_none() && FAIL.iter().any(|m| l.contains(m)) {
            self.failed = Some(line.to_string());
        } else if self.confirmed.is_none() && OK.iter().any(|m| l.contains(m)) {
            self.confirmed = Some(line.to_string());
        }
    }

    fn result(&self) -> Obliteration {
        if let Some(e) = &self.failed {
            Obliteration::Failed {
                evidence: e.clone(),
            }
        } else if let Some(e) = &self.confirmed {
            Obliteration::Confirmed {
                evidence: e.clone(),
            }
        } else {
            Obliteration::Unconfirmed
        }
    }
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
) -> Result<Obliteration> {
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
            Ok(obliteration) => {
                // The worker already emitted the Obliteration event (it fires on
                // failure too); just close out the successful run.
                progress(Event::Done);
                return Ok(obliteration);
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
) -> Result<Obliteration> {
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
        .spawn(move || -> Result<Obliteration> {
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
                        Mode::Obliterate => sys::FLAG_ERASE | sys::FLAG_OBLITERATE_ONLY,
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

                    // Stream each log line as an event for a live log window, and
                    // scan it for the device's effaceable-obliteration signals. The
                    // sink is cleared before the worker's own sender drops, so the
                    // caller's `rx` loop still terminates.
                    let tx_log = tx.clone();
                    let scan = Arc::new(Mutex::new(ObliterationScan::default()));
                    let scan_sink = Arc::clone(&scan);
                    sys::set_log_sink(Some(Box::new(move |level, line| {
                        scan_sink
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .observe(line);
                        let _ = tx_log.send(Event::LogLine {
                            level,
                            line: line.to_string(),
                        });
                    })));
                    let rc = sys::idevicerestore_start(client);
                    sys::set_log_sink(None);
                    let obliteration = match mode {
                        Mode::Revive => Obliteration::NotApplicable,
                        Mode::Erase | Mode::Obliterate => {
                            scan.lock().unwrap_or_else(|e| e.into_inner()).result()
                        }
                    };
                    // Report the wipe verdict for BOTH outcomes: an erase can
                    // destroy the media key early and still fail later (e.g. the
                    // OS-image upload drops), and a refurb audit must capture
                    // that the key is gone regardless. Skip the noise event for a
                    // revive, which never obliterates.
                    if !matches!(obliteration, Obliteration::NotApplicable) {
                        let _ = tx.send(Event::Obliteration {
                            status: obliteration.status().to_string(),
                            detail: obliteration.evidence().unwrap_or_default().to_string(),
                        });
                    }
                    if rc == 0 {
                        Ok(obliteration)
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
    fn obliteration_scan_classifies_signals() {
        // The real host-visible signal (verified on an M1 erase restore): a
        // restore checkpoint with a result code. The "started" line is not a
        // verdict; only the "completed … result=" line is.
        let mut s = ObliterationScan::default();
        s.observe("Checkpoint started   id: 0x61F (format_effaceable_storage)");
        assert_eq!(s.result(), Obliteration::Unconfirmed);
        s.observe("Checkpoint completed id: 0x61F (format_effaceable_storage) result=0");
        assert!(matches!(s.result(), Obliteration::Confirmed { .. }));

        // A non-zero checkpoint result is a failed wipe.
        let mut s = ObliterationScan::default();
        s.observe("Checkpoint completed id: 0x61F (format_effaceable_storage) result=5");
        assert!(matches!(s.result(), Obliteration::Failed { .. }));

        // Unrelated lines and the ambiguous "nothing to do" leave it unconfirmed.
        let mut s = ObliterationScan::default();
        s.observe("preparing NAND");
        s.observe("restored_update: effaceable storage is formatted, nothing to do");
        assert_eq!(s.result(), Obliteration::Unconfirmed);

        // Textual fallback still works; failure dominates a prior success.
        let mut s = ObliterationScan::default();
        s.observe("effaceable storage formatted successfully");
        assert!(matches!(s.result(), Obliteration::Confirmed { .. }));
        s.observe("failed to format effaceable storage");
        assert!(matches!(s.result(), Obliteration::Failed { .. }));
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
