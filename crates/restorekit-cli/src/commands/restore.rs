use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use indicatif::{ProgressBar, ProgressStyle};
use restorekit::progress::Event;
use restorekit::restore::Mode;
use restorekit::{firmware, restore, Device, Error, Result};

use super::render;

pub struct Opts {
    pub revive: bool,
    pub ipsw: Option<PathBuf>,
    pub os_version: Option<String>,
    pub identifier: Option<String>,
    pub ecid: Option<u64>,
    pub dongle: Option<String>,
    pub yes: bool,
    pub cache_dir: Option<PathBuf>,
    pub json: bool,
    pub verbose: bool,
}

/// Ensure a target is in DFU mode (triggering entry if the host can), then
/// resolve → download → restore it.
pub fn run(opts: Opts) -> Result<()> {
    let device = super::dfu::ensure_present(
        opts.json,
        Duration::from_secs(120),
        opts.dongle.clone(),
        opts.ecid,
    )?;
    restore_device(&device, opts)
}

fn restore_device(device: &Device, opts: Opts) -> Result<()> {
    let json = opts.json;
    // Every path into here (ensure_present, the picker, --ecid) selects a
    // DFU-mode device with a parsed serial, so the ECID and identity are set.
    let identity = device
        .identity
        .as_ref()
        .expect("restore target carries an identity");
    let ecid = device.ecid.expect("restore target carries an ECID");
    let cache = match &opts.cache_dir {
        Some(d) => d.clone(),
        None => firmware::default_cache_dir()?,
    };

    // Resolve firmware: explicit --ipsw wins, else resolve for the device model.
    let ipsw_path = if let Some(path) = &opts.ipsw {
        path.clone()
    } else {
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
            // A T2 IPSW carries bridgeOS, not macOS — label it accordingly.
            let os = if device.is_t2() { "bridgeOS" } else { "macOS" };
            println!("  {os} {} (build {})", fw.version, fw.build);
        }

        let bar = ProgressBar::hidden();
        let path = firmware::download(&cache, &fw, &mut |e| render::download(&bar, e, json))?;
        bar.finish_and_clear();
        path
    };

    let mode = if opts.revive {
        Mode::Revive
    } else {
        Mode::Erase
    };

    if !confirm(device, mode, opts.yes, json)? {
        say(json, "Aborted.");
        return Ok(());
    }

    say(json, "Starting restore. Do not disconnect the target.");
    // The restore-mode USB interface needs our WinUSB forced onto it, which is
    // done by an elevated watcher the restore spawns — warn about the UAC prompt.
    #[cfg(target_os = "windows")]
    say(
        json,
        "  A Windows (UAC) prompt will appear to set up restore-mode USB access — approve it.",
    );
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
    let result = restore::restore(
        &ipsw_path,
        ecid,
        Some(&cache),
        mode,
        opts.verbose,
        &mut |event| {
            if let Event::Obliteration { status, detail } = &event {
                let strong = status == "confirmed" || status == "failed";
                if wipe.is_none() || strong {
                    wipe = Some((status.clone(), detail.clone()));
                }
            }
            restore_render(&bar, event, json);
        },
    );
    bar.finish_and_clear();

    // Report the wipe verdict regardless of the restore's overall outcome.
    report_wipe(json, wipe.as_ref());

    match result {
        Ok(_) => {
            record_restore_history(device, mode, wipe.as_ref(), true);
            // A device-reported wipe failure is fatal even if the restore itself
            // returned success — the key is not proven destroyed.
            if wipe.as_ref().map(|(s, _)| s.as_str()) == Some("failed") {
                return Err(Error::RestoreFailed {
                    status: -1,
                    log_tail: "device reported the encryption-key wipe FAILED".into(),
                });
            }
            say(json, completion_message(device, mode));
            Ok(())
        }
        Err(e) => {
            record_restore_history(device, mode, wipe.as_ref(), false);
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
fn record_restore_history(device: &Device, mode: Mode, wipe: Option<&(String, String)>, ok: bool) {
    use restorekit::history::{self, HistoryEntry};

    let obliteration = wipe.map(|(status, _)| status.clone());
    let wiped = matches!(obliteration.as_deref(), Some("confirmed") | Some("failed"));
    if !ok && !wiped {
        return;
    }
    let entry = HistoryEntry {
        serial_number: device.srnm.clone(),
        ecid: device.ecid_hex().unwrap_or_default(),
        model_identifier: device.identifier().map(str::to_string),
        name: device.display_name(),
        mode: match mode {
            Mode::Erase => "erase",
            Mode::Revive => "revive",
        }
        .to_string(),
        status: if ok { "restored" } else { "restore_failed" }.to_string(),
        timestamp_rfc3339: history::now_rfc3339(),
        obliteration,
    };
    if let Err(e) = history::record(&entry) {
        eprintln!("warning: could not record restore history: {e}");
    }
}

#[cfg(not(feature = "history"))]
fn record_restore_history(_: &Device, _: Mode, _: Option<&(String, String)>, _: bool) {}

/// Print the encryption-key obliteration verdict for an erase restore (nothing
/// for a revive, which does not obliterate). Suppressed in `--json` mode, where
/// it already went out as a machine-readable Obliteration event.
fn report_wipe(json: bool, wipe: Option<&(String, String)>) {
    let Some((status, detail)) = wipe else { return };
    match status.as_str() {
        "confirmed" => say(json, &format!("Encryption key obliterated (verified): {detail}")),
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
/// separately; a T2 revive preserves macOS; Apple Silicon restores fully.
fn completion_message(device: &Device, mode: Mode) -> &'static str {
    match (device.is_t2(), mode) {
        (true, Mode::Erase) => {
            "bridgeOS restore complete. Reinstall macOS via internet recovery (hold Cmd-R at boot)."
        }
        (true, Mode::Revive) => {
            "bridgeOS revive complete. macOS and your data are preserved; the Mac should boot normally."
        }
        _ => "Restore complete. The target should boot to Setup Assistant.",
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
    // A T2 erase restore only reinstalls bridgeOS — unlike Apple Silicon, it does
    // not put macOS back. Make that explicit so the user isn't left with a Mac
    // that won't boot. Use `revive` instead to update bridgeOS without erasing.
    if device.is_t2() {
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
            bar.println(format!(
                "Connection to the target dropped; retrying restore ({}/{max_attempts})...",
                attempt + 1
            ));
        }
        Event::Done => bar.set_position(100),
        _ => {}
    }
}
