use std::io::{IsTerminal, Write};
use std::time::Duration;

use restorekit::progress::Event;
use restorekit::{device, dfu, Device, Error, Result, Target};

use super::render;

/// Emit a DFU event: NDJSON in `--json` mode, human stage text otherwise.
pub(crate) fn emit_stage(json: bool, event: Event) {
    if json {
        render::emit_json(&event);
    } else if let Event::DfuTriggerStage { stage } = event {
        println!("  {stage}");
    }
}

/// Send the DFU-trigger VDM sequence, emitting stages. The caller must have
/// already confirmed this host can trigger DFU.
fn trigger_dfu(json: bool) -> Result<()> {
    if !json {
        println!("Triggering DFU mode on the target...");
    }
    #[cfg(target_os = "macos")]
    dfu::vdm::enter_dfu(&mut |e| emit_stage(json, e))?;
    Ok(())
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

/// Trigger DFU electronically if the host can, else print the manual key-combo
/// instructions so the user can put the target into DFU by hand.
fn trigger_or_instruct(json: bool) -> Result<()> {
    if restorekit::host_can_trigger_dfu() {
        trigger_dfu(json)
    } else {
        if !json {
            eprintln!("{}\n", restorekit::manual_dfu_instructions());
        }
        Ok(())
    }
}

/// Ensure a Mac is in DFU mode: if the target isn't there yet, trigger DFU
/// electronically when this host can (otherwise print manual instructions),
/// then wait up to `timeout`. Shared by `restore` and the `dfu` command.
pub(crate) fn ensure_present(json: bool, timeout: Duration, ecid: Option<u64>) -> Result<Device> {
    // Targeting a specific machine: return it if already in DFU, else trigger
    // and wait for that exact ECID. `identify` fills in booted Macs' ECIDs so
    // they can be matched before they enter DFU.
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
        trigger_or_instruct(json)?;
        if !json {
            println!("Waiting for the Mac with ECID {e:#x} in DFU mode...");
        }
        return device::wait_where(timeout, |d| d.in_dfu() && d.ecid == Some(e));
    }

    let present = dfu_devices()?;
    if !present.is_empty() {
        return select_from(present, json);
    }
    // Nothing restorable yet; if a Mac is visible in another mode (booted,
    // recovery), name it so the user knows the trigger has a target.
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
    trigger_or_instruct(json)?;
    if !json {
        println!("Waiting for a Mac in DFU mode...");
    }
    device::wait(Target::One, timeout)
}

/// `restorekit dfu` — trigger DFU on the cabled target, then wait for it.
pub fn enter(json: bool) -> Result<()> {
    if !restorekit::host_can_trigger_dfu() {
        if !json {
            eprintln!("{}", restorekit::manual_dfu_instructions());
        }
        return Err(Error::UnsupportedHost(
            "cannot trigger DFU on this host".into(),
        ));
    }

    // Subscribe before triggering so we report the device that just entered,
    // not one that was connected all along.
    let watch = dfu::watch()?;

    trigger_dfu(json)?;

    if !json {
        println!("Waiting for the target to enter DFU mode...");
    }
    let device = watch.wait(Duration::from_secs(20))?;

    if json {
        emit_stage(true, Event::DeviceDetected { device });
    } else {
        println!("\nTarget is now in DFU mode: {}", device.display_name());
        println!("  ECID: {}", device.ecid_hex().unwrap_or_default());
    }
    Ok(())
}

/// `restorekit reboot` — reboot the cabled target out of DFU / back to normal.
pub fn reboot(json: bool) -> Result<()> {
    if !restorekit::host_can_trigger_dfu() {
        if !json {
            eprintln!("{}", restorekit::manual_dfu_instructions());
        }
        return Err(Error::UnsupportedHost(
            "cannot control the target from this host".into(),
        ));
    }

    if !json {
        println!("Rebooting the target...");
    }
    #[cfg(target_os = "macos")]
    dfu::vdm::reboot(&mut |e| emit_stage(json, e))?;

    if json {
        emit_stage(true, Event::Done);
    } else {
        println!("Done. The target should be booting normally.");
    }
    Ok(())
}
