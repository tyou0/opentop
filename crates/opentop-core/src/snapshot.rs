//! The snapshot data model: everything opentop knows about the system at one
//! point in time. All types are plain data and serializable.

use serde::{Deserialize, Serialize};

/// A section that may be entirely unavailable on some platforms or when an
/// optional external tool is missing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "data", rename_all = "snake_case")]
pub enum Availability<T> {
    Available(T),
    Unavailable {
        /// Human readable reason, e.g. "smartctl not found".
        reason: String,
    },
}

impl<T> Default for Availability<T> {
    fn default() -> Self {
        Availability::Unavailable {
            reason: "not collected yet".into(),
        }
    }
}

impl<T> Availability<T> {
    pub fn available(value: T) -> Self {
        Availability::Available(value)
    }

    pub fn unavailable(reason: impl Into<String>) -> Self {
        Availability::Unavailable {
            reason: reason.into(),
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Availability::Available(_))
    }

    pub fn as_ref(&self) -> Availability<&T> {
        match self {
            Availability::Available(v) => Availability::Available(v),
            Availability::Unavailable { reason } => Availability::Unavailable {
                reason: reason.clone(),
            },
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Availability<U> {
        match self {
            Availability::Available(v) => Availability::Available(f(v)),
            Availability::Unavailable { reason } => Availability::Unavailable { reason },
        }
    }

    pub fn available_value(&self) -> Option<&T> {
        match self {
            Availability::Available(v) => Some(v),
            Availability::Unavailable { .. } => None,
        }
    }
}

/// One complete point-in-time view of the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub timestamp_ms: u64,
    pub system: SystemInfo,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub sensors: SensorsInfo,
    pub network: Vec<NetworkInterface>,
    pub disks: Vec<DiskUsageInfo>,
    pub disk_health: Availability<Vec<DiskHealth>>,
    pub battery: Availability<Vec<BatteryInfo>>,
    pub processes: Vec<ProcessInfo>,
    pub connections: Availability<ConnectionsSummary>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            timestamp_ms: 0,
            system: SystemInfo::default(),
            cpu: CpuInfo::default(),
            memory: MemoryInfo::default(),
            sensors: SensorsInfo::default(),
            network: Vec::new(),
            disks: Vec::new(),
            disk_health: Availability::default(),
            battery: Availability::unavailable("not collected yet"),
            processes: Vec::new(),
            connections: Availability::unavailable(connections_default_reason()),
        }
    }
}

fn connections_default_reason() -> &'static str {
    if cfg!(feature = "connections") {
        "not collected yet"
    } else {
        "connection tracking is not compiled in"
    }
}

impl Snapshot {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub long_os_version: Option<String>,
    pub kernel_version: Option<String>,
    pub kernel_long_version: String,
    pub distribution_id: String,
    pub cpu_arch: String,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    pub uptime_secs: u64,
    pub boot_time_secs: u64,
    pub load_average: Option<LoadAverage>,
    pub motherboard: Option<String>,
    pub product: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CpuInfo {
    pub brand: String,
    pub vendor: String,
    pub frequency_mhz: u64,
    pub usage_percent: f32,
    pub cores: Vec<CoreInfo>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CoreInfo {
    pub name: String,
    pub usage_percent: f32,
    pub frequency_mhz: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub available_bytes: u64,
    pub usage_percent: f32,
    pub total_swap_bytes: u64,
    pub used_swap_bytes: u64,
    pub free_swap_bytes: u64,
    pub swap_percent: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SensorsInfo {
    pub source: String,
    pub temps: Vec<TempReading>,
    pub fans: Vec<FanReading>,
    pub power: Option<PowerReading>,
    pub gpu: Option<GpuReading>,
    pub cpu_tiers: Vec<CpuTier>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempReading {
    pub label: String,
    pub celsius: f32,
    pub max_celsius: Option<f32>,
    pub critical_celsius: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanReading {
    pub name: String,
    pub rpm: u32,
    pub max_rpm: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PowerReading {
    pub cpu_watts: Option<f32>,
    pub gpu_watts: Option<f32>,
    pub ane_watts: Option<f32>,
    pub ram_watts: Option<f32>,
    pub combined_watts: Option<f32>,
    pub system_watts: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GpuReading {
    pub name: Option<String>,
    #[serde(default)]
    pub cores: Option<u32>,
    pub usage_percent: Option<f32>,
    pub scaled_percent: Option<f32>,
    pub frequency_mhz: Option<u32>,
    pub power_watts: Option<f32>,
    pub temperature_celsius: Option<f32>,
    pub memory_used_bytes: Option<u64>,
    pub memory_total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuTier {
    pub label: String,
    pub frequency_mhz: u32,
    pub active_ratio: f32,
    pub cores: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub mac: String,
    pub ips: Vec<String>,
    pub state: String,
    pub mtu: u64,
    pub rx_bytes_per_sec: f64,
    pub tx_bytes_per_sec: f64,
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
    pub total_rx_packets: u64,
    pub total_tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskUsageInfo {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f32,
    pub is_removable: bool,
    pub disk_type: String,
    pub file_system: String,
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiskHealth {
    pub device: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub interface: Option<String>,
    pub smart_supported: bool,
    pub smart_enabled: bool,
    pub overall_health: Option<String>,
    pub temperature_celsius: Option<f64>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    pub unsafe_shutdowns: Option<u64>,
    pub media_errors: Option<u64>,
    pub wear_percent: Option<f64>,
    pub data_written_bytes: Option<u64>,
    pub data_read_bytes: Option<u64>,
    pub reallocated_sectors: Option<u64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub name: String,
    pub percentage: f32,
    pub state: String,
    pub temperature_celsius: Option<f32>,
    pub cycle_count: Option<u32>,
    pub health_percent: Option<f32>,
    pub energy_wh: Option<f32>,
    pub energy_full_wh: Option<f32>,
    pub energy_design_wh: Option<f32>,
    pub power_watts: Option<f32>,
    pub voltage_volts: Option<f32>,
    pub time_to_full_secs: Option<u64>,
    pub time_to_empty_secs: Option<u64>,
    pub vendor: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub cmd: String,
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub status: String,
    pub start_time_secs: u64,
    pub running_time_secs: u64,
    pub user: Option<String>,
    pub num_threads: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConnectionsSummary {
    pub total: usize,
    pub tcp: usize,
    pub tcp_established: usize,
    pub tcp_listen: usize,
    pub tcp_wait: usize,
    pub udp: usize,
    pub processes_with_sockets: usize,
}

#[cfg(test)]
mod tests {
    use super::GpuReading;

    #[test]
    fn older_gpu_snapshots_without_core_count_still_deserialize() {
        let gpu: GpuReading = serde_json::from_str(r#"{"name":"GPU"}"#).unwrap();
        assert_eq!(gpu.name.as_deref(), Some("GPU"));
        assert_eq!(gpu.cores, None);
    }
}
