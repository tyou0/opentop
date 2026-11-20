//! Deterministic performance fixture for `scripts/tui_perf.py`.
//!
//! Runs the real runtime, dashboard and monitor publication path with a
//! synthetic collector instead of OS collection. Build with:
//!
//! ```text
//! cargo build --release --offline --example perf_fixture -p opentop-app
//! ```

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use opentop_app::runtime::{PerfRecorder, RuntimeOptions};
use opentop_app::state::App;
use opentop_app::terminal::{TerminalSession, install_panic_hook};
use opentop_app::widgets::WidgetDescriptor;
use opentop_app::widgets::WidgetId;
use opentop_core::collectors::{Collector, CollectorUpdate};
use opentop_core::monitor::CollectorSpec;
use opentop_core::plugins::{Discovery, Manifest, PluginCandidate, parse as parse_plugin_manifest};
use opentop_core::{
    Availability, BatteryInfo, ConnectionsSummary, CoreInfo, CpuTier, DiskHealth, DiskUsageInfo,
    FanReading, LoadAverage, MemoryInfo, MonitorConfig, MonitorHandle, NetworkInterface,
    PowerReading, ProcessInfo, Snapshot, TempReading,
};

const GATE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
enum Gate {
    None,
    First,
    Samples(u64, u64),
}

fn main() -> io::Result<()> {
    let mut scenario = String::from("idle");
    let mut control_dir = PathBuf::from(".");
    let mut perf_log: Option<PathBuf> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--scenario" => scenario = next_value(&mut args, "--scenario"),
            "--control-dir" => control_dir = PathBuf::from(next_value(&mut args, "--control-dir")),
            "--perf-log" => perf_log = Some(PathBuf::from(next_value(&mut args, "--perf-log"))),
            other => {
                eprintln!("perf_fixture: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    let gate = match scenario.as_str() {
        "slow-collector" => Gate::Samples(2, 2),
        "startup" => Gate::First,
        _ => Gate::None,
    };

    std::fs::create_dir_all(&control_dir)?;
    let control = Arc::new(control_dir);
    let perf = match &perf_log {
        Some(path) => Some(Arc::new(PerfRecorder::create(path)?)),
        None => None,
    };

    install_panic_hook();
    let mut session = TerminalSession::new()?;
    if scenario == "terminal-panic" {
        panic!("synthetic terminal cleanup test");
    }

    let mut app = App::new();
    if scenario == "slow-transport" {
        app.set_focus(WidgetId::builtin("processes"));
    }
    if scenario == "blocked-plugin" {
        let (report, ids) = plugin_fixture(&control)?;
        for candidate in &report.candidates {
            if let Some(manifest) = &candidate.manifest {
                app.catalog.insert(WidgetDescriptor {
                    id: WidgetId::plugin(&manifest.id),
                    label: manifest.name.clone(),
                    description: manifest.description.clone(),
                    source: "performance fixture".into(),
                    available: true,
                    status: Some("waiting".into()),
                    min_height: manifest.min_height,
                });
            }
        }
        app.plugin_report = report;
        app.set_enabled_widgets(ids);
    }
    let collector_perf = perf.clone();
    let collector_control = control.clone();
    let interval = Duration::from_millis(250);
    let monitor = MonitorHandle::start_with_collectors(
        MonitorConfig {
            history_len: 120,
            top_processes: 32,
            smart_enabled: false,
            connections_enabled: false,
            ..Default::default()
        },
        vec![CollectorSpec::new("fixture", interval, move || {
            FixtureCollector {
                perf: collector_perf.clone(),
                control_dir: collector_control.clone(),
                gate,
                samples: 0,
            }
        })],
    );
    let options = RuntimeOptions {
        perf: perf.clone(),
        ..Default::default()
    };
    let result = opentop_app::runtime::run(session.terminal_mut(), &mut app, &monitor, &options);

    drop(session);
    drop(monitor);
    result
}

fn plugin_fixture(control_dir: &Path) -> io::Result<(Discovery, Vec<WidgetId>)> {
    #[cfg(not(unix))]
    {
        let _ = control_dir;
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "plugin fixture needs Unix",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let root = control_dir.join("plugins");
        let mut candidates = Vec::new();
        let mut ids = Vec::new();
        for index in 0..4 {
            let id = format!("fixture.blocked{index}");
            let dir = root.join(&id);
            std::fs::create_dir_all(&dir)?;
            let marker = control_dir.join(format!("plugin-started-{index}"));
            let release = control_dir.join(format!("plugin-release-{index}"));
            let executable = dir.join("fixture.sh");
            let script = format!(
                "#!/bin/sh\nprintf started > '{}'\nwhile [ ! -f '{}' ]; do sleep 0.01; done\nprintf '%s\\n' '{{\"protocol_version\":1,\"lines\":[\"plugin-{index}\"]}}'\n",
                marker.display(),
                release.display()
            );
            std::fs::write(&executable, script)?;
            std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755))?;
            let manifest_text = format!(
                "protocol_version=1\nid=\"{id}\"\nname=\"Plugin {index}\"\nversion=\"1.0\"\ndescription=\"blocked fixture\"\nexecutable=\"fixture.sh\"\nargs=[]\ninterval_ms=15000\ntimeout_ms=10000\nmin_height=5\n"
            );
            std::fs::write(dir.join("plugin.toml"), &manifest_text)?;
            let manifest: Manifest = parse_plugin_manifest(&dir, manifest_text.as_bytes())
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error.message))?;
            ids.push(WidgetId::plugin(&id));
            candidates.push(PluginCandidate {
                id: Some(id),
                dir,
                manifest: Some(manifest),
                error: None,
            });
        }
        Ok((
            Discovery {
                candidates,
                errors: Vec::new(),
            },
            ids,
        ))
    }
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> String {
    args.next().unwrap_or_else(|| {
        eprintln!("perf_fixture: {flag} needs a value");
        std::process::exit(2);
    })
}

