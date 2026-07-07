//! Privileged root helper daemon for the RestoreKit app.
//!
//! Registered once as a LaunchDaemon via `SMAppService` and reached over XPC.
//! It hosts a Mach service (see `csrc/xpc_daemon.m`), which verifies every
//! caller is the signed RestoreKit app before dispatching here — so only the
//! app can ask root to trigger DFU. This is the only restorekit code that ever
//! runs with elevated privileges.

use std::process::ExitCode;

#[cfg(target_os = "macos")]
fn main() -> ExitCode {
    imp::run();
    ExitCode::SUCCESS
}

#[cfg(not(target_os = "macos"))]
fn main() -> ExitCode {
    eprintln!("the RestoreKit helper daemon only runs on macOS");
    ExitCode::FAILURE
}

#[cfg(target_os = "macos")]
mod imp {
    use restorekit::dfu::{vdm, DfuTarget};
    use restorekit::Event;
    use std::os::raw::c_char;

    extern "C" {
        // Runs the XPC listener forever, calling `handle` for each verified
        // command. Defined in csrc/xpc_daemon.m.
        fn rk_daemon_run(handler: extern "C" fn(*const c_char, *mut c_char, usize) -> i32);
    }

    pub fn run() {
        unsafe { rk_daemon_run(handle) };
    }

    /// Invoked by the XPC shim once the peer's signature is verified. Runs the
    /// command as root; returns 0 on success, or fills `err` and returns 1.
    extern "C" fn handle(command: *const c_char, err: *mut c_char, err_len: usize) -> i32 {
        let cmd = unsafe { std::ffi::CStr::from_ptr(command) }
            .to_string_lossy()
            .into_owned();
        let mut sink = |_: Event| {};
        let result: Result<(), String> = match cmd.as_str() {
            "dfu" => vdm::enter_dfu(&DfuTarget::Auto, &mut sink).map_err(|e| e.to_string()),
            "reboot" => vdm::reboot(&DfuTarget::Auto, &mut sink).map_err(|e| e.to_string()),
            other => Err(format!("unknown command: {other}")),
        };
        match result {
            Ok(()) => 0,
            Err(msg) => {
                write_err(err, err_len, &msg);
                1
            }
        }
    }

    fn write_err(err: *mut c_char, err_len: usize, msg: &str) {
        if err.is_null() || err_len == 0 {
            return;
        }
        let bytes = msg.as_bytes();
        let n = bytes.len().min(err_len - 1);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), err as *mut u8, n);
            *err.add(n) = 0;
        }
    }
}
