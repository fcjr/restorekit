use std::path::PathBuf;

use applerestore::progress::Event;
use applerestore::{dfu, firmware, Error, Result};
use indicatif::{ProgressBar, ProgressStyle};

/// Resolve and download firmware. With no `identifier`, detects the DFU device
/// and resolves firmware for it automatically.
pub fn run(
    identifier: Option<String>,
    os_version: Option<String>,
    cache_dir: Option<PathBuf>,
    json: bool,
) -> Result<()> {
    let identifier = match identifier {
        Some(id) => id,
        None => {
            let device = dfu::find_one()?;
            let id = device
                .identifier()
                .ok_or(Error::UnknownModel {
                    cpid: device.cpid,
                    bdid: device.bdid,
                })?
                .to_string();
            if !json {
                println!("Detected {} in DFU mode.", device.display_name());
            }
            id
        }
    };

    if !json {
        println!("Resolving firmware for {identifier}...");
    }
    let fw = firmware::resolve(&identifier, os_version.as_deref())?;

    if json {
        println!("{}", serde_json::to_string(&fw).unwrap());
    } else {
        println!(
            "  macOS {} (build {}), {:.1} GB",
            fw.version,
            fw.build,
            fw.size as f64 / 1e9
        );
    }

    let cache = match cache_dir {
        Some(d) => d,
        None => firmware::default_cache_dir()?,
    };

    let bar = ProgressBar::hidden();
    let path = firmware::download(&cache, &fw, &mut |event| {
        render(&bar, event, json);
    })?;
    bar.finish_and_clear();

    if !json {
        println!("Firmware ready: {}", path.display());
    }
    Ok(())
}

fn render(bar: &ProgressBar, event: Event, json: bool) {
    if json {
        println!("{}", serde_json::to_string(&event).unwrap());
        return;
    }
    match event {
        Event::CacheHit { path } => {
            println!("Already cached: {path}");
        }
        Event::DownloadResumed { received } => {
            println!("Resuming download from {:.1} GB...", received as f64 / 1e9);
        }
        Event::DownloadProgress { received, total } => {
            if bar.length().is_none() && total > 0 {
                bar.set_length(total);
                bar.set_style(
                    ProgressStyle::with_template(
                        "{bar:40.cyan/blue} {bytes}/{total_bytes} ({eta})",
                    )
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
