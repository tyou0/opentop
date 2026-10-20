use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

/// One key hint, e.g. `q` → `quit`.
#[derive(Debug, Clone)]
pub struct Keybind {
    pub key: String,
    pub label: String,
}

impl Keybind {
    pub fn new(key: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            label: label.into(),
        }
    }
}

/// Bottom status bar of key hints.
pub struct FooterKeybinds<'a> {
    pub theme: &'a Theme,
    pub binds: &'a [Keybind],
}

impl Widget for FooterKeybinds<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let mut spans = Vec::new();
        for (i, bind) in self.binds.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("   ", Style::default().fg(theme.muted)));
            }
            spans.push(Span::styled(
                bind.key.clone(),
                Style::default()
                    .fg(theme.accent)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!(" {}", bind.label),
                Style::default().fg(theme.muted),
            ));
        }
        Paragraph::new(Line::from(spans))
            .style(Style::default().fg(theme.fg).bg(theme.bg))
            .render(area, buf);
    }
}
