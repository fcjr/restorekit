use std::path::PathBuf;

use indicatif::ProgressBar;
use restorekit::progress::Event;
use restorekit::{firmware, Error, Result};

use super::render;

/// Resolve and download firmware. With no `identifier`, detects the DFU device
/// and resolves firmware for it automatically.
pub fn run(
    identifier: Option<String>,
    os_version: Option<String>,
    ecid: Option<u64>,
    cache_dir: Option<PathBuf>,
    json: bool,
) -> Result<()> {
    let identifier = match identifier {
        Some(id) => id,
        None => {
            let device = super::dfu::select_device(ecid, json)?;
            let identity = device
                .identity
                .as_ref()
                .expect("selected device carries an identity");
            let id = device
                .identifier()
                .ok_or(Error::UnknownModel {
                    cpid: identity.cpid,
                    bdid: identity.bdid,
                })?
                .to_string();
            if !json {
                println!(
                    "Detected {} in {} mode.",
                    device.display_name(),
                    device.mode
                );
            }
            id
        }
    };

    if !json {
        println!("Resolving firmware for {identifier}...");
    }
    let fw = firmware::resolve(&identifier, os_version.as_deref())?;

    if json {
        render::emit_json(&Event::FirmwareResolved {
            identifier: fw.identifier.clone(),
            version: fw.version.clone(),
            build: fw.build.clone(),
            size: fw.size,
            url: fw.url.clone(),
        });
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
        render::download(&bar, event, json)
    })?;
    bar.finish_and_clear();

    if !json {
        println!("Firmware ready: {}", path.display());
    }
    Ok(())
}
