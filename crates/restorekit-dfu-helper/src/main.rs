//! Privileged DFU-trigger helper for the restorekit desktop app.
//!
//! The DFU trigger opens a root-only IOKit user client, and a GUI must not run
//! as root — so the app runs this tiny helper via the macOS admin prompt. It is
//! the only restorekit code that ever runs with elevated privileges.
//!
//! Usage: `restorekit-dfu-helper <dfu|reboot>`. Stages print to stdout; a
//! non-zero exit means the trigger failed.

use std::process::ExitCode;

enum Mode {
    Dfu,
    Reboot,
}

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("dfu") => run(Mode::Dfu),
        Some("reboot") => run(Mode::Reboot),
        _ => {
            eprintln!("usage: restorekit-dfu-helper <dfu|reboot>");
            ExitCode::from(2)
        }
    }
}

#[cfg(target_os = "macos")]
fn run(mode: Mode) -> ExitCode {
    use restorekit::dfu::vdm;
    use restorekit::Event;

    let mut on_stage = |event: Event| {
        if let Event::DfuTriggerStage { stage } = event {
            println!("{stage}");
        }
    };
    let result = match mode {
        Mode::Dfu => vdm::enter_dfu(&mut on_stage),
        Mode::Reboot => vdm::reboot(&mut on_stage),
    };
    // A machine-readable final line: the app reads this over the authorization
    // pipe (which doesn't surface the exit code) to learn the outcome.
    match result {
        Ok(()) => {
            println!("RESULT: ok");
            ExitCode::SUCCESS
        }
        Err(e) => {
            println!("RESULT: error {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(not(target_os = "macos"))]
fn run(_mode: Mode) -> ExitCode {
    eprintln!("the DFU trigger is only supported on an Apple Silicon Mac host");
    ExitCode::FAILURE
}
