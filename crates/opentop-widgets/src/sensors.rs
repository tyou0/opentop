use opentop_core::history::History;
use opentop_core::snapshot::Snapshot;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Sparkline};

use crate::common::{pad_truncate, panel, unavailable};

/// Temperatures and fan speeds, sourced per-platform (macmon, hwmon, WMI).
pub struct SensorsPanel<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
    pub config: SensorsPanelConfig,
}

#[derive(Debug, Clone)]
pub struct SensorsPanelConfig {
    pub title: String,
    /// Maximum rows of temperatures before truncating.
    pub max_temps: usize,
    /// Show which backend provided the data.
    pub show_source: bool,
}

impl Default for SensorsPanelConfig {
    fn default() -> Self {
        Self {
            title: "Sensors".into(),
            max_temps: 8,
            show_source: false,
        }
    }
}

impl Widget for SensorsPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let sensors = &self.snapshot.sensors;
        let title = if self.config.show_source && !sensors.source.is_empty() {
            format!("{} ({})", self.config.title, sensors.source)
        } else {
            self.config.title.clone()
        };
        let block = panel(theme, title);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.is_empty() {
            return;
        }

        if sensors.temps.is_empty() && sensors.fans.is_empty() {
            unavailable(theme, "no sensor data").render(inner, buf);
            return;
        }

        let label_width = (inner.width.saturating_sub(12)).clamp(8, 18) as usize;
        let mut lines: Vec<Line> = Vec::new();

        for temp in sensors.temps.iter().take(self.config.max_temps) {
            let color = theme.temp_color(temp.celsius);
            lines.push(Line::from(vec![
                Span::styled(
                    pad_truncate(&temp.label, label_width),
                    Style::default().fg(theme.fg),
                ),
                Span::styled(
                    format!("{:>6.1}°C", temp.celsius),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        for fan in &sensors.fans {
            let rpm = match fan.max_rpm {
                Some(max) => format!("{:>4} / {max} rpm", fan.rpm),
                None => format!("{:>4} rpm", fan.rpm),
            };
            lines.push(Line::from(vec![
                Span::styled(
                    pad_truncate(&fan.name, label_width),
                    Style::default().fg(theme.fg),
                ),
                Span::styled(rpm, Style::default().fg(theme.accent)),
            ]));
        }

        Paragraph::new(lines).render(inner, buf);
    }
}

/// Power draw (CPU/GPU/ANE/system) plus a GPU utilization sparkline.
pub struct PowerPanel<'a> {
    pub snapshot: &'a Snapshot,
    pub history: &'a History,
    pub theme: &'a Theme,
    pub config: PowerPanelConfig,
}

#[derive(Debug, Clone)]
pub struct PowerPanelConfig {
    pub title: String,
    pub show_gpu_sparkline: bool,
}

impl Default for PowerPanelConfig {
    fn default() -> Self {
        Self {
            title: "Power".into(),
            show_gpu_sparkline: true,
        }
    }
}

fn watts(value: f32) -> String {
    format!("{value:.1} W")
}

impl Widget for PowerPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let block = panel(theme, &self.config.title);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.is_empty() {
            return;
        }

        let sensors = &self.snapshot.sensors;
        let power = sensors.power.as_ref();
        let gpu = sensors.gpu.as_ref();

        if power.is_none() && gpu.is_none() {
            unavailable(theme, "power metrics unavailable").render(inner, buf);
            return;
        }

        let mut lines: Vec<Line> = Vec::new();
        if let Some(power) = power {
            let mut parts: Vec<Span> = Vec::new();
            let push = |parts: &mut Vec<Span>, span: Span<'static>| {
                if !parts.is_empty() {
                    parts.push(Span::raw("  "));
                }
                parts.push(span);
            };
            if let Some(v) = power.cpu_watts {
                push(
                    &mut parts,
                    Span::styled(format!("CPU {}", watts(v)), Style::default().fg(theme.cpu)),
                );
            }
            if let Some(v) = power.gpu_watts {
                push(
                    &mut parts,
                    Span::styled(
                        format!("GPU {}", watts(v)),
                        Style::default().fg(theme.accent),
                    ),
                );
            }
            if let Some(v) = power.ane_watts {
                push(
                    &mut parts,
                    Span::styled(
                        format!("ANE {}", watts(v)),
                        Style::default().fg(theme.muted),
                    ),
                );
            }
            if let Some(v) = power.ram_watts {
                push(
                    &mut parts,
                    Span::styled(
                        format!("RAM {}", watts(v)),
                        Style::default().fg(theme.memory),
                    ),
                );
            }
            if !parts.is_empty() {
                lines.push(Line::from(parts));
            }
            let mut totals = Vec::new();
            if let Some(v) = power.combined_watts {
                totals.push(format!("combined {}", watts(v)));
            }
            if let Some(v) = power.system_watts {
                totals.push(format!("system {}", watts(v)));
            }
            if !totals.is_empty() {
                lines.push(Line::from(Span::styled(
                    totals.join("  ·  "),
                    Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
                )));
            }
        }
        if let Some(gpu) = gpu {
            let mut parts = Vec::new();
            if let Some(usage) = gpu.usage_percent {
                parts.push(format!("util {usage:.0}%"));
            }
            if let Some(freq) = gpu.frequency_mhz {
                parts.push(format!("{freq} MHz"));
            }
            if let Some(temp) = gpu.temperature_celsius {
                parts.push(format!("{temp:.0}°C"));
            }
            if let Some(name) = &gpu.name {
                parts.push(name.clone());
            }
            if !parts.is_empty() {
                lines.push(Line::from(Span::styled(
                    parts.join("  ·  "),
                    Style::default().fg(theme.muted),
                )));
            }
        }

        let spark_height = if self.config.show_gpu_sparkline
            && !self.history.gpu_usage.is_empty()
            && inner.height as usize > lines.len()
        {
            1
        } else {
            0
        };
        let text_height = inner.height.saturating_sub(spark_height);
        let chunks = Layout::vertical([
            Constraint::Length(text_height),
            Constraint::Length(spark_height),
        ])
        .split(inner);

        Paragraph::new(lines).render(chunks[0], buf);
        if spark_height > 0 {
            let data = crate::common::spark_data(&self.history.gpu_usage);
            Sparkline::default()
                .data(data)
                .style(Style::default().fg(theme.accent))
                .render(chunks[1], buf);
        }
    }
}
