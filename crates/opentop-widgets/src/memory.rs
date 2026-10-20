use opentop_core::history::History;
use opentop_core::snapshot::Snapshot;
use opentop_core::units;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Gauge, LineGauge};

use crate::common::panel;

/// Memory usage gauge with a used/total label and usage-gradient color.
pub struct MemGauge<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
}

impl Widget for MemGauge<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let memory = &self.snapshot.memory;
        let color = theme.usage_color(memory.usage_percent);

        let block = panel(theme, "Memory");
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.is_empty() {
            return;
        }

        let gauge = Gauge::default()
            .ratio((memory.usage_percent / 100.0).clamp(0.0, 1.0) as f64)
            .label(format!(
                "{} / {} ({:.0}%)",
                units::bytes(memory.used_bytes),
                units::bytes(memory.total_bytes),
                memory.usage_percent
            ))
            .gauge_style(Style::default().fg(color).bg(theme.gauge_bg))
            .use_unicode(true);
        gauge.render(inner, buf);
    }
}

/// Swap usage as a single line gauge.
pub struct SwapBar<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
}

impl Widget for SwapBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let memory = &self.snapshot.memory;
        let block = panel(theme, "Swap");
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.is_empty() {
            return;
        }

        let label = format!(
            "{} / {}",
            units::bytes(memory.used_swap_bytes),
            units::bytes(memory.total_swap_bytes),
        );
        let gauge = LineGauge::default()
            .ratio((memory.swap_percent / 100.0).clamp(0.0, 1.0) as f64)
            .label(Span::styled(label, Style::default().fg(theme.fg)))
            .filled_style(Style::default().fg(theme.usage_color(memory.swap_percent)))
            .unfilled_style(Style::default().fg(theme.gauge_bg));
        gauge.render(inner, buf);
    }
}

/// Tiny memory-usage history sparkline.
pub struct MemSparkline<'a> {
    pub history: &'a History,
    pub theme: &'a Theme,
}

impl Widget for MemSparkline<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let data = crate::common::spark_data(&self.history.memory_percent);
        let sparkline = ratatui::widgets::Sparkline::default()
            .data(data)
            .style(Style::default().fg(self.theme.memory));
        sparkline.render(area, buf);
    }
}
