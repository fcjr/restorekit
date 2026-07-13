pub mod discovery;
pub mod trigger;

pub use discovery::{watch, Watch};
pub use trigger::{reboot, serial, trigger_dfu, wait_manual, DfuOutcome, DfuVia, SerialConsole};

#[cfg(target_os = "macos")]
pub mod vdm;

#[cfg(target_os = "macos")]
pub(crate) mod port;

/// Which cabled target a DFU trigger / reboot should act on.
///
/// On hosts with a single DFU-capable port, [`Auto`](DfuTarget::Auto) is all
/// that's ever needed; the other variants disambiguate when several targets are
/// cabled to DFU-capable ports.
#[derive(Debug, Clone, Default)]
pub enum DfuTarget {
    /// The sole (or first) DFU-capable port — the historical behavior.
    #[default]
    Auto,
    /// The Mac with this ECID, resolved to the port it's cabled to.
    Ecid(u64),
    /// A DFU-capable port named by its AppleHPM `RID` (see [`ports`] /
    /// `restorekit list`).
    Port(i32),
}

/// One of the host's USB-C ports, as reported by the port controller topology.
#[derive(Debug, Clone)]
pub struct HostPortInfo {
    /// The AppleHPM `RID` addressing this port — the value `--port` takes.
    pub rid: i32,
    /// Physical location label from the firmware (e.g. "left-back"), if any.
    pub location: Option<String>,
    /// Whether this is a DFU-capable port (one restorekit can trigger DFU on).
    pub dfu: bool,
}

/// Every USB-C port on this host and whether it's DFU-capable. Empty on hosts
/// whose topology can't be read (non-macOS, or unknown hardware).
pub fn ports() -> Vec<HostPortInfo> {
    #[cfg(target_os = "macos")]
    {
        port::all_ports()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// Human label of the host's DFU-capable port (e.g. "left-back"), if the host
/// can trigger DFU and the port topology is known. `None` on hosts that can't
/// trigger DFU or where the label couldn't be read.
pub fn dfu_port_label() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        port::dfu_port_location()
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

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
