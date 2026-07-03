//! applerestore — library for DFU-restoring Apple Silicon Macs.
//!
//! The library performs no I/O to stdout/stdin. Long-running operations report
//! progress through an [`Event`](progress::Event) callback so that CLI and GUI
//! frontends can render it however they like.

pub mod device;
pub mod dfu;
pub mod error;
pub mod progress;

pub use dfu::{DfuDevice, host_can_trigger_dfu, manual_dfu_instructions};
pub use error::{Error, Result};
pub use progress::Event;
