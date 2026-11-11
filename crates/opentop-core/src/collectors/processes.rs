use std::time::{Duration, Instant};

use sysinfo::{
    Process, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System, UpdateKind, Users,
};

use super::{Collector, CollectorUpdate};
use crate::snapshot::ProcessInfo;

/// Process table source, truncated to the top N processes by CPU usage.
/// The frontend is free to sort/filter the list further.
pub struct ProcessesCollector {
    sys: System,
    users: Users,
    users_refreshed_at: Instant,
    top_n: usize,
    primed: bool,
}

impl ProcessesCollector {
    pub fn new(top_n: usize) -> Self {
        Self {
            sys: System::new(),
            users: Users::new_with_refreshed_list(),
            users_refreshed_at: Instant::now(),
            top_n: top_n.max(1),
            primed: false,
        }
    }
}

impl Collector for ProcessesCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        let refresh = ProcessRefreshKind::nothing()
            .with_cpu()
            .with_memory()
            .with_user(UpdateKind::Always)
            .with_cmd(UpdateKind::Always);
        self.sys
            .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);
        if !self.primed {
            self.primed = true;
            std::thread::sleep(Duration::from_millis(300));
            self.sys
                .refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);
        }
        // shortcut: account-name changes can lag 60 seconds; refresh sooner if needed.
        if self.users_refreshed_at.elapsed() >= Duration::from_secs(60) {
            self.users.refresh();
            self.users_refreshed_at = Instant::now();
        }

        let mut candidates: Vec<&Process> = self.sys.processes().values().collect();
        if candidates.len() > self.top_n {
            candidates.select_nth_unstable_by(self.top_n, |a, b| process_order(a, b));
            candidates.truncate(self.top_n);
        }
        candidates.sort_unstable_by(|a, b| process_order(a, b));

        let processes: Vec<ProcessInfo> = candidates
            .into_iter()
            .map(|p| ProcessInfo {
                pid: p.pid().as_u32(),
                parent_pid: p.parent().map(|pid| pid.as_u32()),
                name: p.name().to_string_lossy().into_owned(),
                cmd: p
                    .cmd()
                    .iter()
                    .map(|arg| arg.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                user: p
                    .user_id()
                    .and_then(|uid| self.users.get_user_by_id(uid))
                    .map(|user| user.name().to_string()),
                cpu_percent: p.cpu_usage(),
                memory_bytes: p.memory(),
                virtual_memory_bytes: p.virtual_memory(),
                status: status_str(p.status()),
                num_threads: p.tasks().map(|t| t.len()).unwrap_or(0),
                running_time_secs: p.run_time(),
                start_time_secs: p.start_time(),
            })
            .collect();

        Some(CollectorUpdate::Processes(processes))
    }
}

fn process_order(a: &Process, b: &Process) -> std::cmp::Ordering {
    b.cpu_usage()
        .partial_cmp(&a.cpu_usage())
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.pid().cmp(&b.pid()))
}

fn status_str(status: ProcessStatus) -> String {
    match status {
        ProcessStatus::Idle => "idle",
        ProcessStatus::Run => "run",
        ProcessStatus::Sleep => "sleep",
        ProcessStatus::Stop => "stop",
        ProcessStatus::Zombie => "zombie",
        ProcessStatus::Tracing => "tracing",
        ProcessStatus::Dead => "dead",
        ProcessStatus::Wakekill => "wakekill",
        ProcessStatus::Waking => "waking",
        ProcessStatus::Parked => "parked",
        ProcessStatus::LockBlocked => "lock-blocked",
        ProcessStatus::UninterruptibleDiskSleep => "disk-sleep",
        ProcessStatus::Unknown(_) => "unknown",
        _ => "unknown",
    }
    .to_string()
}

#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
mod tests {
    use super::*;

    #[test]
    fn unlimited_collection_includes_own_process_and_command() {
        let mut collector = ProcessesCollector::new(usize::MAX);
        let Some(CollectorUpdate::Processes(processes)) = collector.sample(Duration::from_secs(2))
        else {
            panic!("expected process data");
        };
        let current = processes
            .iter()
            .find(|process| process.pid == std::process::id())
            .expect("current process should be visible");
        assert!(!current.cmd.is_empty());
    }
}
