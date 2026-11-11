//! The sampling engine: one thread per collector, merged into a shared
//! [`Snapshot`] and [`History`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::collectors::battery::BatteryCollector;
use crate::collectors::cpu_memory::CpuMemoryCollector;
use crate::collectors::disks::DiskCollector;
use crate::collectors::network::NetworkCollector;
use crate::collectors::processes::ProcessesCollector;
use crate::collectors::sensors::SensorsCollector;
use crate::collectors::system_info::SystemInfoCollector;
use crate::collectors::{Collector, sleep_until};
use crate::history::History;
use crate::now_ms;
use crate::smart::SmartCollector;
use crate::snapshot::Snapshot;

#[cfg(feature = "connections")]
use crate::collectors::connections::ConnectionsCollector;

/// Per-collector sampling cadences.
#[derive(Debug, Clone)]
pub struct Intervals {
    pub cpu: Duration,
    pub network: Duration,
    pub sensors: Duration,
    pub system: Duration,
    pub battery: Duration,
    pub processes: Duration,
    pub disks: Duration,
    pub smart: Duration,
    pub connections: Duration,
}

impl Default for Intervals {
    fn default() -> Self {
        Self {
            cpu: Duration::from_secs(1),
            network: Duration::from_secs(1),
            sensors: Duration::from_secs(1),
            system: Duration::from_secs(5),
            battery: Duration::from_secs(30),
            processes: Duration::from_secs(2),
            disks: Duration::from_secs(3),
            smart: Duration::from_secs(60),
            connections: Duration::from_secs(5),
        }
    }
}

/// Monitor configuration: cadences, history depth and table sizes.
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub intervals: Intervals,
    /// Number of samples kept per history series (graph width).
    pub history_len: usize,
    /// Maximum processes kept in the snapshot (top by CPU).
    pub top_processes: usize,
    /// Enable the disk health collector (requires smartctl at runtime).
    pub smart_enabled: bool,
    /// Enable the connections collector.
    pub connections_enabled: bool,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            intervals: Intervals::default(),
            history_len: 120,
            top_processes: 50,
            smart_enabled: true,
            connections_enabled: true,
        }
    }
}

/// The single shared state behind one lock: a consistent (snapshot, history)
/// pair. One lock avoids lock-ordering deadlocks and guarantees that readers
/// never see a snapshot from a different instant than the history beside it.
struct Shared {
    snapshot: Snapshot,
    history: History,
    revision: u64,
}

/// A consistent snapshot and history pair with the revision it represents.
#[derive(Debug, Clone)]
pub struct MonitorFrame {
    pub revision: u64,
    pub snapshot: Snapshot,
    pub history: History,
}

/// A running monitor. Dropping the handle stops and joins all threads.
pub struct MonitorHandle {
    shared: Arc<RwLock<Shared>>,
    stop: Arc<AtomicBool>,
    threads: Mutex<Vec<JoinHandle<()>>>,
    config: MonitorConfig,
}

/// One collector and its cadence, for embedding a monitor in tests or
/// performance fixtures. Normal frontends should use [`MonitorHandle::start`].
#[doc(hidden)]
pub struct CollectorSpec {
    pub name: &'static str,
    pub interval: Duration,
    /// Constructed inside the sampling thread, so it never needs to be `Send`.
    pub make: Box<dyn FnOnce() -> Box<dyn Collector> + Send + 'static>,
}

impl CollectorSpec {
    #[doc(hidden)]
    pub fn new<C: Collector + 'static>(
        name: &'static str,
        interval: Duration,
        make: impl FnOnce() -> C + Send + 'static,
    ) -> Self {
        Self {
            name,
            interval,
            make: Box::new(move || Box::new(make())),
        }
    }
}

