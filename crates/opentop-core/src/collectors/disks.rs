use std::time::{Duration, Instant};

use sysinfo::{DiskKind, Disks};

use super::{percent, Collector, CollectorUpdate};
use crate::snapshot::DiskUsageInfo;

/// Mounted filesystem usage plus per-volume read/write rates, using sysinfo.
pub struct DiskCollector {
    disks: Disks,
    updated_at: Instant,
}

impl DiskCollector {
    pub fn new() -> Self {
        Self {
            disks: Disks::new_with_refreshed_list(),
            updated_at: Instant::now(),
        }
    }
}

impl Collector for DiskCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        if self.updated_at.elapsed() < Duration::from_millis(250) {
            // First tick: give deltas a meaningful window before dividing.
            std::thread::sleep(Duration::from_millis(400));
        }
        self.disks.refresh(true);
        let elapsed = self.updated_at.elapsed().as_secs_f64().max(0.1);
        self.updated_at = Instant::now();

        let mut disks: Vec<DiskUsageInfo> = self
            .disks
            .list()
            .iter()
            .map(|disk| {
                let total = disk.total_space();
                let available = disk.available_space();
                let used = total.saturating_sub(available);
                let usage = disk.usage();
                DiskUsageInfo {
                    name: disk.name().to_string_lossy().into_owned(),
                    mount_point: disk.mount_point().to_string_lossy().into_owned(),
                    file_system: disk.file_system().to_string_lossy().into_owned(),
                    disk_type: kind_str(disk.kind()),
                    total_bytes: total,
                    available_bytes: available,
                    used_bytes: used,
                    usage_percent: percent(used, total),
                    is_removable: disk.is_removable(),
                    read_bytes_per_sec: (usage.read_bytes as f64 / elapsed) as u64,
                    write_bytes_per_sec: (usage.written_bytes as f64 / elapsed) as u64,
                }
            })
            .collect();
        disks.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
        disks.dedup_by(|a, b| a.mount_point == b.mount_point);
        Some(CollectorUpdate::Disks(disks))
    }
}

impl Default for DiskCollector {
    fn default() -> Self {
        Self::new()
    }
}

fn kind_str(kind: DiskKind) -> String {
    match kind {
        DiskKind::HDD => "hdd",
        DiskKind::SSD => "ssd",
        _ => "unknown",
    }
    .to_string()
}
