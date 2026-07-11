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
        println!("  {} ({})", d.serial, d.product);
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
             picotool (or `just fw-flash-full`).",
            d.serial
        );
    }
    Ok(())
}

/// `restorekit dongle update <image.bin>` — stream new firmware over the
/// vendor interface (no bootloader mode, no RPI-RP2 drive).
pub fn update(json: bool, target: DongleTarget, file: &std::path::Path) -> Result<()> {
    let image = std::fs::read(file)?;
    let d = dongle::find(target)?;
    if !json {
        println!(
            "Updating {} with {} ({} KiB)...",
            d.serial,
            file.display(),
            image.len().div_ceil(1024)
        );
    }
    d.open()?.update(&image, |staged, total| {
        if !json {
            print!("\r  staging: {}%", staged * 100 / total);
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
        }
    })?;
    if !json {
        println!("\r  staged and verified; the dongle is rebooting to swap it in.");
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
    if json {
        println!(
            "{}",
            serde_json::json!({ "serial": d.serial, "updated": true, "reenumerated": back })
        );
    } else if back {
        println!("{} is back on the new firmware.", d.serial);
    } else {
        println!(
            "{} did not re-enumerate within 20s — check the board; the bootloader \
             reverts to the old firmware if the new one fails to boot.",
            d.serial
        );
    }
    Ok(())
}
