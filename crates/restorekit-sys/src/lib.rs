//! Raw FFI to the statically-linked idevicerestore client API
//! (see `vendor/idevicerestore/src/idevicerestore.h`).
#![allow(non_camel_case_types)]

// Force the vendored static C deps into the final link. The idevicerestore
// stack references their symbols (OpenSSL, zlib, libcurl) but our Rust code
// doesn't, so we must keep the crates from being pruned.
extern crate curl_sys as _;
extern crate libz_sys as _;
extern crate openssl_sys as _;

use std::os::raw::{c_char, c_int, c_void};

// Restore flags from idevicerestore.h.
pub const FLAG_DEBUG: c_int = 1 << 1;
pub const FLAG_ERASE: c_int = 1 << 2;

// idevicerestore log levels (enum loglevel in log.h).
pub const LL_ERROR: c_int = 0;
pub const LL_WARNING: c_int = 1;
pub const LL_NOTICE: c_int = 2;
pub const LL_INFO: c_int = 3;
pub const LL_VERBOSE: c_int = 4;
pub const LL_DEBUG: c_int = 5;

extern "C" {
    /// idevicerestore's global logging threshold; messages above it are dropped.
    static mut log_level: c_int;
}

/// Set idevicerestore's global log verbosity. Messages more verbose than
/// `level` are never emitted (by default it dumps everything to stdout).
pub fn set_log_level(level: c_int) {
    unsafe {
        log_level = level;
    }
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

extern "C" {
    /// Installs our log trampoline (see csrc/log_capture.c).
    fn restorekit_install_log_capture();
}

static LOG_LINES: Mutex<Vec<(c_int, String)>> = Mutex::new(Vec::new());
static LOG_ECHO: AtomicBool = AtomicBool::new(false);
const LOG_CAPACITY: usize = 512;

/// Called from C (log_capture.c) for each idevicerestore log line.
///
/// # Safety
/// `msg`, if non-null, must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn restorekit_log_capture(level: c_int, msg: *const c_char) {
    if msg.is_null() {
        return;
    }
    let text = std::ffi::CStr::from_ptr(msg).to_string_lossy();
    let line = text.trim_end_matches(['\n', '\r']).to_string();
    if line.is_empty() {
        return;
    }
    if LOG_ECHO.load(Ordering::Relaxed) {
        eprintln!("{line}");
    }
    if let Ok(mut buf) = LOG_LINES.lock() {
        if buf.len() >= LOG_CAPACITY {
            buf.remove(0);
        }
        buf.push((level, line));
    }
}

/// Route idevicerestore's logging into the capture sink. When `echo` is set,
/// lines are also printed to stderr (for verbose mode). Clears prior lines.
pub fn install_log_capture(echo: bool) {
    LOG_ECHO.store(echo, Ordering::Relaxed);
    if let Ok(mut buf) = LOG_LINES.lock() {
        buf.clear();
    }
    unsafe { restorekit_install_log_capture() };
}

/// The last `max_lines` captured error/warning lines, newest-relevant last.
pub fn error_tail(max_lines: usize) -> String {
    let buf = match LOG_LINES.lock() {
        Ok(b) => b,
        Err(e) => e.into_inner(),
    };
    let errors: Vec<&str> = buf
        .iter()
        .filter(|(level, _)| *level <= LL_WARNING)
        .map(|(_, line)| line.as_str())
        .collect();
    let start = errors.len().saturating_sub(max_lines);
    errors[start..].join("\n")
}

// Progress step numbers (enum in idevicerestore.h).
pub const RESTORE_STEP_DETECT: c_int = 0;
pub const RESTORE_STEP_PREPARE: c_int = 1;
pub const RESTORE_STEP_UPLOAD_FS: c_int = 2;
pub const RESTORE_STEP_VERIFY_FS: c_int = 3;
pub const RESTORE_STEP_FLASH_FW: c_int = 4;
pub const RESTORE_STEP_FLASH_BB: c_int = 5;
pub const RESTORE_STEP_FUD: c_int = 6;
pub const RESTORE_STEP_UPLOAD_IMG: c_int = 7;