impl MonitorHandle {
    /// Start collecting immediately. The first samples (including a short
    /// internal priming for CPU/process deltas) land within a second or so.
    pub fn start(config: MonitorConfig) -> Self {
        let (shared, stop) = new_shared(&config);
        let mut threads = Vec::new();

        let intervals = config.intervals.clone();

        threads.push(spawn(
            "cpu",
            intervals.cpu,
            shared.clone(),
            stop.clone(),
            || Box::new(CpuMemoryCollector::new()),
        ));
        threads.push(spawn(
            "network",
            intervals.network,
            shared.clone(),
            stop.clone(),
            || Box::new(NetworkCollector::new()),
        ));
        threads.push(spawn(
            "sensors",
            intervals.sensors,
            shared.clone(),
            stop.clone(),
            || Box::new(SensorsCollector::new()),
        ));
        threads.push(spawn(
            "system",
            intervals.system,
            shared.clone(),
            stop.clone(),
            || Box::new(SystemInfoCollector::new()),
        ));
        threads.push(spawn(
            "battery",
            intervals.battery,
            shared.clone(),
            stop.clone(),
            || Box::new(BatteryCollector::new()),
        ));
        let top_processes = config.top_processes;
        threads.push(spawn(
            "processes",
            intervals.processes,
            shared.clone(),
            stop.clone(),
            move || Box::new(ProcessesCollector::new(top_processes)),
        ));
        threads.push(spawn(
            "disks",
            intervals.disks,
            shared.clone(),
            stop.clone(),
            || Box::new(DiskCollector::new()),
        ));
        if config.smart_enabled {
            let smart_interval = intervals.smart;
            threads.push(spawn(
                "smart",
                intervals.smart,
                shared.clone(),
                stop.clone(),
                move || Box::new(SmartCollector::new(smart_interval)),
            ));
        }
        #[cfg(feature = "connections")]
        if config.connections_enabled {
            let connections_interval = intervals.connections;
            threads.push(spawn(
                "connections",
                intervals.connections,
                shared.clone(),
                stop.clone(),
                move || Box::new(ConnectionsCollector::new(connections_interval)),
            ));
        }

        Self {
            shared,
            stop,
            threads: Mutex::new(threads),
            config,
        }
    }

    /// Start a monitor with an explicit collector list.
    ///
    /// This is the injection seam used by tests and the release-built
    /// performance fixture. It runs the same threads, lock publication, and
    /// sampling loop as [`Self::start`], so a fake source cannot hide a stall
    /// in the real monitor path.
    #[doc(hidden)]
    pub fn start_with_collectors(config: MonitorConfig, collectors: Vec<CollectorSpec>) -> Self {
        let (shared, stop) = new_shared(&config);
        let mut threads = Vec::new();
        for spec in collectors {
            threads.push(spawn(
                spec.name,
                spec.interval,
                shared.clone(),
                stop.clone(),
                spec.make,
            ));
        }

        Self {
            shared,
            stop,
            threads: Mutex::new(threads),
            config,
        }
    }

    /// Clone of the latest snapshot.
    pub fn snapshot(&self) -> Snapshot {
        read(&*self.shared).snapshot.clone()
    }

    /// Clone of the history buffers.
    pub fn history(&self) -> History {
        read(&*self.shared).history.clone()
    }

    /// Current publication revision. Zero means no update has been published.
    pub fn revision(&self) -> u64 {
        read(&*self.shared).revision
    }

    /// Return a consistent frame newer than `revision`, or `None` when the
    /// monitor has not published since then or the lock is momentarily busy.
    /// Never waits, so the UI loop can always service input.
    pub fn try_frame_since(&self, revision: u64) -> Option<MonitorFrame> {
        let guard = match self.shared.try_read() {
            Ok(guard) => guard,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return None,
        };
        if guard.revision <= revision {
            return None;
        }
        Some(MonitorFrame {
            revision: guard.revision,
            snapshot: guard.snapshot.clone(),
            history: guard.history.clone(),
        })
    }

    /// Run `f` with read access to a consistent snapshot/history pair,
    /// avoiding clones for frontends that render straight from the lock.
    ///
    /// Do not call [`Self::read_snapshot`]/[`Self::read_history`] from inside
    /// `f`: read locks must not be nested.
    pub fn read<R>(&self, f: impl FnOnce(&Snapshot, &History) -> R) -> R {
        let guard = read(&*self.shared);
        f(&guard.snapshot, &guard.history)
    }

