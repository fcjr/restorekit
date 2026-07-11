use restorekit::dongle::{self, DongleTarget};
use restorekit::Result;

/// `restorekit dongle list`
pub fn list(json: bool) -> Result<()> {
    let dongles = dongle::list()?;

    if json {
        // Enrich each dongle with a best-effort live status for machine callers.
        let rows: Vec<_> = dongles
            .iter()
            .map(|d| {
                serde_json::json!({
                    "serial": d.serial,
                    "product": d.product,
                    "model": d.model,
                    "fw_version": d.fw_version().ok(),
                    "status": d.status().ok(),
                })
            })
            .collect();
        println!("{}", serde_json::to_string(&rows).unwrap());
        return Ok(());
    }

    if dongles.is_empty() {
        println!("No dongles found. Plug in a RecoverKit dongle.");
        return Ok(());
    }

    println!(
        "Found {} dongle{}:\n",
        dongles.len(),
        if dongles.len() == 1 { "" } else { "s" }
    );
    for d in &dongles {
        let fw = d.fw_version().unwrap_or_else(|_| "?".into());
        println!("  {} ({}, fw {})", d.serial, d.product, fw);
        match d.status() {
            Ok(s) if s.target_attached => {
                let orient = if s.polarity_cc2 { "flipped" } else { "normal" };
                println!("    target attached ({:?}, cable {orient})", s.pd_state);
            }
            Ok(_) => println!("    no target Mac attached"),
            Err(e) => println!("    (status unavailable: {e})"),
        }
        println!();
    }
    Ok(())
}

/// `restorekit dongle status`
pub fn status(json: bool, target: DongleTarget) -> Result<()> {
    let d = dongle::find(target)?;
    let s = d.status()?;

    if json {
        println!("{}", serde_json::json!({ "serial": d.serial, "status": s }));
        return Ok(());
    }

    println!("{} ({})", d.serial, d.product);
    println!(
        "  firmware: {}",
        d.fw_version().unwrap_or_else(|_| "?".into())
    );
    println!("  pd state: {:?}", s.pd_state);
    println!(
        "  target: {}",
        if s.target_attached {
            "attached"
        } else {
            "none"
        }
    );
    if s.target_attached {
        println!(
            "  cable orientation: {}",
            if s.polarity_cc2 {
                "CC2 (flipped)"
            } else {
                "CC1 (normal)"
            }
        );
    }
    Ok(())
}

/// `restorekit dongle bootsel`
pub fn bootsel(json: bool, target: DongleTarget) -> Result<()> {
    let d = dongle::find(target)?;
    d.bootsel()?;
    if json {
        println!(
            "{}",
            serde_json::json!({ "serial": d.serial, "bootsel": true })
        );
    } else {
        println!(
            "{} is rebooting into its USB bootloader; push new firmware with \
             picotool or by copying a UF2 onto the RPI-RP2 drive.",
            d.serial
        );
    }
    Ok(())
}

/// `restorekit dongle update [--file image.bin]` — stream new firmware over
/// the vendor interface (no bootloader mode, no RPI-RP2 drive). Without a
/// file, install the latest published release if it's newer.
pub fn update(json: bool, target: DongleTarget, file: Option<&std::path::Path>) -> Result<()> {
    let d = dongle::find(target)?;
    let handle = d.open()?;
    // Informational only — a version that can't be read (e.g. firmware too
    // broken to answer) must never block the update that would fix it; the
    // release check treats an unknown version as out of date.
    let current = handle.fw_version().unwrap_or_else(|_| "unknown".into());
    // Progress and narration go to stderr; stdout carries only the result.
    let image = match file {
        Some(path) => {
            if !json {
                eprintln!(
                    "Updating {} (fw {}) with {}...",
                    d.serial,
                    current,
                    path.display()
                );
            }
            std::fs::read(path)?
        }
        None => {
            let Some(release) = dongle::latest_firmware(d.model)? else {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({ "serial": d.serial, "fw_version": current, "updated": false, "error": "no published firmware releases" })
                    );
                } else {
                    println!("No published firmware releases for this model yet.");
                }
                return Ok(());
            };
            if !release.newer_than(&current) {
                if json {
                    println!(
                        "{}",
                        serde_json::json!({ "serial": d.serial, "fw_version": current, "latest": release.version, "updated": false })
                    );
                } else {
                    println!(
                        "{} firmware {} is up to date (latest release is {}).",
                        d.serial, current, release.version
                    );
                }
                return Ok(());
            }
            if !json {
                eprintln!(
                    "Updating {} from firmware {} to {} ({})...",
                    d.serial, current, release.version, release.tag
                );
            }
            release.download()?
        }
    };
    handle.update(&image, |staged, total| {
        if !json {
            eprint!("\r  staging: {}%", staged * 100 / total);
            use std::io::Write as _;
            let _ = std::io::stderr().flush();
        }
    })?;
    // The claimed interface is stale once the dongle reboots; release it
    // before polling for re-enumeration.
    drop(handle);
    if !json {
        eprintln!("\r  staged and verified; the dongle is rebooting to swap it in.");
    }

    // The swap takes a moment; report when it's back on the bus.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    let back = loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if dongle::list()?.iter().any(|x| x.serial == d.serial) {
            break true;
        }
        if std::time::Instant::now() >= deadline {
            break false;
        }
    };
    // Report the version actually running after the swap, when visible.
    let new_version = dongle::list()?
        .into_iter()
        .find(|x| x.serial == d.serial)
        .and_then(|x| x.fw_version().ok());
    if json {
        println!(
            "{}",
            serde_json::json!({ "serial": d.serial, "updated": true, "reenumerated": back, "fw_version": new_version })
        );
    } else if back {
        println!(
            "{} is back on firmware {}.",
            d.serial,
            new_version.as_deref().unwrap_or("?")
        );
    }
    if !back {
        return Err(restorekit::Error::Dongle(format!(
            "{} did not re-enumerate within 20s — check the board; the bootloader \
             reverts to the old firmware if the new one fails to boot",
            d.serial
        )));
    }
    Ok(())
}
