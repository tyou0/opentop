//! Dashboard composition from the configured widget catalog.

use opentop_core::plugins::{PluginResult, PluginStatus};
use opentop_core::snapshot::Snapshot;
use opentop_theme::Theme;
use opentop_widgets::{
    battery::BatteryWidget,
    connections::ConnectionsSummaryWidget,
    cpu::{CpuCoreBars, CpuFreqTiers, CpuGraph},
    disks::{DiskHealthCard, DiskUsageList},
    footer::{FooterKeybinds, Keybind},
    header::HeaderBar,
    memory::{MemGauge, SwapBar},
    network::{NetSparklines, NetTable},
    processes::{ProcessSort, ProcessTable, ProcessTableConfig, ProcessTableState},
    sensors::{PowerPanel, SensorsPanel},
};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::*;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::layout::{self, PanelRect};
use crate::state::{App, Overlay};
use crate::widget_picker::WidgetPicker;
use crate::widgets::{WidgetCatalog, WidgetDescriptor};

pub(crate) fn draw(frame: &mut Frame, app: &mut App, theme: &Theme) {
    let area = frame.area();
    if area.is_empty() {
        return;
    }
    let header = Rect::new(area.x, area.y, area.width, area.height.min(1));
    let footer = Rect::new(
        area.x,
        area.y.saturating_add(area.height.saturating_sub(1)),
        area.width,
        area.height.min(1),
    );
    frame.render_widget(
        HeaderBar {
            snapshot: &app.frame.snapshot,
            theme,
        },
        header,
    );

    let ids = app.enabled_widgets.clone();
    let focused = app.focused.clone();
    let mut panel_layout = layout::compute(
        &ids,
        &app.catalog,
        area,
        app.layout_scroll_row,
        focused.as_ref(),
        app.wrap_layout,
        &app.config.layout,
    );
    app.layout_scroll_row = panel_layout.scroll_row;
    if let Some(fullscreen_id) = &app.fullscreen_widget {
        panel_layout.panels = vec![PanelRect {
            id: fullscreen_id.clone(),
            area: Rect::new(
                area.x,
                area.y.saturating_add(1),
                area.width,
                area.height.saturating_sub(2),
            ),
            row: 0,
        }];
        panel_layout.columns = 1;
    }
    app.panel_layout = panel_layout.clone();

    if ids.is_empty() && app.fullscreen_widget.is_none() {
        let body = Rect::new(
            area.x,
            area.y.saturating_add(1),
            area.width,
            area.height.saturating_sub(2),
        );
        frame.render_widget(
            Paragraph::new("No widgets enabled. Press w to choose widgets.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.bg)),
            body,
        );
    }

    let App {
        frame: monitor_frame,
        process_sort,
        process_state,
        process_page_size,
        process_selected_pid,
        catalog,
        config_error,
        plugin_error,
        plugin_discovery_running,
        plugin_results,
        pending_process,
        process_action_status,
        overlay,
        picker,
        ..
    } = app;
    let snapshot = &monitor_frame.snapshot;
    let history = &monitor_frame.history;
    for panel in &panel_layout.panels {
        let mut panel_theme = theme.clone();
        if focused.as_ref() == Some(&panel.id) {
            panel_theme.border = theme.accent;
        }
        render_panel(
            frame,
            panel,
            snapshot,
            history,
            *process_sort,
            process_state,
            process_page_size,
            catalog,
            plugin_results,
            &panel_theme,
        );
    }

    let mut binds = vec![
        Keybind::new("q", "quit"),
        Keybind::new("Tab", "focus"),
        Keybind::new("[]", "columns"),
    ];
    if focused
        .as_ref()
        .is_some_and(|id| id.as_str() == "builtin:processes")
    {
        binds.push(Keybind::new("x", "terminate"));
        if process_selected_pid.is_some() {
            binds.push(Keybind::new("r", "stop following"));
        }
    }
    binds.extend([
        Keybind::new("w", "widgets"),
        Keybind::new("f", "fullscreen"),
        Keybind::new("?", "help"),
    ]);
    frame.render_widget(
        FooterKeybinds {
            theme,
            binds: &binds,
        },
        footer,
    );
    let status = process_action_status.as_deref().or(config_error
        .as_deref()
        .or(plugin_error.as_deref())
        .or_else(|| (*plugin_discovery_running).then_some("Scanning plugin manifests...")));
    if let Some(error) = status {
        frame.render_widget(
            Paragraph::new(error).style(Style::default().fg(theme.accent).bg(theme.bg)),
            footer,
        );
    }

    match *overlay {
        Overlay::None => {}
        Overlay::Help => render_help(frame, area, theme),
        Overlay::ConfirmTerminate => {
            render_termination_confirmation(frame, area, pending_process.as_ref(), theme)
        }
        Overlay::Widgets => render_picker(
            frame,
            area,
            catalog,
            picker.as_ref(),
            *plugin_discovery_running,
            theme,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_panel(
    frame: &mut Frame,
    panel: &PanelRect,
    snapshot: &Snapshot,
    history: &opentop_core::History,
    process_sort: ProcessSort,
    process_state: &mut ProcessTableState,
    process_page_size: &mut usize,
    catalog: &WidgetCatalog,
    plugin_results: &std::collections::HashMap<String, PluginResult>,
    theme: &Theme,
) {
    let area = panel.area;
    match panel.id.as_str() {
        "builtin:cpu_graph" => frame.render_widget(
            CpuGraph {
                snapshot,
                history,
                theme,
                config: Default::default(),
            },
            area,
        ),
        "builtin:cpu_cores" => frame.render_widget(CpuCoreBars { snapshot, theme }, area),
        "builtin:memory" => frame.render_widget(MemGauge { snapshot, theme }, area),
        "builtin:swap" => frame.render_widget(SwapBar { snapshot, theme }, area),
        "builtin:sensors" => frame.render_widget(
            SensorsPanel {
                snapshot,
                theme,
                config: Default::default(),
            },
            area,
        ),
        "builtin:power" => frame.render_widget(
            PowerPanel {
                snapshot,
                history,
                theme,
                config: Default::default(),
            },
            area,
        ),
        "builtin:cpu_tiers" => frame.render_widget(CpuFreqTiers { snapshot, theme }, area),
        "builtin:battery" => frame.render_widget(
            BatteryWidget {
                snapshot,
                theme,
                config: Default::default(),
            },
            area,
        ),
        "builtin:network_graph" => frame.render_widget(
            NetSparklines {
                snapshot,
                history,
                theme,
                config: Default::default(),
            },
            area,
        ),
        "builtin:disk_usage" => frame.render_widget(
            DiskUsageList {
                snapshot,
                theme,
                config: Default::default(),
            },
            area,
        ),
        "builtin:disk_health" => frame.render_widget(
            DiskHealthCard {
                snapshot,
                theme,
                config: Default::default(),
            },
            area,
        ),
        "builtin:processes" => {
            *process_page_size = area.height.saturating_sub(3) as usize;
            frame.render_stateful_widget(
                ProcessTable {
                    snapshot,
                    theme,
                    config: ProcessTableConfig {
                        sort: process_sort,
                        show_command: area.width >= 80,
                        ..Default::default()
                    },
                },
                area,
                process_state,
            );
        }
        "builtin:connections" => frame.render_widget(
            ConnectionsSummaryWidget {
                snapshot,
                theme,
                config: Default::default(),
            },
            area,
        ),
        "builtin:network_table" => frame.render_widget(
            NetTable {
                snapshot,
                theme,
                config: Default::default(),
            },
            area,
        ),
        id if id.starts_with("plugin:") => render_plugin(
            frame,
            area,
            catalog.get(&panel.id),
            plugin_results.get(id.trim_start_matches("plugin:")),
            theme,
        ),
        _ => render_missing(frame, area, catalog.get(&panel.id), theme),
    }
}

fn render_plugin(
    frame: &mut Frame,
    area: Rect,
    descriptor: Option<&WidgetDescriptor>,
    result: Option<&PluginResult>,
    theme: &Theme,
) {
    let title = descriptor.map_or("Plugin", |entry| entry.label.as_str());
    let mut lines = Vec::new();
    if let Some(result) = result {
        let status = match &result.status {
            PluginStatus::Ok => "updated".to_owned(),
            PluginStatus::ExitCode(code) => format!("exit code {code}"),
            PluginStatus::Timeout => "timed out".into(),
            PluginStatus::OutputOverflow => "output limit exceeded".into(),
            PluginStatus::InvalidOutput => "invalid output".into(),
            PluginStatus::Cancelled => "cancelled".into(),
            PluginStatus::Unsupported(reason) => reason.clone(),
            PluginStatus::Disabled => "disabled".into(),
            PluginStatus::Missing => "missing".into(),
        };
        lines.push(Line::from(if result.stale() {
            format!("stale · {status}")
        } else {
            status
        }));
        lines.extend(result.lines.iter().cloned().map(Line::from));
    } else if let Some(descriptor) = descriptor {
        lines.push(Line::from(
            descriptor
                .status
                .as_deref()
                .unwrap_or("waiting for first sample"),
        ));
    } else {
        lines.push(Line::from("missing plugin"));
    }
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: true }).block(block),
        area,
    );
}

fn render_missing(
    frame: &mut Frame,
    area: Rect,
    descriptor: Option<&WidgetDescriptor>,
    theme: &Theme,
) {
    let title = descriptor.map_or("Missing widget", |entry| entry.label.as_str());
    let status = descriptor
        .and_then(|entry| entry.status.as_deref())
        .unwrap_or("not loaded");
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(Paragraph::new(status).block(block), area);
}

fn render_help(frame: &mut Frame, area: Rect, theme: &Theme) {
    let rect = centered(area, area.width.min(68), area.height.min(18));
    frame.render_widget(Clear, rect);
    let help = Paragraph::new(vec![
        Line::from(Span::styled(
            "Keyboard",
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from("q / Esc      Quit"),
        Line::from("↑↓ / j k     Select process"),
        Line::from("PgUp / PgDn  Move one page"),
        Line::from("s            Cycle process sort"),
        Line::from("r            Stop following and return to top"),
        Line::from("x            Terminate selected process (confirm first)"),
        Line::from("Tab          Focus next widget"),
        Line::from("[ / ]        Fewer / more columns"),
        Line::from("Shift+arrows Move focused widget"),
        Line::from("Alt+arrows   Resize focused widget"),
        Line::from("0            Reset layout sizes and columns"),
        Line::from("f / z        Zoom focused widget"),
        Line::from("w            Open Widgets"),
        Line::from("t            Toggle theme"),
        Line::from("Esc          Close this help"),
    ])
    .block(
        Block::default()
            .title("Help")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.border)),
    )
    .style(Style::default().fg(theme.fg).bg(theme.bg));
    frame.render_widget(help, rect);
}

fn render_termination_confirmation(
    frame: &mut Frame,
    area: Rect,
    process: Option<&opentop_core::snapshot::ProcessInfo>,
    theme: &Theme,
) {
    let rect = centered(area, area.width.min(70), area.height.min(7));
    frame.render_widget(Clear, rect);
    let Some(process) = process else { return };
    let name: String = process
        .name
        .chars()
        .filter(|ch| !ch.is_control())
        .take(60)
        .collect();
    let action = if cfg!(windows) {
        "Force terminate"
    } else {
        "Send SIGTERM to"
    };
    let text = vec![
        Line::from(format!("{action} {name} (PID {})?", process.pid)),
        Line::from(""),
        Line::from("y confirm    n / Esc cancel"),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: true })
            .block(
                Block::default()
                    .title("Confirm process action")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent)),
            )
            .style(Style::default().fg(theme.fg).bg(theme.bg)),
        rect,
    );
}

