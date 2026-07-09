use std::io::{IsTerminal, Write};
use std::time::Duration;

use restorekit::progress::Event;
use restorekit::{device, Device, DfuOutcome, DfuVia, Error, Result, Target};

use super::render;

/// Emit a DFU event: NDJSON in `--json` mode, human stage text otherwise.
pub(crate) fn emit_stage(json: bool, event: Event) {
    if json {
        render::emit_json(&event);
    } else if let Event::DfuTriggerStage { stage } = event {
        println!("  {stage}");
    }
}

/// Macs currently in DFU mode — the devices restorekit can act on.
fn dfu_devices() -> Result<Vec<Device>> {
    Ok(device::list()?
        .into_iter()
        .filter(|d| d.restorable())
        .collect())
}

/// Pick the target device: by ECID when given, otherwise the sole DFU device,
/// otherwise an interactive picker (errors in `--json` / non-TTY mode).
pub(crate) fn select_device(ecid: Option<u64>, json: bool) -> Result<Device> {
    match ecid {
        Some(e) => device::find(Target::Ecid(e)),
        None => select_from(dfu_devices()?, json),
    }
}

fn select_from(mut devices: Vec<Device>, json: bool) -> Result<Device> {
    match devices.len() {
        0 => Err(Error::NoDeviceFound),
        1 => Ok(devices.pop().unwrap()),
        n => {
            if json || !std::io::stdin().is_terminal() {
                return Err(Error::MultipleDevices(n));
            }
            println!("Found {n} Macs in DFU mode:\n");
            for (i, d) in devices.iter().enumerate() {
                println!(
                    "  [{}] {} (ECID {})",
                    i + 1,
                    d.display_name(),
                    d.ecid_hex().unwrap_or_default()
                );
            }
            loop {
                print!("\nSelect a device [1-{n}]: ");
                std::io::stdout().flush()?;
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if input.is_empty() {
                    // EOF: stdin closed mid-prompt.
                    return Err(Error::MultipleDevices(n));
                }
                match input.trim().parse::<usize>() {
                    Ok(i) if (1..=n).contains(&i) => return Ok(devices.swap_remove(i - 1)),
                    _ => println!("  Enter a number between 1 and {n}."),
                }
            }
        }
    }
}

/// Build the trigger route selector from the CLI flags.
fn via_from(dongle: Option<String>, ecid: Option<u64>, port: Option<i32>) -> DfuVia {
    if let Some(id) = dongle {
        DfuVia::Dongle(id)
    } else if let Some(e) = ecid {
        DfuVia::Ecid(e)
    } else if let Some(p) = port {
        DfuVia::Host(Some(p))
    } else {
        DfuVia::Auto
    }
}

/// `restorekit dfu` — trigger DFU on the cabled target, routing through a
/// dongle or the host, then wait for (and report) the Mac entering DFU.
pub fn enter(json: bool, dongle: Option<String>, ecid: Option<u64>, port: Option<i32>) -> Result<()> {
    let via = via_from(dongle, ecid, port);
    match restorekit::trigger_dfu(via, Duration::from_secs(30), &mut |e| emit_stage(json, e)) {
        Ok(DfuOutcome::Entered(device)) => {
            if json {
                emit_stage(true, Event::DeviceDetected { device });
            } else {
                println!("\nTarget is now in DFU mode: {}", device.display_name());
                println!("  ECID: {}", device.ecid_hex().unwrap_or_default());
            }
            Ok(())
        }
        Ok(DfuOutcome::Sent) => {
            if json {
                emit_stage(true, Event::Done);
            } else {
                println!(
                    "DFU trigger sent via dongle. No DFU device appeared on this host — if the \
                     target's USB data isn't cabled here, confirm on the Mac's screen."
                );
            }
            Ok(())
        }
        Err(Error::UnsupportedHost(_)) => {
            if !json {
                eprintln!("{}", restorekit::manual_dfu_instructions());
            }
            Err(Error::UnsupportedHost(
                "cannot trigger DFU on this host".into(),
            ))
        }
        Err(e) => Err(e),
    }
}

