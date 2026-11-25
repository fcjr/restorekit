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
/// Stop the restore right after the effaceable media-key wipe, before the OS is
/// written. Combine with `FLAG_ERASE`. Added by patch 0003 (see patches/).
pub const FLAG_OBLITERATE_ONLY: c_int = 1 << 14;

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
// Large enough that capturing at VERBOSE (see restore.rs) doesn't evict the
// trailing error/warning lines that error_tail() reports on failure.
const LOG_CAPACITY: usize = 4096;

/// Optional sink that receives every captured log line as it arrives, for
/// live-streaming the restore log (see [`set_log_sink`]).
type LogSink = Box<dyn Fn(c_int, &str) + Send>;
static LOG_SINK: Mutex<Option<LogSink>> = Mutex::new(None);

/// Install (or clear, with `None`) a callback invoked for each idevicerestore
/// log line as it is captured. Used to stream the restore log to a frontend.
pub fn set_log_sink(sink: Option<LogSink>) {
    *LOG_SINK.lock().unwrap_or_else(|e| e.into_inner()) = sink;
}

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
    if let Ok(guard) = LOG_SINK.lock() {
        if let Some(sink) = guard.as_ref() {
            sink(level, &line);
        }
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
    /// libtatsu's TSS logging. At 1 it prints why a TSS request failed (Apple's
    /// status code and message) to stderr; at 0 (the default) it prints nothing.
    pub fn tss_set_debug_level(level: c_int);
    /// Register a progress bar (or, with a NULL label, just run the one-time
    /// init of idevicerestore's global progress mutex).
    pub fn register_progress(tag: u32, label: *const c_char);
    /// Override the sink for idevicerestore's tagged progress bars. With a NULL
    /// handler it prints them straight to stdout via `print_progress_bar`.
    fn set_update_progress_func(
        func: Option<unsafe extern "C" fn(list: *mut *mut c_void, count: c_int)>,
    );
}

/// No-op sink for the tagged progress-bar system: swallow the update.
unsafe extern "C" fn discard_progress(_list: *mut *mut c_void, _count: c_int) {}