/// Opaque idevicerestore client handle.
#[repr(C)]
pub struct idevicerestore_client_t {
    _private: [u8; 0],
}

pub type idevicerestore_progress_cb_t =
    Option<unsafe extern "C" fn(step: c_int, step_progress: f64, userdata: *mut c_void)>;

extern "C" {
    pub fn idevicerestore_client_new() -> *mut idevicerestore_client_t;
    pub fn idevicerestore_client_free(client: *mut idevicerestore_client_t);
    pub fn idevicerestore_set_ecid(client: *mut idevicerestore_client_t, ecid: u64);
    pub fn idevicerestore_set_udid(client: *mut idevicerestore_client_t, udid: *const c_char);
    pub fn idevicerestore_set_flags(client: *mut idevicerestore_client_t, flags: c_int);
    pub fn idevicerestore_set_ipsw(client: *mut idevicerestore_client_t, path: *const c_char);
    pub fn idevicerestore_set_cache_path(client: *mut idevicerestore_client_t, path: *const c_char);
    pub fn idevicerestore_set_progress_callback(
        client: *mut idevicerestore_client_t,
        cbfunc: idevicerestore_progress_cb_t,
        userdata: *mut c_void,
    );
    pub fn idevicerestore_start(client: *mut idevicerestore_client_t) -> c_int;
    /// Register a progress bar (or, with a NULL label, just run the one-time
    /// init of idevicerestore's global progress mutex).
    pub fn register_progress(tag: u32, label: *const c_char);
}

/// Initialize idevicerestore's progress subsystem before a restore.
///
/// Several of its progress functions (e.g. `finalize_progress`, called from
/// `dfu_client_free`) lock a global mutex without the lazy-init guard that
/// `set_progress`/`register_progress` run. On Windows that mutex is a
/// `CRITICAL_SECTION`, so locking it uninitialized is an access violation.
/// `register_progress` with a NULL label runs the init and returns without
/// adding an entry.
pub fn init_progress() {
    unsafe { register_progress(0, std::ptr::null()) };
}

// ── Embedded usbmuxd server (Linux + Windows) ────────────────────────────────

#[cfg(any(target_os = "linux", target_os = "windows"))]
extern "C" {
    fn restorekit_usbmuxd_start(socket_path: *const c_char) -> c_int;
    fn restorekit_usbmuxd_run();
    fn restorekit_usbmuxd_stop();
    fn restorekit_usbmuxd_cleanup();
}

/// Initialize the embedded usbmuxd server, binding a Unix socket at `path`.
/// Returns `Ok(())` on success.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn usbmuxd_start(path: &std::ffi::CStr) -> std::result::Result<(), c_int> {
    let rc = unsafe { restorekit_usbmuxd_start(path.as_ptr()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(rc)
    }
}

/// Run the usbmuxd event loop (blocks until [`usbmuxd_stop`] is called).
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn usbmuxd_run() {
    unsafe { restorekit_usbmuxd_run() }
}

/// Signal the event loop to exit.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn usbmuxd_stop() {
    unsafe { restorekit_usbmuxd_stop() }
}

/// Tear down USB devices, close the listen socket, and unlink the socket file.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn usbmuxd_cleanup() {
    unsafe { restorekit_usbmuxd_cleanup() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn error_tail_filters_by_level() {
        if let Ok(mut b) = LOG_LINES.lock() {
            b.clear();
        }
        LOG_ECHO.store(false, Ordering::Relaxed);

        let cap = |level, s: &str| {
            let c = CString::new(s).unwrap();
            unsafe { restorekit_log_capture(level, c.as_ptr()) };
        };
        cap(LL_INFO, "info line");
        cap(LL_ERROR, "boom -3");
        cap(LL_VERBOSE, "verbose noise");
        cap(LL_WARNING, "careful");

        let tail = error_tail(10);
        assert!(tail.contains("boom -3"));
        assert!(tail.contains("careful"));
        assert!(!tail.contains("info line"));
        assert!(!tail.contains("verbose noise"));
    }
}
