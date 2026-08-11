use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("no Mac in DFU mode found")]
    NoDeviceFound,

    #[error("multiple Macs in DFU mode found ({0}); select one with --ecid (see `restorekit status`) or disconnect the others")]
    MultipleDevices(usize),

    #[error("no Mac in DFU mode with ECID {0:#x}")]
    EcidNotFound(u64),

    #[error("no Mac with ECID {0:#x} is connected to this host")]
    EcidNotConnected(u64),

    #[error(
        "the Mac with ECID {0:#x} is not on the DFU port; move its cable to the DFU port and retry"
    )]
    EcidNotOnDfuPort(u64),

    #[error("no DFU-capable port with RID {0} on this host (see `restorekit list`)")]
    DfuPortNotFound(i32),

    #[error("timed out waiting for a Mac to appear in DFU mode")]
    WaitTimeout,

    #[error("DFU triggering requires an Apple Silicon Mac host running macOS: {0}")]
    UnsupportedHost(String),

    #[error("this operation requires root; re-run with sudo")]
    NeedsRoot,

    #[error("USB error: {0}")]
    Usb(String),

    #[error("USB permission denied: {0}\n\n{hint}", hint = usb_permission_hint())]
    UsbPermission(String),

    #[error("VDM error: {0}")]
    Vdm(String),

    #[error("no RecoverKit dongle found")]
    NoDongle,

    #[error("multiple dongles found ({0}); specify one by serial")]
    MultipleDongles(String),

    #[error("no target Mac is attached to the dongle")]
    DongleNoTarget,

    #[error("dongle: {0}")]
    Dongle(String),

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

    #[error("history database error: {0}")]
    Database(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
}

/// nusb reports EACCES on the device node as its own error kind; that failure
/// is always a setup problem (missing udev rule / not root), never a bug, so it
/// gets its own variant with instructions instead of a bare errno.
impl From<nusb::Error> for Error {
    fn from(e: nusb::Error) -> Self {
        match e.kind() {
            nusb::ErrorKind::PermissionDenied => Error::UsbPermission(e.to_string()),
            _ => Error::Usb(e.to_string()),
        }
    }
}

/// How to get write access to the USB device nodes, appended to
/// [`Error::UsbPermission`].
///
/// The full path to this binary matters on Linux: sudo resets PATH to
/// `secure_path`, which has neither Homebrew's prefix nor a user-local bin, so
/// `sudo restorekit` after a `brew install` fails with "command not found".
fn usb_permission_hint() -> String {
    let exe = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "restorekit".into());

    if cfg!(target_os = "linux") {
        format!(
            "restorekit needs write access to the Apple and RecoverKit USB devices. Install
the udev rule, then unplug and replug the device:

  curl -fsSL https://raw.githubusercontent.com/fcjr/restorekit/main/udev/51-restorekit.rules \\
    | sudo tee /etc/udev/rules.d/51-restorekit.rules >/dev/null
  sudo udevadm control --reload-rules && sudo udevadm trigger

The brew cask and the .deb install that rule for you; the snap and the plain
tarball can't. Or run this as root instead — sudo's PATH won't find restorekit
on its own, so pass the full path:

  sudo {exe} ..."
        )
    } else if cfg!(target_os = "windows") {
        "Run `restorekit setup-driver` to bind the WinUSB driver to the cabled Mac.".into()
    } else {
        format!("Re-run as root: sudo {exe} ...")
    }
}

pub type Result<T> = std::result::Result<T, Error>;
