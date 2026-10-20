use opentop_core::snapshot::{ProcessInfo, Snapshot};
use opentop_core::units;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Row, StatefulWidget, Table, TableState};

use crate::common::{pad_truncate, panel, unavailable};

/// Sort order for the process table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ProcessSort {
    #[default]
    Cpu,
    Memory,
    Pid,
    Name,
}

impl ProcessSort {
    pub fn label(self) -> &'static str {
        match self {
            ProcessSort::Cpu => "cpu",
            ProcessSort::Memory => "memory",
            ProcessSort::Pid => "pid",
            ProcessSort::Name => "name",
        }
    }

    pub fn cycle(self) -> Self {
        match self {
            ProcessSort::Cpu => ProcessSort::Memory,
            ProcessSort::Memory => ProcessSort::Pid,
            ProcessSort::Pid => ProcessSort::Name,
            ProcessSort::Name => ProcessSort::Cpu,
        }
    }

    pub fn sorted(self, processes: &[ProcessInfo]) -> Vec<&ProcessInfo> {
        let mut list: Vec<&ProcessInfo> = processes.iter().collect();
        match self {
            ProcessSort::Cpu => list.sort_by(|a, b| {
                b.cpu_percent
                    .partial_cmp(&a.cpu_percent)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.pid.cmp(&b.pid))
            }),
            ProcessSort::Memory => list.sort_by(|a, b| {
                b.memory_bytes
                    .cmp(&a.memory_bytes)
                    .then_with(|| a.pid.cmp(&b.pid))
            }),
            ProcessSort::Pid => list.sort_by_key(|p| p.pid),
            ProcessSort::Name => list.sort_by(|a, b| {
                a.name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.pid.cmp(&b.pid))
            }),
        }
        list
    }
}

#[derive(Debug, Clone)]
pub struct ProcessTableConfig {
    pub title: String,
    pub show_user: bool,
    pub show_command: bool,
    pub sort: ProcessSort,
}

impl Default for ProcessTableConfig {
    fn default() -> Self {
        Self {
            title: "Processes".into(),
            show_user: true,
            show_command: false,
            sort: ProcessSort::Cpu,
        }
    }
}

/// Selection and scroll state for [`ProcessTable`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessTableState {
    pub selected: usize,
    pub offset: usize,
}

impl ProcessTableState {
    pub fn move_by(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.selected = 0;
            self.offset = 0;
            return;
        }
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(len.saturating_sub(1));
    }

    pub fn select(&mut self, index: usize) {
        self.selected = index;
    }

    pub fn select_clamped(&mut self, index: usize, len: usize) {
        self.selected = index.min(len.saturating_sub(1));
        if len == 0 {
            self.selected = 0;
        }
    }

    pub fn reset(&mut self) {
        self.selected = 0;
        self.offset = 0;
    }
}

/// Sortable process table with a highlighted, scrollable selection.
pub struct ProcessTable<'a> {
    pub snapshot: &'a Snapshot,
    pub theme: &'a Theme,
    pub config: ProcessTableConfig,
}

impl StatefulWidget for ProcessTable<'_> {
    type State = ProcessTableState;

    fn render(self, area: Rect, buf: &mut Buffer, state: &mut Self::State) {
        if area.is_empty() {
            return;
        }
        let theme = self.theme;
        let block = panel(
            theme,
            format!("{} · {}", self.config.title, self.config.sort.label()),
        );
        let inner = block.inner(area);
        block.render(area, buf);
        if inner.is_empty() {
            return;
        }

        let list = self.config.sort.sorted(&self.snapshot.processes);
        let total = list.len();
        if total == 0 {
            unavailable(theme, "no processes").render(inner, buf);
            return;
        }

        let visible = inner.height.saturating_sub(1) as usize;
        if visible == 0 {
            return;
        }
        state.selected = state.selected.min(total - 1);
        if state.selected < state.offset {
            state.offset = state.selected;
        }
        if state.selected >= state.offset + visible {
            state.offset = state.selected + 1 - visible;
        }
        state.offset = state.offset.min(total.saturating_sub(1));
        let end = (state.offset + visible).min(total);

        let header_cells: Vec<Cell> = {
            let mut cells = vec![Cell::from("pid")];
            if self.config.show_user {
                cells.push(Cell::from("user"));
            }
            cells.push(Cell::from("name"));
            cells.push(Cell::from("%cpu"));
            cells.push(Cell::from("memory"));
            if self.config.show_command {
                cells.push(Cell::from("command"));
            }
            cells
        };
        let header = Row::new(header_cells).style(
            Style::default()
                .fg(theme.muted)
                .add_modifier(Modifier::BOLD),
        );

        let rows: Vec<Row> = list[state.offset..end]
            .iter()
            .map(|process| {
                let mut cells = vec![Cell::from(format!("{:>6}", process.pid))];
                if self.config.show_user {
                    cells.push(Cell::from(Span::styled(
                        pad_truncate(process.user.as_deref().unwrap_or("-"), 10),
                        Style::default().fg(theme.muted),
                    )));
                }
                cells.push(Cell::from(Span::styled(
                    pad_truncate(&process.name, 20),
                    Style::default().fg(theme.fg),
                )));
                cells.push(Cell::from(Span::styled(
                    format!("{:>5.1}", process.cpu_percent),
                    Style::default().fg(theme.usage_color(process.cpu_percent.min(100.0))),
                )));
                cells.push(Cell::from(units::bytes(process.memory_bytes)));
                if self.config.show_command {
                    cells.push(Cell::from(Span::styled(
                        pad_truncate(&process.cmd, 64),
                        Style::default().fg(theme.muted),
                    )));
                }
                Row::new(cells)
            })
            .collect();

        let widths: Vec<Constraint> = {
            let mut widths = vec![Constraint::Length(7)];
            if self.config.show_user {
                widths.push(Constraint::Length(11));
            }
            widths.push(Constraint::Length(21));
            widths.push(Constraint::Length(6));
            widths.push(Constraint::Length(9));
            if self.config.show_command {
                widths.push(Constraint::Min(16));
            }
            widths
        };

        let table = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(
                Style::default()
                    .bg(theme.selection_bg)
                    .fg(theme.selection_fg)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▸ ")
            .style(Style::default().fg(theme.fg).bg(theme.bg));

        let mut table_state = TableState::default();
        // TableState fields are private in ratatui 0.30; drive it via
        // select_next, which selects the first row when nothing is selected.
        for _ in 0..=(state.selected - state.offset) {
            table_state.select_next();
        }
        StatefulWidget::render(table, inner, buf, &mut table_state);
    }
}
