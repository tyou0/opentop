use opentop_core::plugins::{MAX_ARGS, MAX_MANIFEST_BYTES, discover, parse};
use opentop_core::{Plugin, PluginRegistry};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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
    let registry = PluginRegistry::new();
    assert!(registry.list().is_empty());
    assert!(registry.list_enabled().is_empty());
}

struct PluginFixture {
    root: PathBuf,
    dir: PathBuf,
    manifest: String,
}

impl PluginFixture {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("opentop-plugin-{}-{suffix}", std::process::id()));
        let dir = root.join("my_plugin");
        std::fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("run.sh");
        std::fs::write(&executable, "#!/bin/sh\necho 'Hello from plugin'\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let manifest = r#"
protocol_version = 1
id = "my.plugin"
name = "My Plugin"
version = "1.0.0"
description = "A sample plugin"
executable = "run.sh"
args = []
interval_ms = 5000
timeout_ms = 1000
min_height = 5
"#
        .to_string();
        Self {
            root,
            dir,
            manifest,
        }
    }

    fn assert_invalid(&self, manifest: &str, expected: &str) {
        let error = parse(&self.dir, manifest.as_bytes()).unwrap_err();
        assert!(
            error.message.contains(expected),
            "unexpected error: {}",
            error.message
        );
    }
}

impl Drop for PluginFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn valid_plugin_manifest_can_be_discovered() {
    let plugin = PluginFixture::new();
    let manifest = parse(&plugin.dir, plugin.manifest.as_bytes()).unwrap();
    assert_eq!(manifest.id, "my.plugin");
    assert_eq!(manifest.name, "My Plugin");
    assert_eq!(manifest.min_height, 5);

    std::fs::write(plugin.dir.join("plugin.toml"), &plugin.manifest).unwrap();
    let discovery = discover(std::slice::from_ref(&plugin.root));
    assert!(discovery.errors.is_empty());
    assert_eq!(discovery.candidates.len(), 1);
    let candidate = &discovery.candidates[0];
    assert_eq!(candidate.id.as_deref(), Some("my.plugin"));
    assert!(candidate.manifest.is_some());
}

#[test]
fn invalid_plugin_manifests_are_rejected() {
    let plugin = PluginFixture::new();
    for (manifest, expected) in [
        (
            plugin.manifest.replace("my.plugin", "Uppercase.Invalid"),
            "lowercase",
        ),
        (
            plugin.manifest.replace("run.sh", "../escape.sh"),
            "traverse",
        ),
        (
            plugin
                .manifest
                .replace("interval_ms = 5000", "interval_ms = 999"),
            "interval_ms",
        ),
        (
            plugin
                .manifest
                .replace("timeout_ms = 1000", "timeout_ms = 10001"),
            "timeout_ms",
        ),
        (
            plugin
                .manifest
                .replace("protocol_version = 1", "protocol_version = 2"),
            "protocol_version",
        ),
        (
            format!("{}unknown_field = true\n", plugin.manifest),
            "unknown field",
        ),
    ] {
        plugin.assert_invalid(&manifest, expected);
    }

    let args = (0..=MAX_ARGS)
        .map(|_| "\"arg\"")
        .collect::<Vec<_>>()
        .join(", ");
    plugin.assert_invalid(
        &plugin
            .manifest
            .replace("args = []", &format!("args = [{args}]")),
        "args",
    );
    plugin.assert_invalid(&"x".repeat(MAX_MANIFEST_BYTES + 1), "manifest exceeds");
}
