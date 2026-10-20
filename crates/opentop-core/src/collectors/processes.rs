use std::time::Duration;

use sysinfo::{ProcessStatus, ProcessesToUpdate, System, Users};

use super::{Collector, CollectorUpdate};
use crate::snapshot::ProcessInfo;

/// Process table source, truncated to the top N processes by CPU usage.
/// The frontend is free to sort/filter the list further.
pub struct ProcessesCollector {
    sys: System,
    users: Users,
    top_n: usize,
    primed: bool,
}

impl ProcessesCollector {
    pub fn new(top_n: usize) -> Self {
        Self {
            sys: System::new(),
            users: Users::new_with_refreshed_list(),
            top_n: top_n.max(1),
            primed: false,
        }
    }
}

impl Collector for ProcessesCollector {
    fn sample(&mut self, _interval: Duration) -> Option<CollectorUpdate> {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        if !self.primed {
            self.primed = true;
            std::thread::sleep(Duration::from_millis(300));
            self.sys.refresh_processes(ProcessesToUpdate::All, true);
        }
        self.users.refresh();

        let user_map: Vec<(String, String)> = self
            .users
            .list()
            .iter()
            .map(|u| (u.id().to_string(), u.name().to_string()))
            .collect();

        let mut processes: Vec<ProcessInfo> = self
            .sys
            .processes()
            .values()
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
                user: p.user_id().and_then(|uid| {
                    let uid = uid.to_string();
                    user_map
                        .iter()
                        .find(|(id, _)| *id == uid)
                        .map(|(_, name)| name.clone())
                }),
                cpu_percent: p.cpu_usage(),
                memory_bytes: p.memory(),
                virtual_memory_bytes: p.virtual_memory(),
                status: status_str(p.status()),
                num_threads: p.tasks().map(|t| t.len()).unwrap_or(0),
                running_time_secs: p.run_time(),
                start_time_secs: p.start_time(),
            })
            .collect();

        processes.sort_by(|a, b| {
            b.cpu_percent
                .partial_cmp(&a.cpu_percent)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.pid.cmp(&b.pid))
        });
        processes.truncate(self.top_n);
        Some(CollectorUpdate::Processes(processes))
    }
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
