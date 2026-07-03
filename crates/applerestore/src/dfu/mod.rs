pub mod discovery;

pub use discovery::{find_one, list, parse_serial, wait_for_dfu, DfuDevice, APPLE_VID, DFU_PID};

#[cfg(target_os = "macos")]
pub mod vdm;

/// Whether this host can trigger DFU mode on a target over USB-PD.
///
/// Only Apple Silicon Macs running macOS can drive the Type-C port controller.
pub fn host_can_trigger_dfu() -> bool {
    cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")
}

/// Manual DFU-entry instructions for hosts that can't trigger it electronically.
pub fn manual_dfu_instructions() -> &'static str {
    "This host cannot trigger DFU mode electronically (requires an Apple Silicon \
Mac running macOS). Put the target Mac into DFU mode manually:\n\
\n\
  1. Connect the target to this host with a USB-C cable using the target's DFU \
port:\n\
       - MacBook (Air/13\" Pro): the port nearest the screen on the left side.\n\
       - 14\"/16\" MacBook Pro: the port next to MagSafe.\n\
       - Mac mini/Studio: the port nearest the power button/HDMI.\n\
       - iMac: the port nearest the edge.\n\
  2. Disconnect the target from power.\n\
  3. Apple silicon laptop: hold the power button, then while holding it, connect \
power and keep holding ~10s.\n\
     Desktop: unplug power 10s, then press and hold the power button while \
reconnecting power.\n\
  4. Release. The screen stays black in DFU mode. Re-run this command to detect \
it."
}