fn render_picker(
    frame: &mut Frame,
    area: Rect,
    catalog: &WidgetCatalog,
    picker: Option<&WidgetPicker>,
    discovery_running: bool,
    theme: &Theme,
) {
    let rect = centered(area, area.width.min(100), area.height.min(26));
    frame.render_widget(Clear, rect);
    let Some(picker) = picker else { return };
    let inner = Block::default()
        .title("Widgets  Space toggle  +/- reorder  Enter apply  F5 rescan  Esc cancel")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.border))
        .style(Style::default().fg(theme.fg).bg(theme.bg));
    let content = inner.inner(rect);
    frame.render_widget(inner, rect);
    let columns =
        Layout::horizontal([Constraint::Percentage(60), Constraint::Percentage(40)]).split(content);
    let items = picker.ordered_items(catalog);
    let list_items: Vec<ListItem> = items
        .iter()
        .map(|id| {
            let enabled = picker.draft().contains(id);
            let order = picker
                .draft()
                .iter()
                .position(|item| item == id)
                .map_or(String::new(), |i| format!("{:>2} ", i + 1));
            let descriptor = catalog.get(id);
            let label = descriptor.map_or(id.as_str(), |entry| entry.label.as_str());
            let availability = descriptor
                .and_then(|entry| entry.status.as_deref())
                .unwrap_or_else(|| {
                    if descriptor.is_some_and(|entry| entry.available) {
                        "ready"
                    } else {
                        "missing"
                    }
                });
            ListItem::new(format!(
                "{} [{}] {} · {}",
                order,
                if enabled { "x" } else { " " },
                label,
                availability
            ))
        })
        .collect();
    let selected = items.iter().position(|id| Some(id) == picker.selected());
    let mut list_state = ListState::default();
    list_state.select(selected);
    let list = List::new(list_items)
        .highlight_style(
            Style::default()
                .bg(theme.selection_bg)
                .fg(theme.selection_fg)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▸ ");
    frame.render_stateful_widget(list, columns[0], &mut list_state);
    let mut description = picker
        .selected()
        .and_then(|id| catalog.get(id))
        .map_or_else(
            || "No widget selected".to_owned(),
            |entry| {
                format!(
                    "{}\n\nSource: {}\nStatus: {}\n\n{}",
                    entry.label,
                    entry.source,
                    entry.status.as_deref().unwrap_or(if entry.available {
                        "ready"
                    } else {
                        "missing"
                    }),
                    entry.description
                )
            },
        );
    if picker
        .selected()
        .is_some_and(|id| id.as_str().starts_with("plugin:"))
    {
        description = format!("Plugin: runs as your user after Apply.\n\n{description}");
    }
    if discovery_running {
        description.push_str("\n\nScanning plugin directories...");
    }
    frame.render_widget(
        Paragraph::new(description).wrap(Wrap { trim: true }),
        columns[1],
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::{WidgetDescriptor, WidgetId};
    use opentop_core::plugins::{PluginResult, PluginStatus};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    #[test]
    fn catalog_dashboard_renders_at_target_and_tiny_sizes() {
        for (width, height, columns) in [(40, 12, 1), (80, 24, 2), (120, 40, 3), (160, 48, 3)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = App::new();
            app.wrap_layout = false;
            let theme = app.theme().clone();
            terminal
                .draw(|frame| draw(frame, &mut app, &theme))
                .unwrap();
            assert_eq!(
                app.panel_layout
                    .panels
                    .iter()
                    .filter(|panel| panel.row == 0)
                    .count(),
                columns
            );
        }

        let backend = TestBackend::new(1, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new();
        let theme = app.theme().clone();
        terminal
            .draw(|frame| draw(frame, &mut app, &theme))
            .unwrap();
        assert!(app.panel_layout.panels.is_empty());
    }

    #[test]
    fn plugin_panel_renders_cached_lines_without_starting_a_process() {
        let mut app = App::new();
        let id = WidgetId::plugin("example.hello");
        app.catalog.insert(WidgetDescriptor {
            id: id.clone(),
            label: "Hello".into(),
            description: "test panel".into(),
            source: "fixture".into(),
            available: true,
            status: Some("waiting".into()),
            min_height: 5,
        });
        app.set_enabled_widgets(vec![id]);
        app.wrap_layout = false;
        app.plugin_results.insert(
            "example.hello".into(),
            PluginResult {
                id: "example.hello".into(),
                revision: 1,
                at_ms: 1,
                status: PluginStatus::Ok,
                lines: vec!["cached hello output".into()],
                last_success: Some(vec!["cached hello output".into()]),
                last_success_ms: Some(1),
            },
        );
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = app.theme().clone();
        terminal
            .draw(|frame| draw(frame, &mut app, &theme))
            .unwrap();
        let text: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(text.contains("cached hello output"));
        assert!(app.plugin_manager.latest().next().is_none());
    }

    #[test]
    fn layout_keys_resize_move_and_persist_and_zoom_updates_mouse_geometry() {
        use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        use std::time::{SystemTime, UNIX_EPOCH};

        let path = std::env::temp_dir().join(format!(
            "opentop-layout-{}.toml",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut app = App::load(Some(path.clone()));
        let ids = WidgetCatalog::default_ids()[..4].to_vec();
        app.set_enabled_widgets(ids.clone());
        app.config.layout.columns = Some(2);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        let theme = app.theme().clone();
        terminal
            .draw(|frame| draw(frame, &mut app, &theme))
            .unwrap();
        let before = app.panel_layout.panels[0].area;
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::ALT));
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::ALT));
        terminal
            .draw(|frame| draw(frame, &mut app, &theme))
            .unwrap();
        assert!(app.panel_layout.panels[0].area.width > before.width);
        assert!(app.panel_layout.panels[0].area.height > before.height);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT));
        assert_eq!(app.enabled_widgets[1], ids[0]);
        let saved = crate::config::Config::load(&path).unwrap().unwrap();
        assert_eq!(saved.widgets[1], ids[0].as_str());
        assert_eq!(saved.layout.columns, Some(2));
        assert!(!saved.layout.widths.is_empty());
        assert!(!saved.layout.heights.is_empty());

        app.handle_key(KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE));
        terminal
            .draw(|frame| draw(frame, &mut app, &theme))
            .unwrap();
        assert_eq!(app.panel_layout.panels.len(), 1);
        assert_eq!(app.panel_layout.hit_test(99, 27).unwrap().id, ids[0]);
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        terminal
            .draw(|frame| draw(frame, &mut app, &theme))
            .unwrap();
        assert_eq!(app.panel_layout.panels[0].id, app.focused.clone().unwrap());
        std::fs::remove_file(path).unwrap();
    }
}
