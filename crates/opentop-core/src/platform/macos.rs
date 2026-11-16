use std::time::Duration;

use macmon::Sampler;

use super::SensorsBackend;
use crate::snapshot::{CpuTier, FanReading, GpuReading, PowerReading, SensorsInfo, TempReading};

/// Apple Silicon sensors via macmon: CPU/GPU temperatures, fans, package
/// power, GPU utilization and CPU frequency tiers. No sudo required.
pub struct MacmonBackend {
    sampler: Sampler,
}

impl MacmonBackend {
    /// `None` when macmon cannot initialize (e.g. inside a VM); the caller
    /// falls back to the portable hwmon backend.
    pub fn new() -> Option<Self> {
        Sampler::new().ok().map(|sampler| Self { sampler })
    }
}

impl SensorsBackend for MacmonBackend {
    fn sample(&mut self, interval: Duration) -> SensorsInfo {
        let metrics = match self.sampler.get_metrics(interval.as_millis() as u32) {
            Ok(metrics) => metrics,
            Err(err) => {
                // Do not spin: get_metrics may return immediately on failure.
                std::thread::sleep(interval);
                return SensorsInfo {
                    source: format!("macmon ({err})"),
                    ..SensorsInfo::default()
                };
            }
        };

        let mut temps = Vec::new();
        if let Some(celsius) = metrics.temp.cpu_temp_avg {
            temps.push(TempReading {
                label: "CPU avg".into(),
                celsius,
                max_celsius: None,
                critical_celsius: None,
            });
        }
        if let Some(celsius) = metrics.temp.gpu_temp_avg {
            temps.push(TempReading {
                label: "GPU avg".into(),
                celsius,
                max_celsius: None,
                critical_celsius: None,
            });
        }

        let fans = metrics
            .fans
            .iter()
            .map(|fan| FanReading {
                name: fan.name.clone(),
                rpm: fan.rpm,
                max_rpm: fan.max_rpm,
            })
            .collect();

        let power = Some(PowerReading {
            cpu_watts: Some(metrics.cpu_power),
            gpu_watts: Some(metrics.gpu_power),
            ane_watts: Some(metrics.ane_power),
            ram_watts: (metrics.ram_power > 0.0).then_some(metrics.ram_power),
            combined_watts: Some(metrics.all_power),
            system_watts: (metrics.sys_power > 0.0).then_some(metrics.sys_power),
        });

        let soc = self.sampler.get_soc_info();
        let gpu = Some(GpuReading {
            name: Some(if soc.chip_name.is_empty() {
                "Apple GPU".into()
            } else {
                format!("{} GPU", soc.chip_name)
            }),
            cores: (soc.gpu_cores > 0).then_some(u32::from(soc.gpu_cores)),
            usage_percent: Some(metrics.gpu_active_ratio * 100.0),
            scaled_percent: Some(metrics.gpu_scaled_ratio * 100.0),
            frequency_mhz: (metrics.gpu_freq_mhz > 0).then_some(metrics.gpu_freq_mhz),
            power_watts: Some(metrics.gpu_power),
            temperature_celsius: metrics.temp.gpu_temp_avg,
            memory_used_bytes: None,
            memory_total_bytes: None,
        });

        let cpu_tiers = metrics
            .cpu_tiers
            .iter()
            .map(|tier| CpuTier {
                label: tier.label.clone(),
                frequency_mhz: tier.freq_mhz,
                active_ratio: tier.active_ratio,
                cores: tier.cores.len(),
            })
            .collect();

        SensorsInfo {
            source: self.name().to_string(),
            temps,
            fans,
            power,
            gpu,
            cpu_tiers,
        }
    }

    fn name(&self) -> &'static str {
        "macmon"
    }
}
