use opentop_app::state::{App, WidgetFocus};

#[test]
fn test_app_initial_state() {
    let app = App::new();
    assert_eq!(app.focused, WidgetFocus::Processes);
    assert!(!app.show_help);
    assert!(!app.should_exit());
}

#[test]
fn test_focus_navigation() {
    let mut app = App::new();
    let initial = app.focused;
    app.focus_next();
    assert_ne!(app.focused, initial);
    app.focus_prev();
    assert_eq!(app.focused, initial);
}

#[test]
fn test_theme_toggle() {
    let mut app = App::new();
    let initial = app.theme_index;
    app.toggle_theme();
    assert_ne!(app.theme_index, initial);
}
