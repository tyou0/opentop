//! Shared rendering helpers used across widgets.

use opentop_core::history::Ring;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Paragraph};

/// A consistent panel: rounded border, themed colors, bold title.
pub fn panel(theme: &Theme, title: impl Into<String>) -> Block<'static> {
    Block::bordered()
        .style(Style::default().fg(theme.fg).bg(theme.bg))
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.border))
        .title(Span::styled(
            format!(" {} ", title.into()),
            Style::default()
                .fg(theme.title)
                .add_modifier(Modifier::BOLD),
        ))
}

pub(crate) fn table_header_style(theme: &Theme) -> Style {
    Style::default()
        .fg(theme.muted)
        .add_modifier(Modifier::BOLD)
}

/// A unicode progress bar as styled spans: filled part in `color`, rest in
/// the gauge background color.
pub fn mini_bar(theme: &Theme, percent: f32, width: usize, color: Color) -> Vec<Span<'static>> {
    let percent = percent.clamp(0.0, 100.0);
    let filled = ((percent / 100.0) * width as f32).round() as usize;
    vec![
        Span::styled("█".repeat(filled), Style::default().fg(color)),
        Span::styled(
            "░".repeat(width.saturating_sub(filled)),
            Style::default().fg(theme.gauge_bg),
        ),
    ]
}

/// Blend two colors; `t` = 0.0 returns `a`, 1.0 returns `b`. Non-RGB inputs
/// fall back to `a`.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    match (a, b) {
        (Color::Rgb(r1, g1, b1), Color::Rgb(r2, g2, b2)) => {
            let t = t.clamp(0.0, 1.0);
            let lerp = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
            Color::Rgb(lerp(r1, r2), lerp(g1, g2), lerp(b1, b2))
        }
        _ => a,
    }
}

/// Ring samples as `u64` sparkline data (clamped at zero).
pub fn spark_data(ring: &Ring) -> Vec<u64> {
    ring.iter().map(|v| v.max(0.0) as u64).collect()
}

/// Muted "no data" paragraph, used by widgets when a section is unavailable.
pub fn unavailable(theme: &Theme, message: &str) -> Paragraph<'static> {
    Paragraph::new(Line::from(Span::styled(
        message.to_string(),
        Style::default().fg(theme.muted),
    )))
}

/// Truncate to `max` characters, appending `…` when shortened.
pub fn truncate(input: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let mut chars = input.chars();
    let taken: String = chars.by_ref().take(max).collect();
    if chars.next().is_some() {
        let mut out: String = taken.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    } else {
        taken
    }
}

/// Left-align and pad to `width` characters, truncating with `…` if needed.
pub fn pad_truncate(input: &str, width: usize) -> String {
    let text = truncate(input, width);
    let len = text.chars().count();
    format!("{text}{}", " ".repeat(width.saturating_sub(len)))
}
