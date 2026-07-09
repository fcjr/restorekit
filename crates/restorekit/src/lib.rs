//! Restore Apple Silicon Macs over USB — the engine behind the `restorekit`
//! CLI, usable directly from Rust (e.g. to build a GUI).
//!
//! The workflow has four stages, each a module:
//!
//! - [`device`] — the [`device::Device`] primitive: every connected Apple USB
//!   device with its [`device::UsbMode`] and identity (chip/board/ECID/model).
//!   Enumerate with [`device::list`], select with [`device::find`] /
//!   [`device::wait`] and a [`device::Target`], and fill in booted Macs' ECIDs
//!   with [`device::identify`].
//! - [`dfu`] — put a cabled target into DFU mode with the USB-PD trigger
//!   ([`dfu::vdm`], Apple Silicon macOS hosts only), and catch the Mac a trigger
//!   just put *into* DFU with [`dfu::watch`] (subscribe before triggering).
//! - [`firmware`] — resolve the correct IPSW for a model, download it resumably,
//!   verify its checksum, and cache it.
//! - [`restore`] — restore or revive the device via the statically-linked
//!   `idevicerestore` engine (DFU mode only).
//!
//! # Design
//!
//! The library never writes to stdout/stderr and never prompts. Every
//! long-running call takes a `&mut dyn FnMut(`[`Event`]`)` progress callback, so
//! a CLI can draw progress bars, a GUI can update a view, and tests can assert on
//! the event stream — all from the same code. Every fallible call returns
//! [`Result`], a [`std::result::Result`] over [`Error`].
//!
//! # Example
//!
//! Detect a Mac in DFU mode, download the right firmware, and restore it:
//!
//! ```no_run
//! use std::time::Duration;
//! use restorekit::{device, firmware, restore, Event, Mode, Result};
//!
//! fn main() -> Result<()> {
//!     // Pick a target: the sole Mac in DFU mode (Target::Ecid picks one of
//!     // several; device::list shows everything connected, in any mode — put a
//!     // cabled target into DFU first with dfu::vdm::enter_dfu).
//!     let dev = device::find(device::Target::One)?;
//!
//!     let identifier = dev.identifier().expect("known model");
//!     let ecid = dev.ecid.expect("DFU devices always carry an ECID");
//!
//!     // Resolve and download the latest signed firmware into the cache.
//!     let fw = firmware::resolve(identifier, None)?;
//!     let cache = firmware::default_cache_dir()?;
//!     let ipsw = firmware::download(&cache, &fw, &mut |event| {
//!         if let Event::DownloadProgress { received, total } = event {
//!             eprintln!("{received}/{total}");
//!         }
//!     })?;
//!
//!     // Erase-restore the device, printing each step.
//!     restore::restore(&ipsw, ecid, Some(&cache), Mode::Erase, false, &mut |event| {
//!         if let Event::RestoreStep { name, progress, .. } = event {
//!             eprintln!("{name}: {:.0}%", progress * 100.0);
//!         }
//!     })?;
//!     Ok(())
//! }
//! ```

pub mod device;
pub mod dfu;
pub mod dongle;
#[cfg(target_os = "windows")]
pub mod driver;
pub mod error;
pub mod firmware;
#[cfg(feature = "history")]
pub mod history;
pub mod progress;
pub mod restore;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod usbmuxd;

pub use device::{Device, Port, Target, UsbMode};
pub use dfu::{
    host_can_trigger_dfu, manual_dfu_instructions, trigger_dfu, DfuOutcome, DfuTarget, DfuVia,
    HostPortInfo,
};
pub use dongle::{Connection, Dongle, DongleHandle, DongleStatus, DongleTarget, PdState};
pub use error::{Error, Result};
pub use firmware::Firmware;
pub use progress::Event;
pub use restore::Mode;

/// A shared embedded usbmuxd the parent process holds while it spawns per-device
/// restore workers. Child `restore` processes detect it (via the
/// `RESTOREKIT_SHARED_USBMUXD` env var, inherited from the parent) and reuse it
/// rather than each starting their own conflicting instance.
///
/// A no-op on macOS, where the system usbmuxd is always available.
pub struct SharedUsbmuxd {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    _guard: usbmuxd::UsbmuxdGuard,
}

/// Start a [`SharedUsbmuxd`] the parent holds for the lifetime of its restore
/// jobs. Enables true process-per-device parallelism (see [`SharedUsbmuxd`]).
pub fn start_shared_usbmuxd(progress: progress::ProgressFn) -> Result<SharedUsbmuxd> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        let guard = usbmuxd::UsbmuxdGuard::start(progress)?;
        unsafe { std::env::set_var("RESTOREKIT_SHARED_USBMUXD", "1") };
        Ok(SharedUsbmuxd { _guard: guard })
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = progress;
        Ok(SharedUsbmuxd {})
    }
}
