use crate::state::{App, Overlay};
use crate::widgets::WidgetId;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            return false;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.set_exit();
            return true;
        }
        if self.overlay == Overlay::Widgets {
            self.handle_picker_key(key.code);
            return false;
        }
        if self.overlay == Overlay::Help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('?')
            ) {
                self.cancel_overlay();
            }
            return false;
        }
        if self.overlay == Overlay::Hardware {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => self.cancel_overlay(),
                KeyCode::Left | KeyCode::Right => {
                    let count = crate::hardware_info::TABS.len();
                    self.hardware_tab = if key.code == KeyCode::Left {
                        (self.hardware_tab + count - 1) % count
                    } else {
                        (self.hardware_tab + 1) % count
                    };
                    self.hardware_scroll = 0;
                    self.invalidate();
                }
                KeyCode::Up | KeyCode::Down => {
                    self.hardware_scroll = if key.code == KeyCode::Up {
                        self.hardware_scroll.saturating_sub(1)
                    } else {
                        self.hardware_scroll.saturating_add(1)
                    };
                    self.invalidate();
                }
                _ => {}
            }
            return false;
        }
        if self.overlay == Overlay::ConfirmTerminate {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y')
                        if (key.modifiers & !KeyModifiers::SHIFT).is_empty() =>
                    {
                        self.confirm_termination()
                    }
                    KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => self.cancel_overlay(),
                    _ => {}
                }
            }
            return false;
        }
        if self.process_action_status.take().is_some() {
            self.invalidate();
        }
        if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
            self.set_exit();
            return true;
        }
        if key.code == KeyCode::Tab || key.code == KeyCode::BackTab {
            if key.code == KeyCode::BackTab || key.modifiers.contains(KeyModifiers::SHIFT) {
                self.focus_prev();
            } else {
                self.focus_next();
            }
            return false;
        }
        if key.modifiers.contains(KeyModifiers::SHIFT) {
            match key.code {
                KeyCode::Left => self.move_widget(-1, 0),
                KeyCode::Right => self.move_widget(1, 0),
                KeyCode::Up => self.move_widget(0, -1),
                KeyCode::Down => self.move_widget(0, 1),
                _ => {}
            }
            if matches!(
                key.code,
                KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down
            ) {
                return false;
            }
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Left => self.resize_widget(true, false),
                KeyCode::Right => self.resize_widget(true, true),
                KeyCode::Up => self.resize_widget(false, false),
                KeyCode::Down => self.resize_widget(false, true),
                _ => {}
            }
            if matches!(
                key.code,
                KeyCode::Left | KeyCode::Right | KeyCode::Up | KeyCode::Down
            ) {
                return false;
            }
        }
        match key.code {
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('i') => self.open_hardware(),
            KeyCode::Char('w') => self.open_widgets(),
            KeyCode::Char('t') => self.toggle_theme(),
            KeyCode::Char('f' | 'z') => self.toggle_fullscreen(),
            KeyCode::Char('[') => self.change_columns(-1),
            KeyCode::Char(']') => self.change_columns(1),
            KeyCode::Char('0') => self.reset_layout(),
            KeyCode::Char('x')
                if self.is_focused("builtin:processes")
                    && key.kind == KeyEventKind::Press
                    && key.modifiers.is_empty() =>
            {
                self.request_termination()
            }
            KeyCode::Char('s') if self.is_focused("builtin:processes") => self.cycle_sort(),
            KeyCode::Char('r') if self.is_focused("builtin:processes") => {
                self.reset_process_selection()
            }
            KeyCode::Left => self.focus_prev(),
            KeyCode::Right => self.focus_next(),
            KeyCode::Up | KeyCode::Char('k') if self.is_focused("builtin:processes") => {
                self.move_process_selection(-1)
            }
            KeyCode::Down | KeyCode::Char('j') if self.is_focused("builtin:processes") => {
                self.move_process_selection(1)
            }
            KeyCode::PageUp if self.is_focused("builtin:processes") => {
                self.move_process_selection(-(self.process_page_size.max(1) as isize))
            }
            KeyCode::PageDown if self.is_focused("builtin:processes") => {
                self.move_process_selection(self.process_page_size.max(1) as isize)
            }
            KeyCode::Home if self.is_focused("builtin:processes") => self.select_process_home(),
            KeyCode::End if self.is_focused("builtin:processes") => self.select_process_end(),
            _ => {}
        }
        false
    }

    fn handle_picker_key(&mut self, code: KeyCode) {
        let Some(picker) = &mut self.picker else {
            self.cancel_overlay();
            return;
        };
        match code {
            KeyCode::Esc | KeyCode::Char('q') => self.cancel_overlay(),
            KeyCode::Up | KeyCode::Char('k') => picker.move_selection(-1, &self.catalog),
            KeyCode::Down | KeyCode::Char('j') => picker.move_selection(1, &self.catalog),
            KeyCode::Char(' ') => picker.toggle_selected(),
            KeyCode::Char('+') => picker.move_selected(-1),
            KeyCode::Char('-') => picker.move_selected(1),
            KeyCode::Char('r') => picker.restore_defaults(),
            KeyCode::Enter => self.apply_picker(),
            KeyCode::F(5) => self.rescan_plugins(),
            _ => {}
        }
        self.invalidate();
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        if self.overlay != Overlay::None {
            return;
        }
        let hit = self.panel_layout.hit_test(mouse.column, mouse.row).cloned();
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if hit
                    .as_ref()
                    .is_some_and(|panel| panel.id.as_str() == "builtin:processes")
                {
                    self.set_focus(WidgetId::builtin("processes"));
                    self.move_process_selection(-1);
                } else {
                    self.focus_prev();
                }
            }
            MouseEventKind::ScrollDown => {
                if hit
                    .as_ref()
                    .is_some_and(|panel| panel.id.as_str() == "builtin:processes")
                {
                    self.set_focus(WidgetId::builtin("processes"));
                    self.move_process_selection(1);
                } else {
                    self.focus_next();
                }
            }
            MouseEventKind::Down(_) => {
                let Some(panel) = hit else { return };
                self.set_focus(panel.id.clone());
                if panel.id.as_str() == "builtin:processes" {
                    let first_data_row = panel.area.y.saturating_add(2);
                    if mouse.row >= first_data_row {
                        let data_row = (mouse.row - first_data_row) as usize;
                        if data_row < self.process_page_size
                            && let Some(row) = data_row.checked_sub(self.process_state.top_padding)
                        {
                            let index = self.process_state.offset.saturating_add(row);
                            if index < self.process_len() {
                                self.select_process_index(index);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{PanelLayout, PanelRect};
    use crossterm::event::{KeyEventState, MouseButton};
    use opentop_core::snapshot::ProcessInfo;
    use ratatui::layout::Rect;

    #[test]
    fn quit_modifiers_and_overlay_escape_are_explicit() {
        let mut app = App::new();
        assert!(!app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE)));
        assert!(!app.should_exit());
        assert!(app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)));

        let mut app = App::new();
        app.open_help();
        assert!(!app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(app.overlay, Overlay::None);
        assert!(!app.should_exit());
    }

    #[test]
    fn hardware_popup_navigation_does_not_move_focus_or_quit() {
        let mut app = App::new();
        let focus = app.focused.clone();
        app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(app.overlay, Overlay::Hardware);
        assert_eq!(app.hardware_tab, 0);

        app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE));
        assert_eq!(app.hardware_tab, crate::hardware_info::TABS.len() - 1);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.hardware_tab, 0);
        app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(app.hardware_scroll, 1);
        app.handle_key(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE));
        assert_eq!(app.hardware_tab, 1);
        assert_eq!(app.hardware_scroll, 0);
        assert_eq!(app.focused, focus);

        assert!(!app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert_eq!(app.overlay, Overlay::None);
        assert!(!app.should_exit());
        app.handle_key(KeyEvent::new(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(app.hardware_tab, 0);
    }

    #[test]
    fn backtab_and_repeat_events_navigate_focus() {
        let mut app = App::new();
        let initial = app.focused.clone();
        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_ne!(app.focused, initial);

        let before = app.focused.clone();
        let repeat = KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
            state: KeyEventState::NONE,
        };
        app.handle_key(repeat);
        assert_ne!(app.focused, before);
    }

    #[test]
    fn picker_modal_changes_are_not_applied_until_enter() {
        let mut app = App::new();
        let active = app.enabled_widgets.clone();
        app.open_widgets();
        app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
        assert_eq!(app.enabled_widgets, active);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.enabled_widgets, active);
    }

    #[test]
    fn page_moves_are_symmetric_and_process_clicks_account_for_offset() {
        let mut app = App::new();
        app.frame.snapshot.processes = (1..=20)
            .map(|pid| ProcessInfo {
                pid,
                cpu_percent: (21 - pid) as f32,
                ..ProcessInfo::default()
            })
            .collect();
        app.set_focus(WidgetId::builtin("processes"));
        app.process_page_size = 5;
        app.handle_key(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE));
        assert_eq!(app.process_state.selected, 5);
        app.handle_key(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE));
        assert_eq!(app.process_state.selected, 0);

        app.process_state.offset = 1;
        app.panel_layout = PanelLayout {
            panels: vec![PanelRect {
                id: WidgetId::builtin("processes"),
                area: Rect::new(0, 0, 50, 10),
                row: 0,
            }],
            scroll_row: 0,
            columns: 1,
        };
        let mut click = MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row: 3,
            modifiers: KeyModifiers::NONE,
        };
        app.handle_mouse(click);
        assert_eq!(app.process_state.selected, 2);

        app.process_state.offset = 0;
        app.process_state.top_padding = 2;
        app.handle_mouse(click);
        assert_eq!(app.process_state.selected, 2);
        click.row = 4;
        app.handle_mouse(click);
        assert_eq!(app.process_state.selected, 0);

        app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));
        assert_eq!(app.process_selected_pid, None);
        assert_eq!(app.process_state.selected, 0);
        assert_eq!(app.process_state.offset, 0);
        assert_eq!(app.process_state.top_padding, 0);
    }

    #[test]
    fn process_confirmation_keeps_its_target_and_ignores_repeat_or_enter() {
        let mut app = App::new();
        app.set_focus(WidgetId::builtin("processes"));
        app.frame.snapshot.processes = vec![ProcessInfo {
            pid: 123,
            name: "first".into(),
            ..Default::default()
        }];
        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(app.overlay, Overlay::ConfirmTerminate);
        app.frame.snapshot.processes = vec![ProcessInfo {
            pid: 456,
            name: "second".into(),
            ..Default::default()
        }];
        assert_eq!(app.pending_process.as_ref().unwrap().pid, 123);
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::CONTROL));
        let repeated = KeyEvent {
            code: KeyCode::Char('y'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Repeat,
            state: KeyEventState::NONE,
        };
        app.handle_key(repeated);
        assert_eq!(app.overlay, Overlay::ConfirmTerminate);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.pending_process.is_none());
        assert!(!app.should_exit());

        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            app.poll_process_action();
            if app
                .process_action_status
                .as_deref()
                .is_some_and(|s| s.contains("identity is unavailable"))
            {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.overlay, Overlay::None);
    }
}
