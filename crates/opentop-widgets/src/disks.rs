use opentop_core::snapshot::{DiskHealth, Snapshot};
use opentop_core::units;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::common::{mini_bar, pad_truncate, panel, unavailable};
use opentop_core::snapshot::Availability;

/// One row per mounted volume: usage bar plus read/write rates.
pub struct DiskUsageList<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
    pub config: DiskUsageListConfig,
}

#[derive(Debug, Clone)]
pub struct DiskUsageListConfig {
    pub title: String,
    pub max_disks: usize,
    pub show_rates: bool,
}

impl Default for DiskUsageListConfig {
    fn default() -> Self {
        Self {
            title: "Disks".into(),
            max_disks: 12,
            show_rates: true,
        }
    }
}

impl Widget for DiskUsageList<'_> {
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

        if self.snapshot.disks.is_empty() {
            unavailable(theme, "no disks").render(inner, buf);
            return;
        }

        let label_width = 16usize;
        let bar_width = inner.width.saturating_sub(label_width as u16 + 26).max(4) as usize;
        let lines: Vec<Line> = self
            .snapshot
            .disks
            .iter()
            .take(self.config.max_disks)
            .map(|disk| {
                let color = theme.usage_color(disk.usage_percent);
                let mut spans = vec![Span::styled(
                    pad_truncate(&disk.mount_point, label_width),
                    Style::default().fg(theme.fg),
                )];
                spans.extend(mini_bar(theme, disk.usage_percent, bar_width, color));
                spans.push(Span::styled(
                    format!(
                        " {:>3.0}% {:>8}",
                        disk.usage_percent,
                        units::bytes(disk.used_bytes)
                    ),
                    Style::default().fg(theme.fg),
                ));
                if self.config.show_rates {
                    spans.push(Span::styled(
                        format!(" R {:>9}", units::rate(disk.read_bytes_per_sec as f64)),
                        Style::default().fg(theme.muted),
                    ));
                    spans.push(Span::styled(
                        format!(" W {:>9}", units::rate(disk.write_bytes_per_sec as f64)),
                        Style::default().fg(theme.muted),
                    ));
                }
                Line::from(spans)
            })
            .collect();

        Paragraph::new(lines).render(inner, buf);
    }
}

/// SMART health summary for each physical drive.
pub struct DiskHealthCard<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
    pub config: DiskHealthCardConfig,
}

#[derive(Debug, Clone)]
pub struct DiskHealthCardConfig {
    pub title: String,
    pub max_drives: usize,
}

impl Default for DiskHealthCardConfig {
    fn default() -> Self {
        Self {
            title: "Disk health".into(),
            max_drives: 4,
        }
    }
}

fn health_detail_lines<'t>(theme: &'t Theme, drive: &'t DiskHealth) -> Vec<Line<'t>> {
    let mut lines = Vec::new();

    let (status, status_color) = match drive.overall_health.as_deref() {
        Some("PASSED") => ("PASSED", theme.ok),
        Some("FAILED") => ("FAILED", theme.crit),
        _ => ("unknown", theme.muted),
    };
    let mut header = vec![
        Span::styled(
            pad_truncate(
                match &drive.model {
                    Some(m) if !m.is_empty() => m,
                    _ => &drive.device,
                },
                24,
            ),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("  {status}"), Style::default().fg(status_color)),
    ];
    if let Some(protocol) = &drive.interface {
        header.push(Span::styled(
            format!("  {protocol}"),
            Style::default().fg(theme.muted),
        ));
    }
    lines.push(Line::from(header));

    let mut details: Vec<String> = Vec::new();
    if let Some(temp) = drive.temperature_celsius {
        details.push(format!("{temp}°C"));
    }
    if let Some(wear) = drive.wear_percent {
        details.push(format!("wear {wear:.0}%"));
    }

    if let Some(written) = drive.data_written_bytes {
        details.push(format!("{} written", units::bytes(written)));
    }
    if let Some(hours) = drive.power_on_hours {
        details.push(format!("{} power-on", units::duration(hours * 3600)));
    }
    if let Some(cycles) = drive.power_cycles {
        details.push(format!("{cycles} cycles"));
    }
    if let Some(media) = drive.media_errors {
        if media > 0 {
            details.push(format!("{media} media errors"));
        }
    }
    if !details.is_empty() {
        lines.push(Line::from(Span::styled(
            details.join("  ·  "),
            Style::default().fg(theme.fg),
        )));
    }
    if let Some(note) = &drive.note {
        lines.push(Line::from(Span::styled(
            note.clone(),
            Style::default().fg(theme.muted),
        )));
    }
    lines
}

impl Widget for DiskHealthCard<'_> {
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

        match &self.snapshot.disk_health {
            Availability::Available(drives) if drives.is_empty() => {
                unavailable(theme, "no drives detected").render(inner, buf);
            }
            Availability::Available(drives) => {
                let mut lines: Vec<Line> = Vec::new();
                for (i, drive) in drives.iter().take(self.config.max_drives).enumerate() {
                    if i > 0 {
                        lines.push(Line::from(""));
                    }
                    lines.extend(health_detail_lines(theme, drive));
                }
                Paragraph::new(lines).render(inner, buf);
            }
            Availability::Unavailable { reason } => {
                unavailable(theme, reason).render(inner, buf);
            }
        }
    }
}
