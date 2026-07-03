use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

/// How to restore the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Full restore, erasing all data (`idevicerestore --erase`).
    Erase,
    /// Update-style restore that preserves user data ("revive").
    Revive,
}

/// idevicerestore progress step numbers, from `enum` in idevicerestore's
/// `common.h`. Used to give restore steps human names.
fn step_name(step: u32) -> &'static str {
    match step {
        0 => "detecting device",
        1 => "preparing",
        2 => "uploading iBEC",
        3 => "waiting for reconnect",
        4 => "uploading ramdisk",
        5 => "restoring image",
        6 => "verifying restore",
        7 => "checking filesystem",
        8 => "flashing firmware",
        _ => "restoring",
    }
}

/// Locate the idevicerestore binary: explicit override, else PATH.
pub fn find_idevicerestore(override_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(p) = override_path {
        return if p.exists() {
            Ok(p.to_path_buf())
        } else {
            Err(Error::IdevicerestoreNotFound)
        };
    }
    which("idevicerestore").ok_or(Error::IdevicerestoreNotFound)
}

fn which(bin: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(bin))
        .find(|candidate| candidate.is_file())
}

/// Run a restore, streaming progress. `ecid` targets a specific device (hex,
/// e.g. "0x1234"); required when more than one device might be attached.
pub fn restore(
    idevicerestore: &Path,
    ipsw: &Path,
    ecid: Option<&str>,
    mode: Mode,
    progress: ProgressFn,
) -> Result<()> {
    let mut cmd = Command::new(idevicerestore);
    cmd.arg("-y") // non-interactive
        .arg("-P"); // machine-parsable progress
    if mode == Mode::Erase {
        cmd.arg("-e");
    }
    if let Some(ecid) = ecid {
        cmd.arg("-i").arg(ecid);
    }
    cmd.arg(ipsw);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::IdevicerestoreNotFound
        } else {
            Error::Io(e)
        }
    })?;

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    // Keep the last N lines for a useful error message on failure.
    let mut log_tail: VecDeque<String> = VecDeque::with_capacity(64);

    // Drain stderr on a thread so it can't deadlock the pipe.
    let stderr_handle = std::thread::spawn(move || {
        let mut lines = Vec::new();
        for line in BufReader::new(stderr)
            .lines()
            .map_while(std::result::Result::ok)
        {
            lines.push(line);
        }
        lines
    });

    for line in BufReader::new(stdout)
        .lines()
        .map_while(std::result::Result::ok)
    {
        if let Some((step, fraction)) = parse_progress(&line) {
            progress(Event::RestoreStep {
                step,
                name: step_name(step).to_string(),
                progress: fraction,
            });
        } else {
            progress(Event::RestoreLog { line: line.clone() });
        }
        push_tail(&mut log_tail, line);
    }

    let status = child.wait()?;
    for line in stderr_handle.join().unwrap_or_default() {
        push_tail(&mut log_tail, line);
    }

    if status.success() {
        progress(Event::Done);
        Ok(())
    } else {
        Err(Error::RestoreFailed {
            status: status.code().unwrap_or(-1),
            log_tail: log_tail.into_iter().collect::<Vec<_>>().join("\n"),
        })
    }
}

fn push_tail(tail: &mut VecDeque<String>, line: String) {
    if tail.len() == 64 {
        tail.pop_front();
    }
    tail.push_back(line);
}

/// Parse an idevicerestore `-P` progress line: `progress: <step> <fraction>`.
fn parse_progress(line: &str) -> Option<(u32, f32)> {
    let rest = line.trim().strip_prefix("progress:")?;
    let mut parts = rest.split_whitespace();
    let step = parts.next()?.parse::<u32>().ok()?;
    let fraction = parts.next()?.parse::<f32>().ok()?;
    Some((step, fraction))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_line() {
        assert_eq!(parse_progress("progress: 5 0.42"), Some((5, 0.42)));
        assert_eq!(parse_progress("  progress: 0 0"), Some((0, 0.0)));
    }

    #[test]
    fn ignores_non_progress_lines() {
        assert_eq!(parse_progress("Restoring device..."), None);
        assert_eq!(parse_progress("progress: notanumber"), None);
    }

    #[test]
    fn step_names_have_fallback() {
        assert_eq!(step_name(5), "restoring image");
        assert_eq!(step_name(999), "restoring");
    }
}
