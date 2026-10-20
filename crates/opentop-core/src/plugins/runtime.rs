use super::manifest::Manifest;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    pid: Arc<Mutex<Option<u32>>>,
    rx: Receiver<RunOutcome>,
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
        self.stop_all();
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
            self.active.remove(&id);
            self.apply(outcome);
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
            let pid = Arc::new(Mutex::new(None));
            let (tx, rx) = mpsc::channel();
            spawn_worker(plugin.manifest.clone(), cancel.clone(), pid.clone(), tx);
            self.active.insert(
                plugin.manifest.id.clone(),
                Active { cancel, pid, rx },
            );
            started += 1;
        }
    }

    pub fn latest(&self) -> impl Iterator<Item = &PluginResult> {
        self.latest.values()
    }

    pub fn stop(&mut self, id: &str) {
        if let Some(active) = self.active.remove(id) {
            active.cancel.store(true, Ordering::Relaxed);
            kill_group(&active.pid);
        }
    }

    pub fn stop_all(&mut self) {
        let ids: Vec<String> = self.active.keys().cloned().collect();
        for id in ids {
            self.stop(&id);
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
    pid_slot: Arc<Mutex<Option<u32>>>,
    tx: mpsc::Sender<RunOutcome>,
) {
    std::thread::spawn(move || {
        let outcome = run(&manifest, &cancel, &pid_slot);
        let _ = tx.send(RunOutcome {
            id: manifest.id,
            at_ms: crate::now_ms(),
            outcome,
        });
    });
}

fn run(
    manifest: &Manifest,
    cancel: &AtomicBool,
    pid_slot: &Mutex<Option<u32>>,
) -> Result<Vec<String>, PluginStatus> {
    #[cfg(not(unix))]
    {
        let _ = (manifest, cancel, pid_slot);
        return Err(PluginStatus::Unsupported(
            "plugin execution is not supported on this platform".into(),
        ));
    }
    #[cfg(unix)]
    run_unix(manifest, cancel, pid_slot)
}

#[cfg(unix)]
fn run_unix(
    manifest: &Manifest,
    cancel: &AtomicBool,
    pid_slot: &Mutex<Option<u32>>,
) -> Result<Vec<String>, PluginStatus> {
    if cancel.load(Ordering::Relaxed) {
        return Err(PluginStatus::Cancelled);
    }
    let mut command = Command::new(&manifest.executable);
    command
        .args(&manifest.args)
        .current_dir(&manifest.dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => return Err(PluginStatus::InvalidOutput),
    };
    *pid_slot.lock().unwrap() = Some(child.id());
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let (out_tx, out_rx) = mpsc::sync_channel(1);
    let (err_tx, err_rx) = mpsc::sync_channel(1);
    let out_reader = std::thread::spawn(move || {
        let _ = out_tx.send(read_bounded(stdout));
    });
    let err_reader = std::thread::spawn(move || {
        let _ = err_tx.send(read_bounded(stderr));
    });
    let deadline = Instant::now() + Duration::from_millis(manifest.timeout_ms);
    let mut child_status = None;
    let mut out = None;
    let mut err = None;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            kill_child(&mut child);
            break Err(PluginStatus::Cancelled);
        }
        if Instant::now() >= deadline {
            kill_child(&mut child);
            break Err(PluginStatus::Timeout);
        }
        if child_status.is_none() {
            match child.try_wait() {
                Ok(Some(status)) => child_status = Some(status.code()),
                Ok(None) => {}
                Err(_) => {
                    kill_child(&mut child);
                    break Err(PluginStatus::Cancelled);
                }
            }
        }
        if out.is_none() {
            out = out_rx.try_recv().ok();
        }
        if err.is_none() {
            err = err_rx.try_recv().ok();
        }
        if child_status.is_some() && out.is_some() && err.is_some() {
            break Ok(child_status.flatten());
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if status.is_err() {
        kill_group(pid_slot);
    }
    let _ = out_reader.join();
    let _ = err_reader.join();
    *pid_slot.lock().unwrap() = None;
    let (out, out_overflow) = out.unwrap_or((Vec::new(), true));
    let (_err, err_overflow) = err.unwrap_or((Vec::new(), true));
    let code = match status {
        Ok(Some(code)) => code,
        Ok(None) => return Err(PluginStatus::InvalidOutput),
        Err(status) => return Err(status),
    };
    if out_overflow || err_overflow {
        return Err(PluginStatus::OutputOverflow);
    }
    if code != 0 {
        return Err(PluginStatus::ExitCode(code));
    }
    parse_output(&out)
}

fn read_bounded(mut reader: impl Read) -> (Vec<u8>, bool) {
    let mut stored = Vec::new();
    let mut total = 0usize;
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                total = total.saturating_add(n);
                if stored.len() < MAX_STREAM_BYTES {
                    let room = MAX_STREAM_BYTES - stored.len();
                    stored.extend_from_slice(&buffer[..n.min(room)]);
                }
            }
            Err(_) => break,
        }
    }
    (stored, total > MAX_STREAM_BYTES)
}

#[derive(Deserialize)]
struct RawProtocol {
    protocol_version: i64,
    lines: Vec<String>,
}

fn parse_output(bytes: &[u8]) -> Result<Vec<String>, PluginStatus> {
    let text = std::str::from_utf8(bytes).map_err(|_| PluginStatus::InvalidOutput)?;
    let raw: RawProtocol = serde_json::from_str(text).map_err(|_| PluginStatus::InvalidOutput)?;
    if raw.protocol_version != 1 || raw.lines.len() > MAX_LINES {
        return Err(PluginStatus::InvalidOutput);
    }
    let mut lines = Vec::with_capacity(raw.lines.len());
    for line in raw.lines {
        let clean = sanitize(&line);
        if clean.chars().count() > MAX_LINE_SCALARS {
            return Err(PluginStatus::InvalidOutput);
        }
        lines.push(clean);
    }
    Ok(lines)
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .filter(|c| {
            let code = *c as u32;
            !(code < 0x20 || (0x7f..=0x9f).contains(&code))
        })
        .take(MAX_LINE_SCALARS)
        .collect()
}

#[cfg(unix)]
fn kill_child(child: &mut Child) {
    kill_group(&Mutex::new(Some(child.id())));
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn kill_group(pid: &Mutex<Option<u32>>) {
    if let Some(pid) = *pid.lock().unwrap() {
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{pid}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
