use opentop_core::snapshot::{ProcessInfo, Snapshot};
use opentop_theme::Theme;
use opentop_widgets::processes::{
    ProcessSort, ProcessTable, ProcessTableConfig, ProcessTableState,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;

#[test]
fn test_process_sort_cycle() {
    let sort = ProcessSort::Cpu;
    let next = sort.cycle();
    assert_ne!(sort, next);
    let back = next.cycle().cycle().cycle();
    assert_eq!(sort, back);
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
        processes,
        ..Snapshot::default()
    };
    let theme = Theme::dark();
    let backend = TestBackend::new(50, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut state = ProcessTableState {
        selected: 1,
        offset: 0,
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