/// `restorekit reboot` — reboot the cabled target back to normal (dongle/host).
pub fn reboot(json: bool, dongle: Option<String>, ecid: Option<u64>, port: Option<i32>) -> Result<()> {
    let via = via_from(dongle, ecid, port);
    if !json {
        println!("Rebooting the target...");
    }
    match restorekit::dfu::reboot(via, &mut |e| emit_stage(json, e)) {
        Ok(()) => {
            if json {
                emit_stage(true, Event::Done);
            } else {
                println!("Done. The target should be booting normally.");
            }
            Ok(())
        }
        Err(Error::UnsupportedHost(_)) => {
            if !json {
                eprintln!("{}", restorekit::manual_dfu_instructions());
            }
            Err(Error::UnsupportedHost(
                "cannot control the target from this host".into(),
            ))
        }
        Err(e) => Err(e),
    }
}

/// Ensure a Mac is in DFU mode: return it if already there (with the interactive
/// picker when several are present and no ECID pins one), otherwise trigger
/// entry via a dongle or the host and wait. Shared by `restore`.
pub(crate) fn ensure_present(
    json: bool,
    timeout: Duration,
    dongle: Option<String>,
    ecid: Option<u64>,
) -> Result<Device> {
    // An explicit dongle always routes through it.
    if let Some(id) = dongle {
        return finish_trigger(json, DfuVia::Dongle(id), timeout, ecid);
    }
    // Already in DFU? Use it without re-triggering.
    if let Some(e) = ecid {
        let mut devices = device::list()?;
        device::identify(&mut devices);
        match devices.into_iter().find(|d| d.ecid == Some(e)) {
            Some(dev) if dev.in_dfu() => return Ok(dev),
            Some(dev) if !json => {
                println!(
                    "{} is in {} mode; putting it into DFU...",
                    dev.display_name(),
                    dev.mode
                );
            }
            _ => {}
        }
    } else {
        let present = dfu_devices()?;
        if !present.is_empty() {
            return select_from(present, json);
        }
        // Name a target visible in another mode so the user knows the trigger
        // has something to act on.
        if !json {
            if let Some(d) = device::list()?.iter().find(|d| {
                matches!(
                    d.mode,
                    restorekit::UsbMode::Booted | restorekit::UsbMode::Recovery
                )
            }) {
                println!("Detected {} ({} mode).", d.display_name(), d.mode);
            }
        }
    }

    let via = ecid.map(DfuVia::Ecid).unwrap_or(DfuVia::Auto);
    finish_trigger(json, via, timeout, ecid)
}

/// Run the library trigger and map its outcome for `restore`: the confirmed
/// device, a clear error when a dongle triggered a Mac whose USB data isn't
/// cabled here, or a wait for a manual DFU entry when nothing can trigger.
fn finish_trigger(json: bool, via: DfuVia, timeout: Duration, ecid: Option<u64>) -> Result<Device> {
    match restorekit::trigger_dfu(via, timeout, &mut |e| emit_stage(json, e)) {
        Ok(DfuOutcome::Entered(dev)) => Ok(dev),
        Ok(DfuOutcome::Sent) => Err(Error::Dongle(
            "triggered via dongle, but the target's USB data isn't cabled to this host; \
             connect its USB-C data here to restore"
                .into(),
        )),
        Err(Error::UnsupportedHost(_)) => {
            // No dongle and this host can't trigger: wait for a manual DFU entry.
            if !json {
                eprintln!("{}\n", restorekit::manual_dfu_instructions());
                println!("Waiting for the Mac to enter DFU mode...");
            }
            restorekit::dfu::wait_manual(ecid, timeout)
        }
        Err(e) => Err(e),
    }
}
