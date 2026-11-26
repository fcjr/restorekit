use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use indicatif::{ProgressBar, ProgressStyle};
use restorekit::device::{self, UsbMode};
use restorekit::progress::Event;
use restorekit::restore::Mode;
use restorekit::{firmware, recovery, restore, Device, Error, Result};

use super::render;

/// Times where a restore's wall-clock goes. Each moment goes to one phase: the
/// filesystem upload while it streams (images and firmware go out alongside
/// it, but the stream is what the restore waits on), else whatever the device
/// says it's doing, else idevicerestore's own step. idevicerestore's steps
/// alone mislabel it: the device's progress messages for one step keep
/// arriving while the filesystem streams for another.
struct PhaseTimer {
    start: Instant,
    step: Option<String>,
    operation: Option<String>,
    streaming: bool,
    /// (phase, started).
    current: Option<(String, Instant)>,
    phases: Vec<(String, Duration)>,
}

impl PhaseTimer {
    fn new() -> Self {
        Self {
            start: Instant::now(),
            step: None,
            operation: None,
            streaming: false,
            current: None,
            phases: Vec::new(),
        }
    }

    fn event(&mut self, event: &Event) {
        match event {
            Event::RestoreStep { step, progress, .. }
                if *step == restore::FILESYSTEM_STREAM_STEP =>
            {
                self.streaming = *progress < 1.0;
            }
            Event::RestoreStep { name, .. } => self.step = Some(name.clone()),
            Event::DeviceOperation { name, .. } => self.operation = Some(name.clone()),
            _ => return,
        }
        let phase = if self.streaming {
            Some("uploading filesystem".to_string())
        } else {
            self.operation.clone().or_else(|| self.step.clone())
        };
        if let Some(phase) = phase {
            self.enter(phase, Instant::now());
        }
    }

    fn enter(&mut self, phase: String, now: Instant) {
        match &self.current {
            Some((cur, _)) if *cur == phase => {}
            Some((cur, started)) => {
                self.phases.push((cur.clone(), now - *started));
                self.current = Some((phase, now));
            }
            None => self.current = Some((phase, now)),
        }
    }

    fn finish(&mut self) {
        if let Some((cur, started)) = self.current.take() {
            self.phases.push((cur, started.elapsed()));
        }
    }

    /// The phases with repeats folded together, in first-seen order: the
    /// device can go back and forth between operations (updating one
    /// coprocessor while waiting on another).
    fn merged(&self) -> Vec<(String, Duration)> {
        let mut merged: Vec<(String, Duration)> = Vec::new();
        for (name, dur) in &self.phases {
            match merged.iter_mut().find(|(n, _)| n == name) {
                Some((_, d)) => *d += *dur,
                None => merged.push((name.clone(), *dur)),
            }
        }
        merged
    }

    fn report(&self, json: bool) {
        if self.phases.is_empty() {
            return;
        }
        let total = self.start.elapsed();
        let phases = self.merged();
        if json {
            let phases: Vec<_> = phases
                .iter()
                .map(|(n, d)| serde_json::json!({ "phase": n, "seconds": d.as_secs_f64() }))
                .collect();
            println!(
                "{}",
                serde_json::json!({ "event": "phase_timing", "total_seconds": total.as_secs_f64(), "phases": phases })
            );
            return;
        }
        eprintln!("\nRestore phase timing:");
        for (name, dur) in &phases {
            if dur.as_secs() > 0 {
                eprintln!("  {:<30} {:>7}", name, fmt_dur(*dur));
            }
        }
        eprintln!("  {:<30} {:>7}", "total", fmt_dur(total));
    }
}

