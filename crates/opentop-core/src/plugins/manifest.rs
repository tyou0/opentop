use serde::Deserialize;
use std::path::{Component, Path, PathBuf};

pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
pub const MAX_ID_LEN: usize = 64;
pub const MAX_NAME_LEN: usize = 64;
pub const MAX_VERSION_LEN: usize = 32;
pub const MAX_DESCRIPTION_LEN: usize = 256;
pub const MAX_ARGS: usize = 32;
pub const MAX_ARG_LEN: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub interval_ms: u64,
    pub timeout_ms: u64,
    pub min_height: u16,
    pub dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError {
    pub id: Option<String>,
    pub message: String,
}

impl ManifestError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            id: None,
            message: message.into(),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManifest {
    protocol_version: i64,
    id: String,
    name: String,
    version: String,
    description: String,
    executable: String,
    #[serde(default)]
    args: Vec<String>,
    interval_ms: i64,
    timeout_ms: i64,
    min_height: i64,
}

pub fn parse(dir: &Path, bytes: &[u8]) -> Result<Manifest, ManifestError> {
    if bytes.len() > MAX_MANIFEST_BYTES {
        return Err(ManifestError::new(format!(
            "manifest exceeds {MAX_MANIFEST_BYTES} bytes"
        )));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| ManifestError::new("manifest is not valid UTF-8"))?;
    let id = raw_id(text);
    let raw: RawManifest = toml::from_str(text).map_err(|error| {
        let mut err = ManifestError::new(format!("invalid TOML: {error}"));
        err.id = id.clone();
        err
    })?;
    let mut error = ManifestError::new("");
    error.id = Some(raw.id.clone());

    if raw.protocol_version != 1 {
        error.message = "unsupported protocol_version (expected 1)".into();
        return Err(error);
    }
    validate_id(&raw.id).map_err(|message| {
        error.message = message;
        error.clone()
    })?;
    if raw.name.is_empty() || raw.name.chars().count() > MAX_NAME_LEN {
        error.message = format!("name must be 1-{MAX_NAME_LEN} characters");
        return Err(error);
    }
    if raw.version.is_empty() || raw.version.chars().count() > MAX_VERSION_LEN {
        error.message = format!("version must be 1-{MAX_VERSION_LEN} characters");
        return Err(error);
    }
    if raw.description.chars().count() > MAX_DESCRIPTION_LEN {
        error.message = format!("description exceeds {MAX_DESCRIPTION_LEN} characters");
        return Err(error);
    }
    if raw.args.len() > MAX_ARGS {
        error.message = format!("args exceeds {MAX_ARGS} entries");
        return Err(error);
    }
    if raw.args.iter().any(|arg| arg.chars().count() > MAX_ARG_LEN) {
        error.message = format!("an argument exceeds {MAX_ARG_LEN} characters");
        return Err(error);
    }
    if !(1_000..=3_600_000).contains(&raw.interval_ms) {
        error.message = "interval_ms must be 1000-3600000".into();
        return Err(error);
    }
    if !(50..=10_000).contains(&raw.timeout_ms) || raw.timeout_ms >= raw.interval_ms {
        error.message = "timeout_ms must be 50-10000 and below interval_ms".into();
        return Err(error);
    }
    if !(3..=30).contains(&raw.min_height) {
        error.message = "min_height must be 3-30".into();
        return Err(error);
    }
    let executable = validate_executable(dir, &raw.executable).map_err(|message| {
        error.message = message;
        error
    })?;

    Ok(Manifest {
        id: raw.id,
        name: raw.name,
        version: raw.version,
        description: raw.description,
        executable,
        args: raw.args,
        interval_ms: raw.interval_ms as u64,
        timeout_ms: raw.timeout_ms as u64,
        min_height: raw.min_height as u16,
        dir: dir.to_path_buf(),
    })
}

fn raw_id(text: &str) -> Option<String> {
    let value: toml::Value = toml::from_str(text).ok()?;
    value.get("id")?.as_str().map(ToOwned::to_owned)
}

pub fn validate_id(id: &str) -> Result<(), String> {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return Err("id is empty".into());
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err("id must start with a lowercase letter or digit".into());
    }
    if id.len() > MAX_ID_LEN {
        return Err(format!("id exceeds {MAX_ID_LEN} bytes"));
    }
    if !chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-')) {
        return Err("id contains invalid characters".into());
    }
    Ok(())
}

fn validate_executable(dir: &Path, executable: &str) -> Result<PathBuf, String> {
    let relative = Path::new(executable);
    if relative.is_absolute() {
        return Err("executable must be relative".into());
    }
    if relative
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err("executable must not traverse outside the plugin directory".into());
    }
    let canonical_dir = dir
        .canonicalize()
        .map_err(|_| "plugin directory cannot be resolved".to_string())?;
    let canonical = canonical_dir
        .join(relative)
        .canonicalize()
        .map_err(|_| "executable does not exist".to_string())?;
    if !canonical.starts_with(&canonical_dir) {
        return Err("executable escapes the plugin directory".into());
    }
    if !canonical.is_file() {
        return Err("executable is not a file".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = canonical
            .metadata()
            .map_err(|_| "executable metadata is unavailable".to_string())?
            .permissions()
            .mode();
        if mode & 0o111 == 0 {
            return Err("executable is not executable".into());
        }
    }
    Ok(canonical)
}
