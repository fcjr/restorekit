use std::time::Duration;

use applerestore::progress::Event;
use applerestore::{dfu, Error, Result};

use super::render;

/// Emit an event: NDJSON in `--json` mode, human text otherwise.
fn emit(json: bool, event: Event) {
    if json {
        render::emit_json(&event);
    } else if let Event::DfuTriggerStage { stage } = event {
        println!("  {stage}");
    }
}

/// Trigger DFU mode on the cabled target, then wait for it to appear.
pub fn enter(json: bool) -> Result<()> {
    if !dfu::host_can_trigger_dfu() {
        if !json {
            eprintln!("{}", dfu::manual_dfu_instructions());
        }
        return Err(Error::UnsupportedHost(
            "cannot trigger DFU on this host".into(),
        ));
    }

    if !json {
        println!("Triggering DFU mode on the target...");
    }
    #[cfg(target_os = "macos")]
    dfu::vdm::enter_dfu(&mut |e| emit(json, e))?;

    if !json {
        println!("Waiting for the target to enter DFU mode...");
    }
    let device = dfu::wait_for_dfu(Duration::from_secs(20))?;

    if json {
        emit(true, Event::DeviceDetected { device });
    } else {
        println!("\nTarget is now in DFU mode: {}", device.display_name());
        println!("  ECID: {}", device.ecid_hex());
    }
    Ok(())
}

/// Reboot the cabled target out of DFU / back to normal.
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
    dfu::vdm::reboot(&mut |e| emit(json, e))?;

    if json {
        emit(true, Event::Done);
    } else {
        println!("Done. The target should be booting normally.");
    }
    Ok(())
}