fn fmt_dur(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 60 {
        format!("{}m{:02}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

pub struct Opts {
    pub mode: Mode,
    pub ipsw: Option<PathBuf>,
    pub os_version: Option<String>,
    pub identifier: Option<String>,
    pub ecid: Option<u64>,
    pub dongle: Option<String>,
    pub supervisor: Option<PathBuf>,
    pub yes: bool,
    pub cache_dir: Option<PathBuf>,
    pub json: bool,
    pub verbose: bool,
}

/// Ensure a target is ready to restore (a Mac in DFU, triggering entry if the
/// host can, or an iPhone or iPad in DFU or recovery), then resolve → download
/// → restore it.
pub fn run(opts: Opts) -> Result<()> {
    restorekit::restore::preflight()?;
    let supervisor = opts
        .supervisor
        .as_deref()
        .map(super::recovery::load_supervisor)
        .transpose()?;
    // A running iPhone or iPad would sit in recovery mode while its firmware
    // downloads, and iBoot reboots out of recovery mode after 15 idle minutes.
    // Fetch the firmware and confirm the erase first, then take it there.
    let mut early = None;
    if opts.dongle.is_none() {
        if let Some(dev) = running_ios_target(opts.ecid)? {
            check_mode(&dev, opts.mode)?;
            let path = fetch_firmware(&dev, &opts)?;
            if !confirm(&dev, opts.mode, opts.yes, opts.json)? {
                say(opts.json, "Aborted.");
                return Ok(());
            }
            early = Some((dev.ecid, path));
        }
    }
    let device = super::dfu::ensure_present(
        opts.json,
        Duration::from_secs(120),
        opts.dongle.clone(),
        opts.ecid,
        supervisor.as_ref(),
    )?;
    // ensure_present picks the same device unless something else turned up
    // ready to restore meanwhile; that one gets its own firmware and prompt.
    let prepared = early.and_then(|(ecid, path)| (ecid == device.ecid).then_some(path));
    restore_device(&device, opts, prepared)
}

/// The running iPhone or iPad `ensure_present` would take to recovery mode for
/// this restore, if that's what it would do: the one with `ecid`, or with no
/// ECID the sole one when nothing is ready to restore and no Mac or dongle is
/// connected (see `ensure_present`).
pub(crate) fn running_ios_target(ecid: Option<u64>) -> Result<Option<Device>> {
    let mut devices = device::list()?;
    device::identify(&mut devices);
    recovery::identify(&mut devices);
    let running = |d: &Device| d.is_ios() && d.mode == UsbMode::Other && d.ecid.is_some();
    if let Some(e) = ecid {
        return Ok(devices
            .into_iter()
            .find(|d| d.ecid == Some(e))
            .filter(running));
    }
    let mac = devices
        .iter()
        .any(|d| !d.is_ios() && d.mode != UsbMode::Other);
    if devices.iter().any(Device::ready_to_restore)
        || mac
        || restorekit::dongle::list().is_ok_and(|d| !d.is_empty())
    {
        return Ok(None);
    }
    let mut running: Vec<Device> = devices.into_iter().filter(running).collect();
    Ok(if running.len() == 1 {
        running.pop()
    } else {
        None
    })
}

/// Revive and obliterate haven't been tried on an iPhone or iPad.
fn check_mode(device: &Device, mode: Mode) -> Result<()> {
    match mode {
        Mode::Revive if device.is_ios() => Err(Error::UntestedOnIos("revive")),
        Mode::Obliterate if device.is_ios() => Err(Error::UntestedOnIos("obliterate")),
        _ => Ok(()),
    }
}

fn cache_dir(opts: &Opts) -> Result<PathBuf> {
    match &opts.cache_dir {
        Some(d) => Ok(d.clone()),
        None => firmware::default_cache_dir(),
    }
}

/// The IPSW to restore `device` from: `--ipsw` if given, else the firmware
/// resolved for its model, downloaded into the cache.
fn fetch_firmware(device: &Device, opts: &Opts) -> Result<PathBuf> {
    let json = opts.json;
    if let Some(path) = &opts.ipsw {
        return Ok(path.clone());
    }
    let identity = device
        .identity
        .as_ref()
        .expect("restore target carries an identity");
    let identifier = opts
        .identifier
        .clone()
        .or_else(|| device.identifier().map(str::to_string))
        .ok_or(Error::UnknownModel {
            cpid: identity.cpid,
            bdid: identity.bdid,
        })?;
    say(json, &format!("Resolving firmware for {identifier}..."));
    let fw = firmware::resolve(&identifier, opts.os_version.as_deref())?;
    if json {
        render::emit_json(&Event::FirmwareResolved {
            identifier: fw.identifier.clone(),
            version: fw.version.clone(),
            build: fw.build.clone(),
            size: fw.size,
            url: fw.url.clone(),
        });
    } else {
        println!("  {} {} (build {})", fw.os_name(), fw.version, fw.build);
    }

    let bar = ProgressBar::hidden();
    let path = firmware::download(&cache_dir(opts)?, &fw, &mut |e| {
        render::download(&bar, e, json)
    })?;
    bar.finish_and_clear();
    Ok(path)
}

/// Restore `device`. `prepared` is firmware already fetched, with the erase
/// already confirmed, before a running iPhone or iPad was taken to recovery.
fn restore_device(device: &Device, opts: Opts, prepared: Option<PathBuf>) -> Result<()> {
    let json = opts.json;
    // Every path into here (ensure_present, the picker, --ecid) selects a
    // device in DFU or recovery mode with a parsed serial, so the ECID and
    // identity are set.
    let ecid = device.ecid.expect("restore target carries an ECID");
    let cache = cache_dir(&opts)?;
    let mode = opts.mode;
    check_mode(device, mode)?;

    let ipsw_path = match prepared {
        Some(path) => path,
        None => {
            let path = fetch_firmware(device, &opts)?;
            if !confirm(device, mode, opts.yes, json)? {
                say(json, "Aborted.");
                return Ok(());
            }
            path
        }
    };

    say(json, "Starting restore. Do not disconnect the target.");
    // The restore-mode USB interface needs our WinUSB forced onto it, which is
    // done by an elevated watcher the restore spawns — warn about the UAC prompt.
    // Apple's usbmuxd, when running, handles restore mode itself.
    #[cfg(target_os = "windows")]
    if matches!(restorekit::restore::windows_mux_check(), Ok(false)) {
        say(
            json,
            "  A Windows (UAC) prompt will appear to set up restore-mode USB access — approve it.",
        );
    }
    // In verbose mode idevicerestore's log streams to the terminal, so hide the
    // progress bar to avoid interleaving with it.
    let bar = if json || opts.verbose {
        ProgressBar::hidden()
    } else {
        let b = ProgressBar::new(100);
        b.set_style(
            ProgressStyle::with_template("{msg:24} {bar:32.green/black} {percent:>3}%").unwrap(),
        );
        b
    };
    // Capture the wipe verdict from the event stream. It is emitted even when the
    // restore later fails (the key can be destroyed early), so grab it in the
    // progress closure and keep a confirmed/failed verdict over a later
    // unconfirmed one across retries.
    let mut wipe: Option<(String, String)> = None;
    let mut checkpoints: Option<(Vec<String>, Vec<String>)> = None;
    let mut timer = PhaseTimer::new();
    let result = restore::restore(
        &ipsw_path,
        ecid,
        Some(&cache),
        mode,
        opts.verbose,
        &mut |event| {
            timer.event(&event);
            match &event {
                Event::Obliteration { status, detail } => {
                    let strong = status == "confirmed" || status == "failed";
                    if wipe.is_none() || strong {
                        wipe = Some((status.clone(), detail.clone()));
                    }
                }
                Event::Checkpoints { json, raw } => {
                    checkpoints = Some((json.clone(), raw.clone()));
                }
                _ => {}
            }
            restore_render(&bar, event, json);
        },
    );
    timer.finish();
    bar.finish_and_clear();
    // Show where the wall-clock went, whether the restore succeeded or not.
    timer.report(json);

    // Report the wipe verdict regardless of the restore's overall outcome.
    report_wipe(json, wipe.as_ref());

    let wipe_status = wipe.as_ref().map(|(s, _)| s.as_str());

    // Obliterate deliberately stops the restore right after the wipe, so a
    // confirmed wipe is the intended outcome even though idevicerestore may
    // report the truncated run as an error.
    if mode == Mode::Obliterate {
        return match wipe_status {
            Some("confirmed") => {
                record_restore_history(device, mode, wipe.as_ref(), checkpoints.as_ref(), true);
                say(json, &completion_message(device, mode));
                Ok(())
            }
            _ => {
                record_restore_history(device, mode, wipe.as_ref(), checkpoints.as_ref(), false);
                Err(result.err().unwrap_or(Error::RestoreFailed {
                    status: -1,
                    // The wipe checkpoint never showed, so nothing stopped the
                    // erase restore: it reinstalled the OS.
                    log_tail: "obliterate did not confirm the encryption-key wipe; the erase \
                               restore ran to the end instead, so the device was erased and \
                               reinstalled without proof its key was destroyed"
                        .into(),
                }))
            }
        };
    }

    match result {
        Ok(_) => {
            record_restore_history(device, mode, wipe.as_ref(), checkpoints.as_ref(), true);
            // A device-reported wipe failure is fatal even if the restore itself
            // returned success — the key is not proven destroyed.
            if wipe.as_ref().map(|(s, _)| s.as_str()) == Some("failed") {
                return Err(Error::RestoreFailed {
                    status: -1,
                    log_tail: "device reported the encryption-key wipe FAILED".into(),
                });
            }
            say(json, &completion_message(device, mode));
            Ok(())
        }
        Err(e) => {
            record_restore_history(device, mode, wipe.as_ref(), checkpoints.as_ref(), false);
            // If the key was destroyed before the restore failed, the data is
            // already unrecoverable; the machine just needs a re-restore for an OS.
            if wipe.as_ref().map(|(s, _)| s.as_str()) == Some("confirmed") {
                say(
                    json,
                    "The encryption key was destroyed before the restore failed, so the \
                     old data is unrecoverable. Re-run the restore to reinstall the OS.",
                );
            }
            Err(e)
        }
    }
}

/// Log the restore to the shared history DB (best-effort — a write failure never
/// fails the restore). Records a successful restore, or a failed one where the
/// key was still obliterated, so a wipe that completed before a later failure is
/// captured for the refurb audit.
#[cfg(feature = "history")]
fn record_restore_history(
    device: &Device,
    mode: Mode,
    wipe: Option<&(String, String)>,
    checkpoints: Option<&(Vec<String>, Vec<String>)>,
    ok: bool,
) {
    use restorekit::history::{self, HistoryEntry};

    let obliteration = wipe.map(|(status, _)| status.clone());
    let wiped = matches!(obliteration.as_deref(), Some("confirmed") | Some("failed"));
    if !ok && !wiped {
        return;
    }
    // Serialize each checkpoint list to a JSON array of strings (handles the
    // multi-line raw XML records unambiguously). None when nothing was captured.
    let (checkpoints_json, checkpoints_raw) = match checkpoints {
        Some((j, r)) => (
            (!j.is_empty()).then(|| serde_json::to_string(j).unwrap_or_default()),
            (!r.is_empty()).then(|| serde_json::to_string(r).unwrap_or_default()),
        ),
        None => (None, None),
    };
    let entry = HistoryEntry {
        serial_number: device.srnm.clone(),
        ecid: device.ecid_hex().unwrap_or_default(),
        model_identifier: device.identifier().map(str::to_string),
        name: device.display_name(),
        mode: match mode {
            Mode::Erase => "restore",
            Mode::Revive => "revive",
            Mode::Obliterate => "obliterate",
        }
        .to_string(),
        status: match (ok, mode) {
            (true, Mode::Obliterate) => "obliterated",
            (true, _) => "restored",
            (false, _) => "restore_failed",
        }
        .to_string(),
        timestamp_rfc3339: history::now_rfc3339(),
        obliteration,
        checkpoints_json,
        checkpoints_raw,
    };
    if let Err(e) = history::record(&entry) {
        eprintln!("warning: could not record restore history: {e}");
    }
}

#[cfg(not(feature = "history"))]
fn record_restore_history(
    _: &Device,
    _: Mode,
    _: Option<&(String, String)>,
    _: Option<&(Vec<String>, Vec<String>)>,
    _: bool,
) {
}

/// Print the encryption-key obliteration verdict for an erase restore (nothing
/// for a revive, which does not obliterate). Suppressed in `--json` mode, where
/// it already went out as a machine-readable Obliteration event.
fn report_wipe(json: bool, wipe: Option<&(String, String)>) {
    let Some((status, detail)) = wipe else { return };
    match status.as_str() {
        "confirmed" => say(
            json,
            &format!("Encryption key obliterated (verified): {detail}"),
        ),
        "failed" => say(
            json,
            &format!("WARNING: the device reported the encryption-key wipe FAILED: {detail}"),
        ),
        "unconfirmed" => say(
            json,
            "Note: no encryption-key obliteration signal appeared in the device log. The \
             erase still destroys the key; it just could not be verified. Re-run with \
             --verbose to inspect the device log.",
        ),
        _ => {}
    }
}

/// The closing status line, tailored to what the target will actually do next.
/// A T2 erase restore reinstalls bridgeOS only, so macOS must be recovered
/// separately; a revive preserves the data; any other erase restores fully.
fn completion_message(device: &Device, mode: Mode) -> String {
    match (device.is_t2(), mode) {
        (_, Mode::Obliterate) => format!(
            "Key obliterated. The {} is wiped with no OS — run `restorekit restore` to reinstall.",
            device.kind()
        ),
        (true, Mode::Erase) => {
            "bridgeOS restore complete. Reinstall macOS via internet recovery (hold Cmd-R at boot)."
                .into()
        }
        (true, Mode::Revive) => {
            "bridgeOS revive complete. macOS and your data are preserved; the Mac should boot normally."
                .into()
        }
        (false, Mode::Revive) => format!(
            "Revive complete. Your data is preserved; the {} should boot normally.",
            device.kind()
        ),
        _ => "Restore complete. The target should boot to Setup Assistant.".into(),
    }
}

/// Print a human status line, suppressed in `--json` mode.
fn say(json: bool, msg: &str) {
    if !json {
        println!("{msg}");
    }
}

fn confirm(device: &Device, mode: Mode, yes: bool, json: bool) -> Result<bool> {
    if mode == Mode::Revive || yes {
        return Ok(true);
    }
    // Can't prompt interactively in machine-readable mode.
    if json {
        return Err(Error::RestoreFailed {
            status: -1,
            log_tail: "refusing to erase without --yes in --json mode".into(),
        });
    }
    println!();
    println!(
        "  WARNING: this will ERASE ALL DATA on {} (ECID {}).",
        device.display_name(),
        device.ecid_hex().unwrap_or_default()
    );
    // Obliterate destroys the key and stops — no OS is put back at all. Make the
    // "you must restore afterward" part explicit so nobody is surprised by a Mac
    // that won't boot.
    if mode == Mode::Obliterate {
        println!(
            "  This destroys the encryption key and STOPS — no OS is reinstalled. The {} will",
            device.kind()
        );
        println!(
            "  be left wiped and unbootable; run `restorekit restore` afterward to make it usable."
        );
    } else if device.is_t2() {
        // A T2 erase restore only reinstalls bridgeOS — unlike Apple Silicon, it
        // does not put macOS back. Make that explicit so the user isn't left with
        // a Mac that won't boot. Use `revive` to update bridgeOS without erasing.
        println!(
            "  This T2 Mac will be wiped and restored to bridgeOS only — macOS is NOT reinstalled."
        );
        println!(
            "  Afterward you must reinstall macOS via internet recovery (hold Cmd-R at boot)."
        );
        println!("  To update bridgeOS without erasing, run `restorekit revive` instead.");
    }
    print!("  Type ERASE to continue: ");
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    Ok(input.trim() == "ERASE")
}

fn restore_render(bar: &ProgressBar, event: Event, json: bool) {
    if json {
        render::emit_json(&event);
        return;
    }
    match event {
        Event::RestoreStep { name, progress, .. } => {
            bar.set_message(name);
            bar.set_position((progress * 100.0) as u64);
        }
        Event::RestoreRetrying {
            attempt,
            max_attempts,
            ..
        } => {
            // Not bar.println: a hidden bar (verbose, or no terminal) drops it.
            bar.suspend(|| {
                println!(
                    "Connection to the target dropped; retrying restore ({}/{max_attempts})...",
                    attempt + 1
                )
            });
        }
        Event::Done => bar.set_position(100),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filesystem_stream_outranks_the_device_operations_beside_it() {
        let step = |step, name: &str, progress| Event::RestoreStep {
            step,
            name: name.into(),
            progress,
        };
        let op = |name: &str| Event::DeviceOperation {
            code: 0,
            name: name.into(),
        };
        let fs = restore::FILESYSTEM_STREAM_STEP;
        let mut timer = PhaseTimer::new();
        timer.event(&step(1, "preparing", 1.0));
        timer.event(&op("creating filesystem"));
        timer.event(&step(fs, "uploading filesystem", 0.0));
        timer.event(&op("requesting FUD data"));
        timer.event(&step(6, "flashing firmware updater", 0.5));
        timer.event(&step(fs, "uploading filesystem", 0.5));
        timer.event(&step(fs, "uploading filesystem", 1.0));
        timer.event(&op("updating baseband"));
        timer.finish();
        let names: Vec<_> = timer.phases.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "preparing",
                "creating filesystem",
                "uploading filesystem",
                "requesting FUD data",
                "updating baseband"
            ]
        );
    }

    #[test]
    fn merged_folds_repeated_phases_in_first_seen_order() {
        let secs = Duration::from_secs;
        let mut timer = PhaseTimer::new();
        timer.phases = vec![
            ("preparing".into(), secs(45)),
            ("updating Rose".into(), secs(3)),
            ("updating Stockholm".into(), secs(4)),
            ("updating Rose".into(), secs(2)),
        ];
        assert_eq!(
            timer.merged(),
            vec![
                ("preparing".into(), secs(45)),
                ("updating Rose".into(), secs(5)),
                ("updating Stockholm".into(), secs(4)),
            ]
        );
    }
}
