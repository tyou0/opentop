use opentop_core::snapshot::{Availability, BatteryInfo, Snapshot};
use opentop_core::units;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::common::{mini_bar, panel, unavailable};

fn state_color(theme: &Theme, state: &str) -> Color {
    match state {
        "charging" => theme.ok,
        "full" => theme.ok,
        "discharging" => theme.accent,
        "empty" => theme.crit,
        _ => theme.muted,
    }
}

fn battery_lines<'t>(theme: &'t Theme, battery: &'t BatteryInfo, width: u16) -> Vec<Line<'t>> {
    let bar_width = ((width as usize).saturating_sub(8)).clamp(4, 30);
    let mut bar = mini_bar(
        theme,
        battery.percentage,
        bar_width,
        theme.usage_color(battery.percentage),
    );
    let mut first = vec![Span::styled(
        format!("{:<10} ", battery.name),
        Style::default().fg(theme.fg),
    )];
    first.append(&mut bar);
    first.push(Span::styled(
        format!(" {:>3.0}%", battery.percentage),
        Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
    ));
    let mut lines = vec![Line::from(first)];

    let mut status = vec![Span::styled(
        battery.state.clone(),
        Style::default().fg(state_color(theme, &battery.state)),
    )];
    if let Some(power) = battery.power_watts {
        status.push(Span::styled(
            format!("  {power:.1} W"),
            Style::default().fg(theme.fg),
        ));
    }
    if let Some(temp) = battery.temperature_celsius {
        status.push(Span::styled(
            format!("  {temp:.1}°C"),
            Style::default().fg(theme.fg),
        ));
    }
    lines.push(Line::from(status));

    let mut details: Vec<String> = Vec::new();
    if let Some(health) = battery.health_percent {
        details.push(format!("health {health:.0}%"));
    }
    if let Some(cycles) = battery.cycle_count {
        details.push(format!("{cycles} cycles"));
    }
    match (battery.state.as_str(), battery.time_to_full_secs) {
        ("charging", Some(secs)) if secs > 0 => {
            details.push(format!("full in {}", units::duration(secs)))
        }
        _ => {}
    }
    if let Some(secs) = battery.time_to_empty_secs {
        if secs > 0 {
            details.push(format!("{} left", units::duration(secs)));
        }
    }
    if !details.is_empty() {
        lines.push(Line::from(Span::styled(
            details.join("  ·  "),
            Style::default().fg(theme.muted),
        )));
    }
    lines
}

/// Battery state, charge level and health.
pub struct BatteryWidget<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
    pub config: BatteryWidgetConfig,
}

#[derive(Debug, Clone)]
pub struct BatteryWidgetConfig {
    pub title: String,
    pub max_batteries: usize,
}

impl Default for BatteryWidgetConfig {
    fn default() -> Self {
        Self {
            title: "Battery".into(),
            max_batteries: 2,
        }
    }
}

impl Widget for BatteryWidget<'_> {
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

        match &self.snapshot.battery {
            Availability::Available(batteries) if batteries.is_empty() => {
                unavailable(theme, "no batteries").render(inner, buf);
            }
            Availability::Available(batteries) => {
                let mut lines: Vec<Line> = Vec::new();
                for (i, battery) in batteries.iter().take(self.config.max_batteries).enumerate() {
                    if i > 0 {
                        lines.push(Line::from(""));
                    }
                    lines.extend(battery_lines(theme, battery, inner.width));
                }
                Paragraph::new(lines).render(inner, buf);
            }
            Availability::Unavailable { reason } => {
                unavailable(theme, reason).render(inner, buf);
            }
        }
    }
}
