use super::manifest::Manifest;
use std::collections::HashMap;
#[cfg(unix)]
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(unix)]
use crate::subprocess::{RunError, run_bounded_with_limit};

pub const MAX_ACTIVE: usize = 4;
pub const MAX_ENABLED: usize = 32;
pub const MAX_STREAM_BYTES: usize = 64 * 1024;
pub const MAX_LINES: usize = 200;
pub const MAX_LINE_SCALARS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginStatus {
    Ok,
    ExitCode(i32),
    Timeout,
    OutputOverflow,
    InvalidOutput,
    Cancelled,
    Unsupported(String),
    Disabled,
    Missing,
}

#[derive(Debug, Clone)]
pub struct PluginResult {
    pub id: String,
    pub revision: u64,
    pub at_ms: u64,
    pub status: PluginStatus,
    pub lines: Vec<String>,
    pub last_success: Option<Vec<String>>,
    pub last_success_ms: Option<u64>,
}

impl PluginResult {
    pub fn stale(&self) -> bool {
        self.last_success.is_some() && self.status != PluginStatus::Ok
    }
}

struct RuntimePlugin {
    manifest: Manifest,
    next_due: Instant,
}

struct Active {
    cancel: Arc<AtomicBool>,
    rx: Receiver<RunOutcome>,
    worker: JoinHandle<()>,
}

struct RunOutcome {
    id: String,
    at_ms: u64,
    outcome: Result<Vec<String>, PluginStatus>,
}

pub struct PluginManager {
    plugins: Vec<RuntimePlugin>,
    active: HashMap<String, Active>,
    latest: HashMap<String, PluginResult>,
    revision: u64,
    max_active: usize,
    max_enabled: usize,
}

impl PluginManager {
    pub fn new(max_active: usize, max_enabled: usize) -> Self {
        Self {
            plugins: Vec::new(),
            active: HashMap::new(),
            latest: HashMap::new(),
            revision: 0,
            max_active: max_active.clamp(1, MAX_ACTIVE),
            max_enabled: max_enabled.clamp(1, MAX_ENABLED),
        }
    }

    pub fn set_manifests(&mut self, manifests: Vec<Manifest>) -> Result<(), String> {
        if manifests.len() > self.max_enabled {
            return Err(format!(
                "{} enabled plugins exceeds the limit of {}",
                manifests.len(),
                self.max_enabled
            ));
        }
        self.cancel_active();
        let configured: std::collections::HashSet<String> = manifests
            .iter()
            .map(|manifest| manifest.id.clone())
            .collect();
        self.latest.retain(|id, _| configured.contains(id));
        self.plugins = manifests
            .into_iter()
            .map(|manifest| RuntimePlugin {
                manifest,
                next_due: Instant::now(),
            })
            .collect();
        Ok(())
    }

    pub fn remove(&mut self, id: &str) {
        self.plugins.retain(|p| p.manifest.id != id);
        self.stop(id);
        self.latest.remove(id);
    }

    pub fn tick(&mut self, now_ms: u64) {
        let mut finished = Vec::new();
        for (id, active) in &self.active {
            match active.rx.try_recv() {
                Ok(outcome) => finished.push((id.clone(), outcome)),
                Err(TryRecvError::Disconnected) => finished.push((
                    id.clone(),
                    RunOutcome {
                        id: id.clone(),
                        at_ms: now_ms,
                        outcome: Err(PluginStatus::Cancelled),
                    },
                )),
                Err(TryRecvError::Empty) => {}
            }
        }
        for (id, outcome) in finished {
            if let Some(active) = self.active.remove(&id) {
                let _ = active.worker.join();
            }
            if !matches!(&outcome.outcome, Err(PluginStatus::Cancelled)) {
                self.apply(outcome);
            }
        }
        let now = Instant::now();
        let mut started = self.active.len();
        for plugin in &mut self.plugins {
            if started >= self.max_active {
                break;
            }
            if self.active.contains_key(&plugin.manifest.id) || plugin.next_due > now {
                continue;
            }
            plugin.next_due = now + Duration::from_millis(plugin.manifest.interval_ms);
            let cancel = Arc::new(AtomicBool::new(false));
            let (tx, rx) = mpsc::sync_channel(1);
            let worker = spawn_worker(plugin.manifest.clone(), cancel.clone(), tx);
            self.active
                .insert(plugin.manifest.id.clone(), Active { cancel, rx, worker });
            started += 1;
        }
    }

    pub fn latest(&self) -> impl Iterator<Item = &PluginResult> {
        self.latest.values()
    }

