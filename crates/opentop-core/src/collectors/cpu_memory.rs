use std::time::Duration;

use sysinfo::System;

use super::{Collector, CollectorUpdate, percent};
use crate::snapshot::{CoreInfo, CpuInfo, MemoryInfo};

/// CPU usage (global + per core) and memory/swap, using sysinfo.
///
/// sysinfo needs two samples separated by at least ~200ms before CPU usage is
/// meaningful, so the very first collect primes itself with a short internal
/// second refresh.
pub struct CpuMemoryCollector {
    sys: System,
    primed: bool,
}

impl CpuMemoryCollector {
    pub fn new() -> Self {
        Self {
            sys: System::new(),
            primed: false,
        }
    }
}

impl Collector for CpuMemoryCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        self.sys.refresh_cpu_all();
        self.sys.refresh_memory();
        if !self.primed {
            self.primed = true;
            std::thread::sleep(Duration::from_millis(300));
            self.sys.refresh_cpu_all();
        }

        let cores: Vec<CoreInfo> = self
            .sys
            .cpus()
            .iter()
            .map(|cpu| CoreInfo {
                name: cpu.name().to_string(),
                usage_percent: cpu.cpu_usage(),
                frequency_mhz: cpu.frequency(),
            })
            .collect();
        let first = self.sys.cpus().first();

        let cpu = CpuInfo {
            brand: first.map(|c| c.brand().to_string()).unwrap_or_default(),
            vendor: first.map(|c| c.vendor_id().to_string()).unwrap_or_default(),
            frequency_mhz: first.map(|c| c.frequency()).unwrap_or(0),
            usage_percent: self.sys.global_cpu_usage(),
            cores,
        };
        let memory = MemoryInfo {
            total_bytes: self.sys.total_memory(),
            used_bytes: self.sys.used_memory(),
            free_bytes: self.sys.free_memory(),
            available_bytes: self.sys.available_memory(),
            usage_percent: percent(self.sys.used_memory(), self.sys.total_memory()),
            total_swap_bytes: self.sys.total_swap(),
            used_swap_bytes: self.sys.used_swap(),
            free_swap_bytes: self.sys.free_swap(),
            swap_percent: percent(self.sys.used_swap(), self.sys.total_swap()),
        };

        Some(CollectorUpdate::CpuMemory { cpu, memory })
    }
}

impl Default for CpuMemoryCollector {
    fn default() -> Self {
        Self::new()
    }
}