/// Stop idevicerestore's tagged progress bars (the `dfu.c`/`recovery.c`
/// "Uploading" bars) from being dumped straight to stdout, where they corrupt
/// `--json` output and interleave with our own progress UI. Step-level progress
/// still flows through the progress callback. Call before starting a restore.
pub fn suppress_tagged_progress() {
    unsafe { set_update_progress_func(Some(discard_progress)) };
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

// ── libimobiledevice: lockdownd on a running device ──────────────────────────

/// Opaque libimobiledevice device.
#[repr(C)]
pub struct idevice_private {
    _private: [u8; 0],
}
pub type idevice_t = *mut idevice_private;

/// Opaque lockdownd client.
#[repr(C)]
pub struct lockdownd_client_private {
    _private: [u8; 0],
}
pub type lockdownd_client_t = *mut lockdownd_client_private;

/// Opaque pair record passed to lockdownd's pairing calls.
#[repr(C)]
pub struct lockdownd_pair_record {
    _private: [u8; 0],
}
pub type lockdownd_pair_record_t = *mut lockdownd_pair_record;

/// A libplist node.
pub type plist_t = *mut c_void;

// idevice_error_t (libimobiledevice.h) and lockdownd_error_t (lockdown.h).
pub const IDEVICE_E_SUCCESS: c_int = 0;
pub const LOCKDOWN_E_SUCCESS: c_int = 0;
pub const LOCKDOWN_E_PASSWORD_PROTECTED: c_int = -17;
pub const LOCKDOWN_E_USER_DENIED_PAIRING: c_int = -18;
pub const LOCKDOWN_E_PAIRING_DIALOG_RESPONSE_PENDING: c_int = -19;
pub const LOCKDOWN_E_INVALID_HOST_ID: c_int = -21;
pub const LOCKDOWN_E_SESSION_INACTIVE: c_int = -23;
pub const LOCKDOWN_E_MC_CHALLENGE_REQUIRED: c_int = -39;

// libplist's `plist_type`.
pub const PLIST_INT: c_int = 1;
pub const PLIST_DICT: c_int = 5;

extern "C" {
    /// The UDIDs of the USB devices usbmuxd lists. Fails only when usbmuxd
    /// can't be reached.
    pub fn idevice_get_device_list(devices: *mut *mut *mut c_char, count: *mut c_int) -> c_int;
    pub fn idevice_device_list_free(devices: *mut *mut c_char) -> c_int;
    pub fn idevice_new(device: *mut idevice_t, udid: *const c_char) -> c_int;
    pub fn idevice_free(device: idevice_t) -> c_int;
    /// Connect to lockdownd without pairing.
    pub fn lockdownd_client_new(
        device: idevice_t,
        client: *mut lockdownd_client_t,
        label: *const c_char,
    ) -> c_int;
    /// Connect to lockdownd in a paired session. Pairs first when the host has
    /// no pair record for the device, which asks the device's user to trust
    /// this computer.
    pub fn lockdownd_client_new_with_handshake(
        device: idevice_t,
        client: *mut lockdownd_client_t,
        label: *const c_char,
    ) -> c_int;
    pub fn lockdownd_client_free(client: lockdownd_client_t) -> c_int;
    /// The service's type; free it with [`plist_mem_free`].
    pub fn lockdownd_query_type(client: lockdownd_client_t, kind: *mut *mut c_char) -> c_int;
    pub fn lockdownd_get_value(
        client: lockdownd_client_t,
        domain: *const c_char,
        key: *const c_char,
        value: *mut plist_t,
    ) -> c_int;
    pub fn lockdownd_enter_recovery(client: lockdownd_client_t) -> c_int;
    /// Send a pair request. With a null `pair_record` it generates a new one,
    /// and on success saves it as this host's pair record for the device.
    pub fn lockdownd_pair(
        client: lockdownd_client_t,
        pair_record: lockdownd_pair_record_t,
    ) -> c_int;
    /// [`lockdownd_pair`] with these pairing options in place of its own,
    /// returning the device's response.
    pub fn lockdownd_pair_with_options(
        client: lockdownd_client_t,
        pair_record: lockdownd_pair_record_t,
        options: plist_t,
        response: *mut plist_t,
    ) -> c_int;
    pub fn lockdownd_strerror(err: c_int) -> *const c_char;
    /// Read the host's pair record for a device by UDID through usbmuxd;
    /// returns 0 when there is one, which the caller frees.
    pub fn usbmuxd_read_pair_record(
        record_id: *const c_char,
        record_data: *mut *mut c_char,
        record_size: *mut u32,
    ) -> c_int;
    /// Point the calling thread's usbmuxd connections at `address`, in
    /// `USBMUXD_SOCKET_ADDRESS`'s forms, or the platform's default usbmuxd when
    /// null, ahead of that variable. `enable` 0 goes back to the variable. Our
    /// patch to libusbmuxd (patches/libusbmuxd).
    pub fn libusbmuxd_set_thread_socket_address(enable: c_int, address: *const c_char);
    pub fn plist_new_dict() -> plist_t;
    pub fn plist_new_bool(val: u8) -> plist_t;
    pub fn plist_new_data(val: *const c_char, length: u64) -> plist_t;
    /// Takes ownership of `item`.
    pub fn plist_dict_set_item(node: plist_t, key: *const c_char, item: plist_t) -> c_int;
    /// A borrowed child of `node`, or null.
    pub fn plist_dict_get_item(node: plist_t, key: *const c_char) -> plist_t;
    pub fn plist_get_node_type(node: plist_t) -> c_int;
    pub fn plist_get_uint_val(node: plist_t, val: *mut u64);
    /// A pointer into `node`'s own bytes.
    pub fn plist_get_data_ptr(node: plist_t, length: *mut u64) -> *const c_char;
    /// A pointer into `node`'s own string, or null when it isn't one.
    pub fn plist_get_string_ptr(node: plist_t, length: *mut u64) -> *const c_char;
    pub fn plist_free(plist: plist_t);
    pub fn plist_mem_free(ptr: *mut c_void);
}

// ── Embedded usbmuxd server (Linux + Windows) ────────────────────────────────

#[cfg(any(target_os = "linux", target_os = "windows"))]
extern "C" {
    fn restorekit_usbmuxd_start(socket_path: *const c_char) -> c_int;
    fn restorekit_usbmuxd_run();
    fn restorekit_usbmuxd_stop();
    fn restorekit_usbmuxd_cleanup();
    fn restorekit_usbmuxd_set_switch_running(on: c_int);
    fn restorekit_usbmuxd_set_config_dir(path: *const c_char) -> c_int;
}

/// Let the embedded server switch running iPhones and iPads to the USB
/// configuration with their mux interface, as a standalone usbmuxd does. It
/// otherwise only takes devices whose active configuration already has one.
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn usbmuxd_set_switch_running(on: bool) {
    unsafe { restorekit_usbmuxd_set_switch_running(on.into()) }
}

/// Keep the embedded server's pair records and SystemBUID in `path` (created
/// if missing). Call before [`usbmuxd_start`].
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn usbmuxd_set_config_dir(path: &std::ffi::CStr) -> std::result::Result<(), c_int> {
    match unsafe { restorekit_usbmuxd_set_config_dir(path.as_ptr()) } {
        0 => Ok(()),
        rc => Err(rc),
    }
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
