use opentop_core::snapshot::{NetworkInterface, ProcessInfo, Snapshot};
use opentop_theme::Theme;
use opentop_widgets::network::{NetTable, NetTableConfig};
use opentop_widgets::processes::{
    ProcessSort, ProcessTable, ProcessTableConfig, ProcessTableState,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

#[test]
fn interface_and_process_tables_share_theme_styles() {
    let snapshot = Snapshot {
        network: vec![NetworkInterface {
            name: "en0".into(),
            state: "up".into(),
            ..Default::default()
        }],
        processes: vec![
            ProcessInfo {
                pid: 1,
                name: "alpha".into(),
                cpu_percent: 10.0,
                ..Default::default()
            },
            ProcessInfo {
                pid: 2,
                name: "beta".into(),
                ..Default::default()
            },
        ]
        .into(),
        ..Default::default()
    };
    for theme in [Theme::dark(), Theme::light()] {
        let mut terminal = Terminal::new(TestBackend::new(160, 8)).unwrap();
        let mut state = ProcessTableState {
            selected: 1,
            ..Default::default()
        };
        terminal
            .draw(|frame| {
                frame.render_widget(
                    NetTable {
                        snapshot: &snapshot,
                        theme: &theme,
                        config: NetTableConfig::default(),
                    },
                    Rect::new(0, 0, 80, 8),
                );
                frame.render_stateful_widget(
                    ProcessTable {
                        snapshot: &snapshot,
                        theme: &theme,
                        config: ProcessTableConfig {
                            focused: false,
                            ..Default::default()
                        },
                    },
                    Rect::new(80, 0, 80, 8),
                    &mut state,
                );
            })
            .unwrap();
        let buffer = terminal.backend().buffer();
        let process_row: String = (80..160).map(|x| buffer[(x, 2)].symbol()).collect();
        let name_x = 80 + process_row.find("alpha").unwrap() as u16;
        assert_eq!(buffer[(1, 2)].fg, theme.fg);
        assert_eq!(buffer[(name_x, 2)].fg, theme.fg);
        assert_eq!(buffer[(0, 0)].bg, theme.bg);
        assert_eq!(buffer[(80, 0)].bg, theme.bg);
        for x in [1, 83] {
            assert_eq!(buffer[(x, 1)].fg, theme.muted);
            assert!(buffer[(x, 1)].modifier.contains(Modifier::BOLD));
        }
        assert_eq!(buffer[(81, 3)].bg, theme.gauge_bg);
        assert_eq!(buffer[(81, 3)].fg, theme.fg);
    }
}

#[test]
fn process_table_keeps_metrics_visible_in_a_narrow_panel() {
    let snapshot = Snapshot {
        processes: vec![ProcessInfo {
            pid: 42,
            name: "worker".into(),
            user: Some("operator".into()),
            cmd: "worker --run".into(),
            cpu_percent: 42.5,
            memory_bytes: 64 * 1024 * 1024,
            ..Default::default()
        }]
        .into(),
        ..Default::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(34, 5)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_stateful_widget(
                ProcessTable {
                    snapshot: &snapshot,
                    theme: &Theme::dark(),
                    config: ProcessTableConfig {
                        show_command: true,
                        ..Default::default()
                    },
                },
                frame.area(),
                &mut ProcessTableState::default(),
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row: String = (0..34).map(|x| buffer[(x, 2)].symbol()).collect();
    assert!(
        row.contains("worker") && row.contains("42.5") && row.contains("64.0 MB"),
        "{row}"
    );
    assert!(!row.contains("operator"), "{row}");
}

#[test]
fn test_process_sort_cycle() {
    let sort = ProcessSort::Cpu;
    let next = sort.cycle();
    assert_ne!(sort, next);
    let back = next.cycle().cycle().cycle();
    assert_eq!(sort, back);
}

#[test]
fn process_name_sort_is_case_insensitive_and_breaks_ties_by_pid() {
    let processes = vec![
        ProcessInfo {
            pid: 3,
            name: "zebra".into(),
            ..Default::default()
        },
        ProcessInfo {
            pid: 2,
            name: "Alpha".into(),
            ..Default::default()
        },
        ProcessInfo {
            pid: 1,
            name: "alpha".into(),
            ..Default::default()
        },
    ];
    assert_eq!(
        ProcessSort::Name
            .sorted(&processes)
            .iter()
            .map(|process| process.pid)
            .collect::<Vec<_>>(),
        [1, 2, 3]
    );
}

#[test]
fn process_table_keeps_top_process_on_its_selected_screen_row() {
    let snapshot = Snapshot {
        processes: vec![
            ProcessInfo {
                pid: 1,
                name: "first".into(),
                cpu_percent: 1.0,
                ..Default::default()
            },
            ProcessInfo {
                pid: 2,
                name: "busiest".into(),
                cpu_percent: 99.0,
                ..Default::default()
            },
        ]
        .into(),
        ..Default::default()
    };
    let theme = Theme::dark();
    let mut terminal = Terminal::new(TestBackend::new(60, 8)).unwrap();
    let mut state = ProcessTableState {
        top_padding: 2,
        ..Default::default()
    };
    terminal
        .draw(|frame| {
            frame.render_stateful_widget(
                ProcessTable {
                    snapshot: &snapshot,
                    theme: &theme,
                    config: ProcessTableConfig::default(),
                },
                Rect::new(0, 0, 60, 8),
                &mut state,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let selected_row: String = (0..60).map(|x| buffer[(x, 4)].symbol()).collect();
    let next_row: String = (0..60).map(|x| buffer[(x, 5)].symbol()).collect();
    assert!(selected_row.contains("busiest"), "{selected_row}");
    assert!(next_row.contains("first"), "{next_row}");
    assert_eq!(buffer[(1, 4)].bg, theme.selection_bg);
}

#[test]
fn navigating_down_from_a_padded_top_row_scrolls_smoothly() {
    let mut state = ProcessTableState {
        top_padding: 3,
        ..Default::default()
    };
    state.move_by(1, 6);
    state.ensure_visible(6, 4);
    assert_eq!((state.selected, state.offset, state.top_padding), (1, 0, 2));
    state.move_by(4, 6);
    state.ensure_visible(6, 4);
    assert_eq!((state.selected, state.offset, state.top_padding), (5, 2, 0));
}

#[test]
fn process_sort_uses_pid_ties_and_selection_highlights_the_rendered_row() {
    let processes = vec![
        ProcessInfo {
            pid: 20,
            cpu_percent: 10.0,
            ..ProcessInfo::default()
        },
        ProcessInfo {
            pid: 10,
            cpu_percent: 10.0,
            ..ProcessInfo::default()
        },
    ];
    let sorted = ProcessSort::Cpu.sorted(&processes);
    assert_eq!(
        sorted.iter().map(|process| process.pid).collect::<Vec<_>>(),
        [10, 20]
    );

    let snapshot = Snapshot {
        processes: processes.into(),
        ..Snapshot::default()
    };
    let theme = Theme::dark();
    let backend = TestBackend::new(50, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = ProcessTableState {
        selected: 1,
        offset: 0,
        ..Default::default()
    };
    terminal
        .draw(|frame| {
            frame.render_stateful_widget(
                ProcessTable {
                    snapshot: &snapshot,
                    theme: &theme,
                    config: ProcessTableConfig {
                        sort: ProcessSort::Cpu,
                        ..Default::default()
                    },
                },
                Rect::new(0, 0, 50, 10),
                &mut state,
            );
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert_eq!(buffer[(1, 3)].bg, theme.selection_bg);
}
