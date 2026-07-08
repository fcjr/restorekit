use std::time::{Duration, Instant};

use crate::device::{self, Device, APPLE_VID, DFU_PID};
use crate::error::{Error, Result};

/// The restorable subset of the bus: Macs in DFU mode with a readable identity.
fn restorable() -> Result<Vec<Device>> {
    Ok(device::list()?
        .into_iter()
        .filter(|d| d.restorable())
        .collect())
}

/// Watches for a Mac *entering* DFU mode, as opposed to one already there.
///
/// Create it with [`watch`] *before* triggering DFU, so the arrival cannot slip
/// past between checking and waiting; Macs already in DFU at that point are
/// never returned.
pub struct Watch {
    events: std::pin::Pin<Box<nusb::hotplug::HotplugWatch>>,
    /// ECIDs already in DFU when the watch started.
    already_present: Vec<u64>,
}

/// Start watching for a Mac to enter DFU mode. See [`Watch`].
pub fn watch() -> Result<Watch> {
    // Subscribe first, snapshot second: a device arriving in between is then
    // both queued as an event and in the snapshot, so it is (correctly, since
    // it predates the caller's DFU trigger) treated as already present.
    let events = nusb::watch_devices().map_err(|e| Error::Usb(e.to_string()))?;
    let already_present = restorable()?.iter().filter_map(|d| d.ecid).collect();
    Ok(Watch {
        events: Box::pin(events),
        already_present,
    })
}

impl Watch {
    /// Block until a Mac newly enters DFU mode, or the timeout elapses.
    ///
    /// Takes `&mut self` so a caller can re-trigger DFU and wait again on the
    /// same watch; Macs that entered DFU during an earlier wait are returned
    /// then, not silently skipped.
    pub fn wait(&mut self, timeout: Duration) -> Result<Device> {
        use nusb::hotplug::HotplugEvent;

        let deadline = Instant::now() + timeout;
        loop {
            // Drain queued hotplug events: the OS telling us a device arrived.
            while let Some(event) = self.next_event() {
                if let HotplugEvent::Connected(info) = event {
                    if info.vendor_id() != APPLE_VID || info.product_id() != DFU_PID {
                        continue;
                    }
                    let dev = device::from_usb(&info);
                    if let Some(ecid) = dev.ecid {
                        if !self.already_present.contains(&ecid) {
                            return Ok(dev);
                        }
                    }
                }
            }
            // Backstop in case a platform's hotplug stream drops an event:
            // diff the bus against the snapshot.
            if let Some(dev) = restorable()?
                .into_iter()
                .find(|d| d.ecid.is_some_and(|e| !self.already_present.contains(&e)))
            {
                return Ok(dev);
            }
            if Instant::now() >= deadline {
                return Err(Error::WaitTimeout);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Pop the next queued hotplug event without blocking.
    fn next_event(&mut self) -> Option<nusb::hotplug::HotplugEvent> {
        use futures_core::Stream;
        use std::task::{Context, Poll, Waker};

        let mut cx = Context::from_waker(Waker::noop());
        match self.events.as_mut().poll_next(&mut cx) {
            Poll::Ready(event) => event,
            Poll::Pending => None,
        }
    }
}
