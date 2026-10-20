//! Optional GPU data via `nvidia-smi`, available on Linux and Windows when
//! an NVIDIA driver is installed. Data is merged into the primary sensor
//! backend's result and never overwrites richer platform data (macmon).

use std::process::Command;

use crate::snapshot::{GpuReading, PowerReading, SensorsInfo, TempReading};

const QUERY: &str =
    "name,utilization.gpu,clocks.current.graphics,power.draw,temperature.gpu,memory.used,memory.total";

#[derive(Debug, Clone, Default)]
struct NvidiaData {
    name: Option<String>,
    utilization_percent: Option<f32>,
    frequency_mhz: Option<u32>,
    power_watts: Option<f32>,
    temperature_celsius: Option<f32>,
    memory_used_bytes: Option<u64>,
    memory_total_bytes: Option<u64>,
}

pub struct NvidiaSampler;

impl NvidiaSampler {
    /// `None` when nvidia-smi is missing or fails (no NVIDIA GPU).
    pub fn new() -> Option<Self> {
        Self::query().map(|_| Self)
    }

    fn query() -> Option<NvidiaData> {
        let output = Command::new("nvidia-smi")
            .args(["--query-gpu", QUERY, "--format", "csv,noheader,nounits"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let line = stdout.lines().next()?;
        Some(parse_csv_line(line))
    }

    /// Fill in GPU/power data when the primary backend has none.
    pub fn merge(&mut self, info: &mut SensorsInfo) {
        let Some(data) = Self::query() else { return };

        let gpu_missing = info.gpu.is_none();
        if let Some(temp) = data.temperature_celsius {
            if gpu_missing {
                info.temps.push(TempReading {
                    label: data
                        .name
                        .clone()
                        .map(|n| format!("{n} temp"))
                        .unwrap_or_else(|| "GPU temp".into()),
                    celsius: temp,
                    max_celsius: None,
                    critical_celsius: None,
                });
            }
        }
        if gpu_missing {
            info.gpu = Some(GpuReading {
                name: data.name.clone(),
                usage_percent: data.utilization_percent,
                scaled_percent: None,
                frequency_mhz: data.frequency_mhz,
                power_watts: data.power_watts,
                temperature_celsius: data.temperature_celsius,
                memory_used_bytes: data.memory_used_bytes,
                memory_total_bytes: data.memory_total_bytes,
            });
        }
        if info.power.is_none() {
            info.power = Some(PowerReading {
                gpu_watts: data.power_watts,
                ..PowerReading::default()
            });
        }
        if info.source.is_empty() || info.source == "none" {
            info.source = "nvidia".into();
        } else if !info.source.contains("nvidia") {
            info.source = format!("{} + nvidia", info.source);
        }
    }
}

/// One CSV row with units suppressed, e.g.:
/// `NVIDIA GeForce RTX 4090, 45, 2610, 120.50, 52, 2048, 24564`
fn parse_csv_line(line: &str) -> NvidiaData {
    let parts: Vec<&str> = line.split(',').map(str::trim).collect();
    let num = |i: usize| -> Option<f64> { parts.get(i).and_then(|v| v.parse().ok()) };
    NvidiaData {
        name: parts.first().map(|s| s.to_string()).filter(|s| !s.is_empty()),
        utilization_percent: num(1).map(|v| v as f32),
        frequency_mhz: num(2).map(|v| v as u32),
        power_watts: num(3).map(|v| v as f32),
        temperature_celsius: num(4).map(|v| v as f32),
        memory_used_bytes: num(5).map(|v| (v * 1024.0 * 1024.0) as u64),
        memory_total_bytes: num(6).map(|v| (v * 1024.0 * 1024.0) as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csv_rows_leniently() {
        let data = parse_csv_line("NVIDIA GeForce RTX 4090, 45, 2610, 120.50, 52, 2048, 24564");
        assert_eq!(data.name.as_deref(), Some("NVIDIA GeForce RTX 4090"));
        assert_eq!(data.utilization_percent, Some(45.0));
        assert_eq!(data.frequency_mhz, Some(2610));
        assert_eq!(data.power_watts, Some(120.5));
        assert_eq!(data.temperature_celsius, Some(52.0));
        assert_eq!(data.memory_used_bytes, Some(2048 * 1024 * 1024));

        let data = parse_csv_line("NVIDIA GPU, [N/A], [Not Supported], 10.0, 40, [N/A], 8192");
        assert_eq!(data.utilization_percent, None);
        assert_eq!(data.frequency_mhz, None);
        assert_eq!(data.power_watts, Some(10.0));
        assert_eq!(data.memory_used_bytes, None);
    }
}
