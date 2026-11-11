//! Disk health (SMART) via smartctl's JSON interface.
//!
//! Works on macOS/Linux/Windows wherever `smartctl` (smartmontools) is
//! installed. Devices are discovered with `smartctl --scan[-open] --json`
//! and queried with `smartctl -n standby -a --json <device>`, so sleeping
//! disks are not woken up. When smartctl is missing or a device cannot be
//! read, the section reports [`Availability::Unavailable`] or a note instead
//! of failing.
//!
//! smartctl runs can take seconds, so all subprocess work happens on an
//! internal worker thread; [`Collector::sample`] only picks up the latest
//! result and never holds the shared lock while smartctl runs.

use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::Value;

use super::collectors::{Collector, CollectorUpdate, sleep_until};
use crate::snapshot::{Availability, DiskHealth};
use crate::subprocess::{RunError, run_bounded};
use crate::units;

/// How long a failed device scan is remembered before retrying (the user may
/// install smartmontools while opentop runs).
const SCAN_RETRY: Duration = Duration::from_secs(600);

pub struct SmartCollector {
    rx: Receiver<Availability<Vec<DiskHealth>>>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    latest: Availability<Vec<DiskHealth>>,
    dirty: bool,
}

#[derive(Debug, Clone)]
struct SmartDevice {
    /// Device name to pass back to smartctl.
    name: String,
    protocol: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ScanOutput {
    #[serde(default)]
    devices: Vec<ScanDevice>,
}

#[derive(Debug, Deserialize)]
struct ScanDevice {
    name: String,
    #[serde(default)]
    protocol: Option<String>,
}

impl SmartCollector {
    pub fn new(scan_interval: Duration) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("opentop-smart-worker".into())
            .spawn(move || smart_worker(scan_interval, tx, worker_stop))
            .expect("failed to spawn smart worker thread");
        Self {
            rx,
            stop,
            worker: Some(worker),
            latest: Availability::unavailable("scanning for SMART devices…"),
            dirty: true,
        }
    }
}

impl Drop for SmartCollector {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
    }
}

/// Runs smartctl periodically and reports the latest result. Exits when the
/// receiver is dropped or `stop` is set.
fn smart_worker(
    interval: Duration,
    tx: Sender<Availability<Vec<DiskHealth>>>,
    stop: Arc<AtomicBool>,
) {
    let mut devices: Vec<SmartDevice> = Vec::new();
    let mut last_scan: Option<Instant> = None;
    let mut scan_error: Option<String> = None;

    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let started = Instant::now();

        let should_rescan = match last_scan {
            None => true,
            Some(last) => {
                // Periodic rescans pick up newly attached drives; when the
                // scan is failing we retry more slowly so users have time to
                // install smartmontools.
                if devices.is_empty() && scan_error.is_some() {
                    last.elapsed() >= SCAN_RETRY
                } else {
                    last.elapsed() >= interval
                }
            }
        };
        if should_rescan {
            last_scan = Some(Instant::now());
            match scan_devices(&stop) {
                Ok(found) => {
                    scan_error = if found.is_empty() {
                        Some("no SMART-capable devices found".into())
                    } else {
                        None
                    };
                    devices = found;
                }
                Err(err) => {
                    devices.clear();
                    scan_error = Some(err);
                }
            }
        }

        let result = if devices.is_empty() {
            Availability::unavailable(
                scan_error
                    .clone()
                    .unwrap_or_else(|| "no SMART devices available".into()),
            )
        } else {
            Availability::available(
                devices
                    .iter()
                    .map(|device| query_device(device, &stop))
                    .collect(),
            )
        };

        if tx.send(result).is_err() {
            return;
        }
        sleep_until(started + interval, &stop);
    }
}

