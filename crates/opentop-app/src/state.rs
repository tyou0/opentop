//! Application state: theme, sort order, selection, scroll, dirty flags.

use opentop_core::MonitorHandle;
use opentop_core::history::History;
use opentop_core::monitor::MonitorFrame;
use opentop_core::plugins::{
    Discovery, MAX_ACTIVE, MAX_ENABLED, Manifest, PluginManager, PluginRegistry, PluginResult,
    discover, resolve_roots,
};
use opentop_core::snapshot::ProcessInfo;
use opentop_theme::Theme;
use opentop_widgets::processes::{ProcessSort, ProcessTableState};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use crate::config::Config;
use crate::layout::PanelLayout;
use crate::widget_picker::WidgetPicker;
use crate::widgets::{WidgetCatalog, WidgetId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Help,
    Hardware,
    Widgets,
    ConfirmTerminate,
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
    process_selected_start_time_secs: Option<u64>,
    pub pending_process: Option<ProcessInfo>,
    pub process_action_status: Option<String>,
    process_action_rx: Option<Receiver<Result<u32, String>>>,
    pub exit: bool,
    pub focused: Option<WidgetId>,
    pub overlay: Overlay,
    pub hardware_tab: usize,
    pub hardware_scroll: u16,
    pub picker: Option<WidgetPicker>,
    pub panel_layout: PanelLayout,
    pub layout_scroll_row: usize,
    pub wrap_layout: bool,
    pub fullscreen_widget: Option<WidgetId>,
    pub plugins: PluginRegistry,
    pub frame: MonitorFrame,
    pub config: Config,
    pub config_path: Option<PathBuf>,
    pub config_error: Option<String>,
    config_load_failed: bool,
    pub catalog: WidgetCatalog,
    pub enabled_widgets: Vec<WidgetId>,
    pub cli_plugin_dirs: Vec<PathBuf>,
    pub plugin_report: Discovery,
    pub plugin_manager: PluginManager,
    pub plugin_results: std::collections::HashMap<String, PluginResult>,
    pub plugin_error: Option<String>,
    pub plugin_discovery_running: bool,
    discovery_rx: Option<Receiver<Discovery>>,
    current_manifests: Vec<Manifest>,
    dirty: bool,
}

impl App {
    pub fn new() -> Self {
        Self::from_config(Config::default(), None, None)
    }

    pub fn load(config_path: Option<PathBuf>) -> Self {
        let loaded = config_path.as_deref().map(Config::load).transpose();
        match loaded {
            Ok(Some(Some(config))) => Self::from_config(config, config_path, None),
            Ok(Some(None)) | Ok(None) => Self::from_config(Config::default(), config_path, None),
            Err(error) => Self::from_config(Config::default(), config_path, Some(error)),
        }
    }

