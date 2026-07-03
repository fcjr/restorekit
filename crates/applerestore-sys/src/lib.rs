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
}