/// Discover SMART-capable devices. The error string is user-facing.
fn scan_devices(stop: &AtomicBool) -> Result<Vec<SmartDevice>, String> {
    // --scan-open tries to open devices, which is what we need; fall back
    // to plain --scan on older builds.
    let mut command = Command::new("smartctl");
    command.args(["--scan-open", "--json"]);
    let output = bounded_smartctl(&mut command, stop)?;

    let parsed = serde_json::from_slice::<ScanOutput>(&output).or_else(|_| {
        let mut fallback = Command::new("smartctl");
        fallback.args(["--scan", "--json"]);
        let output = bounded_smartctl(&mut fallback, stop).map_err(|_| ())?;
        serde_json::from_slice::<ScanOutput>(&output).map_err(|_| ())
    });

    match parsed {
        Ok(scan) => Ok(scan
            .devices
            .into_iter()
            .map(|device| SmartDevice {
                name: device.name,
                protocol: device.protocol,
            })
            .collect()),
        Err(_) => Err("could not parse smartctl --scan output".into()),
    }
}

fn query_device(device: &SmartDevice, stop: &AtomicBool) -> DiskHealth {
    let mut command = Command::new("smartctl");
    command
        .args(["-n", "standby", "-a", "--json"])
        .arg(&device.name);

    match run_bounded(&mut command, Duration::from_secs(5), Some(stop)) {
        Ok(output) => {
            // smartctl uses non-zero exit codes for partial success; the
            // JSON is still valid, so only parse failures matter.
            match serde_json::from_slice::<Value>(&output.stdout) {
                Ok(json) => {
                    let mut health = parse_smart_json(&json, &device.name);
                    if health.interface.is_none() {
                        health.interface = device.protocol.clone();
                    }
                    if health.note.is_none() {
                        if let Some(protocol) = &device.protocol {
                            if protocol == "NVMe"
                                && health
                                    .overall_health
                                    .as_ref()
                                    .map(|_| health.overall_health.as_deref() == Some("PASSED"))
                                    .or_else(|| {
                                        json.pointer("/smart_status/passed")
                                            .and_then(|v| v.as_bool())
                                    })
                                    .is_none()
                            {
                                health.note = Some("device did not report SMART status".into());
                            }
                        }
                    }
                    health
                }
                Err(_) => DiskHealth {
                    device: device.name.clone(),
                    interface: device.protocol.clone(),
                    note: Some(format!(
                        "smartctl produced unparseable output (exit {:?})",
                        output.status.code()
                    )),
                    ..DiskHealth::default()
                },
            }
        }
        Err(err) => DiskHealth {
            device: device.name.clone(),
            interface: device.protocol.clone(),
            note: Some(format!("smartctl {}", run_error_label(&err))),
            ..DiskHealth::default()
        },
    }
}

fn bounded_smartctl(command: &mut Command, stop: &AtomicBool) -> Result<Vec<u8>, String> {
    run_bounded(command, Duration::from_secs(5), Some(stop))
        .map(|output| output.stdout)
        .map_err(|error| match &error {
            RunError::Spawn(io_error) if io_error.kind() == std::io::ErrorKind::NotFound => {
                "smartctl not found (install smartmontools)".into()
            }
            _ => format!("smartctl {}", run_error_label(&error)),
        })
}

fn run_error_label(error: &RunError) -> &'static str {
    match error {
        RunError::Spawn(_) => "failed to start",
        RunError::Timeout => "timed out",
        RunError::Cancelled => "cancelled",
        RunError::OutputLimit => "output exceeded the limit",
        RunError::Wait(_) => "failed while waiting",
    }
}

impl Collector for SmartCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        while let Ok(result) = self.rx.try_recv() {
            self.latest = result;
            self.dirty = true;
        }
        if self.dirty {
            self.dirty = false;
            Some(CollectorUpdate::DiskHealth(self.latest.clone()))
        } else {
            None
        }
    }
}

