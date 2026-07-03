//! `restorekit setup-driver` (Windows) — bind the WinUSB driver to the cabled
//! Apple device so libusb can reach it, without the user running Zadig.
//!
//! Installing a driver needs administrator rights, so if we aren't elevated we
//! relaunch ourselves through the UAC prompt and wait for that copy to finish.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use restorekit::progress::Event;
use restorekit::{driver, Error, Result};

use super::render;

/// Run the driver setup.
///
/// `elevated` marks the copy we relaunched through UAC (so it won't loop), and
/// `result_file` is where that copy records its outcome — the elevated process
/// runs in its own console, so this is how the error/result reaches the user's
/// original window.
pub fn run(json: bool, elevated: bool, result_file: Option<PathBuf>) -> Result<()> {
    if is_elevated() {
        let outcome = install(json);
        if let Some(path) = &result_file {
            // Spawned worker: hand the outcome back to the waiting parent.
            let line = match &outcome {
                Ok(n) => format!("OK:{n}"),
                Err(e) => format!("ERR:{e}"),
            };
            let _ = std::fs::write(path, line);
        } else if !json {
            // User ran us already-elevated: report here.
            report(outcome.as_ref().ok().copied(), outcome.as_ref().err());
        }
        return outcome.map(|_| ());
    }
    if elevated {
        // Spawned as the elevated worker but without an admin token — don't loop.
        return Err(Error::DriverInstall(
            "failed to acquire administrator rights".into(),
        ));
    }

    if !json {
        println!("Installing the WinUSB driver needs administrator rights.");
        println!("Approve the Windows (UAC) prompt to continue...");
    }

    let result_path = std::env::temp_dir().join(format!(
        "restorekit-setup-driver-{}.txt",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&result_path);
    let code = relaunch_elevated(&result_path)?;
    let outcome = std::fs::read_to_string(&result_path).ok();
    let _ = std::fs::remove_file(&result_path);

    match outcome.as_deref().map(str::trim) {
        Some(s) if s.starts_with("OK:") => {
            let n: usize = s[3..].trim().parse().unwrap_or(0);
            if !json {
                report(Some(n), None);
            }
            Ok(())
        }
        Some(s) if s.starts_with("ERR:") => Err(Error::DriverInstall(s[4..].trim().to_string())),
        // The elevated copy died before writing a result (crash, or UAC path
        // issue) — fall back to its exit code.
        _ if code == 0 => Ok(()),
        _ => Err(Error::DriverInstall(format!(
            "the elevated setup exited with code {code}"
        ))),
    }
}

/// Print the human-readable outcome.
fn report(count: Option<usize>, err: Option<&Error>) {
    match (count, err) {
        (Some(n), _) => println!(
            "Done — set up {n} device{}. Run `restorekit status` to detect the Mac.",
            if n == 1 { "" } else { "s" }
        ),
        (None, Some(e)) => eprintln!("error: {e}"),
        (None, None) => {}
    }
}

/// Perform the install (already elevated). Returns the number of devices bound.
fn install(json: bool) -> Result<usize> {
    let mut on_event = |e: Event| {
        if json {
            render::emit_json(&e);
        } else if let Event::DriverBound { name } = &e {
            println!("  bound WinUSB to {name}");
        }
    };
    driver::install_winusb(&mut on_event)
}

/// Is this process running with an elevated (administrator) token?
fn is_elevated() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elevation = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut core::ffi::c_void,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        ok != 0 && elevation.TokenIsElevated != 0
    }
}

/// Relaunch this exe elevated (triggers UAC) and return its exit code. The
/// elevated copy writes its outcome to `result_path`.
fn relaunch_elevated(result_path: &Path) -> Result<u32> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = std::env::current_exe().map_err(|e| Error::DriverInstall(e.to_string()))?;
    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let params = wide(&format!(
        "setup-driver --elevated --result-file \"{}\"",
        result_path.display()
    ));

    unsafe {
        let mut sei: SHELLEXECUTEINFOW = std::mem::zeroed();
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.lpParameters = params.as_ptr();
        // Hide the elevated copy's console — the user's original window shows the
        // result (via the temp file), and the GUI must not flash a terminal.
        sei.nShow = SW_HIDE;

        if ShellExecuteExW(&mut sei) == 0 {
            return Err(Error::DriverInstall(
                "could not start the elevated setup (UAC declined?)".into(),
            ));
        }
        if sei.hProcess.is_null() {
            return Ok(0);
        }
        WaitForSingleObject(sei.hProcess, INFINITE);
        let mut code: u32 = 0;
        GetExitCodeProcess(sei.hProcess, &mut code);
        CloseHandle(sei.hProcess);
        Ok(code)
    }
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
