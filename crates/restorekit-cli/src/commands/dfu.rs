use std::time::Duration;

use restorekit::progress::Event;
use restorekit::{dfu, DfuDevice, Error, Result};

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

/// Ensure a Mac is in DFU mode: if none is present, trigger it electronically
/// when this host can, otherwise print manual instructions. Then wait up to
/// `timeout` for the device to appear. Shared by `run` and the `dfu` command.
pub(crate) fn ensure_present(json: bool, timeout: Duration) -> Result<DfuDevice> {
    if dfu::list()?.is_empty() {
        if dfu::host_can_trigger_dfu() {
            trigger_dfu(json)?;
        } else if !json {
            eprintln!("{}\n", dfu::manual_dfu_instructions());
        }
    }
    if !json {
        println!("Waiting for a Mac in DFU mode...");
    }
    dfu::wait_for_dfu(timeout)
}

/// `restorekit dfu` — trigger DFU on the cabled target, then wait for it.
pub fn enter(json: bool) -> Result<()> {
    if !dfu::host_can_trigger_dfu() {
        if !json {
            eprintln!("{}", dfu::manual_dfu_instructions());
        }
        return Err(Error::UnsupportedHost(
            "cannot trigger DFU on this host".into(),
        ));
    }

    trigger_dfu(json)?;

    if !json {
        println!("Waiting for the target to enter DFU mode...");
    }
    let device = dfu::wait_for_dfu(Duration::from_secs(20))?;

    if json {
        emit_stage(true, Event::DeviceDetected { device });
    } else {
        println!("\nTarget is now in DFU mode: {}", device.display_name());
        println!("  ECID: {}", device.ecid_hex());
    }
    Ok(())
}

/// `restorekit reboot` — reboot the cabled target out of DFU / back to normal.
pub fn reboot(json: bool) -> Result<()> {
    if !dfu::host_can_trigger_dfu() {
        if !json {
            eprintln!("{}", dfu::manual_dfu_instructions());
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
