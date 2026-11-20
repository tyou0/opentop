use super::manifest::{self, Manifest};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const MANIFEST_NAME: &str = "plugin.toml";

#[derive(Debug, Clone)]
pub struct PluginCandidate {
    pub id: Option<String>,
    pub dir: PathBuf,
    pub manifest: Option<Manifest>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Discovery {
    pub candidates: Vec<PluginCandidate>,
    pub errors: Vec<String>,
}

impl Discovery {
    pub fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.candidates.iter().filter_map(|c| c.manifest.as_ref())
    }

    pub fn by_id(&self, id: &str) -> Option<&PluginCandidate> {
        self.candidates.iter().find(|c| c.id.as_deref() == Some(id))
    }
}

pub fn resolve_roots(
    cli_dirs: &[PathBuf],
    config_dirs: &[String],
    config_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let mut roots = Vec::new();
    roots.extend(cli_dirs.iter().map(|p| absolute(p, &cwd)));
    roots.extend(
        config_dirs
            .iter()
            .map(|p| absolute(Path::new(p), config_dir.unwrap_or(&cwd))),
    );
    if let Some(path) = std::env::var_os("OPENTOP_PLUGIN_PATH") {
        roots.extend(std::env::split_paths(&path).map(|p| absolute(&p, &cwd)));
    }
    if let Some(data) = data_home() {
        roots.push(data.join("opentop").join("plugins"));
    }
    #[cfg(unix)]
    {
        roots.push(PathBuf::from("/usr/local/lib/opentop/plugins"));
        roots.push(PathBuf::from("/usr/lib/opentop/plugins"));
    }
    #[cfg(windows)]
    {
        if let Some(program_data) = std::env::var_os("PROGRAMDATA") {
            roots.push(PathBuf::from(program_data).join("opentop").join("plugins"));
        }
    }
    dedupe_roots(roots)
}

pub fn discover(roots: &[PathBuf]) -> Discovery {
    let mut report = Discovery::default();
    let mut seen: HashMap<String, PathBuf> = HashMap::new();
    for root in roots {
        let canonical_root = match root.canonicalize() {
            Ok(path) if path.is_dir() => path,
            Ok(_) => {
                report.errors.push(format!(
                    "plugin root is not a directory: {}",
                    root.display()
                ));
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                report.errors.push(format!(
                    "cannot resolve plugin root {}: {error}",
                    root.display()
                ));
                continue;
            }
        };
        let entries = match std::fs::read_dir(&canonical_root) {
            Ok(entries) => entries,
            Err(error) => {
                report.errors.push(format!(
                    "cannot read plugin root {}: {error}",
                    canonical_root.display()
                ));
                continue;
            }
        };
        let mut dirs = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => {
                    let path = entry.path();
                    if path.is_dir() {
                        dirs.push(path);
                    }
                }
                Err(error) => report.errors.push(format!(
                    "cannot read an entry in {}: {error}",
                    canonical_root.display()
                )),
            }
        }
        dirs.sort();
        for dir in dirs {
            let manifest_path = dir.join(MANIFEST_NAME);
            let bytes = match read_manifest(&manifest_path) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => continue,
                Err(error) => {
                    report.candidates.push(PluginCandidate {
                        id: None,
                        dir,
                        manifest: None,
                        error: Some(error),
                    });
                    continue;
                }
            };
            let parsed = manifest::parse(&dir, &bytes);
            let (id, manifest, error) = match parsed {
                Ok(manifest) => (Some(manifest.id.clone()), Some(manifest), None),
                Err(error) => (error.id, None, Some(error.message)),
            };
            if let Some(id) = &id {
                if let Some(previous) = seen.get(id) {
                    report.candidates.push(PluginCandidate {
                        id: Some(id.clone()),
                        dir,
                        manifest: None,
                        error: Some(format!(
                            "duplicate plugin id {id}; using higher-priority {}",
                            previous.display()
                        )),
                    });
                    continue;
                }
                seen.insert(id.clone(), dir.clone());
            }
            report.candidates.push(PluginCandidate {
                id,
                dir,
                manifest,
                error,
            });
        }
    }
    report
}

