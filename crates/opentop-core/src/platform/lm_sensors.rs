//! Fan speeds via `sensors -j` (lm-sensors). Linux only.

use std::process::Command;

use crate::snapshot::FanReading;

/// Thin wrapper around the `sensors` binary.
pub struct LmSensors;

impl LmSensors {
    /// `None` when lm-sensors is not installed.
    pub fn probe() -> Option<Self> {
        let output = Command::new("sensors").arg("-j").output().ok()?;
        output.status.success().then_some(Self)
    }

    /// Parse fan RPMs out of `sensors -j` output. The JSON nests chips ->
    /// features -> readings, e.g. `{"nct6798": {"fan1": {"fan1_input": 1200.0}}}`.
    pub fn fans(&self) -> Vec<FanReading> {
        let Ok(output) = Command::new("sensors").arg("-j").output() else {
            return Vec::new();
        };
        if !output.status.success() {
            return Vec::new();
        }
        let Ok(json) = serde_json::from_slice::<serde_json::Value>(&output.stdout) else {
            return Vec::new();
        };

        let mut fans = Vec::new();
        let Some(chips) = json.as_object() else {
            return fans;
        };
        for (chip, features) in chips {
            let Some(features) = features.as_object() else {
                continue;
            };
            for (feature, readings) in features {
                let Some(readings) = readings.as_object() else {
                    continue;
                };
                for (key, value) in readings {
                    if !key.ends_with("_input") || !key.contains("fan") {
                        continue;
                    }
                    let Some(rpm) = value.as_f64() else { continue };
                    if rpm <= 0.0 {
                        continue;
                    }
                    fans.push(FanReading {
                        name: format!("{chip}/{feature}"),
                        rpm: rpm as u32,
                        max_rpm: None,
                    });
                }
            }
        }
        fans
    }
}
