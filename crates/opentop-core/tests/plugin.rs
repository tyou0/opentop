use opentop_core::{Plugin, PluginRegistry};

#[test]
fn test_plugin_creation() {
    let plugin = Plugin {
        name: "custom_widget".to_string(),
        version: "0.1.0".to_string(),
        description: "Adds a custom widget".to_string(),
        enabled: true,
    };
    assert_eq!(plugin.name, "custom_widget");
    assert!(plugin.enabled);
}

#[test]
fn test_registry_operations() {
    let mut registry = PluginRegistry::new();
    assert_eq!(registry.list().len(), 0);
    assert_eq!(registry.list_enabled().len(), 0);
}
