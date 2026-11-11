use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::layout::LayoutSettings;
use crate::widgets::WidgetCatalog;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub widgets: Vec<String>,
    #[serde(default)]
    pub plugin_dirs: Vec<String>,
    #[serde(default)]
    pub wrap: bool,
    #[serde(default)]
    pub layout: LayoutSettings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: 1,
            widgets: WidgetCatalog::default_ids()
                .into_iter()
                .map(|id| id.as_str().to_owned())
                .collect(),
            plugin_dirs: Vec::new(),
            wrap: true,
            layout: LayoutSettings::default(),
        }
    }
}

impl Config {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut config: Self = toml::from_str(text).map_err(|error| error.to_string())?;
        if config.schema_version != 1 {
            return Err(format!(
                "unsupported config schema_version {}",
                config.schema_version
            ));
        }
        let mut seen = HashSet::new();
        config.widgets.retain(|id| seen.insert(id.clone()));
        Ok(config)
    }

    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text).map(Some),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("cannot read config: {error}")),
        }
    }

    pub fn save_atomic(&self, path: &Path) -> Result<(), String> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create config directory: {error}"))?;
        let name = path
            .file_name()
            .ok_or_else(|| "config path has no file name".to_string())?
            .to_string_lossy();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temporary = parent.join(format!(".{name}.{}.{}.tmp", std::process::id(), nonce));
        let contents = toml::to_string(self).map_err(|error| error.to_string())?;
        let write_result = (|| -> io::Result<()> {
            let mut file = File::create(&temporary)?;
            file.write_all(contents.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, path)
        })();
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temporary);
            return Err(format!("cannot save config: {error}"));
        }
        Ok(())
    }
}

pub fn resolve_config_path(override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = override_path {
        let cwd = std::env::current_dir().ok()?;
        return Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        });
    }
    #[cfg(unix)]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(base.join("opentop").join("config.toml"))
    }
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(|path| PathBuf::from(path).join("opentop").join("config.toml"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("opentop-config-{nonce}"));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn defaults_preserve_empty_layout_and_deduplicate_ids() {
        assert_eq!(Config::default().schema_version, 1);
        assert!(
            Config::parse("schema_version = 1\nwidgets = []\n")
                .unwrap()
                .widgets
                .is_empty()
        );
        assert_eq!(
            Config::parse("schema_version=1\nwidgets=['x','x','plugin.missing']\n")
                .unwrap()
                .widgets,
            ["x", "plugin.missing"]
        );
        assert!(Config::parse("widgets = [\n").is_err());
        assert!(Config::parse("schema_version=2\nwidgets=[]").is_err());
    }

    #[test]
    fn save_reload_round_trip_keeps_unknown_ids() {
        let dir = TempDir::new();
        let path = dir.0.join("opentop").join("config.toml");
        let config = Config {
            schema_version: 1,
            widgets: vec!["builtin:cpu_graph".into(), "plugin.missing".into()],
            plugin_dirs: vec!["plugins".into()],
            wrap: false,
            layout: LayoutSettings::default(),
        };
        config.save_atomic(&path).unwrap();
        assert_eq!(
            Config::load(&path).unwrap().unwrap().widgets,
            config.widgets
        );
        assert_eq!(
            Config::load(&path).unwrap().unwrap().plugin_dirs,
            config.plugin_dirs
        );

        let blocker = dir.0.join("not-a-directory");
        fs::write(&blocker, "preserve").unwrap();
        assert!(config.save_atomic(&blocker.join("config.toml")).is_err());
        assert_eq!(fs::read_to_string(blocker).unwrap(), "preserve");
    }
}
