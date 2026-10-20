//! Plugin manifests, discovery and the bounded runtime.

mod discovery;
mod manifest;
mod runtime;

pub use discovery::{discover, resolve_roots, Discovery, PluginCandidate, MANIFEST_NAME};
pub use manifest::{
    parse, validate_id, Manifest, ManifestError, MAX_ARG_LEN, MAX_ARGS, MAX_DESCRIPTION_LEN,
    MAX_ID_LEN, MAX_MANIFEST_BYTES, MAX_NAME_LEN, MAX_VERSION_LEN,
};
pub use runtime::{
    PluginManager, PluginResult, PluginStatus, MAX_ACTIVE, MAX_ENABLED, MAX_LINE_SCALARS,
    MAX_LINES, MAX_STREAM_BYTES,
};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A plugin that can add custom functionality to opentop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plugin {
    pub name: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
}

impl Default for Plugin {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: "0.1.0".to_string(),
            description: String::new(),
            enabled: true,
        }
    }
}

/// Compatibility registry for metadata-only plugins.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginRegistry {
    plugins: HashMap<String, Plugin>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, plugin: Plugin) {
        let name = plugin.name.clone();
        self.plugins.insert(name, plugin);
    }

    pub fn unregister(&mut self, name: &str) -> Option<Plugin> {
        self.plugins.remove(name)
    }

    pub fn enable(&mut self, name: &str) {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.enabled = true;
        }
    }

    pub fn disable(&mut self, name: &str) {
        if let Some(plugin) = self.plugins.get_mut(name) {
            plugin.enabled = false;
        }
    }

    pub fn get(&self, name: &str) -> Option<&Plugin> {
        self.plugins.get(name)
    }

    pub fn list(&self) -> Vec<&Plugin> {
        self.plugins.values().collect()
    }

    pub fn list_enabled(&self) -> Vec<&Plugin> {
        self.plugins.values().filter(|p| p.enabled).collect()
    }
}
