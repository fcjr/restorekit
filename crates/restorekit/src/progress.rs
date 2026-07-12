use serde::Serialize;

use crate::device::Device;

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
        device: Device,
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
    /// One line of idevicerestore's log, streamed live for a log window.
    LogLine {
        level: i32,
        line: String,
    },
    /// Result of the encryption-key obliteration check after an erase restore.
    /// `status` is one of `confirmed`, `failed`, `unconfirmed`, `not_applicable`;
    /// `detail` carries the device log line that classified it, when any.
    Obliteration {
        status: String,
        detail: String,
    },
    /// The full checkpoint messages the device reported during the restore,
    /// captured for the history audit trail. `json` holds one compact-JSON plist
    /// per entry; `raw` holds the exact plist as XML (lossless). Emitted once
    /// near the end. These are the device's self-reported status messages, not
    /// Apple-signed attestations.
    Checkpoints {
        json: Vec<String>,
        raw: Vec<String>,
    },
    /// A restore attempt failed on a transient transport error (e.g. a dropped
    /// USB write mid-transfer) and is being retried from the top.
    RestoreRetrying {
        /// The attempt that just failed (1-based).
        attempt: u32,
        max_attempts: u32,
        message: String,
    },
    /// The embedded usbmuxd server is starting (Linux only).
    UsbmuxdStarting,
    /// Binding the WinUSB driver to Apple's DFU/recovery devices (Windows only).
    DriverSetupStarting,
    /// WinUSB was bound to a device class (Windows only).
    DriverBound {
        name: String,
    },
    Done,
}

pub type ProgressFn<'a> = &'a mut dyn FnMut(Event);
