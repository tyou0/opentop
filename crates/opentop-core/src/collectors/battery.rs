use std::time::Duration;

use starship_battery::units::{energy, power, ratio, thermodynamic_temperature, time};
use starship_battery::{Manager, State};

use super::{Collector, CollectorUpdate};
use crate::snapshot::{Availability, BatteryInfo};

/// Battery charge, health and remaining time, using starship-battery.
pub struct BatteryCollector {
    manager: Option<Manager>,
}

impl BatteryCollector {
    pub fn new() -> Self {
        // A missing manager usually just means "no battery on this machine"
        // (desktop / server); the section simply stays unavailable.
        Self {
            manager: Manager::new().ok(),
        }
    }
}

impl Collector for BatteryCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        let Some(manager) = &self.manager else {
            return Some(CollectorUpdate::Battery(Availability::unavailable(
                "no battery manager available",
            )));
        };

        let batteries = match manager.batteries() {
            Ok(batteries) => batteries,
            Err(err) => {
                return Some(CollectorUpdate::Battery(Availability::unavailable(
                    format!("battery query failed: {err}"),
                )));
            }
        };

        let mut infos = Vec::new();
        for battery in batteries {
            let Ok(mut battery) = battery else { continue };
            if manager.refresh(&mut battery).is_err() {
                continue;
            }

            infos.push(BatteryInfo {
                name: battery
                    .model()
                    .map(str::to_string)
                    .unwrap_or_else(|| "Battery".into()),
                percentage: battery.state_of_charge().get::<ratio::percent>(),
                state: state_str(battery.state()).to_string(),
                temperature_celsius: battery
                    .temperature()
                    .map(|t| t.get::<thermodynamic_temperature::degree_celsius>()),
                cycle_count: battery.cycle_count(),
                health_percent: Some(battery.state_of_health().get::<ratio::percent>()),
                energy_wh: Some(battery.energy().get::<energy::watt_hour>()),
                energy_full_wh: Some(battery.energy_full().get::<energy::watt_hour>()),
                energy_design_wh: Some(battery.energy_full_design().get::<energy::watt_hour>()),
                power_watts: Some(battery.energy_rate().get::<power::watt>()),
                voltage_volts: None,
                time_to_full_secs: battery
                    .time_to_full()
                    .map(|t| t.get::<time::second>() as u64),
                time_to_empty_secs: battery
                    .time_to_empty()
                    .map(|t| t.get::<time::second>() as u64),
                vendor: battery.vendor().map(str::to_string),
                model: battery.model().map(str::to_string),
            });
        }

        if infos.is_empty() {
            Some(CollectorUpdate::Battery(Availability::unavailable(
                "no batteries detected",
            )))
        } else {
            Some(CollectorUpdate::Battery(Availability::available(infos)))
        }
    }
}

impl Default for BatteryCollector {
    fn default() -> Self {
        Self::new()
    }
}

fn state_str(state: State) -> &'static str {
    match state {
        State::Charging => "charging",
        State::Discharging => "discharging",
        State::Full => "full",
        State::Empty => "empty",
        _ => "unknown",
    }
}
