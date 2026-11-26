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
    fn rk_helper_send(
        command: *const c_char,
        version: *const c_char,
        err: *mut c_char,
        err_len: usize,
    ) -> i32;
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
    // Registering is best-effort: the first call reports "Operation not
    // permitted" as it moves the daemon into requiresApproval — expected, and no
    // reason to skip opening the Settings pane the user needs.
    let _ = register();
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

/// Run a helper command (`dfu` / `reboot`, optionally followed by a port RID) as
/// root over XPC. If the daemon isn't enabled yet, registers it and returns
/// [`APPROVAL_REQUIRED`] so the UI can walk the user through the one-time
/// approval.
pub fn run_helper(command: &str) -> Result<(), String> {
    if status() == "unavailable" {
        return Err("The helper needs macOS 13 or later.".into());
    }
    // A freshly-bundled daemon reports notRegistered/notFound until registered,
    // and the *first* register() commonly returns an "Operation not permitted"
    // error precisely as it moves the daemon into requiresApproval — that's the
    // signal to guide the user, not a fatal error. So register best-effort and
    // let the resulting status decide.
    if status() != "enabled" {
        let reg = register();
        match status() {
            "enabled" => {}
            "requiresApproval" => return Err(APPROVAL_REQUIRED.into()),
            // It didn't even reach requiresApproval — surface the real reason if
            // register() reported one, otherwise still route to the approval UI.
            _ => return reg.and(Err(APPROVAL_REQUIRED.into())),
        }
    }

    // A helper whose version differs from ours exits after saying so, and
    // launchd starts the bundled one on the next connection. That relaunch can
    // take a moment, during which the service may look unreachable.
    let mut restarts = 0;
    loop {
        match send(command) {
            Err(e)
                if restarts < 5
                    && (e == VERSION_MISMATCH || (restarts > 0 && e == APPROVAL_REQUIRED)) =>
            {
                restarts += 1;
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            Err(e) if e.starts_with("unknown command: ") => return run_legacy(command),
            result => return result,
        }
    }
}

/// Error a helper of another version answers with, just before it exits.
const VERSION_MISMATCH: &str = "helper-version-mismatch";

/// A helper from before 0.6.6 ignores the version, never exits, and only takes
/// a bare `dfu` / `reboot`, so it stays in charge until the host restarts. Send
/// it the bare verb: its Auto target finds the one Mac on a DFU-capable port,
/// and refuses when there are several.
fn run_legacy(command: &str) -> Result<(), String> {
    let Some((verb, _port)) = command.split_once(' ') else {
        return Err(format!("the helper doesn't know `{command}`"));
    };
    send(verb).map_err(|e| {
        format!("{e}. RestoreKit's helper is still running an older version that can't pick a port; restart this Mac to finish updating it.")
    })
}

fn send(command: &str) -> Result<(), String> {
    let c = CString::new(command).map_err(|_| "bad command".to_string())?;
    let version = CString::new(env!("CARGO_PKG_VERSION")).expect("version has no NUL");
    let mut err = [0u8; 512];
    let rc = unsafe {
        rk_helper_send(
            c.as_ptr(),
            version.as_ptr(),
            err.as_mut_ptr() as *mut c_char,
            err.len(),
        )
    };
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
