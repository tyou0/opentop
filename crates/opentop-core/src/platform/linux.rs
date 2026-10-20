use std::time::Duration;

use sysinfo::Components;

use super::hwmon::HwmonBackend;
use super::SensorsBackend;
use crate::snapshot::{FanReading, SensorsInfo};

#[cfg(feature = "sensors-lm-sensors")]
use super::lm_sensors::LmSensors;

/// Linux sensors: hwmon temperatures via sysinfo components, plus fans via
/// `sensors -j` (lm-sensors) when the feature is enabled and the binary is
/// installed.
pub struct LinuxBackend {
    hwmon: HwmonBackend,
    #[cfg(feature = "sensors-lm-sensors")]
    lm: Option<LmSensors>,
}

impl LinuxBackend {
    pub fn new() -> Self {
        Self {
            hwmon: HwmonBackend::new(),
            #[cfg(feature = "sensors-lm-sensors")]
            lm: LmSensors::probe(),
        }
    }
}

impl SensorsBackend for LinuxBackend {
    fn sample(&mut self, interval: Duration) -> SensorsInfo {
        let mut info = self.hwmon.sample(interval);
        #[cfg(feature = "sensors-lm-sensors")]
        if let Some(lm) = &self.lm {
            let fans: Vec<FanReading> = lm.fans();
            if !fans.is_empty() {
                info.fans = fans;
                info.source = "hwmon + lm-sensors".into();
            }
        }
        info
    }

    fn name(&self) -> &'static str {
        "hwmon"
    }
}