struct FixtureCollector {
    perf: Option<Arc<PerfRecorder>>,
    control_dir: Arc<PathBuf>,
    gate: Gate,
    samples: u64,
}

impl Collector for FixtureCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        let started = Instant::now();
        self.samples += 1;
        let sample = self.samples;

        let gated = match self.gate {
            Gate::First => sample == 1,
            Gate::Samples(from, to) => sample >= from && sample <= to,
            Gate::None => false,
        };
        let mut gate_wait = Duration::ZERO;
        if gated {
            let _ = std::fs::write(
                self.control_dir.join(format!("started-{sample}")),
                sample.to_string(),
            );
            gate_wait = wait_for_release(&self.control_dir, sample, GATE_TIMEOUT);
        }

        let update = (sample == 1).then(fixture_update);

        if let Some(perf) = &self.perf {
            perf.record(
                "collect",
                &[
                    ("sample", sample),
                    ("collect_us", micros(started.elapsed())),
                    ("gate_wait_us", micros(gate_wait)),
                ],
            );
        }

        update
    }
}

fn wait_for_release(control_dir: &Path, sample: u64, timeout: Duration) -> Duration {
    let started = Instant::now();
    let release = control_dir.join(format!("release-{sample}"));
    while started.elapsed() < timeout {
        if release.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    started.elapsed()
}

fn micros(duration: Duration) -> u64 {
    duration.as_micros().try_into().unwrap_or(u64::MAX)
}

const GIB: u64 = 1024 * 1024 * 1024;

fn fixture_update() -> CollectorUpdate {
    let mut snapshot = Snapshot::default();
    fill_snapshot(&mut snapshot);
    CollectorUpdate::Batch(vec![
        CollectorUpdate::System(snapshot.system),
        CollectorUpdate::CpuMemory {
            cpu: snapshot.cpu,
            memory: snapshot.memory,
        },
        CollectorUpdate::Sensors(snapshot.sensors),
        CollectorUpdate::Network(snapshot.network),
        CollectorUpdate::Disks(snapshot.disks),
        CollectorUpdate::DiskHealth(snapshot.disk_health),
        CollectorUpdate::Battery(snapshot.battery),
        CollectorUpdate::Processes(std::sync::Arc::unwrap_or_clone(snapshot.processes)),
        CollectorUpdate::Connections(snapshot.connections),
    ])
}

fn fill_snapshot(snapshot: &mut Snapshot) {
    snapshot.system.hostname = Some("perf-fixture".into());
    snapshot.system.os_name = Some("SyntheticOS".into());
    snapshot.system.os_version = Some("1.0".into());
    snapshot.system.long_os_version = Some("SyntheticOS 1.0".into());
    snapshot.system.kernel_version = Some("6.0.0-fixture".into());
    snapshot.system.distribution_id = "fixture".into();
    snapshot.system.cpu_arch = "x86_64".into();
    snapshot.system.physical_cores = Some(4);
    snapshot.system.logical_cores = 8;
    snapshot.system.uptime_secs = 3600;
    snapshot.system.boot_time_secs = 0;
    snapshot.system.load_average = Some(LoadAverage {
        one: 0.5,
        five: 0.4,
        fifteen: 0.3,
    });
    snapshot.system.motherboard = Some("Fixture Board".into());
    snapshot.system.product = Some("Fixture Workstation".into());

    snapshot.cpu.brand = "Fixture CPU".into();
    snapshot.cpu.vendor = "Synthetic".into();
    snapshot.cpu.frequency_mhz = 2400;
    snapshot.cpu.usage_percent = 42.5;
    snapshot.cpu.cores = (0..8)
        .map(|index| CoreInfo {
            name: format!("core-{index}"),
            usage_percent: 15.0 + (index as f32) * 5.0,
            frequency_mhz: 2400,
        })
        .collect();

    snapshot.memory = MemoryInfo {
        total_bytes: 16 * GIB,
        used_bytes: 6 * GIB,
        free_bytes: 10 * GIB,
        available_bytes: 10 * GIB,
        usage_percent: 37.5,
        total_swap_bytes: 4 * GIB,
        used_swap_bytes: GIB,
        free_swap_bytes: 3 * GIB,
        swap_percent: 25.0,
    };

    snapshot.sensors.source = "fixture".into();
    snapshot.sensors.temps = vec![
        TempReading {
            label: "cpu".into(),
            celsius: 45.0,
            max_celsius: Some(80.0),
            critical_celsius: Some(100.0),
        },
        TempReading {
            label: "gpu".into(),
            celsius: 51.0,
            max_celsius: Some(85.0),
            critical_celsius: Some(105.0),
        },
    ];
    snapshot.sensors.fans = vec![FanReading {
        name: "fan0".into(),
        rpm: 1200,
        max_rpm: Some(6000),
    }];
    snapshot.sensors.power = Some(PowerReading {
        cpu_watts: Some(12.5),
        gpu_watts: Some(7.5),
        ane_watts: None,
        ram_watts: Some(2.0),
        combined_watts: Some(22.0),
        system_watts: Some(30.0),
    });
    snapshot.sensors.cpu_tiers = vec![CpuTier {
        label: "P-cores".into(),
        frequency_mhz: 2400,
        active_ratio: 0.5,
        cores: 8,
    }];

    snapshot.network = vec![NetworkInterface {
        name: "fixture0".into(),
        mac: "02:00:00:00:00:01".into(),
        ips: vec!["192.0.2.10".into()],
        state: "up".into(),
        mtu: 1500,
        rx_bytes_per_sec: 65_536.0,
        tx_bytes_per_sec: 32_768.0,
        total_rx_bytes: 1_000_000_000,
        total_tx_bytes: 500_000_000,
        total_rx_packets: 1_000_000,
        total_tx_packets: 500_000,
        rx_errors: 0,
        tx_errors: 0,
    }];

    snapshot.disks = vec![DiskUsageInfo {
        name: "/dev/fixture0".into(),
        mount_point: "/".into(),
        total_bytes: 512 * GIB,
        available_bytes: 256 * GIB,
        used_bytes: 256 * GIB,
        usage_percent: 50.0,
        is_removable: false,
        disk_type: "SSD".into(),
        file_system: "fixturefs".into(),
        read_bytes_per_sec: 1_048_576,
        write_bytes_per_sec: 524_288,
    }];
    snapshot.disk_health = Availability::available(vec![DiskHealth {
        device: "/dev/fixture0".into(),
        model: Some("Fixture SSD".into()),
        overall_health: Some("PASSED".into()),
        temperature_celsius: Some(35.0),
        wear_percent: Some(2.5),
        ..Default::default()
    }]);

    snapshot.battery = Availability::available(vec![BatteryInfo {
        name: "fixture-battery".into(),
        percentage: 75.0,
        state: "Discharging".into(),
        health_percent: Some(95.0),
        ..Default::default()
    }]);

    snapshot.processes = (0..32)
        .map(|index| ProcessInfo {
            pid: 1000 + index,
            parent_pid: Some(1),
            name: format!("fixture-{index:02}"),
            cmd: format!("fixture-{index:02} --synthetic"),
            cpu_percent: (40.0 - index as f32).max(0.5),
            memory_bytes: (64 + index as u64) * 1024 * 1024,
            virtual_memory_bytes: (128 + index as u64) * 1024 * 1024,
            status: "Run".into(),
            start_time_secs: 0,
            running_time_secs: 3600,
            user: Some("fixture".into()),
            num_threads: 1 + index as usize,
        })
        .collect::<Vec<_>>()
        .into();

    snapshot.connections = Availability::available(ConnectionsSummary {
        total: 24,
        tcp: 18,
        tcp_established: 10,
        tcp_listen: 6,
        tcp_wait: 2,
        udp: 6,
        processes_with_sockets: 4,
    });
}
