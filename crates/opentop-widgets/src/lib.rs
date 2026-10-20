//! The opentop widget library: ratatui widgets that render an
//! [`opentop_core::snapshot::Snapshot`] (plus recent history) with an
//! [`opentop_theme::Theme`].
//!
//! Every widget is a plain ratatui [`Widget`](ratatui::widgets::Widget) that
//! borrows what it needs, so it can be composed freely. For convenience,
//! [`traits::DashboardWidget`] offers a uniform entry point taking a
//! [`traits::DashboardContext`].
//!
//! # Example
//!
//! ```no_run
//! use opentop_core::{MonitorConfig, MonitorHandle};
//! use opentop_theme::Theme;
//! use opentop_widgets::{cpu::CpuGraph, traits::{DashboardContext, DashboardWidget}};
//!
//! let monitor = MonitorHandle::start(MonitorConfig::default());
//! monitor.wait_ready(std::time::Duration::from_secs(3));
//!
//! let snapshot = monitor.snapshot();
//! let history = monitor.history();
//! let theme = Theme::dark();
//! let ctx = DashboardContext::new(&snapshot, &history, &theme);
//!
//! let graph = CpuGraph {
//!     snapshot: &snapshot,
//!     history: &history,
//!     theme: &theme,
//!     config: Default::default(),
//! };
//! # let area = ratatui::layout::Rect::default();
//! # let mut buf = ratatui::buffer::Buffer::empty(area);
//! graph.dashboard_render(area, &mut buf, &ctx);
//! ```

pub mod battery;
mod common;
pub mod connections;
pub mod cpu;
pub mod disks;
pub mod footer;
pub mod header;
pub mod memory;
pub mod network;
pub mod processes;
pub mod sensors;
pub mod traits;

pub use battery::{BatteryWidget, BatteryWidgetConfig};
pub use common::{mini_bar, mix, pad_truncate, panel, spark_data, truncate, unavailable};
pub use connections::{ConnectionsSummaryConfig, ConnectionsSummaryWidget};
pub use cpu::{CpuCoreBars, CpuFreqTiers, CpuGraph, CpuGraphConfig};
pub use disks::{
    DiskHealthCard, DiskHealthCardConfig, DiskUsageList, DiskUsageListConfig,
};
pub use footer::{FooterKeybinds, Keybind};
pub use header::HeaderBar;
pub use memory::{MemGauge, MemSparkline, SwapBar};
pub use network::{NetSparklines, NetSparklinesConfig, NetTable, NetTableConfig};
pub use processes::{ProcessSort, ProcessTable, ProcessTableConfig, ProcessTableState};
pub use sensors::{
    PowerPanel, PowerPanelConfig, SensorsPanel, SensorsPanelConfig,
};
pub use traits::{DashboardContext, DashboardStatefulWidget, DashboardWidget};
