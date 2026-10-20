//! Temporary diagnostic: watch history ring lengths over time.
//! Run: cargo run -p opentop-core --example history_probe

use std::time::{Duration, Instant};

use opentop_core::{MonitorConfig, MonitorHandle};

fn main() {
    let monitor = MonitorHandle::start(MonitorConfig::default());
    let start = Instant::now();
    for i in 0..20 {
        std::thread::sleep(Duration::from_millis(500));
        monitor.read_history(|h| {
            println!(
                "t={:>5.2}s cpu_total={:<3} memory={:<3} net_en0_rx={:<3} procs={:<3} disks={:<3}",
                start.elapsed().as_secs_f64(),
                h.cpu_total.len(),
                h.memory_percent.len(),
                h.net.get("en0").map(|n| n.rx.len()).unwrap_or(0),
                monitor.read_snapshot(|s| s.processes.len()),
                monitor.read_snapshot(|s| s.disks.len()),
            );
        });
        if i == 14 {
            println!("--- expected: ~8 cpu samples by now");
        }
    }
}
