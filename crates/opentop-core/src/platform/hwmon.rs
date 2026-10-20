use std::time::Duration;

use sysinfo::Components;

use super::SensorsBackend;
use crate::snapshot::{SensorsInfo, TempReading};

/// Temperatures from sysinfo "components": hwmon on Linux, SMC/IOKit on
/// macOS (best effort), WMI on Windows (may come back empty without
/// privileges). Portable fallback everywhere.
pub struct HwmonBackend {
    components: Components,
}

impl HwmonBackend {
    pub fn new() -> Self {
        Self {
            components: Components::new_with_refreshed_list(),
        }
    }
}

impl Default for HwmonBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl SensorsBackend for HwmonBackend {
    fn sample(&mut self, _interval: Duration) -> SensorsInfo {
        self.components.refresh(false);
        let temps = self
            .components
            .list()
            .iter()
            .filter_map(|component| {
                let celsius = component.temperature()?;
                Some(TempReading {
                    label: component.label().to_string(),
                    celsius,
                    max_celsius: component.max(),
                    critical_celsius: component.critical(),
                })
            })
            .collect();

        SensorsInfo {
            source: self.name().to_string(),
            temps,
            ..SensorsInfo::default()
        }
    }

    fn name(&self) -> &'static str {
        "hwmon"
    }
}