fn read_manifest(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read {}: {error}", path.display())),
    };
    if metadata.len() > manifest::MAX_MANIFEST_BYTES as u64 {
        return Err(format!(
            "{} exceeds {} bytes",
            path.display(),
            manifest::MAX_MANIFEST_BYTES
        ));
    }
    std::fs::read(path)
        .map(Some)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

fn absolute(path: &Path, base: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

fn dedupe_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for root in roots {
        let key = root.canonicalize().unwrap_or_else(|_| root.clone());
        if seen.insert(key) {
            result.push(root);
        }
    }
    result
}

fn data_home() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
            return Some(PathBuf::from(xdg));
        }
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local").join("share"))
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let suffix = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("opentop-discovery-{suffix}-{id}"));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn add_plugin(root: &Path, folder: &str, id: &str, version: i32, marker: &Path) {
        let dir = root.join(folder);
        std::fs::create_dir_all(&dir).unwrap();
        let executable = dir.join("run.sh");
        std::fs::write(
            &executable,
            format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        std::fs::write(
            dir.join(MANIFEST_NAME),
            format!(
                "protocol_version = {version}\nid = \"{id}\"\nname = \"Example\"\nversion = \"1.0\"\ndescription = \"test\"\nexecutable = \"run.sh\"\ninterval_ms = 5000\ntimeout_ms = 1000\nmin_height = 5\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn higher_priority_duplicate_wins_without_running_code() {
        let first = TempDir::new();
        let second = TempDir::new();
        let marker = first.0.join("executed");
        add_plugin(&first.0, "one", "example.hello", 1, &marker);
        add_plugin(&second.0, "two", "example.hello", 1, &marker);

        let report = discover(&[first.0.clone(), second.0.clone()]);
        assert_eq!(report.candidates.len(), 2);
        assert!(report.candidates[0].manifest.is_some());
        assert!(
            report.candidates[1]
                .error
                .as_deref()
                .is_some_and(|error| error.contains("duplicate plugin id"))
        );
        assert!(!marker.exists());
    }

    #[test]
    fn invalid_higher_priority_id_blocks_lower_duplicate() {
        let first = TempDir::new();
        let second = TempDir::new();
        let marker = first.0.join("executed");
        add_plugin(&first.0, "broken", "example.hello", 2, &marker);
        add_plugin(&second.0, "valid", "example.hello", 1, &marker);

        let report = discover(&[first.0.clone(), second.0.clone()]);
        assert!(report.candidates[0].manifest.is_none());
        assert!(
            report.candidates[0]
                .error
                .as_deref()
                .unwrap()
                .contains("unsupported protocol")
        );
        assert!(
            report.candidates[1]
                .error
                .as_deref()
                .unwrap()
                .contains("duplicate plugin id")
        );
        assert!(!marker.exists());
    }

    #[test]
    fn malformed_higher_priority_manifest_with_readable_id_blocks_fallback() {
        let first = TempDir::new();
        let second = TempDir::new();
        let marker = first.0.join("executed");
        add_plugin(&first.0, "broken", "example.hello", 1, &marker);
        add_plugin(&second.0, "valid", "example.hello", 1, &marker);
        let manifest = first.0.join("broken").join(MANIFEST_NAME);
        let mut text = std::fs::read_to_string(&manifest).unwrap();
        text.push_str("broken = [\n");
        std::fs::write(manifest, text).unwrap();

        let report = discover(&[first.0.clone(), second.0.clone()]);
        assert_eq!(report.candidates[0].id.as_deref(), Some("example.hello"));
        assert!(
            report.candidates[0]
                .error
                .as_deref()
                .unwrap()
                .contains("invalid TOML")
        );
        assert!(
            report.candidates[1]
                .error
                .as_deref()
                .unwrap()
                .contains("duplicate plugin id")
        );
        assert!(!marker.exists());
    }

    #[test]
    fn cli_roots_precede_config_roots_and_missing_roots_are_harmless() {
        let base = TempDir::new();
        let cli = base.0.join("cli plugins");
        let config = base.0.join("configured plugins");
        let roots = resolve_roots(
            std::slice::from_ref(&cli),
            &["configured plugins".into()],
            Some(&base.0),
        );
        assert_eq!(roots[0], cli);
        assert_eq!(roots[1], config);

        let missing = discover(&[base.0.join("does-not-exist")]);
        assert!(missing.candidates.is_empty());
        assert!(missing.errors.is_empty());
    }
}
