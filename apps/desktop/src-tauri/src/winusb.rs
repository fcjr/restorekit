//! Windows WinUSB driver setup for the desktop app.
//!
//! A Mac in DFU/recovery can't be opened by libusb until WinUSB is bound to it.
//! Binding a driver needs admin, so the app self-elevates a *headless* copy of
//! itself (`--install-winusb-driver`) through the UAC prompt; that copy runs the
//! shared `restorekit::driver::install_winusb` and reports back via a temp file.
//! This mirrors the macOS privileged-helper flow, minus a long-lived daemon.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};

/// Elevated headless entry point (called from `main` when relaunched). Installs
/// the WinUSB driver, records the outcome in `result_file`, and returns the
/// process exit code.
pub fn install_winusb_headless(result_file: Option<PathBuf>) -> i32 {
    let outcome = restorekit::driver::install_winusb(&mut |_| {});
    if let Some(path) = &result_file {
        let line = match &outcome {
            Ok(n) => format!("OK:{n}"),
            Err(e) => format!("ERR:{e}"),
        };
        let _ = std::fs::write(path, line);
    }
    match outcome {
        Ok(_) => 0,
        Err(_) => 1,
    }
}

/// Self-elevate a headless copy to bind WinUSB (shows the UAC prompt). Blocks
/// until it finishes; returns the driver-setup error text on failure.
pub fn setup_driver() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let result_path =
        std::env::temp_dir().join(format!("restorekit-winusb-gui-{}.txt", std::process::id()));
    let _ = std::fs::remove_file(&result_path);

    let code = relaunch_elevated(&exe, &result_path)?;

    let outcome = std::fs::read_to_string(&result_path).ok();
    let _ = std::fs::remove_file(&result_path);
    match outcome.as_deref().map(str::trim) {
        Some(s) if s.starts_with("OK:") => Ok(()),
        Some(s) if s.starts_with("ERR:") => Err(s[4..].trim().to_string()),
        _ if code == 0 => Ok(()),
        _ => Err(format!("driver setup exited with code {code}")),
    }
}

/// Relaunch `exe` elevated and hidden, running the headless installer; return
/// its exit code.
fn relaunch_elevated(exe: &Path, result_path: &Path) -> Result<u32, String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let verb = wide("runas");
    let file = wide(&exe.to_string_lossy());
    let params = wide(&format!(
        "--install-winusb-driver --result-file \"{}\"",
        result_path.display()
    ));

    unsafe {
        let mut sei: SHELLEXECUTEINFOW = std::mem::zeroed();
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.lpParameters = params.as_ptr();
        sei.nShow = SW_HIDE;

        if ShellExecuteExW(&mut sei) == 0 {
            return Err("could not start the elevated installer (UAC declined?)".into());
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

/// Is WinUSB (a libusb-class driver) bound to this device — i.e. can restorekit
/// open it? Opening succeeds only when a compatible driver is attached.
pub fn device_ready(info: &nusb::DeviceInfo) -> bool {
    use nusb::MaybeFuture;
    info.open().wait().is_ok()
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}
