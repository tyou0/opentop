//! Uniform dashboard rendering entry points: every widget can be rendered
//! from a shared context without the caller juggling constructor arguments.

use opentop_core::history::History;
use opentop_core::snapshot::Snapshot;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::StatefulWidget;

use crate::battery::BatteryWidget;
use crate::connections::ConnectionsSummaryWidget;
use crate::cpu::{CpuCoreBars, CpuFreqTiers, CpuGraph};
use crate::disks::{DiskHealthCard, DiskUsageList};
use crate::footer::FooterKeybinds;
use crate::header::HeaderBar;
use crate::memory::{MemGauge, MemSparkline, SwapBar};
use crate::network::{NetSparklines, NetTable};
use crate::processes::{ProcessTable, ProcessTableState};
use crate::sensors::{PowerPanel, SensorsPanel};

/// Everything a widget needs to render one frame.
pub struct DashboardContext<'a> {
    pub snapshot: &'a Snapshot,
    pub history: &'a History,
    pub theme: &'a Theme,
}

impl<'a> DashboardContext<'a> {
    pub fn new(snapshot: &'a Snapshot, history: &'a History, theme: &'a Theme) -> Self {
        Self {
            snapshot,
            history,
            theme,
        }
    }
}

/// A stateless widget that renders from the dashboard context.
pub trait DashboardWidget {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>);
}

/// A stateful widget (selection, scrolling) that renders from the context.
pub trait DashboardStatefulWidget {
    type State;

    fn dashboard_render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        ctx: &DashboardContext<'_>,
        state: &mut Self::State,
    );
}

impl DashboardWidget for HeaderBar<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        HeaderBar {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
        }
        .render(area, buf);
    }
}

impl DashboardWidget for CpuGraph<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        CpuGraph {
            snapshot: ctx.snapshot,
            history: ctx.history,
            theme: ctx.theme,
            config: self.config.clone(),
        }
        .render(area, buf);
    }
}

impl DashboardWidget for CpuCoreBars<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        CpuCoreBars {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
        }
        .render(area, buf);
    }
}

impl DashboardWidget for CpuFreqTiers<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        CpuFreqTiers {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
        }
        .render(area, buf);
    }
}

impl DashboardWidget for MemGauge<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        MemGauge {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
        }
        .render(area, buf);
    }
}

impl DashboardWidget for SwapBar<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        SwapBar {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
        }
        .render(area, buf);
    }
}

impl DashboardWidget for MemSparkline<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        MemSparkline {
            history: ctx.history,
            theme: ctx.theme,
        }
        .render(area, buf);
    }
}

impl DashboardWidget for SensorsPanel<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        SensorsPanel {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
            config: self.config.clone(),
        }
        .render(area, buf);
    }
}

impl DashboardWidget for PowerPanel<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        PowerPanel {
            snapshot: ctx.snapshot,
            history: ctx.history,
            theme: ctx.theme,
            config: self.config.clone(),
        }
        .render(area, buf);
    }
}

impl DashboardWidget for NetSparklines<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        NetSparklines {
            snapshot: ctx.snapshot,
            history: ctx.history,
            theme: ctx.theme,
            config: self.config.clone(),
        }
        .render(area, buf);
    }
}

impl DashboardWidget for NetTable<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        NetTable {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
            config: self.config.clone(),
        }
        .render(area, buf);
    }
}

impl DashboardWidget for DiskUsageList<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        DiskUsageList {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
            config: self.config.clone(),
        }
        .render(area, buf);
    }
}

impl DashboardWidget for DiskHealthCard<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        DiskHealthCard {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
            config: self.config.clone(),
        }
        .render(area, buf);
    }
}

impl DashboardWidget for BatteryWidget<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        BatteryWidget {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
            config: self.config.clone(),
        }
        .render(area, buf);
    }
}

impl DashboardWidget for ConnectionsSummaryWidget<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        ConnectionsSummaryWidget {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
            config: self.config.clone(),
        }
        .render(area, buf);
    }
}

impl DashboardWidget for FooterKeybinds<'_> {
    fn dashboard_render(&self, area: Rect, buf: &mut Buffer, ctx: &DashboardContext<'_>) {
        FooterKeybinds {
            theme: ctx.theme,
            binds: self.binds,
        }
        .render(area, buf);
    }
}

impl DashboardStatefulWidget for ProcessTable<'_> {
    type State = ProcessTableState;

    fn dashboard_render(
        &self,
        area: Rect,
        buf: &mut Buffer,
        ctx: &DashboardContext<'_>,
        state: &mut Self::State,
    ) {
        let table = ProcessTable {
            snapshot: ctx.snapshot,
            theme: ctx.theme,
            config: self.config.clone(),
        };
        StatefulWidget::render(table, area, buf, state);
    }
}
