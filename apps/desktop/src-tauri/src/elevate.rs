//! Run the privileged DFU helper as root via the macOS admin prompt.
//!
//! The helper (`restorekit-dfu-helper`) is the only thing that needs root. We
//! invoke it through `osascript … "with administrator privileges"`, which pops
//! the native authentication dialog and runs the command as root.

use std::path::PathBuf;
use std::process::Command;

/// Run `restorekit-dfu-helper <subcommand>` elevated. Returns the helper's
/// stderr on failure (including "User canceled." when the prompt is dismissed).
pub fn run_helper(subcommand: &str) -> Result<(), String> {
    let helper = helper_path().ok_or_else(|| {
        "DFU helper not found (the app bundle is incomplete)".to_string()
    })?;

    // Build the shell command (single-quoted path) and wrap it in an AppleScript
    // string for `do shell script`.
    let shell_cmd = format!("{} {}", shell_quote(&helper.to_string_lossy()), subcommand);
    let script = format!(
        "do shell script {} with administrator privileges",
        applescript_quote(&shell_cmd)
    );

    let output = Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map_err(|e| format!("failed to run osascript: {e}"))?;

    if output.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        let err = err.trim();
        if err.contains("User canceled") {
            Err("Authorization was cancelled.".to_string())
        } else if err.is_empty() {
            Err("The DFU trigger failed.".to_string())
        } else {
            Err(err.to_string())
        }
    }
}

/// Locate the bundled helper across dev and bundled layouts.
fn helper_path() -> Option<PathBuf> {
    let arch = std::env::consts::ARCH; // "aarch64"
    let triple_name = format!("restorekit-dfu-helper-{arch}-apple-darwin");
    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Bundled: sidecar sits next to the app binary in Contents/MacOS.
            candidates.push(dir.join("restorekit-dfu-helper"));
            candidates.push(dir.join(&triple_name));
        }
    }

    // Dev: the staged externalBin, and the workspace target dir.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    candidates.push(manifest.join("binaries").join(&triple_name));
    for profile in ["debug", "release"] {
        candidates.push(
            manifest
                .join("../../../target")
                .join(profile)
                .join("restorekit-dfu-helper"),
        );
    }

    candidates.into_iter().find(|p| p.exists())
}

/// Single-quote a string for /bin/sh.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Quote a string as an AppleScript literal.
fn applescript_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
