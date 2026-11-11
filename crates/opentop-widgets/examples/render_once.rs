//! Renders one full dashboard frame to a `TestBackend` buffer and prints it
//! as plain text. Used to verify widget output without an interactive
//! terminal, and as a smoke test for the whole stack.
//!
//! Usage: `cargo run -p opentop-widgets --example render_once [-- dark|light] [--width 140] [--height 48]`

use std::time::Duration;

use opentop_core::{MonitorConfig, MonitorHandle};
use opentop_theme::Theme;
use opentop_widgets::{
    battery::BatteryWidget,
    connections::ConnectionsSummaryWidget,
    cpu::CpuCoreBars,
    cpu::CpuFreqTiers,
    cpu::CpuGraph,
    disks::DiskHealthCard,
    disks::DiskUsageList,
    footer::{FooterKeybinds, Keybind},
    header::HeaderBar,
    memory::MemGauge,
    memory::SwapBar,
    network::NetSparklines,
    network::NetTable,
    processes::ProcessTable,
    processes::ProcessTableState,
    sensors::PowerPanel,
    sensors::SensorsPanel,
};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::layout::{Constraint, Layout};

struct Options {
    theme: String,
    width: u16,
    height: u16,
}

fn parse_options() -> Options {
    let mut options = Options {
        theme: "dark".into(),
        width: 140,
        height: 48,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--theme" | "--" if args[i] == "--theme" => {
                i += 1;
                if let Some(value) = args.get(i) {
                    options.theme = value.clone();
                }
            }
            "--width" => {
                i += 1;
                if let Some(value) = args.get(i).and_then(|v| v.parse().ok()) {
                    options.width = value;
                }
            }
            "--height" => {
                i += 1;
                if let Some(value) = args.get(i).and_then(|v| v.parse().ok()) {
                    options.height = value;
                }
            }
            _ => {}
        }
        i += 1;
    }
    options
}

fn main() {
    let options = parse_options();
    let theme = Theme::builtin(&options.theme).unwrap_or_else(|| {
        eprintln!(
            "unknown theme {:?} (expected dark or light), using dark",
            options.theme
        );
        Theme::dark()
    });

    let monitor = MonitorHandle::start(MonitorConfig::default());
    monitor.wait_ready(Duration::from_secs(4));
    // Give the one-second collectors a couple of samples for the graphs.
    std::thread::sleep(Duration::from_millis(2300));

    let snapshot = monitor.snapshot();
    let history = monitor.history();
    eprintln!(
        "samples: cpu_total={} memory={} net={} core_rings={}",
        history.cpu_total.len(),
        history.memory_percent.len(),
        history.net.len(),
        history.cpu_cores.len(),
    );

    let backend = TestBackend::new(options.width, options.height);
    let mut terminal = Terminal::new(backend).expect("terminal");
    terminal
        .draw(|frame| draw_dashboard(frame, &snapshot, &history, &theme))
        .expect("draw");

    let buffer = terminal.backend().buffer().clone();
    let width = buffer.area.width as usize;
    for row in buffer.content().chunks(width) {
        let line: String = row.iter().map(|cell| cell.symbol()).collect();
        println!("{}", line.trim_end());
    }
}

fn draw_dashboard(
    frame: &mut ratatui::Frame,
    snapshot: &opentop_core::snapshot::Snapshot,
    history: &opentop_core::history::History,
    theme: &Theme,
) {
    let area = frame.area();
    let rows = Layout::vertical([
        Constraint::Length(1),  // header
        Constraint::Length(11), // cpu / memory
        Constraint::Length(10), // sensors / power / clusters / battery
        Constraint::Length(10), // network / disks
        Constraint::Min(12),    // processes / connections
        Constraint::Length(1),  // footer
    ])
    .split(area);

    frame.render_widget(HeaderBar { snapshot, theme }, rows[0]);

    let row1 = Layout::horizontal([
        Constraint::Ratio(2, 4), // cpu graph
        Constraint::Ratio(1, 4), // core bars
        Constraint::Ratio(1, 4), // memory + swap
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
    let memory_column =
        Layout::vertical([Constraint::Min(6), Constraint::Length(4)]).split(row1[2]);
    frame.render_widget(MemGauge { snapshot, theme }, memory_column[0]);
    frame.render_widget(SwapBar { snapshot, theme }, memory_column[1]);

    let row2 = Layout::horizontal([
        Constraint::Ratio(3, 10), // sensors
        Constraint::Ratio(3, 10), // power
        Constraint::Ratio(2, 10), // clusters
        Constraint::Ratio(2, 10), // battery
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
        Constraint::Ratio(4, 10), // network sparklines
        Constraint::Ratio(3, 10), // disk usage
        Constraint::Ratio(3, 10), // disk health
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

    let row4 =
        Layout::horizontal([Constraint::Ratio(6, 10), Constraint::Ratio(4, 10)]).split(rows[4]);
    let mut process_state = ProcessTableState::default();
    frame.render_stateful_widget(
        ProcessTable {
            snapshot,
            theme,
            config: Default::default(),
        },
        row4[0],
        &mut process_state,
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
    frame.render_widget(
        FooterKeybinds {
            theme,
            binds: &binds,
        },
        rows[5],
    );
}
