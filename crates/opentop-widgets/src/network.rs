use opentop_core::history::History;
use opentop_core::snapshot::{NetworkInterface, Snapshot};
use opentop_core::units;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Paragraph, Row, Sparkline, Table};

use crate::common::{pad_truncate, panel, unavailable};

fn is_loopback(name: &str) -> bool {
    name == "lo" || name.starts_with("lo0") || name.starts_with("lo1")
}

fn select_interfaces(
    interfaces: &[NetworkInterface],
    include_down: bool,
    max: usize,
) -> Vec<&NetworkInterface> {
    let mut selected: Vec<&NetworkInterface> = interfaces
        .iter()
        .filter(|iface| !is_loopback(&iface.name))
        .filter(|iface| include_down || iface.state == "up")
        .collect();
    selected.sort_by(|a, b| {
        let rate = |i: &NetworkInterface| i.rx_bytes_per_sec + i.tx_bytes_per_sec;
        rate(b)
            .partial_cmp(&rate(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    selected.truncate(max);
    selected
}

/// Per-interface download/upload sparklines with current rates.
pub struct NetSparklines<'a> {
    pub snapshot: &'a Snapshot,
    pub history: &'a History,
    pub theme: &'a Theme,
    pub config: NetSparklinesConfig,
}

#[derive(Debug, Clone)]
pub struct NetSparklinesConfig {
    pub title: String,
    pub max_interfaces: usize,
    pub include_down: bool,
}

impl Default for NetSparklinesConfig {
    fn default() -> Self {
        Self {
            title: "Network".into(),
            max_interfaces: 4,
            include_down: false,
        }
    }
}

impl Widget for NetSparklines<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let block = panel(theme, &self.config.title);
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.height < 3 || inner.is_empty() {
            return;
        }

        let capacity = (inner.height / 3) as usize;
        let interfaces = select_interfaces(
            &self.snapshot.network,
            self.config.include_down,
            self.config.max_interfaces.min(capacity.max(1)),
        );
        if interfaces.is_empty() {
            unavailable(theme, "no active interfaces").render(inner, buf);
            return;
        }

        let heights: Vec<Constraint> = interfaces
            .iter()
            .enumerate()
            .map(|(i, _)| {
                if i + 1 == interfaces.len() {
                    Constraint::Min(3)
                } else {
                    Constraint::Length(3)
                }
            })
            .collect();
        let rows = Layout::vertical(heights).split(inner);

        for (iface, row) in interfaces.iter().zip(rows.iter()) {
            let header = Line::from(vec![
                Span::styled(
                    pad_truncate(&iface.name, 10),
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("↓ ", Style::default().fg(theme.net_rx)),
                Span::styled(
                    pad_truncate(&units::rate(iface.rx_bytes_per_sec), 12),
                    Style::default().fg(theme.net_rx),
                ),
                Span::styled("↑ ", Style::default().fg(theme.net_tx)),
                Span::styled(
                    units::rate(iface.tx_bytes_per_sec),
                    Style::default().fg(theme.net_tx),
                ),
            ]);
            let header_area = Rect { height: 1, ..*row };
            Paragraph::new(header).render(header_area, buf);

            let spark_area = Rect {
                y: row.y + 1,
                height: row.height.saturating_sub(1),
                ..*row
            };
            let halves = Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
                .split(spark_area);
            let (rx_data, tx_data) = self
                .history
                .net
                .get(&iface.name)
                .map(|h| {
                    (
                        crate::common::spark_data(&h.rx),
                        crate::common::spark_data(&h.tx),
                    )
                })
                .unwrap_or_default();
            Sparkline::default()
                .data(rx_data)
                .style(Style::default().fg(theme.net_rx))
                .render(halves[0], buf);
            Sparkline::default()
                .data(tx_data)
                .style(Style::default().fg(theme.net_tx))
                .render(halves[1], buf);
        }
    }
}

/// Tabular interface details: state and cumulative counters.
pub struct NetTable<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
    pub config: NetTableConfig,
}

#[derive(Debug, Clone)]
pub struct NetTableConfig {
    pub title: String,
    pub include_down: bool,
    pub max_rows: usize,
}

impl Default for NetTableConfig {
    fn default() -> Self {
        Self {
            title: "Interfaces".into(),
            include_down: true,
            max_rows: 8,
        }
    }
}

impl Widget for NetTable<'_> {
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

        let interfaces = select_interfaces(&self.snapshot.network, self.config.include_down, 64);
        if interfaces.is_empty() {
            unavailable(theme, "no interfaces").render(inner, buf);
            return;
        }

        let header = Row::new(vec![
            Cell::from("iface"),
            Cell::from("state"),
            Cell::from("↓ rate"),
            Cell::from("↑ rate"),
            Cell::from("total ↓"),
            Cell::from("total ↑"),
        ])
        .style(
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        );

        let rows: Vec<Row> = interfaces
            .iter()
            .take(self.config.max_rows)
            .map(|iface| {
                let state_color = match iface.state.as_str() {
                    "up" => theme.ok,
                    "down" => theme.crit,
                    _ => theme.muted,
                };
                Row::new(vec![
                    Cell::from(Span::styled(
                        pad_truncate(&iface.name, 10),
                        Style::default().fg(theme.accent),
                    )),
                    Cell::from(Span::styled(
                        pad_truncate(&iface.state, 6),
                        Style::default().fg(state_color),
                    )),
                    Cell::from(Span::styled(
                        units::rate(iface.rx_bytes_per_sec),
                        Style::default().fg(theme.net_rx),
                    )),
                    Cell::from(Span::styled(
                        units::rate(iface.tx_bytes_per_sec),
                        Style::default().fg(theme.net_tx),
                    )),
                    Cell::from(units::bytes(iface.total_rx_bytes)),
                    Cell::from(units::bytes(iface.total_tx_bytes)),
                ])
            })
            .collect();

        let widths = [
            Constraint::Length(11),
            Constraint::Length(7),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(11),
            Constraint::Min(9),
        ];
        let table = Table::new(rows, widths)
            .header(header)
            .style(Style::default().fg(theme.fg).bg(theme.bg));
        Widget::render(table, inner, buf);
    }
}
