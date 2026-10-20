use std::time::Duration;

use sysinfo::{Motherboard, Product, System};

use super::{Collector, CollectorUpdate};
use crate::snapshot::{LoadAverage, SystemInfo};

/// Host, OS, uptime, boot time and load average. Static fields are cached at
/// construction; only uptime/load are refreshed per tick.
pub struct SystemInfoCollector {
    static_info: SystemInfo,
}

impl SystemInfoCollector {
    pub fn new() -> Self {
        let motherboard = Motherboard::new().map(|m| {
            let name = m.name().unwrap_or_default();
            let vendor = m.vendor_name().unwrap_or_default();
            match (vendor.is_empty(), name.is_empty()) {
                (false, false) => format!("{vendor} {name}"),
                (false, true) => vendor,
                (true, false) => name,
                (true, true) => String::new(),
            }
        });
        let product = {
            let parts: Vec<String> = [Product::family(), Product::name(), Product::version()]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect();
            Some(parts.join(" "))
        };

        Self {
            static_info: SystemInfo {
                hostname: System::host_name(),
                os_name: System::name(),
                os_version: System::os_version(),
                long_os_version: System::long_os_version(),
                kernel_version: System::kernel_version(),
                kernel_long_version: System::kernel_long_version(),
                distribution_id: System::distribution_id(),
                cpu_arch: System::cpu_arch(),
                physical_cores: System::physical_core_count(),
                logical_cores: std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(0),
                motherboard: motherboard.filter(|s| !s.is_empty()),
                product: product.filter(|s| !s.is_empty()),
                ..SystemInfo::default()
            },
        }
    }
}

impl Collector for SystemInfoCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        let mut info = self.static_info.clone();
        info.uptime_secs = System::uptime();
        info.boot_time_secs = System::boot_time();
        #[cfg(not(windows))]
        {
            let load = System::load_average();
            info.load_average = Some(LoadAverage {
                one: load.one,
                five: load.five,
                fifteen: load.fifteen,
            });
        }
        Some(CollectorUpdate::System(info))
    }
}

impl Default for SystemInfoCollector {
    fn default() -> Self {
        Self::new()
    }
}
