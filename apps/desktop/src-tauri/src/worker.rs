//! Restore worker: a self-exec of this same binary that runs one device's
//! restore in an isolated process, streaming each progress event as NDJSON to
//! stdout. The parent (the GUI) spawns one of these per device for true
//! process-per-device parallelism; each gets its own copy of idevicerestore's
//! global C state, and shares the parent's single usbmuxd.

use std::io::Write;
use std::path::PathBuf;

use restorekit::restore::{self, Mode};

/// Marker arg that puts this binary into restore-worker mode.
pub const RESTORE_WORKER_ARG: &str = "--restore-worker";

/// If invoked with [`RESTORE_WORKER_ARG`], run the requested restore to
/// completion (streaming NDJSON events to stdout) and exit — returning `true`
/// so the caller skips starting the GUI. Otherwise return `false`.
pub fn maybe_run() -> bool {
    let args: Vec<String> = std::env::args().collect();
    if !args.iter().any(|a| a == RESTORE_WORKER_ARG) {
        return false;
    }

    let get = |key: &str| {
        args.iter()
            .position(|a| a == key)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };

    let ipsw = get("--ipsw").map(PathBuf::from);
    let ecid = get("--ecid").and_then(|s| {
        let s = s.trim();
        match s.strip_prefix("0x") {
            Some(hex) => u64::from_str_radix(hex, 16).ok(),
            None => s.parse().ok(),
        }
    });
    let mode = match get("--mode").as_deref() {
        Some("revive") => Mode::Revive,
        Some("obliterate") => Mode::Obliterate,
        _ => Mode::Erase,
    };
    let cache = get("--cache-dir").map(PathBuf::from);

    let (Some(ipsw), Some(ecid)) = (ipsw, ecid) else {
        emit_error("restore worker: missing or invalid --ipsw / --ecid");
        std::process::exit(2);
    };

    let mut stdout = std::io::stdout();
    let result = restore::restore(&ipsw, ecid, cache.as_deref(), mode, None, true, &mut |event| {
        if let Ok(line) = serde_json::to_string(&event) {
            let _ = writeln!(stdout, "{line}");
            let _ = stdout.flush();
        }
    });

    match result {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            emit_error(&e.to_string());
            std::process::exit(1);
        }
    }
}

/// Emit a terminal error event in the same NDJSON shape the CLI uses.
fn emit_error(message: &str) {
    let mut stdout = std::io::stdout();
    let _ = writeln!(
        stdout,
        "{}",
        serde_json::json!({ "event": "error", "message": message })
    );
    let _ = stdout.flush();
}
