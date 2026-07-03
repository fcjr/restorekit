//! Restore Apple Silicon Macs over USB — the engine behind the `restorekit`
//! CLI, usable directly from Rust (e.g. to build a GUI).
//!
//! The workflow has four stages, each a module:
//!
//! - [`dfu`] — put a cabled target into DFU mode ([`dfu::vdm`], macOS on Apple
//!   Silicon only) and detect a Mac already in DFU ([`dfu::list`],
//!   [`dfu::wait_for_dfu`]).
//! - [`device`] — identify a detected device's exact model from its chip and
//!   board IDs.
//! - [`firmware`] — resolve the correct IPSW for a model, download it resumably,
//!   verify its checksum, and cache it.
//! - [`restore`] — restore or revive the device via the statically-linked
//!   `idevicerestore` engine.
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
//! use restorekit::{dfu, firmware, restore, Event, Mode, Result};
//!
//! fn main() -> Result<()> {
//!     // Wait for a target to appear in DFU mode (trigger it first with
//!     // `dfu::vdm::enter_dfu` on an Apple Silicon macOS host).
//!     let device = dfu::wait_for_dfu(Duration::from_secs(60))?;
//!     let identifier = device.identifier().expect("known model");
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
//!     restore::restore(&ipsw, device.ecid, Some(&cache), Mode::Erase, false, &mut |event| {
//!         if let Event::RestoreStep { name, progress, .. } = event {
//!             eprintln!("{name}: {:.0}%", progress * 100.0);
//!         }
//!     })?;
//!     Ok(())
//! }
//! ```

pub mod device;
pub mod dfu;
#[cfg(target_os = "windows")]
pub mod driver;
pub mod error;
pub mod firmware;
pub mod progress;
pub mod restore;
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub mod usbmuxd;

pub use dfu::{host_can_trigger_dfu, manual_dfu_instructions, DfuDevice};
pub use error::{Error, Result};
pub use firmware::Firmware;
pub use progress::Event;
pub use restore::Mode;