    pub fn stop(&mut self, id: &str) {
        if let Some(active) = self.active.get(id) {
            active.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn stop_all(&mut self) {
        self.cancel_active();
        let active = std::mem::take(&mut self.active);
        for (_, active) in active {
            let _ = active.worker.join();
        }
    }

    fn cancel_active(&self) {
        for active in self.active.values() {
            active.cancel.store(true, Ordering::Relaxed);
        }
    }

    fn apply(&mut self, outcome: RunOutcome) {
        self.revision += 1;
        let previous = self.latest.get(&outcome.id);
        match outcome.outcome {
            Ok(lines) => {
                let result = PluginResult {
                    id: outcome.id.clone(),
                    revision: self.revision,
                    at_ms: outcome.at_ms,
                    status: PluginStatus::Ok,
                    lines: lines.clone(),
                    last_success: Some(lines),
                    last_success_ms: Some(outcome.at_ms),
                };
                self.latest.insert(outcome.id, result);
            }
            Err(status) => {
                let (lines, last_success, last_success_ms) = previous
                    .map(|p| {
                        (
                            p.last_success.clone().unwrap_or_default(),
                            p.last_success.clone(),
                            p.last_success_ms,
                        )
                    })
                    .unwrap_or_default();
                self.latest.insert(
                    outcome.id.clone(),
                    PluginResult {
                        id: outcome.id,
                        revision: self.revision,
                        at_ms: outcome.at_ms,
                        status,
                        lines,
                        last_success,
                        last_success_ms,
                    },
                );
            }
        }
    }
}

impl Drop for PluginManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}

fn spawn_worker(
    manifest: Manifest,
    cancel: Arc<AtomicBool>,
    tx: SyncSender<RunOutcome>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let outcome = run(&manifest, &cancel);
        let _ = tx.send(RunOutcome {
            id: manifest.id,
            at_ms: crate::now_ms(),
            outcome,
        });
    })
}

fn run(manifest: &Manifest, cancel: &AtomicBool) -> Result<Vec<String>, PluginStatus> {
    #[cfg(not(unix))]
    {
        let _ = (manifest, cancel);
        return Err(PluginStatus::Unsupported(
            "plugin execution is not supported on this platform".into(),
        ));
    }
    #[cfg(unix)]
    run_unix(manifest, cancel)
}

