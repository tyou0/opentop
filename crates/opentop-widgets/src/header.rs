use opentop_core::snapshot::Snapshot;
use opentop_core::units;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

/// One-line host header: hostname, OS, uptime, load average, CPU brand.
pub struct HeaderBar<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
}

impl Widget for HeaderBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let system = &self.snapshot.system;

        let separator = || Span::styled("  ·  ", Style::default().fg(theme.muted));
        let mut spans = vec![Span::styled(
            system
                .hostname
                .clone()
                .unwrap_or_else(|| "unknown host".into()),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )];

        if let Some(os) = system
            .long_os_version
            .clone()
            .or_else(|| system.os_name.clone())
        {
            spans.push(separator());
            spans.push(Span::styled(os, Style::default().fg(theme.fg)));
        }
        spans.push(separator());
        spans.push(Span::styled(
            format!("up {}", units::duration(system.uptime_secs)),
            Style::default().fg(theme.fg),
        ));
        if let Some(load) = system.load_average {
            spans.push(separator());
            spans.push(Span::styled(
                format!("load {:.2} {:.2} {:.2}", load.one, load.five, load.fifteen),
                Style::default().fg(theme.fg),
            ));
        }
        if !self.snapshot.cpu.brand.is_empty() {
            spans.push(separator());
            spans.push(Span::styled(
                self.snapshot.cpu.brand.clone(),
                Style::default().fg(theme.muted),
            ));
        }

        Paragraph::new(Line::from(spans))
            .style(Style::default().fg(theme.fg).bg(theme.bg))
            .render(area, buf);
    }
}
