use std::path::PathBuf;

use indicatif::ProgressBar;
use restorekit::progress::Event;
use restorekit::{firmware, Error, Result};

use super::render;

/// Resolve and download firmware. With no `identifier`, detects the device (one
/// ready to restore, else a running iPhone or iPad) and resolves firmware for
/// it automatically.
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
            // A running iPhone or iPad isn't ready to restore, but its model is
            // known, which is all a download needs.
            let device = match super::dfu::select_device(ecid, json) {
                Ok(device) => device,
                Err(e) => super::restore::running_ios_target(ecid)?.ok_or(e)?,
            };
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
                    device.mode_label()
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
            "  {} {} (build {}), {:.1} GB",
            fw.os_name(),
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
