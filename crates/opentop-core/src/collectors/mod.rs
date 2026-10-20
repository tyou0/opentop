//! Collector implementations. Each collector owns the state it needs and
//! returns a typed [`CollectorUpdate`] describing the section it sampled.
//! They run on independent threads driven by the monitor.
//!
//! Sampling happens outside the shared lock. The monitor takes the write lock
//! only long enough to apply an update and append its history, so a slow
//! collector cannot stop other collectors or readers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::history::History;
use crate::snapshot::{
    Availability, BatteryInfo, ConnectionsSummary, CpuInfo, DiskHealth, DiskUsageInfo, MemoryInfo,
    NetworkInterface, ProcessInfo, SensorsInfo, Snapshot, SystemInfo,
};

pub mod battery;
pub mod cpu_memory;
pub mod disks;
pub mod network;
pub mod processes;
pub mod sensors;
pub mod system_info;

#[cfg(feature = "connections")]
pub mod connections;

/// One section sampled by a collector. Applying it updates the snapshot and
/// appends any history series that section owns.
#[derive(Debug)]
pub enum CollectorUpdate {
    CpuMemory { cpu: CpuInfo, memory: MemoryInfo },
    Network(Vec<NetworkInterface>),
    Disks(Vec<DiskUsageInfo>),
    Processes(Vec<ProcessInfo>),
    Sensors(SensorsInfo),
    System(SystemInfo),
    Battery(Availability<Vec<BatteryInfo>>),
    DiskHealth(Availability<Vec<DiskHealth>>),
    Connections(Availability<ConnectionsSummary>),
    Batch(Vec<CollectorUpdate>),
}

impl CollectorUpdate {
    pub fn apply(self, snapshot: &mut Snapshot, history: &mut History) {
        match self {
            Self::CpuMemory { cpu, memory } => {
                let core_usages: Vec<f32> = cpu.cores.iter().map(|c| c.usage_percent).collect();
                history.push_cpu(cpu.usage_percent, &core_usages);
                history.push_memory(memory.used_bytes, memory.usage_percent, memory.used_swap_bytes);
                snapshot.cpu = cpu;
                snapshot.memory = memory;
            }
            Self::Network(interfaces) => {
                for iface in &interfaces {
                    history.push_net(&iface.name, iface.rx_bytes_per_sec, iface.tx_bytes_per_sec);
                }
                snapshot.network = interfaces;
            }
            Self::Disks(disks) => snapshot.disks = disks,
            Self::Processes(processes) => snapshot.processes = processes,
            Self::Sensors(info) => {
                let cpu_temp = info
                    .temps
                    .iter()
                    .find(|t| t.label.to_lowercase().contains("cpu"))
                    .map(|t| t.celsius);
                let gpu_temp = info
                    .temps
                    .iter()
                    .find(|t| t.label.to_lowercase().contains("gpu"))
                    .map(|t| t.celsius);
                let (cpu_watts, gpu_watts) = info
                    .power
                    .as_ref()
                    .map(|p| (p.cpu_watts, p.gpu_watts))
                    .unwrap_or((None, None));
                let gpu_usage = info.gpu.as_ref().and_then(|g| g.usage_percent);
                history.push_temps(cpu_temp, gpu_temp);
                history.push_power(cpu_watts, gpu_watts, gpu_usage);
                snapshot.sensors = info;
            }
            Self::System(info) => snapshot.system = info,
            Self::Battery(battery) => {
                if let Availability::Available(batteries) = &battery
                    && let Some(first) = batteries.first()
                {
                    history.push_battery(first.percentage);
                }
                snapshot.battery = battery;
            }
            Self::DiskHealth(health) => snapshot.disk_health = health,
            Self::Connections(connections) => snapshot.connections = connections,
            Self::Batch(updates) => {
                for update in updates {
                    update.apply(snapshot, history);
                }
            }
        }
    }
}

/// A source of snapshot data with its own sampling cadence.
///
/// Collectors are constructed lazily *inside* their sampling thread (see
/// [`crate::monitor`]), so they never need to be `Send`.
pub trait Collector {
    /// Sample once, outside the shared lock, and return the section to
    /// publish. `interval` is the configured cadence for rate calculations.
    fn sample(&mut self, interval: Duration) -> Option<CollectorUpdate>;
}

pub(crate) fn percent(used: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0) as f32
    }
}

/// Sleep until `deadline`, waking early (within ~100ms) when `stop` is set.
pub(crate) fn sleep_until(deadline: Instant, stop: &AtomicBool) {
    const GRANULARITY: Duration = Duration::from_millis(100);
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let now = Instant::now();
        if now >= deadline {
            return;
        }
        std::thread::sleep((deadline - now).min(GRANULARITY));
    }
}
