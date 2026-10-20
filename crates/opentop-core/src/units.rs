//! Small human-readable formatting helpers shared by the CLI and widgets.

/// "1.5 GB" style formatting with binary units.
pub fn bytes(value: u64) -> String {
    bytes_f64(value as f64)
}

pub fn bytes_f64(value: f64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut value = value.max(0.0);
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// "1.5 MB/s" style formatting for a rate in bytes per second.
pub fn rate(bytes_per_sec: f64) -> String {
    format!("{}/s", bytes_f64(bytes_per_sec))
}

/// "2d 4h", "4h 12m", "12m 30s" or "30s".
pub fn duration(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    let seconds = secs % 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

/// "43.2°C" or "-" when missing.
pub fn celsius(value: Option<f32>) -> String {
    match value {
        Some(v) => format!("{v:.1}°C"),
        None => "-".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_formatting() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.0 KB");
        assert_eq!(bytes(1536), "1.5 KB");
        assert_eq!(bytes(16 * 1024 * 1024 * 1024), "16.0 GB");
    }

    #[test]
    fn duration_formatting() {
        assert_eq!(duration(30), "30s");
        assert_eq!(duration(750), "12m 30s");
        assert_eq!(duration(15_120), "4h 12m");
        assert_eq!(duration(187_200), "2d 4h");
    }
}