    /// Run `f` with a read lock on the snapshot, avoiding a clone for
    /// frontends that need only that section. See [`Self::read`] for the
    /// combined form (preferred when history is needed too).
    pub fn read_snapshot<R>(&self, f: impl FnOnce(&Snapshot) -> R) -> R {
        f(&read(&*self.shared).snapshot)
    }

    /// Run `f` with a read lock on the history. See [`Self::read`].
    pub fn read_history<R>(&self, f: impl FnOnce(&History) -> R) -> R {
        f(&read(&*self.shared).history)
    }

    pub fn config(&self) -> &MonitorConfig {
        &self.config
    }

    /// Block until the first meaningful sample has arrived (CPU cores and
    /// processes are populated) or `timeout` elapses.
    pub fn wait_ready(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let ready = self.read_snapshot(|s| !s.cpu.cores.is_empty() && !s.processes.is_empty());
            if ready {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let threads = std::mem::take(&mut *lock(&self.threads));
        for thread in &threads {
            thread.thread().unpark();
        }
        for thread in threads {
            let _ = thread.join();
        }
    }
}

/// Spawn a collector thread. The collector is constructed *inside* the
/// thread: some backends (macmon) hold non-`Send` handles, and construction
/// can be slow (smartctl scan, process listing), so it must not block the
/// caller.
fn spawn(
    name: &str,
    interval: Duration,
    shared: Arc<RwLock<Shared>>,
    stop: Arc<AtomicBool>,
    make: impl FnOnce() -> Box<dyn Collector> + Send + 'static,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name(format!("opentop-{name}"))
        .spawn(move || {
            let mut collector = make();
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let started = Instant::now();
                if let Some(update) = collector.sample(interval) {
                    let mut guard = write(&shared);
                    let Shared {
                        snapshot,
                        history,
                        revision,
                    } = &mut *guard;
                    update.apply(snapshot, history);
                    snapshot.timestamp_ms = now_ms();
                    *revision = revision.wrapping_add(1);
                }
                sleep_until(started + interval, &stop);
            }
        })
        .expect("failed to spawn collector thread")
}

fn new_shared(config: &MonitorConfig) -> (Arc<RwLock<Shared>>, Arc<AtomicBool>) {
    let shared = Arc::new(RwLock::new(Shared {
        snapshot: Snapshot::default(),
        history: History::new(config.history_len),
        revision: 0,
    }));
    (shared, Arc::new(AtomicBool::new(false)))
}

fn read<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_frame_since_never_waits_for_a_busy_writer() {
        let config = MonitorConfig::default();
        let (shared, stop) = new_shared(&config);
        shared.write().unwrap().revision = 1;
        let monitor = MonitorHandle {
            shared: shared.clone(),
            stop,
            threads: Mutex::new(Vec::new()),
            config,
        };

        assert_eq!(
            monitor.try_frame_since(0).map(|frame| frame.revision),
            Some(1)
        );
        assert!(monitor.try_frame_since(1).is_none());

        let _guard = shared.write().unwrap();
        let started = Instant::now();
        assert!(monitor.try_frame_since(0).is_none());
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn stopping_a_sleeping_collector_does_not_wait_for_its_interval() {
        use std::sync::mpsc;

        struct ReadyCollector(mpsc::Sender<()>);
        impl Collector for ReadyCollector {
            fn sample(&mut self, _: Duration) -> Option<crate::collectors::CollectorUpdate> {
                let _ = self.0.send(());
                None
            }
        }

        let (sampled_tx, sampled_rx) = mpsc::channel();
        let monitor = MonitorHandle::start_with_collectors(
            MonitorConfig::default(),
            vec![CollectorSpec::new(
                "test",
                Duration::from_secs(60),
                move || ReadyCollector(sampled_tx),
            )],
        );
        sampled_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let (stopped_tx, stopped_rx) = mpsc::channel();
        thread::spawn(move || {
            drop(monitor);
            stopped_tx.send(()).unwrap();
        });
        stopped_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    }
}
