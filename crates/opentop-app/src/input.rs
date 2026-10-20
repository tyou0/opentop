use crate::state::{App, WidgetFocus};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};

impl App {
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        let control = key.modifiers.contains(KeyModifiers::CONTROL);
        if control && key.code == KeyCode::Char('c') {
            self.set_exit();
            return true;
        }
        if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
            if self.show_help {
                self.set_show_help(false);
                return false;
            }
            self.set_exit();
            return true;
        }
        if key.code == KeyCode::BackTab
            || (key.code == KeyCode::Tab && key.modifiers.contains(KeyModifiers::SHIFT))
        {
            self.focus_prev();
            return false;
        }
        if key.code == KeyCode::Tab {
            self.focus_next();
            return false;
        }
        if key.code == KeyCode::Char('?') {
            self.set_show_help(!self.show_help);
            return false;
        }
        if key.code == KeyCode::Char('t') {
            self.toggle_theme();
            return false;
        }
        if key.code == KeyCode::Char('s') {
            self.cycle_sort();
            return false;
        }
        if key.code == KeyCode::Char('h') && (control || key.modifiers.contains(KeyModifiers::ALT)) {
            self.focus_prev();
            return false;
        }
        if key.code == KeyCode::Char('l') && (control || key.modifiers.contains(KeyModifiers::ALT)) {
            self.focus_next();
            return false;
        }

        let page = self.process_page_size.max(1) as isize;
        match key.code {
            KeyCode::Left => self.focus_prev(),
            KeyCode::Right => self.focus_next(),
            KeyCode::Up | KeyCode::Char('k') => self.move_process_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_process_selection(1),
            KeyCode::PageUp => self.move_process_selection(-page),
            KeyCode::PageDown => self.move_process_selection(page),
            KeyCode::Home => self.select_process_home(),
            KeyCode::End => self.select_process_end(),
            _ => {}
        }
        false
    }

    pub fn handle_mouse(&mut self, mouse: MouseEvent) {
        match mouse.kind {
            MouseEventKind::ScrollUp => {
                if self.focused == WidgetFocus::Processes {
                    self.move_process_selection(-1);
                } else {
                    self.focus_prev();
                }
            }
            MouseEventKind::ScrollDown => {
                if self.focused == WidgetFocus::Processes {
                    self.move_process_selection(1);
                } else {
                    self.focus_next();
                }
            }
            MouseEventKind::Down(_) => {
                if mouse.row == 0 {
                    self.set_focus(WidgetFocus::Header);
                } else if mouse.row == u16::MAX {
                    self.set_focus(WidgetFocus::Footer);
                } else {
                    self.set_focus(WidgetFocus::Processes);
                }
            }
            _ => {}
        }
    }
}