    fn from_config(
        config: Config,
        config_path: Option<PathBuf>,
        config_error: Option<String>,
    ) -> Self {
        let enabled_widgets: Vec<WidgetId> =
            config.widgets.iter().cloned().map(WidgetId::from).collect();
        let mut catalog = WidgetCatalog::builtins();
        catalog.add_missing(&enabled_widgets);
        let focused = enabled_widgets.first().cloned();
        let config_load_failed = config_error.is_some();
        Self {
            theme: Theme::dark(),
            theme_index: 0,
            process_sort: ProcessSort::Cpu,
            process_state: ProcessTableState::default(),
            process_page_size: 10,
            process_selected_pid: None,
            process_selected_start_time_secs: None,
            pending_process: None,
            process_action_status: None,
            process_action_rx: None,
            exit: false,
            focused,
            overlay: Overlay::None,
            hardware_tab: 0,
            hardware_scroll: 0,
            picker: None,
            panel_layout: PanelLayout::default(),
            layout_scroll_row: 0,
            wrap_layout: config.wrap,
            fullscreen_widget: None,
            plugins: PluginRegistry::new(),
            frame: MonitorFrame {
                revision: 0,
                snapshot: Default::default(),
                history: History::new(120),
            },
            config,
            config_path,
            config_error,
            config_load_failed,
            catalog,
            enabled_widgets,
            cli_plugin_dirs: Vec::new(),
            plugin_report: Discovery::default(),
            plugin_manager: PluginManager::new(MAX_ACTIVE, MAX_ENABLED),
            plugin_results: std::collections::HashMap::new(),
            plugin_error: None,
            plugin_discovery_running: false,
            discovery_rx: None,
            current_manifests: Vec::new(),
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

    pub fn set_enabled_widgets(&mut self, widgets: Vec<WidgetId>) {
        let previous_index = self
            .focused
            .as_ref()
            .and_then(|id| self.enabled_widgets.iter().position(|item| item == id))
            .unwrap_or(0);
        self.enabled_widgets = widgets;
        self.catalog.add_missing(&self.enabled_widgets);
        self.config.widgets = self
            .enabled_widgets
            .iter()
            .map(|widget| widget.as_str().to_owned())
            .collect();
        let plugin_statuses: Vec<_> = self
            .plugin_report
            .manifests()
            .map(|manifest| {
                let id = WidgetId::plugin(&manifest.id);
                let status = if self.enabled_widgets.contains(&id) {
                    "waiting"
                } else {
                    "disabled"
                };
                (id, status.to_owned())
            })
            .collect();
        for (id, status) in plugin_statuses {
            if let Some(descriptor) = self.catalog.get_mut(&id) {
                descriptor.status = Some(status);
            }
        }
        self.configure_enabled_plugins();
        if !self
            .focused
            .as_ref()
            .is_some_and(|id| self.enabled_widgets.contains(id))
        {
            self.focused = self
                .enabled_widgets
                .get(previous_index.min(self.enabled_widgets.len().saturating_sub(1)))
                .cloned();
        }
        if self
            .fullscreen_widget
            .as_ref()
            .is_some_and(|id| !self.enabled_widgets.contains(id))
        {
            self.fullscreen_widget = None;
        }
        self.dirty = true;
    }

    pub fn save_config(&mut self) -> Result<(), String> {
        if self.config_load_failed {
            return Err("invalid config was not overwritten".into());
        }
        let Some(path) = self.config_path.as_deref() else {
            self.config_error = Some("no config path is available".into());
            self.dirty = true;
            return Err("no config path is available".into());
        };
        match self.config.save_atomic(path) {
            Ok(()) => {
                self.config_error = None;
                self.dirty = true;
                Ok(())
            }
            Err(error) => {
                self.config_error = Some(error.clone());
                self.dirty = true;
                Err(error)
            }
        }
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
        self.process_sort = self.process_sort.cycle();
        self.sync_process_selection();
        self.dirty = true;
    }

    pub fn reset_process_selection(&mut self) {
        self.process_selected_pid = None;
        self.process_selected_start_time_secs = None;
        self.process_state.reset();
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
        self.process_state
            .ensure_visible(self.process_len(), self.process_page_size);
        self.remember_process_selection();
        self.dirty = true;
    }

    pub fn select_process_index(&mut self, index: usize) {
        self.process_state.select_clamped(index, self.process_len());
        self.process_state
            .ensure_visible(self.process_len(), self.process_page_size);
        self.remember_process_selection();
        self.dirty = true;
    }

    pub fn select_process_home(&mut self) {
        self.select_process_index(0);
    }

    pub fn select_process_end(&mut self) {
        self.select_process_index(self.process_len().saturating_sub(1));
    }

    pub fn request_termination(&mut self) {
        if self.process_action_rx.is_some() {
            return;
        }
        self.pending_process = self
            .process_sort
            .sorted(&self.frame.snapshot.processes)
            .get(self.process_state.selected)
            .map(|process| (*process).clone());
        if self.pending_process.is_some() {
            self.overlay = Overlay::ConfirmTerminate;
            self.process_action_status = None;
        } else {
            self.process_action_status = Some("No process selected".into());
        }
        self.dirty = true;
    }

    pub fn confirm_termination(&mut self) {
        let Some(process) = self.pending_process.take() else {
            return;
        };
        self.overlay = Overlay::None;
        self.process_action_status = Some(format!("Terminating PID {}...", process.pid));
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = opentop_core::process_control::terminate(&process).map(|()| process.pid);
            let _ = tx.send(result);
        });
        self.process_action_rx = Some(rx);
        self.dirty = true;
    }

