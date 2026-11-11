use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use opentop_core::collectors::{Collector, CollectorUpdate};
use opentop_core::monitor::CollectorSpec;
use opentop_core::{CpuInfo, MemoryInfo, MonitorConfig, MonitorHandle, SystemInfo};

#[test]
fn slow_collector_does_not_block_fast_collector_or_readers() {
    let release = Arc::new((Mutex::new(false), Condvar::new()));
    let (started_tx, started_rx) = mpsc::channel();
    let fast_samples = Arc::new(AtomicUsize::new(0));

    let gate_release = release.clone();
    let fast_count = fast_samples.clone();
    let monitor = MonitorHandle::start_with_collectors(
        test_config(),
        vec![
            CollectorSpec::new("gated", Duration::from_millis(50), move || GatedCollector {
                started: Mutex::new(Some(started_tx)),
                release: gate_release.clone(),
                seeded: false,
            }),
            CollectorSpec::new("fast", Duration::from_millis(10), move || FastCollector {
                samples: fast_count.clone(),
            }),
        ],
    );

    started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("gated collector never reached its blocking sample");

    let samples_before = fast_samples.load(Ordering::SeqCst);
    let (reader_tx, reader_rx) = mpsc::channel();

    thread::scope(|scope| {
        scope.spawn(|| {
            let _ = reader_tx.send(monitor.snapshot());
        });

        thread::sleep(Duration::from_millis(200));
        let samples_while_blocked = fast_samples.load(Ordering::SeqCst);
        let snapshot = reader_rx
            .recv_timeout(Duration::from_millis(500))
            .expect("reader blocked while a collector was sampling");

        assert!(
            samples_while_blocked > samples_before,
            "fast collector stopped publishing while the gated collector was blocked"
        );
        assert_eq!(snapshot.system.hostname.as_deref(), Some("gated-seed"));
        assert!(
            snapshot.cpu.usage_percent > 0.0,
            "reader did not observe the fast collector's updates"
        );

        *release.0.lock().unwrap() = true;
        release.1.notify_all();
    });

    let deadline = Instant::now() + Duration::from_secs(2);
    while monitor.snapshot().system.hostname.as_deref() != Some("gated-released") {
        assert!(
            Instant::now() < deadline,
            "gated update was never published"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn each_published_update_appends_one_history_point() {
    let samples = Arc::new(AtomicUsize::new(0));
    let count = samples.clone();
    let config = test_config();
    let monitor = MonitorHandle::start_with_collectors(
        config.clone(),
        vec![CollectorSpec::new(
            "history",
            Duration::from_millis(5),
            move || HistoryCollector {
                samples: count.clone(),
                remaining: 5,
            },
        )],
    );

    let deadline = Instant::now() + Duration::from_secs(2);
    while samples.load(Ordering::SeqCst) < 5 {
        assert!(Instant::now() < deadline, "collector never published");
        thread::sleep(Duration::from_millis(5));
    }

    let history = monitor.history();
    drop(monitor);
    assert_eq!(history.cpu_total.len(), 5);
    assert_eq!(history.memory_used.len(), 5);
}

fn test_config() -> MonitorConfig {
    MonitorConfig {
        history_len: 256,
        top_processes: 8,
        smart_enabled: false,
        connections_enabled: false,
        ..Default::default()
    }
}

struct GatedCollector {
    started: Mutex<Option<mpsc::Sender<()>>>,
    release: Arc<(Mutex<bool>, Condvar)>,
    seeded: bool,
}

impl Collector for GatedCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        if !self.seeded {
            self.seeded = true;
            let info = SystemInfo {
                hostname: Some("gated-seed".into()),
                ..SystemInfo::default()
            };
            return Some(CollectorUpdate::System(info));
        }

        if let Some(tx) = self.started.lock().unwrap().take() {
            let _ = tx.send(());
        }

        let (lock, cvar) = &*self.release;
        let mut released = lock.lock().unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !*released {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let (guard, _) = cvar.wait_timeout(released, deadline - now).unwrap();
            released = guard;
        }

        let info = SystemInfo {
            hostname: Some("gated-released".into()),
            ..SystemInfo::default()
        };
        Some(CollectorUpdate::System(info))
    }
}

struct FastCollector {
    samples: Arc<AtomicUsize>,
}

impl Collector for FastCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        let sample = self.samples.fetch_add(1, Ordering::SeqCst) + 1;
        Some(CollectorUpdate::CpuMemory {
            cpu: CpuInfo {
                usage_percent: sample as f32,
                ..CpuInfo::default()
            },
            memory: MemoryInfo::default(),
        })
    }
}

struct HistoryCollector {
    samples: Arc<AtomicUsize>,
    remaining: usize,
}

impl Collector for HistoryCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let sample = self.samples.fetch_add(1, Ordering::SeqCst) + 1;
        let cpu = CpuInfo {
            usage_percent: sample as f32,
            ..CpuInfo::default()
        };
        Some(CollectorUpdate::CpuMemory {
            cpu,
            memory: MemoryInfo::default(),
        })
    }
}
