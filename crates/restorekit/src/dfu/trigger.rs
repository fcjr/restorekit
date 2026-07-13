//! Unified DFU trigger / reboot that routes through a dongle or the host.
//!
//! This is the single entry point callers should use to put a Mac into DFU (or
//! reboot it), regardless of *how* the Mac is reachable: a RecoverKit dongle
//! (works from any host OS) or the host's own Type-C port controller (Apple
//! Silicon macOS). The routing, retry, and wait-for-entry logic all live here
//! so the CLI, the desktop app, and `restore` share one implementation.

use std::time::{Duration, Instant};

use crate::device::{self, Device, Target};
use crate::dongle::{self, Dongle, DongleTarget};
use crate::error::{Error, Result};
use crate::progress::{Event, ProgressFn};

#[cfg(target_os = "macos")]
use super::vdm;
use super::{discovery, host_can_trigger_dfu, DfuTarget};

/// How to reach the target for a DFU trigger or reboot.
#[derive(Debug, Clone, Default)]
pub enum DfuVia {
    /// The sole connected dongle if one is present; otherwise the host's own
    /// DFU port. The zero-config default.
    #[default]
    Auto,
    /// A specific dongle by its id (USB serial, e.g. `DL-1A2B3C4D`).
    Dongle(String),
    /// The Mac with this ECID — via the dongle it's cabled to if it's behind
    /// one (resolved by USB topology), otherwise the host's own port.
    Ecid(u64),
    /// Force the host's electronic trigger (Apple Silicon macOS): a specific
    /// port by AppleHPM RID, or the sole DFU port when `None`.
    Host(Option<i32>),
}

/// Outcome of a DFU trigger.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum DfuOutcome {
    /// The Mac was observed entering (or already in) DFU on this host.
    Entered(Device),
    /// Triggered via a dongle, but the Mac's USB data isn't cabled to this
    /// host, so entry into DFU can't be confirmed here.
    Sent,
}

/// A resolved transport for a trigger/reboot.
enum Route {
    Host(DfuTarget),
    Dongle(Dongle),
}