    pub fn poll_process_action(&mut self) {
        let Some(receiver) = &self.process_action_rx else {
            return;
        };
        let result = match receiver.try_recv() {
            Ok(Ok(pid)) => format!("Termination sent to PID {pid}; it may take time to exit"),
            Ok(Err(error)) => error,
            Err(TryRecvError::Disconnected) => "Process action failed".into(),
            Err(TryRecvError::Empty) => return,
        };
        self.process_action_rx = None;
        self.process_action_status = Some(result);
        self.dirty = true;
    }

    pub fn update_frame(&mut self, monitor: &MonitorHandle) -> bool {
        let Some(frame) = monitor.try_frame_since(self.frame.revision) else {
            return false;
        };
        self.apply_frame(frame);
        true
    }

    fn apply_frame(&mut self, frame: MonitorFrame) {
        let processes_changed =
            !Arc::ptr_eq(&self.frame.snapshot.processes, &frame.snapshot.processes);
        self.frame = frame;
        if processes_changed {
            self.sync_process_selection();
        }
    }

    fn remember_process_selection(&mut self) {
        let identity = self
            .process_sort
            .sorted(&self.frame.snapshot.processes)
            .get(self.process_state.selected)
            .map(|process| (process.pid, process.start_time_secs));
        self.process_selected_pid = identity.map(|(pid, _)| pid);
        self.process_selected_start_time_secs = identity.map(|(_, start)| start);
    }

    fn sync_process_selection(&mut self) {
        let processes = &self.frame.snapshot.processes;
        if processes.is_empty() {
            self.process_state.reset();
            self.process_selected_pid = None;
            self.process_selected_start_time_secs = None;
            return;
        }
        if let Some(pid) = self.process_selected_pid {
            let sorted = self.process_sort.sorted(processes);
            if let Some(index) = sorted.iter().position(|process| {
                process.pid == pid
                    && Some(process.start_time_secs) == self.process_selected_start_time_secs
            }) {
                self.process_state.follow_index(index);
            } else {
                self.process_selected_pid = None;
                self.process_selected_start_time_secs = None;
                self.process_state.top_padding = 0;
            }
        } else {
            self.process_state
                .select_clamped(self.process_state.selected, processes.len());
        }
        self.process_state
            .ensure_visible(processes.len(), self.process_page_size);
    }

    pub(crate) fn set_exit(&mut self) {
        self.exit = true;
    }

    pub fn open_help(&mut self) {
        self.overlay = Overlay::Help;
        self.dirty = true;
    }

    pub fn open_hardware(&mut self) {
        self.hardware_tab = 0;
        self.hardware_scroll = 0;
        self.overlay = Overlay::Hardware;
        self.dirty = true;
    }

    pub fn open_widgets(&mut self) {
        self.picker = Some(WidgetPicker::new(&self.enabled_widgets));
        self.overlay = Overlay::Widgets;
        self.dirty = true;
    }

    pub fn cancel_overlay(&mut self) {
        if self.overlay == Overlay::Widgets {
            self.picker = None;
        }
        self.pending_process = None;
        self.overlay = Overlay::None;
        self.dirty = true;
    }

    pub fn apply_picker(&mut self) {
        if let Some(picker) = self.picker.take() {
            self.overlay = Overlay::None;
            self.set_enabled_widgets(picker.apply());
            let _ = self.save_config();
        }
    }

