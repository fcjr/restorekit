use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no Mac in DFU mode found")]
    NoDeviceFound,

    #[error("multiple Macs in DFU mode found ({0}); select one with --ecid (see `restorekit status`) or disconnect the others")]
    MultipleDevices(usize),

    #[error("no Mac in DFU mode with ECID {0:#x}")]
    EcidNotFound(u64),

    #[error("timed out waiting for a Mac to appear in DFU mode")]
    WaitTimeout,

    #[error("DFU triggering requires an Apple Silicon Mac host running macOS: {0}")]
    UnsupportedHost(String),

    #[error("this operation requires root; re-run with sudo")]
    NeedsRoot,

    #[error("USB error: {0}")]
    Usb(String),

    #[error("VDM error: {0}")]
    Vdm(String),

    #[error("unknown Mac model (CPID:{cpid:04x} BDID:{bdid:02x}); no firmware mapping")]
    UnknownModel { cpid: u16, bdid: u8 },

    #[error("firmware resolution failed: {0}")]
    FirmwareResolution(String),

    #[error("no signed firmware found for {identifier}{version}")]
    NoFirmwareFound { identifier: String, version: String },

    #[error("download failed: {0}")]
    Download(String),

    #[error("checksum mismatch for {path}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error(
        "idevicerestore not found on PATH; install it (macOS: `brew install idevicerestore`, \
         Debian/Ubuntu: `sudo apt install idevicerestore`) or pass --idevicerestore-path"
    )]
    IdevicerestoreNotFound,

    #[error("failed to start usbmuxd: {0}")]
    UsbmuxdFailed(String),

    #[error("WinUSB driver setup failed: {0}")]
    DriverInstall(String),

    #[error("restore failed (exit {status}); last output:\n{log_tail}")]
    RestoreFailed { status: i32, log_tail: String },

    #[error("could not determine home directory")]
    NoHomeDir,

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