#[cfg(unix)]
fn run_unix(manifest: &Manifest, cancel: &AtomicBool) -> Result<Vec<String>, PluginStatus> {
    if cancel.load(Ordering::Relaxed) {
        return Err(PluginStatus::Cancelled);
    }
    let mut command = Command::new(&manifest.executable);
    command
        .args(&manifest.args)
        .current_dir(&manifest.dir)
        .env_clear()
        .env(
            "PATH",
            if cfg!(target_os = "macos") {
                "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin"
            } else {
                "/usr/bin:/bin:/usr/sbin:/sbin"
            },
        );
    let output = run_bounded_with_limit(
        &mut command,
        Duration::from_millis(manifest.timeout_ms),
        Some(cancel),
        MAX_STREAM_BYTES,
    );
    match output {
        Ok(output) if !output.status.success() => {
            Err(PluginStatus::ExitCode(output.status.code().unwrap_or(-1)))
        }
        Ok(output) => super::protocol::parse_output(&output.stdout),
        Err(RunError::Timeout) => Err(PluginStatus::Timeout),
        Err(RunError::Cancelled) => Err(PluginStatus::Cancelled),
        Err(RunError::OutputLimit) => Err(PluginStatus::OutputOverflow),
        Err(RunError::Spawn(_) | RunError::Wait(_)) => Err(PluginStatus::InvalidOutput),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::SystemTime;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let suffix = SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!("opentop-plugin-runtime-{suffix}"));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn script(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    fn wait_result(
        manager: &mut PluginManager,
        expected: impl Fn(&PluginResult) -> bool,
    ) -> PluginResult {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            manager.tick(crate::now_ms());
            if let Some(result) = manager.latest().find(|result| expected(result)) {
                return result.clone();
            }
            assert!(Instant::now() < deadline, "plugin result was not published");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn manifest(dir: &Path, executable: PathBuf) -> Manifest {
        Manifest {
            id: "example.hello".into(),
            name: "Hello".into(),
            version: "1.0".into(),
            description: "test".into(),
            executable,
            args: Vec::new(),
            interval_ms: 60_000,
            timeout_ms: 5_000,
            min_height: 5,
            dir: dir.to_path_buf(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn publishes_success_and_keeps_it_stale_after_failure() {
        let dir = TempDir::new();
        let success = script(
            &dir.0,
            "success.sh",
            "#!/bin/sh\nprintf '%s\\n' '{\"protocol_version\":1,\"lines\":[\"hello\"]}'\n",
        );
        let failure = script(&dir.0, "failure.sh", "#!/bin/sh\nexit 7\n");
        let mut manager = PluginManager::new(MAX_ACTIVE, MAX_ENABLED);
        manager
            .set_manifests(vec![manifest(&dir.0, success)])
            .unwrap();
        manager.tick(crate::now_ms());
        let success = wait_result(&mut manager, |result| result.status == PluginStatus::Ok);
        assert_eq!(success.lines, vec!["hello".to_owned()]);

        manager
            .set_manifests(vec![manifest(&dir.0, failure)])
            .unwrap();
        manager.tick(crate::now_ms());
        let stale = wait_result(&mut manager, |result| {
            result.status == PluginStatus::ExitCode(7)
        });
        assert!(stale.stale());
        assert_eq!(stale.lines, vec!["hello".to_owned()]);
    }

    #[cfg(unix)]
    #[test]
    fn supervisor_enforces_timeout_and_independent_stream_caps() {
        let dir = TempDir::new();
        let timeout_path = script(&dir.0, "timeout.sh", "#!/bin/sh\n(sleep 3) & exit 0\n");
        let mut timeout = manifest(&dir.0, timeout_path);
        timeout.timeout_ms = 100;
        assert!(matches!(
            run(&timeout, &AtomicBool::new(false)),
            Err(PluginStatus::Timeout)
        ));

        let stdout_path = script(&dir.0, "stdout.sh", "#!/bin/sh\nprintf '%70000s' ''\n");
        let stdout_result = run(&manifest(&dir.0, stdout_path), &AtomicBool::new(false));
        assert_eq!(stdout_result.unwrap_err(), PluginStatus::OutputOverflow);

        let stderr_path = script(&dir.0, "stderr.sh", "#!/bin/sh\nprintf '%70000s' '' >&2\n");
        assert!(matches!(
            run(&manifest(&dir.0, stderr_path), &AtomicBool::new(false)),
            Err(PluginStatus::OutputOverflow)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn removing_running_plugin_cancels_and_reaps_it() {
        let dir = TempDir::new();
        let marker = dir.0.join("started");
        let executable = script(
            &dir.0,
            "slow.sh",
            &format!("#!/bin/sh\ntouch '{}'\nsleep 3\n", marker.display()),
        );
        let mut manifest = manifest(&dir.0, executable);
        manifest.timeout_ms = 5_000;
        let mut manager = PluginManager::new(MAX_ACTIVE, MAX_ENABLED);
        manager.set_manifests(vec![manifest]).unwrap();
        manager.tick(crate::now_ms());
        let deadline = Instant::now() + Duration::from_secs(10);
        while !marker.exists() {
            assert!(Instant::now() < deadline, "plugin child did not start");
            std::thread::sleep(Duration::from_millis(5));
        }
        let started = Instant::now();
        manager.remove("example.hello");
        assert!(started.elapsed() < Duration::from_secs(1));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !manager.active.is_empty() {
            manager.tick(crate::now_ms());
            assert!(Instant::now() < deadline, "cancelled plugin was not reaped");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(manager.latest().next().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn manager_starts_at_most_four_children() {
        let dir = TempDir::new();
        let executable = script(&dir.0, "slow.sh", "#!/bin/sh\nsleep 3\n");
        let manifests = (0..5)
            .map(|index| {
                let mut manifest = manifest(&dir.0, executable.clone());
                manifest.id = format!("example.plugin{index}");
                manifest
            })
            .collect();
        let mut manager = PluginManager::new(MAX_ACTIVE, MAX_ENABLED);
        manager.set_manifests(manifests).unwrap();
        manager.tick(crate::now_ms());
        assert_eq!(manager.active.len(), MAX_ACTIVE);
        manager.stop_all();
        assert!(manager.active.is_empty());
    }

    #[test]
    fn manager_rejects_more_than_thirty_two_enabled_plugins() {
        let dir = PathBuf::from("/unused");
        let manifests = (0..=MAX_ENABLED)
            .map(|index| {
                let mut manifest = manifest(&dir, dir.join("plugin"));
                manifest.id = format!("example.plugin{index}");
                manifest
            })
            .collect();
        let mut manager = PluginManager::new(MAX_ACTIVE, MAX_ENABLED);
        assert!(manager.set_manifests(manifests).unwrap_err().contains("32"));
    }
}
