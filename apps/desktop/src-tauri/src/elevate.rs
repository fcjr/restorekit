//! Drive the privileged helper daemon (`com.leftshift.restorekit.helper`).
//!
//! The daemon is registered once via `SMAppService`; after the user approves it
//! in System Settings it runs as root and we reach it over XPC — no password
//! prompt. The daemon verifies our code signature before doing anything, so only
//! this signed app can command it. All the ObjC glue lives in `csrc/privhelper.m`.

use std::ffi::{c_char, CString};

/// Sentinel error returned when the helper needs the one-time approval. The
/// frontend matches this to show the approval screen.
pub const APPROVAL_REQUIRED: &str = "helper-approval-required";

extern "C" {
    fn rk_helper_status() -> i32;
    fn rk_helper_register(err: *mut c_char, err_len: usize) -> i32;
    fn rk_open_login_items_settings();
    fn rk_helper_send(command: *const c_char, err: *mut c_char, err_len: usize) -> i32;
}

/// One of "enabled" | "requiresApproval" | "notRegistered" | "notFound" |
/// "unavailable" — what the UI polls to decide whether to show the approval screen.
pub fn status() -> &'static str {
    match unsafe { rk_helper_status() } {
        0 => "notRegistered",
        1 => "enabled",
        2 => "requiresApproval",
        3 => "notFound",
        _ => "unavailable",
    }
}

/// Register the daemon (idempotent) and open the approval UI so the user can
/// enable it. Called when the app decides approval is needed.
pub fn approve() -> Result<(), String> {
    register()?;
    unsafe { rk_open_login_items_settings() };
    Ok(())
}

fn register() -> Result<(), String> {
    let mut err = [0u8; 512];
    let rc = unsafe { rk_helper_register(err.as_mut_ptr() as *mut c_char, err.len()) };
    if rc == 0 {
        Ok(())
    } else {
        Err(read(&err))
    }
}

/// Run a helper command (`dfu` / `reboot`) as root over XPC. If the daemon isn't
/// enabled yet, registers it and returns [`APPROVAL_REQUIRED`] so the UI can
/// walk the user through the one-time approval.
pub fn run_helper(command: &str) -> Result<(), String> {
    match status() {
        "enabled" => {}
        "notRegistered" => {
            register()?;
            return Err(APPROVAL_REQUIRED.into());
        }
        "requiresApproval" => return Err(APPROVAL_REQUIRED.into()),
        "notFound" => {
            return Err("The helper is missing from the app bundle (reinstall RestoreKit).".into())
        }
        _ => return Err("The helper needs macOS 13 or later.".into()),
    }

    let c = CString::new(command).map_err(|_| "bad command".to_string())?;
    let mut err = [0u8; 512];
    let rc = unsafe { rk_helper_send(c.as_ptr(), err.as_mut_ptr() as *mut c_char, err.len()) };
    match rc {
        0 => Ok(()),
        // Unreachable despite being "enabled" — treat as an approval hiccup so
        // the UI re-walks the flow rather than showing a dead end.
        2 => Err(APPROVAL_REQUIRED.into()),
        _ => Err(read(&err)),
    }
}

fn read(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..end]).into_owned()
}
