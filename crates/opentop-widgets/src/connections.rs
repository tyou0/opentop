use opentop_core::snapshot::{Availability, Snapshot};
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::common::{panel, unavailable};

/// Socket counts grouped by protocol and state.
pub struct ConnectionsSummaryWidget<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
    pub config: ConnectionsSummaryConfig,
}

#[derive(Debug, Clone)]
pub struct ConnectionsSummaryConfig {
    pub title: String,
}

impl Default for ConnectionsSummaryConfig {
    fn default() -> Self {
        Self {
            title: "Connections".into(),
        }
    }
}

impl Widget for ConnectionsSummaryWidget<'_> {
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

        let summary = match &self.snapshot.connections {
            Availability::Available(summary) => summary,
            Availability::Unavailable { reason } => {
                unavailable(theme, reason).render(inner, buf);
                return;
            }
        };

        let metric = |label: &'static str, value: String, color: Color| {
            vec![
                Span::styled(
                    format!("{value} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(label, Style::default().fg(theme.muted)),
            ]
        };

        let mut first = metric("sockets", summary.total.to_string(), theme.accent);
        first.push(Span::styled("   ·   ", Style::default().fg(theme.muted)));
        first.extend(metric("TCP", summary.tcp.to_string(), theme.net_tx));
        first.push(Span::styled("   ·   ", Style::default().fg(theme.muted)));
        first.extend(metric("UDP", summary.udp.to_string(), theme.net_rx));

        let second = Line::from(vec![
            Span::styled(
                format!("{} established", summary.tcp_established),
                Style::default().fg(theme.ok),
            ),
            Span::styled("  ·  ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{} listening", summary.tcp_listen),
                Style::default().fg(theme.fg),
            ),
            Span::styled("  ·  ", Style::default().fg(theme.muted)),
            Span::styled(
                format!("{} time-wait", summary.tcp_wait),
                Style::default().fg(theme.muted),
            ),
        ]);

        let third = Line::from(Span::styled(
            format!("{} processes with sockets", summary.processes_with_sockets),
            Style::default().fg(theme.muted),
        ));

        Paragraph::new(vec![Line::from(first), second, third]).render(inner, buf);
    }
}
