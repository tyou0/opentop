use std::time::Duration;

use super::{Collector, CollectorUpdate};
use crate::platform::SensorsAggregate;

/// Platform-specific temperatures, fans, power, GPU and CPU tiers.
///
/// The aggregate picks the best backend per OS (macmon on Apple Silicon,
/// hwmon/lm-sensors on Linux, sysinfo components on Windows) and merges in
/// nvidia-smi data when the `gpu-nvidia` feature is enabled and the tool
/// exists.
pub struct SensorsCollector {
    sensors: SensorsAggregate,
}

impl SensorsCollector {
    pub fn new() -> Self {
        Self {
            sensors: SensorsAggregate::create(),
        }
    }
}

impl Collector for SensorsCollector {
    fn sample(&mut self, interval: Duration) -> Option<CollectorUpdate> {
        Some(CollectorUpdate::Sensors(self.sensors.sample(interval)))
    }
}

impl Default for SensorsCollector {
    fn default() -> Self {
        Self::new()
    }
}