/// Parse a `smartctl -a --json` document into a [`DiskHealth`].
///
/// Lenient by design: every field is optional, so partial output (standby
/// disks, permission problems) still yields a usable record.
pub fn parse_smart_json(json: &Value, device: &str) -> DiskHealth {
    let string_at = |pointer: &str| -> Option<String> {
        json.pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|s| !s.is_empty())
    };
    let u64_at = |pointer: &str| -> Option<u64> {
        json.pointer(pointer).and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().and_then(|i| u64::try_from(i).ok()))
        })
    };
    let f64_at = |pointer: &str| -> Option<f64> {
        json.pointer(pointer).and_then(|v| {
            v.as_f64()
                .or_else(|| v.as_u64().map(|u| u as f64))
                .or_else(|| v.as_i64().map(|i| i as f64))
        })
    };

    let nvme = |pointer: &str| format!("/nvme_smart_health_information_log/{pointer}");

    let data_units_to_bytes = |units: Option<u64>| {
        // NVMe data units are 512,000 bytes each (1000 * 512).
        units.map(|u| u.saturating_mul(512_000))
    };

    let mut health = DiskHealth {
        device: device.to_string(),
        model: string_at("/model_name")
            .or_else(|| string_at("/model_family"))
            .or_else(|| string_at("/device/model_name"))
            .or(Some("unknown".to_string())),
        serial: string_at("/serial_number"),
        interface: string_at("/device/protocol").map(|p| p.to_uppercase()),
        smart_supported: true,
        smart_enabled: true,
        overall_health: json
            .pointer("/smart_status/passed")
            .and_then(Value::as_bool)
            .map(|p| {
                if p {
                    "PASSED".to_string()
                } else {
                    "FAILED".to_string()
                }
            }),
        temperature_celsius: u64_at("/temperature/current")
            .or_else(|| u64_at(&nvme("temperature")))
            .map(|t| t as f64),
        wear_percent: f64_at(&nvme("percentage_used")),
        data_written_bytes: data_units_to_bytes(u64_at(&nvme("data_units_written"))),
        data_read_bytes: data_units_to_bytes(u64_at(&nvme("data_units_read"))),
        power_on_hours: u64_at("/power_on_time/hours").or_else(|| u64_at(&nvme("power_on_hours"))),
        power_cycles: u64_at("/power_cycle_count").or_else(|| u64_at(&nvme("power_cycles"))),
        unsafe_shutdowns: u64_at(&nvme("unsafe_shutdowns")),
        media_errors: u64_at(&nvme("media_errors")),
        reallocated_sectors: ata_attribute_raw(json, 5),
        note: None,
    };

    if let Some(message) = json
        .pointer("/messages")
        .and_then(Value::as_array)
        .and_then(|messages| messages.first())
        .and_then(|m| m.pointer("/string"))
        .and_then(Value::as_str)
    {
        health.note = Some(message.to_string());
    }

    health
}

/// Raw value of a SATA SMART attribute by id (e.g. 5 = reallocated sectors).
fn ata_attribute_raw(json: &Value, id: u64) -> Option<u64> {
    json.pointer("/ata_smart_attributes/table")
        .and_then(Value::as_array)
        .and_then(|table| {
            table
                .iter()
                .find(|attr| attr.pointer("/id").and_then(Value::as_u64) == Some(id))
        })
        .and_then(|attr| attr.pointer("/raw/value"))
        .and_then(|v| {
            v.as_u64()
                .or_else(|| v.as_i64().and_then(|i| u64::try_from(i).ok()))
        })
}

