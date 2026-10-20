//! Application state: theme, sort order, selection, scroll, dirty flags.

use opentop_core::MonitorHandle;
use opentop_core::history::History;
use opentop_core::monitor::MonitorFrame;
use opentop_core::plugins::PluginRegistry;
use opentop_theme::Theme;
use opentop_widgets::processes::{ProcessSort, ProcessTableState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WidgetFocus {
    Header,
    CpuGraph,
    CpuCoreBars,
    MemGauge,
    SwapBar,
    Sensors,
    Power,
    CpuTiers,
    Battery,
    NetworkSparklines,
    DiskUsage,
    DiskHealth,
    Processes,
    Connections,
    NetworkTable,
    Footer,
}

impl WidgetFocus {
    pub const ALL: [WidgetFocus; 16] = [
        WidgetFocus::Header,
        WidgetFocus::CpuGraph,
        WidgetFocus::CpuCoreBars,
        WidgetFocus::MemGauge,
        WidgetFocus::SwapBar,
        WidgetFocus::Sensors,
        WidgetFocus::Power,
        WidgetFocus::CpuTiers,
        WidgetFocus::Battery,
        WidgetFocus::NetworkSparklines,
        WidgetFocus::DiskUsage,
        WidgetFocus::DiskHealth,
        WidgetFocus::Processes,
        WidgetFocus::Connections,
        WidgetFocus::NetworkTable,
        WidgetFocus::Footer,
    ];
}

impl WidgetFocus {
    pub fn next(self) -> Self {
        let idx = Self::ALL.iter().position(|&w| w == self).unwrap_or(0);
        Self::ALL[(idx + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        let idx = Self::ALL.iter().position(|&w| w == self).unwrap_or(0);
        if idx == 0 {
            Self::ALL[Self::ALL.len() - 1]
        } else {
            Self::ALL[idx - 1]
        }
    }
}

/// Mutable UI state that survives across frames.
#[allow(dead_code)]
pub struct App {
    pub theme: Theme,
    pub theme_index: usize,
    pub process_sort: ProcessSort,
    pub process_state: ProcessTableState,
    pub process_page_size: usize,
    pub process_selected_pid: Option<u32>,
    pub show_help: bool,
    pub exit: bool,
    pub focused: WidgetFocus,
    pub plugins: PluginRegistry,
    pub frame: MonitorFrame,
    dirty: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            theme: Theme::dark(),
            theme_index: 0,
            process_sort: ProcessSort::Cpu,
            process_state: ProcessTableState::default(),
            process_page_size: 10,
            process_selected_pid: None,
            show_help: false,
            exit: false,
            focused: WidgetFocus::Processes,
            plugins: PluginRegistry::new(),
            frame: MonitorFrame {
                revision: 0,
                snapshot: Default::default(),
                history: History::new(120),
            },
            dirty: true,
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn needs_render(&self) -> bool {
        self.dirty
    }

    pub fn mark_rendered(&mut self) {
        self.dirty = false;
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    pub fn should_exit(&self) -> bool {
        self.exit
    }

    pub fn toggle_theme(&mut self) {
        self.theme_index = (self.theme_index + 1) % 2;
        self.theme = match self.theme_index {
            0 => Theme::dark(),
            _ => Theme::light(),
        };
        self.dirty = true;
    }

    pub fn cycle_sort(&mut self) {
        let pid = self.selected_process_pid();
        self.process_sort = self.process_sort.cycle();
        self.process_selected_pid = pid;
        self.sync_process_selection();
        self.dirty = true;
    }

    pub fn process_len(&self) -> usize {
        self.frame.snapshot.processes.len()
    }

    pub fn selected_process_pid(&self) -> Option<u32> {
        self.process_sort
            .sorted(&self.frame.snapshot.processes)
            .get(self.process_state.selected)
            .map(|p| p.pid)
    }

    pub fn move_process_selection(&mut self, delta: isize) {
        self.process_state.move_by(delta, self.process_len());
        self.process_selected_pid = self.selected_process_pid();
        self.dirty = true;
    }

    pub fn select_process_index(&mut self, index: usize) {
        self.process_state
            .select_clamped(index, self.process_len());
        self.process_selected_pid = self.selected_process_pid();
        self.dirty = true;
    }

    pub fn select_process_home(&mut self) {
        self.select_process_index(0);
    }

    pub fn select_process_end(&mut self) {
        self.select_process_index(self.process_len().saturating_sub(1));
    }

    pub fn update_frame(&mut self, monitor: &MonitorHandle) -> bool {
        let Some(frame) = monitor.try_frame_since(self.frame.revision) else {
            return false;
        };
        self.frame = frame;
        self.sync_process_selection();
        self.dirty = true;
        true
    }

    fn sync_process_selection(&mut self) {
        let sorted = self.process_sort.sorted(&self.frame.snapshot.processes);
        if sorted.is_empty() {
            self.process_state.reset();
            self.process_selected_pid = None;
            return;
        }
        let index = self
            .process_selected_pid
            .and_then(|pid| sorted.iter().position(|p| p.pid == pid))
            .unwrap_or_else(|| {
                self.process_state
                    .selected
                    .min(sorted.len().saturating_sub(1))
            });
        self.process_state.select(index);
        self.process_selected_pid = Some(sorted[index].pid);
    }

    /// Expose dirty flag mutation for input handlers (same module).
    pub(crate) fn set_dirty(&mut self) {
        self.dirty = true;
    }

    pub(crate) fn set_exit(&mut self) {
        self.exit = true;
    }

    pub(crate) fn set_show_help(&mut self, value: bool) {
        self.show_help = value;
        self.dirty = true;
    }

    pub fn focus_next(&mut self) {
        self.focused = self.focused.next();
        self.dirty = true;
    }

    pub fn focus_prev(&mut self) {
        self.focused = self.focused.prev();
        self.dirty = true;
    }

    pub fn set_focus(&mut self, focus: WidgetFocus) {
        self.focused = focus;
        self.dirty = true;
    }

    pub fn plugins_mut(&mut self) -> &mut PluginRegistry {
        &mut self.plugins
    }

    pub fn plugins(&self) -> &PluginRegistry {
        &self.plugins
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}