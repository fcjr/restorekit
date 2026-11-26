use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    #[error("no device ready to restore found: a Mac in DFU mode, or an iPhone or iPad in DFU or recovery mode")]
    NoDeviceFound,

    #[error("multiple devices ready to restore found ({0}); select one with --ecid (see `restorekit list`) or disconnect the others")]
    MultipleDevices(usize),

    #[error("no device in DFU mode found")]
    NoDfuDevice,

    #[error("multiple devices in DFU mode found ({0}); select one with --ecid (see `restorekit list`) or disconnect the others")]
    MultipleDfuDevices(usize),

    #[error("no device with ECID {0:#x} is connected")]
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

    #[error("{}", kis_unreadable_message())]
    KisUnreadable,

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

    #[error(
        "the target Mac didn't answer the dongle over USB-PD; make sure it's on and awake \
         (open the lid or press a key), then try again"
    )]
    DongleTargetSilent,

    #[error("dongle: {0}")]
    Dongle(String),

    #[error(
        "unknown model (CPID:{cpid:04x} BDID:{bdid:02x}); no firmware mapping, pass --identifier"
    )]
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

    #[error("can't reach usbmuxd{hint}", hint = running_device_hint())]
    UsbmuxdUnreachable,

    #[error(
        "no running iPhone or iPad with ECID {0:#x} is visible to usbmuxd. A Mac can't be \
         reached this way: put it in DFU mode instead{hint}",
        hint = running_device_hint()
    )]
    RunningDeviceNotFound(u64),

    #[error(
        "no running iPhone or iPad is visible to usbmuxd. A Mac can't be reached this way: put \
         it in DFU mode instead{hint}",
        hint = running_device_hint()
    )]
    NoRunningDevice,

    #[error("{0} running iPhones and iPads are connected; select one by its ECID (see `restorekit list`)")]
    MultipleRunningDevices(usize),

    #[error("the device is locked; unlock it and retry")]
    DeviceLocked,

    #[error("the device is asking whether to trust this computer; tap Trust on it and retry")]
    TrustPending,

    #[error("the device declined to trust this computer; reconnect it to be asked again")]
    TrustDenied,

    #[error("couldn't boot the device out of recovery mode: {0}")]
    RecoveryExit(String),

    #[error("lockdown error {code}: {message}")]
    Lockdown { code: i32, message: String },

    #[error("unusable supervision identity: {0}")]
    SupervisionIdentity(String),

    #[error("WinUSB driver setup failed: {0}")]
    DriverInstall(String),

    #[error("{0} isn't available for an iPhone or iPad yet; `restorekit restore` erases and restores it")]
    UntestedOnIos(&'static str),

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

/// Why a Mac in Debug USB DFU (`05ac:1881`) couldn't be identified, and what to
/// do instead, per host OS.
fn kis_unreadable_message() -> &'static str {
    if cfg!(target_os = "macos") {
        "the Mac is in DFU over Apple's Debug USB interface (05ac:1881), but restorekit couldn't \
         read its identity through deviceinterfaced. Usually another app has the device open \
         (Chrome, Apple Configurator). Quit it, then put the Mac back into DFU (`restorekit \
         reboot`, then `restorekit dfu`) and retry. macOS only hands the device out when it \
         shows up, so retrying without re-entering DFU won't help"
    } else if cfg!(target_os = "windows") {
        "the Mac is in DFU over Apple's Debug USB interface (05ac:1881), but restorekit couldn't \
         read its identity. Windows' driver for it only picks up a Mac that arrives in DFU, so \
         one that was already there when the driver was installed or changed stays unreadable. \
         Put the Mac back into DFU (`restorekit reboot`, then `restorekit dfu`; with a dongle \
         restorekit does this itself) and retry. If it still can't be read, run \
         `restorekit setup-driver`"
    } else {
        "the Mac is in DFU over Apple's Debug USB interface (05ac:1881), but restorekit couldn't \
         read its identity. Check USB permissions (the udev rule covers 05ac:1881) and retry"
    }
}

/// Which usbmuxd reaches a running iPhone or iPad on this host, appended to
/// [`Error::UsbmuxdUnreachable`], [`Error::RunningDeviceNotFound`] and
/// [`Error::NoRunningDevice`]. On macOS
/// it is always Apple's.
fn running_device_hint() -> &'static str {
    if cfg!(target_os = "linux") {
        "\n\nOn Linux a running device is reached through the system usbmuxd when one is \
         running, else restorekit's own, which needs restorekit's udev rules \
         (udev/51-restorekit.rules) to open the device."
    } else if cfg!(target_os = "windows") {
        "\n\nOn Windows a running device is reached through Apple Mobile Device \
         Service, which Apple Devices and iTunes install."
    } else {
        ""
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
        "Run `restorekit setup-driver` to bind the WinUSB driver to the cabled Mac and the \
         RecoverKit dongle."
            .into()
    } else {
        format!("Re-run as root: sudo {exe} ...")
    }
}

pub type Result<T> = std::result::Result<T, Error>;
