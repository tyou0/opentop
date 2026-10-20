//! Platform-specific sensor backends.
//!
//! [`SensorsAggregate`] picks the richest backend available for the current
//! OS at runtime and degrades gracefully:
//!
//! | Platform | Backend |
//! |---|---|
//! | macOS aarch64 | macmon (temps, fans, power, GPU, CPU tiers), sudo-free |
//! | Linux | sysinfo components (hwmon) + optional `sensors -j` fans |
//! | Windows | sysinfo components (WMI, best effort) |
//! | anything else / failure | sysinfo components |
//!
//! With the `gpu-nvidia` feature, nvidia-smi data is merged in wherever the
//! backend does not provide GPU data itself.

use std::time::Duration;

use crate::snapshot::SensorsInfo;

pub mod hwmon;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub mod macos;

#[cfg(feature = "gpu-nvidia")]
pub mod nvidia;

#[cfg(all(target_os = "linux", feature = "sensors-lm-sensors"))]
pub mod lm_sensors;

/// One source of sensor readings for the current platform.
///
/// Backends are constructed inside the sensors thread and never cross
/// threads, which matters on macOS where macmon holds non-`Send`
/// CoreFoundation handles.
pub trait SensorsBackend {
    /// Take one sample. Backends that need a measurement window (macmon)
    /// block for approximately `interval`.
    fn sample(&mut self, interval: Duration) -> SensorsInfo;

    /// Short backend name, exposed in [`SensorsInfo::source`].
    fn name(&self) -> &'static str;
}

/// The composed sensor source used by the monitor.
pub struct SensorsAggregate {
    primary: Box<dyn SensorsBackend>,
    #[cfg(feature = "gpu-nvidia")]
    nvidia: Option<nvidia::NvidiaSampler>,
}

impl SensorsAggregate {
    pub fn create() -> Self {
        Self {
            primary: primary_backend(),
            #[cfg(feature = "gpu-nvidia")]
            nvidia: nvidia::NvidiaSampler::new(),
        }
    }

    pub fn source_name(&self) -> &'static str {
        self.primary.name()
    }

    pub fn sample(&mut self, interval: Duration) -> SensorsInfo {
        let mut info = self.primary.sample(interval);
        #[cfg(feature = "gpu-nvidia")]
        if let Some(nvidia) = &mut self.nvidia {
            nvidia.merge(&mut info);
        }
        info
    }
}

fn primary_backend() -> Box<dyn SensorsBackend> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        if let Some(backend) = macos::MacmonBackend::new() {
            return Box::new(backend);
        }
    }
    #[cfg(target_os = "linux")]
    {
        return Box::new(linux::LinuxBackend::new());
    }
    #[allow(unreachable_code)]
    Box::new(hwmon::HwmonBackend::new())
}
