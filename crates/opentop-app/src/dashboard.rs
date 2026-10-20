//! Dashboard composition: lays out every widget into one frame.
//!
//! The monitor is read once per frame (under a single read lock) so the
//! snapshot and history stay consistent with each other.

use opentop_theme::Theme;
use opentop_widgets::{
    battery::BatteryWidget, connections::ConnectionsSummaryWidget, cpu::CpuCoreBars,
    cpu::CpuFreqTiers, cpu::CpuGraph, disks::DiskHealthCard, disks::DiskUsageList,
    footer::{FooterKeybinds, Keybind}, header::HeaderBar, memory::MemGauge,
    memory::SwapBar, network::NetSparklines, network::NetTable, processes::ProcessTable,
    processes::ProcessTableConfig, sensors::PowerPanel,
    sensors::SensorsPanel,
};
use ratatui::layout::{Constraint, Layout};
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::state::App;

pub(crate) fn draw(frame: &mut Frame, app: &mut App, theme: &Theme) {
    let App {
        frame: monitor_frame,
        process_sort,
        process_state,
        process_page_size,
        ..
    } = app;
    let snapshot = &monitor_frame.snapshot;
    let history = &monitor_frame.history;
    let area = frame.area();
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(11),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Min(12),
        Constraint::Length(1),
    ])
    .split(area);

    frame.render_widget(HeaderBar { snapshot, theme }, rows[0]);

    let row1 = Layout::horizontal([
        Constraint::Ratio(2, 4),
        Constraint::Ratio(1, 4),
        Constraint::Ratio(1, 4),
    ])
    .split(rows[1]);
    frame.render_widget(
        CpuGraph {
            snapshot,
            history,
            theme,
            config: Default::default(),
        },
        row1[0],
    );
    frame.render_widget(CpuCoreBars { snapshot, theme }, row1[1]);
    let mem_col = Layout::vertical([Constraint::Min(6), Constraint::Length(4)]).split(row1[2]);
    frame.render_widget(MemGauge { snapshot, theme }, mem_col[0]);
    frame.render_widget(SwapBar { snapshot, theme }, mem_col[1]);

    let row2 = Layout::horizontal([
        Constraint::Ratio(3, 10),
        Constraint::Ratio(3, 10),
        Constraint::Ratio(2, 10),
        Constraint::Ratio(2, 10),
    ])
    .split(rows[2]);
    frame.render_widget(
        SensorsPanel {
            snapshot,
            theme,
            config: Default::default(),
        },
        row2[0],
    );
    frame.render_widget(
        PowerPanel {
            snapshot,
            history,
            theme,
            config: Default::default(),
        },
        row2[1],
    );
    frame.render_widget(CpuFreqTiers { snapshot, theme }, row2[2]);
    frame.render_widget(
        BatteryWidget {
            snapshot,
            theme,
            config: Default::default(),
        },
        row2[3],
    );

    let row3 = Layout::horizontal([
        Constraint::Ratio(4, 10),
        Constraint::Ratio(3, 10),
        Constraint::Ratio(3, 10),
    ])
    .split(rows[3]);
    frame.render_widget(
        NetSparklines {
            snapshot,
            history,
            theme,
            config: Default::default(),
        },
        row3[0],
    );
    frame.render_widget(
        DiskUsageList {
            snapshot,
            theme,
            config: Default::default(),
        },
        row3[1],
    );
    frame.render_widget(
        DiskHealthCard {
            snapshot,
            theme,
            config: Default::default(),
        },
        row3[2],
    );

    let row4 = Layout::horizontal([Constraint::Ratio(6, 10), Constraint::Ratio(4, 10)]).split(rows[4]);
    *process_page_size = row4[0].height.saturating_sub(3) as usize;
    frame.render_stateful_widget(
        ProcessTable {
            snapshot,
            theme,
            config: ProcessTableConfig {
                sort: *process_sort,
                ..Default::default()
            },
        },
        row4[0],
        process_state,
    );
    let side = Layout::vertical([Constraint::Length(7), Constraint::Min(5)]).split(row4[1]);
    frame.render_widget(
        ConnectionsSummaryWidget {
            snapshot,
            theme,
            config: Default::default(),
        },
        side[0],
    );
    frame.render_widget(
        NetTable {
            snapshot,
            theme,
            config: Default::default(),
        },
        side[1],
    );

    let binds = [
        Keybind::new("q", "quit"),
        Keybind::new("↑↓", "select"),
        Keybind::new("s", "sort"),
        Keybind::new("t", "theme"),
        Keybind::new("d", "details"),
    ];
    frame.render_widget(FooterKeybinds { theme, binds: &binds }, rows[5]);

    if app.show_help {
        render_help(frame, area, theme);
    }
}

fn render_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let help = Paragraph::new(Line::from(vec![
        Span::styled("opentop help", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("  press any key or click to dismiss\n"),
        Span::styled("q / Esc", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("        quit\n"),
        Span::styled("↑↓ / jk", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("      select process\n"),
        Span::styled("s", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("           cycle sort (cpu/mem/pid/name)\n"),
        Span::styled("t / Tab", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("       toggle dark/light theme\n"),
        Span::styled("PgUp / PgDn", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("     scroll process list\n"),
        Span::styled("Home / End", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("      jump to top/bottom\n"),
        Span::styled("scroll", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("           mouse wheel over process list\n"),
        Span::styled("click", Style::default().fg(theme.accent).add_modifier(Modifier::BOLD)),
        Span::raw("           select a process row\n"),
    ]))
    .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(theme.border)).title("help"))
    .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(help, area);
}