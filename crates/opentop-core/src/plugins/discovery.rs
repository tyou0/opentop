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
                report
                    .errors
                    .push(format!("plugin root is not a directory: {}", root.display()));
                continue;
            }
            Err(_) => continue,
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
        let mut dirs: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect();
        dirs.sort();
        for dir in dirs {
            let manifest_path = dir.join(MANIFEST_NAME);
            let bytes = match read_manifest(&manifest_path) {
                Ok(Some(bytes)) => bytes,
                Ok(None) => continue,
                Err(error) => {
                    report.errors.push(error);
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
            if let Some(error) = &error {
                report
                    .errors
                    .push(format!("{}: {error}", dir.display()));
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
