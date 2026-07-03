use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use applerestore::progress::Event;
use applerestore::restore::Mode;
use applerestore::{dfu, firmware, restore, DfuDevice, Error, Result};
use indicatif::{ProgressBar, ProgressStyle};

use super::render;

pub struct Opts {
    pub revive: bool,
    pub ipsw: Option<PathBuf>,
    pub os_version: Option<String>,
    pub identifier: Option<String>,
    pub yes: bool,
    pub cache_dir: Option<PathBuf>,
    pub json: bool,
    pub verbose: bool,
}

/// Detect → resolve → download → restore the DFU device.
pub fn run(opts: Opts) -> Result<()> {
    let device = dfu::find_one()?;
    restore_device(&device, opts)
}

/// One-shot: trigger DFU (if the host can), wait for it, then restore.
pub fn run_oneshot(opts: Opts) -> Result<()> {
    let device = super::dfu::ensure_present(opts.json, Duration::from_secs(120))?;
    restore_device(&device, opts)
}

fn restore_device(device: &DfuDevice, opts: Opts) -> Result<()> {
    let json = opts.json;
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
                cpid: device.cpid,
                bdid: device.bdid,
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
            println!("  macOS {} (build {})", fw.version, fw.build);
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
    restore::restore(
        &ipsw_path,
        device.ecid,
        Some(&cache),
        mode,
        opts.verbose,
        &mut |event| restore_render(&bar, event, json),
    )?;
    bar.finish_and_clear();
    say(
        json,
        "Restore complete. The target should boot to Setup Assistant.",
    );
    Ok(())
}

/// Print a human status line, suppressed in `--json` mode.
fn say(json: bool, msg: &str) {
    if !json {
        println!("{msg}");
    }
}

fn confirm(device: &DfuDevice, mode: Mode, yes: bool, json: bool) -> Result<bool> {
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
        device.ecid_hex()
    );
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
        Event::Done => bar.set_position(100),
        _ => {}
    }
}