/// Decide the transport from a [`DfuVia`] selector.
fn resolve(via: DfuVia) -> Result<Route> {
    match via {
        DfuVia::Dongle(id) => Ok(Route::Dongle(dongle::find(DongleTarget::Id(id))?)),
        DfuVia::Ecid(e) => match dongle::find_for_ecid(e) {
            Ok(d) => Ok(Route::Dongle(d)),
            Err(_) => Ok(Route::Host(DfuTarget::Ecid(e))),
        },
        DfuVia::Host(port) => Ok(Route::Host(match port {
            Some(rid) => DfuTarget::Port(rid),
            None => DfuTarget::Auto,
        })),
        DfuVia::Auto => {
            let mut ds = dongle::list()?;
            match ds.len() {
                0 => Ok(Route::Host(DfuTarget::Auto)),
                1 => Ok(Route::Dongle(ds.remove(0))),
                _ => Err(Error::MultipleDongles(
                    ds.iter()
                        .map(|d| d.serial.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                )),
            }
        }
    }
}

/// How many times to send the trigger before giving up.
const TRIGGER_ATTEMPTS: u32 = 3;
/// How long to wait for DFU enumeration before concluding the target booted
/// normally and re-sending the trigger.
const RETRY_AFTER: Duration = Duration::from_secs(10);

/// Trigger DFU on the selected target, routing through a dongle or the host,
/// retrying, and (when observable) waiting for the Mac to enter DFU.
///
/// Returns [`DfuOutcome::Entered`] with the device once it's in DFU on this
/// host, or [`DfuOutcome::Sent`] when a dongle fired the trigger but the Mac's
/// USB data isn't cabled here to confirm. Errors with [`Error::UnsupportedHost`]
/// when the route is the host and this host can't trigger electronically (and
/// no dongle is present) — callers may then show
/// [`manual_dfu_instructions`](super::manual_dfu_instructions).
pub fn trigger_dfu(via: DfuVia, timeout: Duration, progress: ProgressFn) -> Result<DfuOutcome> {
    let want_ecid = match &via {
        DfuVia::Ecid(e) => Some(*e),
        _ => None,
    };
    match resolve(via)? {
        Route::Dongle(d) => trigger_via_dongle(&d, want_ecid, timeout, progress),
        Route::Host(target) => trigger_via_host(&target, want_ecid, timeout, progress),
    }
}

/// Reboot the selected target back to normal, via a dongle or the host.
pub fn reboot(via: DfuVia, progress: ProgressFn) -> Result<()> {
    match resolve(via)? {
        Route::Dongle(d) => reboot_via_dongle(&d, progress),
        Route::Host(target) => host_reboot(&target, progress),
    }
}

/// Where a [`serial`] session's console shows up, so the caller knows what to
/// read from.
pub enum SerialConsole {
    /// Host VDM path: read `/dev/cu.debug-console`.
    Host,
    /// Bridged through a dongle: read this dongle's target-UART CDC port.
    Dongle(Dongle),
}

/// Put the selected target into serial-console mode — via a dongle if one is
/// present (its firmware bridges the target's SBU UART to a CDC port and keeps
/// it live across reboots) or the host's own port controller (the two-Apple-
/// Silicon-Macs case). Routes like [`trigger_dfu`]/[`reboot`]. Returns where the
/// console appears.
pub fn serial(via: DfuVia, progress: ProgressFn) -> Result<SerialConsole> {
    match resolve(via)? {
        Route::Host(target) => {
            host_serial(&target, progress)?;
            Ok(SerialConsole::Host)
        }
        Route::Dongle(d) => {
            progress(Event::DfuTriggerStage {
                stage: format!("putting the target into serial mode via dongle {}", d.serial),
            });
            d.serial()?;
            Ok(SerialConsole::Dongle(d))
        }
    }
}

/// Is the Mac cabled to `d` currently in DFU mode (visible on this host)?
fn dfu_attached(d: &Dongle) -> bool {
    matches!(d.attached_device(), Ok(Some(dev)) if dev.in_dfu())
}

fn reboot_via_dongle(d: &Dongle, progress: ProgressFn) -> Result<()> {
    // A booted Mac acts on the reboot VDM immediately. A Mac in the DFU bootrom
    // processes PD unreliably (~40% per send), so if the cabled Mac is visible
    // in DFU here, keep re-firing until it leaves DFU. Booting out of DFU can't
    // be done any other way over USB (the bootrom is waiting for firmware).
    if !dfu_attached(d) {
        return d.reboot();
    }
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut attempt = 0;
    loop {
        attempt += 1;
        progress(Event::DfuTriggerStage {
            stage: format!("rebooting the target out of DFU (attempt {attempt})"),
        });
        // Ignore a send error (e.g. a slow reply) — the VDM may still have gone
        // out; the mode check below is the source of truth.
        let _ = d.reboot();
        // Wait past the re-establish USB blip, then check whether it actually
        // left DFU rather than just flickering during the CC cycle.
        std::thread::sleep(Duration::from_secs(8));
        if !dfu_attached(d) {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(Error::Dongle(
                "target stayed in DFU after repeated reboots; hold its power \
                 button ~10s or run a restore to exit DFU"
                    .into(),
            ));
        }
    }
}

fn trigger_via_host(
    target: &DfuTarget,
    want_ecid: Option<u64>,
    timeout: Duration,
    progress: ProgressFn,
) -> Result<DfuOutcome> {
    if !host_can_trigger_dfu() {
        return Err(Error::UnsupportedHost(
            "cannot trigger DFU on this host".into(),
        ));
    }
    // Subscribe before triggering so we catch the target as it enters DFU.
    let mut watch = discovery::watch()?;
    let deadline = Instant::now() + timeout;
    for attempt in 1..=TRIGGER_ATTEMPTS {
        host_trigger(target, progress)?;
        let slice = attempt_slice(attempt, deadline);
        match wait_entered(&mut watch, want_ecid, slice) {
            Ok(dev) => return Ok(DfuOutcome::Entered(dev)),
            Err(Error::WaitTimeout) if attempt < TRIGGER_ATTEMPTS && Instant::now() < deadline => {}
            Err(e) => return Err(e),
        }
    }
    Err(Error::WaitTimeout)
}

fn trigger_via_dongle(
    d: &Dongle,
    want_ecid: Option<u64>,
    timeout: Duration,
    progress: ProgressFn,
) -> Result<DfuOutcome> {
    let mut watch = discovery::watch()?;
    let deadline = Instant::now() + timeout;
    for attempt in 1..=TRIGGER_ATTEMPTS {
        progress(Event::DfuTriggerStage {
            stage: if attempt == 1 {
                format!("sending DFU trigger via dongle {}", d.serial)
            } else {
                format!(
                    "target didn't enter DFU; re-sending via dongle ({attempt}/{TRIGGER_ATTEMPTS})"
                )
            },
        });
        d.dfu()?;
        progress(Event::DfuTriggerStage {
            stage: "waiting for the target to enter DFU mode".into(),
        });
        let slice = attempt_slice(attempt, deadline);
        match wait_entered(&mut watch, want_ecid, slice) {
            Ok(dev) => return Ok(DfuOutcome::Entered(dev)),
            // The dongle fired; the Mac just isn't visible on this host to
            // confirm (its D+/D- aren't cabled here) — not an error.
            Err(Error::WaitTimeout) if attempt < TRIGGER_ATTEMPTS && Instant::now() < deadline => {}
            Err(Error::WaitTimeout) => return Ok(DfuOutcome::Sent),
            Err(e) => return Err(e),
        }
    }
    Ok(DfuOutcome::Sent)
}

/// The wait budget for one attempt: a short slice for early attempts (so we
/// re-send if the target booted normally), the whole remainder for the last.
fn attempt_slice(attempt: u32, deadline: Instant) -> Duration {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if attempt < TRIGGER_ATTEMPTS {
        RETRY_AFTER.min(remaining)
    } else {
        remaining
    }
}

/// Wait for a Mac in DFU (optionally matching `want_ecid`): return one already
/// present, else block on the hotplug watch for one entering.
fn wait_entered(
    watch: &mut discovery::Watch,
    want_ecid: Option<u64>,
    timeout: Duration,
) -> Result<Device> {
    let matches = |d: &Device| d.in_dfu() && want_ecid.is_none_or(|e| d.ecid == Some(e));

    if let Some(d) = device::list()?.into_iter().find(&matches) {
        return Ok(d);
    }
    let deadline = Instant::now() + timeout;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Error::WaitTimeout);
        }
        let dev = watch.wait(remaining)?;
        if matches(&dev) {
            return Ok(dev);
        }
    }
}