    pub fn rescan_plugins(&mut self) {
        if self.discovery_rx.is_some() {
            return;
        }
        let config_dir = self
            .config_path
            .as_deref()
            .and_then(std::path::Path::parent);
        let roots = resolve_roots(&self.cli_plugin_dirs, &self.config.plugin_dirs, config_dir);
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(discover(&roots));
        });
        self.discovery_rx = Some(rx);
        self.plugin_discovery_running = true;
        self.plugin_error = None;
        self.dirty = true;
    }

    pub fn poll_plugins(&mut self) -> bool {
        if let Some(receiver) = &self.discovery_rx {
            match receiver.try_recv() {
                Ok(report) => {
                    self.discovery_rx = None;
                    self.plugin_discovery_running = false;
                    self.apply_discovery(report);
                }
                Err(TryRecvError::Disconnected) => {
                    self.discovery_rx = None;
                    self.plugin_discovery_running = false;
                    self.plugin_error = Some("plugin discovery worker stopped".into());
                    self.dirty = true;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
        self.plugin_manager.tick(opentop_core::now_ms());
        let mut changed = false;
        for result in self.plugin_manager.latest() {
            let is_new = self
                .plugin_results
                .get(&result.id)
                .is_none_or(|previous| previous.revision != result.revision);
            if is_new {
                self.plugin_results
                    .insert(result.id.clone(), result.clone());
                changed = true;
            }
        }
        if changed {
            self.dirty = true;
        }
        changed
    }

    fn apply_discovery(&mut self, report: Discovery) {
        let issue_count = report.errors.len()
            + report
                .candidates
                .iter()
                .filter(|candidate| candidate.error.is_some())
                .count();
        self.plugin_error =
            (issue_count > 0).then(|| format!("{issue_count} plugin discovery issue(s)"));
        let mut catalog = WidgetCatalog::builtins();
        let mut seen = HashSet::new();
        for candidate in &report.candidates {
            let Some(id) = candidate.id.as_deref() else {
                continue;
            };
            if !seen.insert(id.to_owned()) {
                continue;
            }
            let widget_id = WidgetId::plugin(id);
            let manifest = candidate.manifest.as_ref();
            let status = candidate.error.clone().or_else(|| {
                Some(if self.enabled_widgets.contains(&widget_id) {
                    "waiting".into()
                } else {
                    "disabled".into()
                })
            });
            catalog.insert(crate::widgets::WidgetDescriptor {
                id: widget_id,
                label: manifest.map_or_else(|| id.to_owned(), |entry| entry.name.clone()),
                description: manifest.map_or_else(
                    || {
                        candidate
                            .error
                            .clone()
                            .unwrap_or_else(|| "Plugin unavailable".into())
                    },
                    |entry| entry.description.clone(),
                ),
                source: candidate.dir.display().to_string(),
                available: manifest.is_some(),
                status,
                min_height: manifest.map_or(5, |entry| entry.min_height),
            });
        }
        catalog.add_missing(&self.enabled_widgets);
        self.catalog = catalog;
        self.plugin_report = report;
        self.configure_enabled_plugins();
        self.dirty = true;
    }

    fn configure_enabled_plugins(&mut self) {
        let manifests: Vec<Manifest> = self
            .plugin_report
            .manifests()
            .filter(|manifest| {
                self.enabled_widgets
                    .contains(&WidgetId::plugin(&manifest.id))
            })
            .cloned()
            .collect();
        if manifests == self.current_manifests {
            return;
        }
        for manifest in &manifests {
            if let Some(descriptor) = self.catalog.get_mut(&WidgetId::plugin(&manifest.id)) {
                descriptor.status = Some("waiting".into());
            }
        }
        match self.plugin_manager.set_manifests(manifests.clone()) {
            Ok(()) => {
                self.current_manifests = manifests;
                let enabled: HashSet<String> = self
                    .current_manifests
                    .iter()
                    .map(|manifest| manifest.id.clone())
                    .collect();
                self.plugin_results.retain(|id, _| enabled.contains(id));
            }
            Err(error) => {
                let _ = self.plugin_manager.set_manifests(Vec::new());
                self.current_manifests.clear();
                self.plugin_results.clear();
                self.plugin_error = Some(error);
            }
        }
        self.dirty = true;
    }

    pub fn focus_next(&mut self) {
        if self.enabled_widgets.is_empty() {
            self.focused = None;
        } else {
            let index = self
                .focused
                .as_ref()
                .and_then(|id| self.enabled_widgets.iter().position(|item| item == id))
                .unwrap_or(self.enabled_widgets.len() - 1);
            self.focused =
                Some(self.enabled_widgets[(index + 1) % self.enabled_widgets.len()].clone());
        }
        if self.fullscreen_widget.is_some() {
            self.fullscreen_widget = self.focused.clone();
        }
        self.dirty = true;
    }

    pub fn focus_prev(&mut self) {
        if self.enabled_widgets.is_empty() {
            self.focused = None;
        } else {
            let index = self
                .focused
                .as_ref()
                .and_then(|id| self.enabled_widgets.iter().position(|item| item == id))
                .unwrap_or(0);
            let previous = if index == 0 {
                self.enabled_widgets.len() - 1
            } else {
                index - 1
            };
            self.focused = Some(self.enabled_widgets[previous].clone());
        }
        if self.fullscreen_widget.is_some() {
            self.fullscreen_widget = self.focused.clone();
        }
        self.dirty = true;
    }

    pub fn set_focus(&mut self, focus: WidgetId) {
        if self.enabled_widgets.contains(&focus) {
            self.focused = Some(focus.clone());
            if self.fullscreen_widget.is_some() {
                self.fullscreen_widget = Some(focus);
            }
        }
        self.dirty = true;
    }

    pub fn is_focused(&self, id: &str) -> bool {
        self.focused
            .as_ref()
            .is_some_and(|focus| focus.as_str() == id)
    }

    fn persist_layout(&mut self) {
        if self.config_path.is_some() {
            let _ = self.save_config();
        }
        self.dirty = true;
    }

    pub fn change_columns(&mut self, delta: isize) {
        if self.fullscreen_widget.is_some() || self.enabled_widgets.is_empty() {
            return;
        }
        let current = self
            .config
            .layout
            .columns
            .unwrap_or(self.panel_layout.columns)
            .max(1);
        let columns = current
            .saturating_add_signed(delta)
            .clamp(1, self.enabled_widgets.len().min(8));
        if columns != current {
            self.config.layout.columns = Some(columns);
            self.persist_layout();
        }
    }

    pub fn reset_layout(&mut self) {
        self.config.layout = Default::default();
        self.persist_layout();
    }

    pub fn move_widget(&mut self, horizontal: isize, vertical: isize) {
        if self.fullscreen_widget.is_some() {
            return;
        }
        let Some(index) = self
            .focused
            .as_ref()
            .and_then(|id| self.enabled_widgets.iter().position(|item| item == id))
        else {
            return;
        };
        let columns = self.panel_layout.columns.max(1);
        let Some(target) = index.checked_add_signed(horizontal + vertical * columns as isize)
        else {
            return;
        };
        if target >= self.enabled_widgets.len()
            || (horizontal != 0 && index / columns != target / columns)
        {
            return;
        }
        let mut widgets = self.enabled_widgets.clone();
        widgets.swap(index, target);
        self.set_enabled_widgets(widgets);
        self.persist_layout();
    }

    pub fn resize_widget(&mut self, horizontal: bool, increase: bool) {
        if self.fullscreen_widget.is_some() {
            return;
        }
        let Some(index) = self
            .focused
            .as_ref()
            .and_then(|id| self.enabled_widgets.iter().position(|item| item == id))
        else {
            return;
        };
        let columns = self.panel_layout.columns.max(1);
        let row_start = index / columns * columns;
        let (first, second) = if horizontal {
            let row_end = (row_start + columns).min(self.enabled_widgets.len());
            if row_end - row_start < 2 {
                return;
            }
            (
                index,
                if index + 1 < row_end {
                    index + 1
                } else {
                    index - 1
                },
            )
        } else {
            let row = index / columns;
            let adjacent = self
                .panel_layout
                .panels
                .iter()
                .find(|panel| panel.row == row + 1)
                .or_else(|| {
                    row.checked_sub(1).and_then(|previous| {
                        self.panel_layout
                            .panels
                            .iter()
                            .find(|panel| panel.row == previous)
                    })
                });
            let Some(adjacent) = adjacent else { return };
            (row_start, adjacent.row * columns)
        };
        let first = self.enabled_widgets[first].as_str().to_owned();
        let second = self.enabled_widgets[second].as_str().to_owned();
        let weights = if horizontal {
            &mut self.config.layout.widths
        } else {
            &mut self.config.layout.heights
        };
        let a = weights.get(&first).copied().unwrap_or(100).clamp(10, 1000);
        let b = weights.get(&second).copied().unwrap_or(100).clamp(10, 1000);
        let amount = if increase {
            15.min(1000 - a).min(b - 10)
        } else {
            15.min(a - 10).min(1000 - b)
        };
        if amount == 0 {
            return;
        }
        weights.insert(first, if increase { a + amount } else { a - amount });
        weights.insert(second, if increase { b - amount } else { b + amount });
        self.persist_layout();
    }

    pub fn toggle_wrap(&mut self) {
        self.wrap_layout = !self.wrap_layout;
        self.dirty = true;
    }

    pub fn is_wrap_enabled(&self) -> bool {
        self.wrap_layout
    }

    pub fn toggle_fullscreen(&mut self) {
        if let Some(focused) = &self.focused {
            if self.fullscreen_widget.as_ref() == Some(focused) {
                self.fullscreen_widget = None;
            } else {
                self.fullscreen_widget = Some(focused.clone());
            }
        }
        self.dirty = true;
    }

    pub fn is_fullscreen_for(&self, id: &WidgetId) -> bool {
        self.fullscreen_widget.as_ref() == Some(id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use opentop_core::snapshot::ProcessInfo;
    use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

    fn process(pid: u32, cpu: f32, memory: u64) -> ProcessInfo {
        ProcessInfo {
            pid,
            cpu_percent: cpu,
            memory_bytes: memory,
            name: format!("process-{pid}"),
            ..ProcessInfo::default()
        }
    }

    #[test]
    fn selection_tracks_pid_through_sort_shrink_and_empty_refresh() {
        let mut app = App::new();
        app.frame.snapshot.processes = vec![
            process(10, 90.0, 10),
            process(20, 60.0, 30),
            process(30, 10.0, 20),
        ]
        .into();
        app.sync_process_selection();
        app.select_process_index(1);
        assert_eq!(app.selected_process_pid(), Some(20));

        app.cycle_sort();
        assert_eq!(app.selected_process_pid(), Some(20));
        assert_eq!(app.process_state.top_padding, 1);

        Arc::make_mut(&mut app.frame.snapshot.processes).retain(|process| process.pid != 20);
        app.sync_process_selection();
        assert_eq!(app.selected_process_pid(), Some(30));
        app.select_process_end();
        app.move_process_selection(1);
        assert_eq!(app.selected_process_pid(), Some(10));

        app.frame.snapshot.processes = Arc::default();
        app.sync_process_selection();
        assert_eq!(app.selected_process_pid(), None);
        assert_eq!(app.process_state.selected, 0);
        assert_eq!(app.process_state.offset, 0);
    }

    #[test]
    fn selected_process_keeps_its_row_while_sorting_stays_live() {
        for sort in [ProcessSort::Cpu, ProcessSort::Memory] {
            let mut app = App::new();
            app.process_sort = sort;
            app.process_page_size = 4;
            app.frame.snapshot.processes = (1..=6)
                .map(|pid| process(pid, (70 - pid * 10) as f32, (70 - pid * 10) as u64))
                .collect::<Vec<_>>()
                .into();
            app.sync_process_selection();
            app.select_process_index(3);
            assert_eq!(app.selected_process_pid(), Some(4));

            let mut frame = app.frame.clone();
            let processes = Arc::make_mut(&mut frame.snapshot.processes);
            processes.reverse();
            let selected = processes.iter_mut().find(|p| p.pid == 4).unwrap();
            selected.cpu_percent = 100.0;
            selected.memory_bytes = 100;
            processes.push(process(7, 90.0, 90));
            app.apply_frame(frame);

            assert_eq!(
                app.process_sort
                    .sorted(&app.frame.snapshot.processes)
                    .iter()
                    .map(|p| p.pid)
                    .collect::<Vec<_>>(),
                [4, 7, 1, 2, 3, 5, 6]
            );
            assert_eq!(app.selected_process_pid(), Some(4));
            assert_eq!(app.process_state.selected, 0);
            assert_eq!(app.process_state.top_padding, 3);

            let mut frame = app.frame.clone();
            let selected = Arc::make_mut(&mut frame.snapshot.processes)
                .iter_mut()
                .find(|p| p.pid == 4)
                .unwrap();
            selected.cpu_percent = 0.0;
            selected.memory_bytes = 0;
            app.apply_frame(frame);
            assert_eq!(app.selected_process_pid(), Some(4));
            assert_eq!(app.process_state.selected, 6);
            assert_eq!(app.process_state.selected - app.process_state.offset, 3);
            assert_eq!(app.process_state.top_padding, 0);

            app.reset_process_selection();
            assert_eq!(app.selected_process_pid(), Some(7));
            assert_eq!(app.process_state.offset, 0);
        }
    }

    #[test]
    fn reused_pid_does_not_keep_following_a_different_process() {
        let mut app = App::new();
        let mut original = process(42, 90.0, 10);
        original.start_time_secs = 10;
        app.frame.snapshot.processes = vec![original].into();
        app.select_process_index(0);

        let mut frame = app.frame.clone();
        Arc::make_mut(&mut frame.snapshot.processes)[0].start_time_secs = 11;
        app.apply_frame(frame);
        assert_eq!(app.process_selected_pid, None);
    }

    #[test]
    fn dashboard_keeps_selected_process_on_the_same_rendered_row() {
        use ratatui::{Terminal, backend::TestBackend};

        let mut app = App::new();
        let processes = WidgetId::builtin("processes");
        app.set_enabled_widgets(vec![processes.clone()]);
        app.set_focus(processes);
        app.toggle_fullscreen();
        app.frame.snapshot.processes = (1..=6)
            .map(|pid| process(pid, (70 - pid * 10) as f32, (70 - pid * 10) as u64))
            .collect::<Vec<_>>()
            .into();
        let mut terminal = Terminal::new(TestBackend::new(80, 10)).unwrap();
        let theme = app.theme().clone();
        terminal
            .draw(|frame| crate::dashboard::draw(frame, &mut app, &theme))
            .unwrap();
        app.select_process_index(3);

        for usage in [30.0, 100.0, 0.0] {
            let mut frame = app.frame.clone();
            Arc::make_mut(&mut frame.snapshot.processes)
                .iter_mut()
                .find(|p| p.pid == 4)
                .unwrap()
                .cpu_percent = usage;
            app.apply_frame(frame);
            terminal
                .draw(|frame| crate::dashboard::draw(frame, &mut app, &theme))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let row: String = (0..80).map(|x| buffer[(x, 6)].symbol()).collect();
            assert!(row.contains("process-4"), "{row}");
            assert_eq!(buffer[(1, 6)].bg, theme.selection_bg);
        }
    }

    #[test]
    fn live_sort_does_not_follow_the_initial_top_process() {
        let mut app = App::new();
        app.frame.snapshot.processes = vec![process(1, 90.0, 10), process(2, 10.0, 20)].into();
        app.sync_process_selection();
        assert_eq!(app.selected_process_pid(), Some(1));

        let mut frame = app.frame.clone();
        Arc::make_mut(&mut frame.snapshot.processes)[1].cpu_percent = 100.0;
        app.apply_frame(frame);
        assert_eq!(app.selected_process_pid(), Some(2));
        assert_eq!(app.process_state.selected, 0);
        assert_eq!(app.process_state.offset, 0);
    }

    #[cfg(unix)]
    #[test]
    fn plugin_discovery_does_not_execute_until_enabled() {
        use std::os::unix::fs::PermissionsExt;

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("opentop-plugin-测试-{nonce}"));
        let plugin_dir = root.join("hello");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        let marker = root.join("executed");
        let executable = plugin_dir.join("hello.sh");
        std::fs::write(
            &executable,
            format!(
                "#!/bin/sh\ntouch '{}'\nprintf '%s\\n' '{{\"protocol_version\":1,\"lines\":[\"hello\"]}}'\n",
                marker.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            "protocol_version=1\nid='example.hello'\nname='Hello'\nversion='1.0'\ndescription='test'\nexecutable='hello.sh'\nargs=[]\ninterval_ms=5000\ntimeout_ms=1000\nmin_height=5\n",
        )
        .unwrap();

        let mut app = App::new();
        app.cli_plugin_dirs = vec![root.clone()];
        app.config_path = Some(root.join("config.toml"));
        app.rescan_plugins();
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.plugin_discovery_running {
            app.poll_plugins();
            assert!(Instant::now() < deadline, "plugin discovery did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(!marker.exists());

        let mut enabled = app.enabled_widgets.clone();
        enabled.push(WidgetId::plugin("example.hello"));
        app.set_enabled_widgets(enabled);
        app.save_config().unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        while app
            .plugin_results
            .get("example.hello")
            .is_none_or(|r| r.lines != vec!["hello".to_owned()])
        {
            app.poll_plugins();
            assert!(
                Instant::now() < deadline,
                "enabled plugin result was not published"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(marker.exists());
        assert_eq!(
            app.plugin_results["example.hello"].lines,
            vec!["hello".to_owned()]
        );

        std::fs::remove_file(&marker).unwrap();
        let mut restarted = App::load(Some(root.join("config.toml")));
        restarted.cli_plugin_dirs = vec![root.clone()];
        restarted.rescan_plugins();
        let deadline = Instant::now() + Duration::from_secs(2);
        while restarted.plugin_discovery_running {
            restarted.poll_plugins();
            assert!(
                Instant::now() < deadline,
                "restart discovery did not finish"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        let deadline = Instant::now() + Duration::from_secs(15);
        while restarted
            .plugin_results
            .get("example.hello")
            .is_none_or(|r| r.lines != vec!["hello".to_owned()])
        {
            restarted.poll_plugins();
            assert!(
                Instant::now() < deadline,
                "persisted plugin did not start on restart"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(marker.exists());

        std::fs::remove_dir_all(&plugin_dir).unwrap();
        restarted.rescan_plugins();
        let deadline = Instant::now() + Duration::from_secs(2);
        while restarted.plugin_discovery_running {
            restarted.poll_plugins();
            assert!(Instant::now() < deadline, "plugin rescan did not finish");
            std::thread::sleep(Duration::from_millis(5));
        }
        let descriptor = restarted
            .catalog
            .get(&WidgetId::plugin("example.hello"))
            .unwrap();
        assert!(!descriptor.available);
        assert_eq!(descriptor.status.as_deref(), Some("missing"));
        assert!(!restarted.plugin_results.contains_key("example.hello"));
        std::fs::remove_dir_all(root).unwrap();
    }
}