/// Human summary of a [`DiskHealth`] record, e.g. for logs or the CLI.
pub fn health_summary(health: &DiskHealth) -> String {
    use std::fmt::Write;

    let mut out = String::new();
    let status = match health.overall_health.as_deref() {
        Some("PASSED") => "PASSED",
        Some("FAILED") => "FAILED",
        _ => "unknown",
    };
    let _ = write!(out, "{status}");
    if let Some(t) = health.temperature_celsius {
        let _ = write!(out, " | {}°C", t);
    }

    if let Some(wear) = health.wear_percent {
        let _ = write!(out, " | wear {:.0}%", wear);
    }
    if let Some(written) = health.data_written_bytes {
        let _ = write!(out, " | {} written", units::bytes(written));
    }
    if let Some(hours) = health.power_on_hours {
        let _ = write!(out, " | {}", units::duration(hours * 3600));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const NVME_SAMPLE: &str = r#"{
        "model_name": "APPLE SSD AP0512R",
        "serial_number": "0ba016eb81606e3c",
        "firmware_version": "555",
        "smart_status": { "passed": true, "nvme": { "value": 0 } },
        "temperature": { "current": 43 },
        "power_on_time": { "hours": 7353 },
        "power_cycle_count": 298,
        "nvme_smart_health_information_log": {
            "critical_warning": 0,
            "temperature": 43,
            "available_spare": 100,
            "available_spare_threshold": 99,
            "percentage_used": 8,
            "data_units_read": 1053438156,
            "data_units_written": 328307308,
            "power_on_hours": 7353,
            "power_cycles": 298,
            "unsafe_shutdowns": 33,
            "media_errors": 0
        },
        "messages": null
    }"#;

    const ATA_SAMPLE: &str = r#"{
        "model_name": "Samsung SSD 860",
        "serial_number": "S1234",
        "smart_status": { "passed": false },
        "temperature": { "current": 31 },
        "power_on_time": { "hours": 20000 },
        "ata_smart_attributes": {
            "table": [
                { "id": 1, "name": "Raw_Read_Error_Rate", "raw": { "value": 0 } },
                { "id": 5, "name": "Reallocated_Sector_Ct", "raw": { "value": 12 } }
            ]
        },
        "messages": [ { "severity": "error", "string": "SMART error logged" } ]
    }"#;

    #[test]
    fn parses_nvme_json() {
        let json: Value = serde_json::from_str(NVME_SAMPLE).unwrap();
        let health = parse_smart_json(&json, "disk0");
        assert_eq!(health.device, "disk0");
        assert_eq!(health.model.as_deref(), Some("APPLE SSD AP0512R"));
        assert_eq!(health.serial.as_deref(), Some("0ba016eb81606e3c"));
        assert_eq!(health.overall_health.as_deref(), Some("PASSED"));
        assert_eq!(health.temperature_celsius, Some(43.0));
        assert_eq!(health.wear_percent, Some(8.0));
        assert_eq!(health.power_on_hours, Some(7353));
        assert_eq!(health.power_cycles, Some(298));
        assert_eq!(health.unsafe_shutdowns, Some(33));
        assert_eq!(health.media_errors, Some(0));
        assert_eq!(health.data_written_bytes, Some(328_307_308 * 512_000));
    }

    #[test]
    fn parses_ata_json_with_note_and_attributes() {
        let json: Value = serde_json::from_str(ATA_SAMPLE).unwrap();
        let health = parse_smart_json(&json, "/dev/sda");
        assert_eq!(
            health
                .overall_health
                .as_ref()
                .map(|_| health.overall_health.as_deref() == Some("PASSED"))
                .or_else(|| json
                    .pointer("/smart_status/passed")
                    .and_then(|v| v.as_bool())),
            Some(false)
        );
        assert_eq!(health.temperature_celsius, Some(31.0));
        assert_eq!(health.reallocated_sectors, Some(12));
        assert_eq!(health.note.as_deref(), Some("SMART error logged"));
    }

    #[test]
    fn handles_standby_partial_output() {
        let json: Value = serde_json::from_str(r#"{"power_mode": {"is_standby": true}}"#).unwrap();
        let health = parse_smart_json(&json, "disk1");
        assert_eq!(health.overall_health, None);
        assert_eq!(health.temperature_celsius, None);
        assert_eq!(health.model.as_deref(), Some("unknown"));
    }
}