fn host_trigger(target: &DfuTarget, progress: ProgressFn) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        vdm::enter_dfu(target, progress)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (target, progress);
        Err(Error::UnsupportedHost(
            "cannot trigger DFU on this host".into(),
        ))
    }
}

fn host_reboot(target: &DfuTarget, progress: ProgressFn) -> Result<()> {
    if !host_can_trigger_dfu() {
        return Err(Error::UnsupportedHost(
            "cannot control the target from this host".into(),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        vdm::reboot(target, progress)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (target, progress);
        Ok(())
    }
}

fn host_serial(target: &DfuTarget, progress: ProgressFn) -> Result<()> {
    if !host_can_trigger_dfu() {
        return Err(Error::UnsupportedHost(
            "serial mode needs an Apple Silicon macOS host".into(),
        ));
    }
    #[cfg(target_os = "macos")]
    {
        vdm::serial(target, progress)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (target, progress);
        Ok(())
    }
}

/// Wait for a manually-triggered DFU entry (host can't trigger, no dongle).
/// Callers show [`manual_dfu_instructions`](super::manual_dfu_instructions)
/// first, then block here for the user to perform the key combo.
pub fn wait_manual(want_ecid: Option<u64>, timeout: Duration) -> Result<Device> {
    match want_ecid {
        Some(e) => device::wait_where(timeout, |d| d.in_dfu() && d.ecid == Some(e)),
        None => device::wait(Target::One, timeout),
    }
}
