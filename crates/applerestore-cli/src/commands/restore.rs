use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use applerestore::progress::Event;
use applerestore::restore::Mode;
use applerestore::{dfu, firmware, restore, DfuDevice, Error, Result};
use indicatif::{ProgressBar, ProgressStyle};

pub struct Opts {
    pub revive: bool,
    pub ipsw: Option<PathBuf>,
    pub os_version: Option<String>,
    pub identifier: Option<String>,
    pub yes: bool,
    pub cache_dir: Option<PathBuf>,
    pub json: bool,
}

/// Detect → resolve → download → restore the DFU device.
pub fn run(opts: Opts) -> Result<()> {
    let device = dfu::find_one()?;
    restore_device(&device, opts)
}

/// One-shot: trigger DFU (if the host can), wait for it, then restore.
pub fn run_oneshot(opts: Opts) -> Result<()> {
    if dfu::host_can_trigger_dfu() && dfu::list()?.is_empty() {
        println!("Triggering DFU mode on the target...");
        #[cfg(target_os = "macos")]
        dfu::vdm::enter_dfu(&mut |e| {
            if let Event::DfuTriggerStage { stage } = e {
                println!("  {stage}");
            }
        })?;
    } else if dfu::list()?.is_empty() {
        eprintln!("{}\n", dfu::manual_dfu_instructions());
    }

    println!("Waiting for a Mac in DFU mode...");
    let device = dfu::wait_for_dfu(Duration::from_secs(120))?;
    restore_device(&device, opts)
}

fn restore_device(device: &DfuDevice, opts: Opts) -> Result<()> {
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
        println!("Resolving firmware for {identifier}...");
        let fw = firmware::resolve(&identifier, opts.os_version.as_deref())?;
        println!("  macOS {} (build {})", fw.version, fw.build);

        let bar = ProgressBar::hidden();
        let path = firmware::download(&cache, &fw, &mut |e| download_render(&bar, e))?;
        bar.finish_and_clear();
        path
    };

    let mode = if opts.revive {
        Mode::Revive
    } else {
        Mode::Erase
    };

    if !confirm(device, mode, opts.yes)? {
        println!("Aborted.");
        return Ok(());
    }

    println!("Starting restore. Do not disconnect the target.");
    let bar = ProgressBar::new(100);
    bar.set_style(
        ProgressStyle::with_template("{msg:24} {bar:32.green/black} {percent:>3}%").unwrap(),
    );
    restore::restore(&ipsw_path, device.ecid, Some(&cache), mode, &mut |event| {
        restore_render(&bar, event, opts.json)
    })?;
    bar.finish_and_clear();
    println!("Restore complete. The target should boot to Setup Assistant.");
    Ok(())
}

fn confirm(device: &DfuDevice, mode: Mode, yes: bool) -> Result<bool> {
    if mode == Mode::Revive || yes {
        return Ok(true);
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

fn download_render(bar: &ProgressBar, event: Event) {
    match event {
        Event::CacheHit { path } => println!("Using cached firmware: {path}"),
        Event::DownloadResumed { received } => {
            println!("Resuming download from {:.1} GB...", received as f64 / 1e9)
        }
        Event::DownloadProgress { received, total } => {
            if bar.length() != Some(total) && total > 0 {
                bar.set_length(total);
                bar.set_style(
                    ProgressStyle::with_template("  {bytes}/{total_bytes} {bar:30} ({eta})")
                        .unwrap(),
                );
                bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
            }
            bar.set_position(received);
        }
        Event::Verifying => {
            bar.finish_and_clear();
            println!("Verifying checksum...");
        }
        _ => {}
    }
}

fn restore_render(bar: &ProgressBar, event: Event, json: bool) {
    if json {
        println!("{}", serde_json::to_string(&event).unwrap());
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
