use serde::Serialize;

use crate::dfu::DfuDevice;

/// Progress events emitted by long-running library operations.
///
/// The library never prints or prompts; frontends (CLI, GUI) render these.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// A stage of the VDM DFU trigger (host-side USB-PD dance).
    DfuTriggerStage {
        stage: String,
    },
    /// A Mac in DFU mode was detected.
    DeviceDetected {
        device: DfuDevice,
    },
    /// Firmware metadata was resolved for the target.
    FirmwareResolved {
        identifier: String,
        version: String,
        build: String,
        size: u64,
        url: String,
    },
    /// The requested firmware is already in the cache.
    CacheHit {
        path: String,
    },
    /// A previous partial download is being resumed.
    DownloadResumed {
        received: u64,
    },
    DownloadProgress {
        received: u64,
        total: u64,
    },
    /// Verifying the downloaded file's checksum.
    Verifying,
    /// A restore step reported by idevicerestore.
    RestoreStep {
        step: u32,
        name: String,
        progress: f32,
    },
    /// The embedded usbmuxd server is starting (Linux only).
    UsbmuxdStarting,
    Done,
}

pub type ProgressFn<'a> = &'a mut dyn FnMut(Event);
