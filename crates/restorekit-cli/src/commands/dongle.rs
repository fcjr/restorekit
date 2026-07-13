use restorekit::dongle::{self, DongleTarget};
use restorekit::Result;

/// `restorekit dongle list [ID]` — every connected dongle in full detail, or
/// just the one selected.
pub fn list(json: bool, target: Option<DongleTarget>) -> Result<()> {
    let dongles = match target {
        Some(t) => vec![dongle::find(t)?],
        None => dongle::list()?,
    };

    if json {
        // Enrich each dongle with best-effort live detail for machine callers.
        let rows: Vec<_> = dongles
            .iter()
            .map(|d| {
                serde_json::json!({
                    "serial": d.serial,
                    "product": d.product,
                    "model": d.model,
                    "fw_version": d.fw_version().ok(),
                    "status": d.status().ok(),
                    "target": d.attached_device().ok().flatten(),
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

    for d in &dongles {
        let fw = d.fw_version().unwrap_or_else(|_| "?".into());
        println!("{} ({}, fw {})", d.serial, d.product, fw);
        match d.status() {
            Ok(s) => {
                println!("  pd state: {:?}", s.pd_state);
                if s.target_attached {
                    let target = match d.attached_device().ok().flatten() {
                        Some(dev) => format!("{} [{} mode]", dev.display_name(), dev.mode),
                        None => "attached (its USB isn't visible to this host)".into(),
                    };
                    println!("  target: {target}");
                    println!(
                        "  cable orientation: {}",
                        if s.polarity_cc2 {
                            "CC2 (flipped)"
                        } else {
                            "CC1 (normal)"
                        }
                    );
                } else {
                    println!("  target: none");
                }
            }
            Err(e) => println!("  (status unavailable: {e})"),
        }
        println!();
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

/// `restorekit dongle update [--file image.bin] [--check]` — stream new
/// firmware over the vendor interface (no bootloader mode, no RPI-RP2 drive).
/// Without a file, install the latest published release if it's newer; with
/// `--check`, only report whether one is available.
pub fn update(
    json: bool,
    target: DongleTarget,
    file: Option<&std::path::Path>,
    check: bool,
) -> Result<()> {
    let d = dongle::find(target)?;
    let handle = d.open()?;
    // Informational only — a version that can't be read (e.g. firmware too
    // broken to answer) must never block the update that would fix it; the
    // release check treats an unknown version as out of date.
    let current = handle.fw_version().unwrap_or_else(|_| "unknown".into());
    if check {
        let (latest, available) = match dongle::latest_firmware(d.model)? {
            Some(r) => {
                let available = r.newer_than(&current);
                (Some(r.version), available)
            }
            None => (None, false),
        };
        if json {
            println!(
                "{}",
                serde_json::json!({ "serial": d.serial, "fw_version": current, "latest": latest, "update_available": available })
            );
        } else {
            match latest {
                Some(latest) if available => println!(
                    "{}: update available, firmware {current} -> {latest} \
                     (run `restorekit dongle update` to install)",
                    d.serial
                ),
                Some(latest) => println!(
                    "{}: firmware {current} is up to date (latest release is {latest}).",
                    d.serial
                ),
                None => println!("No published firmware releases for this model yet."),
            }
        }
        return Ok(());
    }
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
        if json {
            // NDJSON progress, matching the erase/download event stream style.
            println!(
                "{}",
                serde_json::json!({ "event": "fw_staging", "serial": d.serial, "staged": staged, "total": total })
            );
        } else {
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

/// `restorekit dongle console` — print the dongle's serial-console tty paths.
/// The dongle exposes two CDC ports: the control console (CDC0) and the
/// target's UART bridged over SBU (CDC1, live after `serial`).
pub fn console(json: bool, target: DongleTarget) -> Result<()> {
    let d = dongle::find(target)?;
    let paths = serial_ttys(&d);
    let (control, target_serial) = (paths.first(), paths.get(1));

    if json {
        println!(
            "{}",
            serde_json::json!({ "serial": d.serial, "control": control, "target_serial": target_serial })
        );
        return Ok(());
    }
    let Some(control) = control else {
        return Err(restorekit::Error::Dongle(format!(
            "no serial console tty found for {} — is its USB data on this host? \
             (on Windows, look for the dongle's first COM port instead)",
            d.serial
        )));
    };
    println!("control console: {control}");
    if let Some(ts) = target_serial {
        println!("target serial:   {ts}");
    }
    println!("tip: screen {control}  (Ctrl-A K to exit)");
    Ok(())
}

/// The dongle's CDC tty paths, sorted so `[0]` is the control console (CDC0) and
/// `[1]` is the target-UART bridge (CDC1, live after `serial`). The OS embeds the
/// USB serial in the tty name but mangles it (macOS: `cu.usbmodemDL_5F4175361`;
/// Linux by-id keeps it intact), so compare with non-alphanumerics stripped.
#[cfg(unix)]
pub(crate) fn serial_ttys(d: &restorekit::Dongle) -> Vec<String> {
    let key = normalize(&d.serial);
    let mut paths: Vec<String> = Vec::new();
    for dir in ["/dev", "/dev/serial/by-id"] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let is_serial_dev = name.starts_with("cu.") || dir.ends_with("by-id");
            if is_serial_dev && normalize(&name).contains(&key) {
                paths.push(entry.path().to_string_lossy().into_owned());
            }
        }
    }
    // The control console enumerates before the target bridge, and the OS
    // suffixes (interface number) sort the same way.
    paths.sort();
    paths
}

/// Non-Unix (Windows) variant: enumerate serial ports and match the dongle by
/// its USB serial number, since COM-port names don't embed it. Same sort order —
/// `[0]` control console, `[1]` target-UART bridge — as the COM numbers follow
/// the CDC interface order.
#[cfg(not(unix))]
pub(crate) fn serial_ttys(d: &restorekit::Dongle) -> Vec<String> {
    let key = normalize(&d.serial);
    let mut paths: Vec<String> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| match p.port_type {
            serialport::SerialPortType::UsbPort(usb) => {
                let sn = normalize(usb.serial_number.as_deref()?);
                (sn.contains(&key) || key.contains(&sn)).then_some(p.port_name)
            }
            _ => None,
        })
        .collect();
    paths.sort();
    paths
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase()
}
