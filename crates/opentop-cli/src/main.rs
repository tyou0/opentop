use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use opentop_core::snapshot::Snapshot;
use opentop_core::units;
use opentop_core::{MonitorConfig, MonitorHandle};

#[derive(Parser)]
#[command(name = "opentop-cli", version, about = "opentop CLI harness")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Dump(DumpArgs),
}

#[derive(Args)]
struct DumpArgs {
    #[arg(long)]
    json: bool,
    #[arg(long)]
    pretty: bool,
    #[arg(long, default_value_t = 0)]
    watch: u64,
    #[arg(long, default_value_t = 10)]
    top: usize,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Dump(args) => dump(args),
    }
}

fn dump(args: DumpArgs) -> ExitCode {
    let config = MonitorConfig {
        top_processes: args.top.max(50),
        ..MonitorConfig::default()
    };
    let monitor = MonitorHandle::start(config);
    monitor.wait_ready(Duration::from_millis(1500));
    std::thread::sleep(Duration::from_millis(300));

    loop {
        let snapshot = monitor.snapshot();
        if args.json {
            let line = if args.pretty && args.watch == 0 {
                serde_json::to_string_pretty(&snapshot)
            } else {
                serde_json::to_string(&snapshot)
            };
            match line {
                Ok(line) => println!("{line}"),
                Err(err) => {
                    eprintln!("failed to serialize snapshot: {err}");
                    return ExitCode::FAILURE;
                }
            }
        } else {
            print_summary(&snapshot);
        }

        if args.watch == 0 {
            break;
        }
        std::thread::sleep(Duration::from_secs(args.watch.max(1)));
    }
    ExitCode::SUCCESS
}

fn print_summary(snapshot: &Snapshot) {
    let system = &snapshot.system;
    println!(
        "opentop — {} ({})",
        system.hostname.as_deref().unwrap_or("unknown host"),
        system
            .long_os_version
            .clone()
            .or_else(|| system.os_name.clone())
            .unwrap_or_else(|| "unknown OS".into()),
    );
    println!(
        "  kernel {} | {} | uptime {} | load {}",
        system.kernel_long_version,
        system.cpu_arch,
        units::duration(system.uptime_secs),
        system
            .load_average
            .map(|l| format!("{:.2} {:.2} {:.2}", l.one, l.five, l.fifteen))
            .unwrap_or_else(|| "-".into()),
    );

    let cpu = &snapshot.cpu;
    println!(
        "\nCPU  {} | {:.1}% total | {} logical cores",
        cpu.brand,
        cpu.usage_percent,
        cpu.cores.len()
    );
    if !snapshot.sensors.cpu_tiers.is_empty() {
        let tiers: Vec<String> = snapshot
            .sensors
            .cpu_tiers
            .iter()
            .map(|t| {
                format!(
                    "{}: {} cores @ {:.2} GHz ({:.0}% active)",
                    t.label,
                    t.cores,
                    t.frequency_mhz as f32 / 1000.0,
                    t.active_ratio * 100.0
                )
            })
            .collect();
        println!("  tiers {}", tiers.join(" | "));
    }

    let memory = &snapshot.memory;
    println!(
        "\nMEM  {} / {} ({:.0}%) | swap {} / {}",
        units::bytes(memory.used_bytes),
        units::bytes(memory.total_bytes),
        memory.usage_percent,
        units::bytes(memory.used_swap_bytes),
        units::bytes(memory.total_swap_bytes),
    );
}
