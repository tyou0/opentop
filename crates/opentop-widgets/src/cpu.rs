use opentop_core::history::History;
use opentop_core::snapshot::Snapshot;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine};
use ratatui::widgets::Paragraph;

use crate::common::{mix, mini_bar, panel};

/// Braille line graph of total CPU usage over time, with a translucent fill.
pub struct CpuGraph<'a> {
    pub snapshot: &'a Snapshot,
    pub history: &'a History,
    pub theme: &'a Theme,
    pub config: CpuGraphConfig,
}

#[derive(Debug, Clone)]
pub struct CpuGraphConfig {
    /// Panel title; defaults to "CPU".
    pub title: String,
    /// Shade the area under the curve.
    pub fill: bool,
    /// Vertical scale ceiling.
    pub max_percent: f32,
}

impl Default for CpuGraphConfig {
    fn default() -> Self {
        Self {
            title: "CPU".into(),
            fill: true,
            max_percent: 100.0,
        }
    }
}

impl Widget for CpuGraph<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let usage = self.snapshot.cpu.usage_percent;
        let block = panel(
            theme,
            format!("{} {:.0}%", self.config.title, usage),
        );
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.is_empty() {
            return;
        }

        let data: Vec<f64> = self.history.cpu_total.iter().map(f64::from).collect();
        if data.len() < 2 {
            crate::common::unavailable(theme, "collecting…").render(inner, buf);
            return;
        }

        let len = data.len() as f64;
        let max = self.config.max_percent.max(1.0) as f64;
        let fill_color = mix(theme.bg, theme.cpu, 0.75);
        let line_color = theme.cpu;
        let fill = self.config.fill;

        let canvas = Canvas::default()
            .x_bounds([0.0, len - 1.0])
            .y_bounds([0.0, max])
            .marker(Marker::Braille)
            .paint(|ctx| {
                if fill {
                    for (i, value) in data.iter().enumerate() {
                        ctx.draw(&CanvasLine {
                            x1: i as f64,
                            y1: 0.0,
                            x2: i as f64,
                            y2: *value,
                            color: fill_color,
                        });
                    }
                }
                for pair in data.windows(2).enumerate() {
                    let (i, window) = pair;
                    ctx.draw(&CanvasLine {
                        x1: i as f64,
                        y1: window[0],
                        x2: (i + 1) as f64,
                        y2: window[1],
                        color: line_color,
                    });
                }
            });
        canvas.render(inner, buf);
    }
}

/// Rows of per-core usage bars with a mini bar each.
pub struct CpuCoreBars<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
}

impl Widget for CpuCoreBars<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let block = panel(theme, "Cores");
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.is_empty() {
            return;
        }

        let bar_width = inner.width.saturating_sub(10).max(3) as usize;
        let lines: Vec<Line> = self
            .snapshot
            .cpu
            .cores
            .iter()
            .take(inner.height as usize)
            .enumerate()
            .map(|(i, core)| {
                let color = theme.usage_color(core.usage_percent);
                let mut spans = vec![Span::styled(
                    format!("c{:<2} ", i),
                    Style::default().fg(theme.muted),
                )];
                spans.extend(mini_bar(theme, core.usage_percent, bar_width, color));
                spans.push(Span::styled(
                    format!(" {:>3.0}%", core.usage_percent),
                    Style::default().fg(theme.fg),
                ));
                Line::from(spans)
            })
            .collect();

        Paragraph::new(lines).render(inner, buf);
    }
}

/// CPU frequency tiers (E/P/S clusters) with frequency and activity.
pub struct CpuFreqTiers<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
}

impl Widget for CpuFreqTiers<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let block = panel(theme, "Clusters");
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.is_empty() {
            return;
        }

        let tiers = &self.snapshot.sensors.cpu_tiers;
        if tiers.is_empty() {
            // Fall back to the flat frequency sysinfo reports (Linux).
            let mhz = self.snapshot.cpu.frequency_mhz;
            let text = if mhz > 0 {
                format!("{:.2} GHz", mhz as f32 / 1000.0)
            } else {
                "no frequency data".into()
            };
            let color = if mhz > 0 { theme.fg } else { theme.muted };
            Paragraph::new(Line::from(Span::styled(text, Style::default().fg(color))))
                .render(inner, buf);
            return;
        }

        let lines: Vec<Line> = tiers
            .iter()
            .take(inner.height as usize)
            .map(|tier| {
                Line::from(vec![
                    Span::styled(
                        format!("{:<4}", format!("{} ×{}", tier.label, tier.cores)),
                        Style::default().fg(theme.accent),
                    ),
                    Span::styled(
                        format!("{:>6.2} GHz", tier.frequency_mhz as f32 / 1000.0),
                        Style::default().fg(theme.fg),
                    ),
                    Span::styled(
                        format!("  {:>3.0}% active", tier.active_ratio * 100.0),
                        Style::default()
                            .fg(theme.usage_color(tier.active_ratio * 100.0)),
                    ),
                ])
            })
            .collect();

        Paragraph::new(lines).render(inner, buf);
    }
}
