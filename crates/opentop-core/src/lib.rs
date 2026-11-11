//! opentop-core: cross-platform system metrics backend.
//!
//! The crate is built around three pieces:
//!
//! * [`Snapshot`] — a serializable, point-in-time view of the whole system.
//! * [`History`] — ring buffers of recent samples for graph widgets.
//! * [`MonitorHandle`] — a sampling engine that runs one thread per collector
//!   group and merges results into a shared [`Snapshot`] + [`History`].
//!
//! Every section that a platform cannot provide is marked
//! [`Availability::Unavailable`] with a reason instead of failing the whole
//! snapshot, so a missing battery, fan or `smartctl` binary never breaks the
//! monitor.

pub mod collectors;
pub mod history;
pub mod monitor;
pub mod platform;
pub mod plugins;
pub mod process_control;
pub mod smart;
pub mod snapshot;
pub mod subprocess;
pub mod units;

pub use history::{History, Ring};
pub use monitor::{MonitorConfig, MonitorHandle};
pub use plugins::{Plugin, PluginRegistry};
pub use snapshot::*;

/// Milliseconds since the unix epoch.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
