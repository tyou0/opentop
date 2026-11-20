use opentop_core::snapshot::{ProcessInfo, Snapshot};
use opentop_core::units;
use opentop_theme::Theme;
use ratatui::prelude::*;
use ratatui::widgets::{Cell, Row, StatefulWidget, Table, TableState};

use crate::common::{panel, table_header_style, unavailable};

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
            ProcessSort::Name => list.sort_by_cached_key(|p| (p.name.to_lowercase(), p.pid)),
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
    /// Whether this panel currently receives process navigation input.
    pub focused: bool,
}

impl Default for ProcessTableConfig {
    fn default() -> Self {
        Self {
            title: "Processes".into(),
            show_user: true,
            show_command: false,
            sort: ProcessSort::Cpu,
            focused: true,
        }
    }
}

/// Selection and scroll state for [`ProcessTable`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessTableState {
    pub selected: usize,
    pub offset: usize,
    /// Empty rows needed to keep the selection anchored near the top.
    pub top_padding: usize,
}

impl ProcessTableState {
    pub fn move_by(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.reset();
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
            self.reset();
        }
    }

    /// Keep the selected process on the same screen row as its rank changes.
    pub fn follow_index(&mut self, index: usize) {
        let row = self
            .selected
            .saturating_sub(self.offset)
            .saturating_add(self.top_padding);
        self.selected = index;
        self.offset = index.saturating_sub(row);
        self.top_padding = row.saturating_sub(index);
    }

    pub fn ensure_visible(&mut self, len: usize, visible: usize) {
        if len == 0 {
            self.reset();
            return;
        }
        self.selected = self.selected.min(len - 1);
        self.offset = self.offset.min(len - 1);
        if visible == 0 {
            self.top_padding = 0;
            return;
        }
        self.top_padding = self.top_padding.min(visible - 1);
        if self.selected < self.offset {
            self.offset = self.selected;
            self.top_padding = 0;
        }
        let row = (self.selected - self.offset).saturating_add(self.top_padding);
        if row >= visible {
            let shift = row - visible + 1;
            let removed_padding = shift.min(self.top_padding);
            self.top_padding -= removed_padding;
            self.offset = self.offset.saturating_add(shift - removed_padding);
        }
    }

    pub fn reset(&mut self) {
        self.selected = 0;
        self.offset = 0;
        self.top_padding = 0;
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

        let total = self.snapshot.processes.len();
        if total == 0 {
            unavailable(theme, "no processes").render(inner, buf);
            return;
        }

        let visible = inner.height.saturating_sub(1) as usize;
        if visible == 0 {
            return;
        }
        let list = self.config.sort.sorted(&self.snapshot.processes);
        state.ensure_visible(total, visible);
        let end = state
            .offset
            .saturating_add(visible - state.top_padding)
            .min(total);

        // Reserve space for the selection marker and keep CPU and memory ahead of optional fields.
        let show_cpu = inner.width >= 16;
        let show_memory = inner.width >= 25;
        let show_pid = inner.width >= 33;
        let show_user = self.config.show_user && inner.width >= 52;
        let show_command = self.config.show_command && inner.width >= 78;
        let name_width = if show_command {
            Constraint::Length(20)
        } else {
            Constraint::Min(0)
        };
        let columns = [
            (show_pid, "PID", Constraint::Length(6)),
            (show_user, "User", Constraint::Length(10)),
            (true, "Process", name_width),
            (show_cpu, "CPU%", Constraint::Length(6)),
            (show_memory, "Memory", Constraint::Length(8)),
            (show_command, "Command", Constraint::Min(16)),
        ];
        let header = Row::new(
            columns
                .iter()
                .filter_map(|&(visible, title, _)| visible.then_some(title)),
        )
        .style(table_header_style(theme));
        let widths = columns
            .iter()
            .filter_map(|&(visible, _, width)| visible.then_some(width))
            .collect::<Vec<_>>();

        let mut rows: Vec<Row> = (0..state.top_padding)
            .map(|_| Row::new(Vec::<Cell>::new()))
            .collect();
        rows.extend(list[state.offset..end].iter().map(|process| {
            let mut cells = Vec::with_capacity(widths.len());
            if show_pid {
                cells.push(Cell::from(format!("{:>6}", process.pid)));
            }
            if show_user {
                cells.push(Cell::from(Span::styled(
                    process.user.as_deref().unwrap_or("-"),
                    Style::default().fg(theme.muted),
                )));
            }
            cells.push(Cell::from(process.name.as_str()));
            if show_cpu {
                cells.push(Cell::from(Span::styled(
                    format!("{:>5.1}", process.cpu_percent),
                    Style::default().fg(theme.usage_color(process.cpu_percent.min(100.0))),
                )));
            }
            if show_memory {
                cells.push(Cell::from(units::bytes(process.memory_bytes)));
            }
            if show_command {
                cells.push(Cell::from(Span::styled(
                    process.cmd.as_str(),
                    Style::default().fg(theme.muted),
                )));
            }
            Row::new(cells)
        }));

        let selection_style = if self.config.focused {
            Style::default()
                .bg(theme.selection_bg)
                .fg(theme.selection_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().bg(theme.gauge_bg).fg(theme.fg)
        };
        let table = Table::new(rows, widths)
            .header(header)
            .row_highlight_style(selection_style)
            .highlight_symbol("▸ ")
            .style(Style::default().fg(theme.fg).bg(theme.bg));

        let mut table_state = TableState::default();
        table_state.select(Some(state.selected - state.offset + state.top_padding));
        StatefulWidget::render(table, inner, buf, &mut table_state);
    }
}
