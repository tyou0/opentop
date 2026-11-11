use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use opentop_app::state::{App, Overlay};
use opentop_app::widgets::{WidgetCatalog, WidgetId};

#[test]
fn test_app_initial_state() {
    let app = App::new();
    assert_eq!(app.focused, Some(WidgetId::builtin("cpu_graph")));
    assert_eq!(app.overlay, Overlay::None);
    assert_eq!(app.enabled_widgets.len(), 14);
    assert!(!app.should_exit());
}

#[test]
fn test_focus_navigation() {
    let mut app = App::new();
    let initial = app.focused.clone();
    app.focus_next();
    assert_ne!(app.focused, initial);
    app.focus_prev();
    assert_eq!(app.focused, initial);
}

#[test]
fn config_load_keeps_empty_layout_and_missing_widget_choices() {
    let mut app = App::new();
    app.set_enabled_widgets(vec![WidgetId::from("plugin.not_installed")]);
    assert_eq!(
        app.enabled_widgets,
        [WidgetId::from("plugin.not_installed")]
    );
    assert!(
        app.catalog
            .get(&WidgetId::from("plugin.not_installed"))
            .is_some()
    );
    app.set_enabled_widgets(Vec::new());
    assert!(app.enabled_widgets.is_empty());
    assert_eq!(WidgetCatalog::default_ids().len(), 14);
}

#[test]
fn invalid_config_falls_back_without_overwriting_the_file() {
    let path = std::env::temp_dir().join(format!(
        "opentop-invalid-config-{}.toml",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::write(&path, "invalid = [").unwrap();
    let mut app = App::load(Some(path.clone()));
    assert!(app.config_error.is_some());
    assert_eq!(app.enabled_widgets.len(), 14);
    app.change_columns(1);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid = [");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn applying_widget_draft_saves_the_new_order() {
    let path = std::env::temp_dir().join(format!(
        "opentop-applied-config-{}.toml",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut app = App::load(Some(path.clone()));
    app.open_widgets();
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    assert!(
        !app.enabled_widgets
            .contains(&WidgetId::builtin("cpu_graph"))
    );
    assert_eq!(app.overlay, Overlay::None);
    let saved = opentop_app::config::Config::load(&path).unwrap().unwrap();
    assert!(!saved.widgets.contains(&"builtin:cpu_graph".to_owned()));
    std::fs::remove_file(path).unwrap();
}

#[test]
fn test_theme_toggle() {
    let mut app = App::new();
    let initial = app.theme_index;
    app.toggle_theme();
    assert_ne!(app.theme_index, initial);
}

#[test]
fn test_widget_picker_toggle() {
    let mut app = App::new();
    let active_before = app.enabled_widgets.clone();

    app.open_widgets();
    assert_eq!(app.overlay, Overlay::Widgets);
    assert!(app.picker.is_some());

    let picker = app.picker.as_ref().unwrap();
    assert!(!picker.draft().is_empty());

    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    assert_eq!(app.enabled_widgets, active_before);

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert_eq!(app.overlay, Overlay::None);
    assert!(app.picker.is_none());
}

#[test]
fn test_widget_picker_reorder() {
    let mut app = App::new();
    app.open_widgets();

    app.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
    let selected = app.picker.as_ref().unwrap().selected().cloned();
    app.handle_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE));
    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.selected(), selected.as_ref());
    assert_eq!(picker.draft().first(), selected.as_ref());

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
}

#[test]
fn test_widget_picker_apply_changes() {
    let mut app = App::new();
    let initial_enabled_count = app.enabled_widgets.len();

    app.open_widgets();
    app.handle_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE));
    app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_eq!(app.overlay, Overlay::None);
    assert!(app.picker.is_none());
    assert_ne!(app.enabled_widgets.len(), initial_enabled_count);
}

#[test]
fn test_widget_picker_restore_defaults() {
    let mut app = App::new();

    app.open_widgets();
    app.handle_key(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE));

    let picker = app.picker.as_ref().unwrap();
    assert_eq!(picker.draft().len(), 14); // Default number of built-in widgets

    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
}

#[test]
fn test_plugin_discovery_runs_on_startup() {
    use opentop_core::plugins::discover;
    use std::fs;

    let temp_dir = std::env::temp_dir().join(format!(
        "opentop-discovery-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let plugin_dir = temp_dir.join("discovered_plugin");
    fs::create_dir_all(&plugin_dir).unwrap();
    let manifest_content = r#"
protocol_version = 1
id = "discovered.plugin"
name = "Discovered Plugin"
version = "1.0.0"
description = "A plugin discovered by the system"
executable = "script.sh"
args = []
interval_ms = 5000
timeout_ms = 1000
min_height = 5
"#;
    fs::write(plugin_dir.join("plugin.toml"), manifest_content).unwrap();

    let executable_path = plugin_dir.join("script.sh");
    fs::write(
        &executable_path,
        "#!/bin/sh\necho 'Discovered and running'\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&executable_path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let discovery = discover(std::slice::from_ref(&temp_dir));

    assert_eq!(discovery.candidates.len(), 1);
    let candidate = &discovery.candidates[0];
    assert_eq!(candidate.id, Some("discovered.plugin".to_string()));
    assert!(candidate.manifest.is_some());
    assert!(discovery.errors.is_empty());
    fs::remove_dir_all(temp_dir).unwrap();
}

#[test]
fn test_plugin_panel_renders_without_starting_process() {
    use opentop_core::plugins::{PluginResult, PluginStatus};

    let mut app = App::new();

    let plugin_result = PluginResult {
        id: "test.plugin".to_string(),
        status: PluginStatus::Ok,
        lines: vec!["Plugin test output".to_string()],
        revision: 1,
        at_ms: 1,
        last_success: Some(vec!["Plugin test output".to_string()]),
        last_success_ms: Some(1),
    };

    app.plugin_results
        .insert("test.plugin".to_string(), plugin_result);
    app.enabled_widgets = vec![WidgetId::plugin("test.plugin")];
    app.catalog.add_missing(&app.enabled_widgets);

    let descriptor = app.catalog.get(&WidgetId::plugin("test.plugin"));
    assert!(descriptor.is_some());
    assert_eq!(descriptor.unwrap().label, "plugin:test.plugin");
}
