use sysinfo::{Pid, ProcessesToUpdate, System};

use crate::snapshot::ProcessInfo;

/// Send a termination request only if the selected process still matches.
pub fn terminate(selected: &ProcessInfo) -> Result<(), String> {
    if selected.pid == 0 || selected.start_time_secs == 0 || selected.name.is_empty() {
        return Err("Process identity is unavailable; refresh and try again".into());
    }

    #[cfg(target_os = "linux")]
    let handle = {
        use rustix::process::{Pid as RustixPid, PidfdFlags, pidfd_open};
        let raw = i32::try_from(selected.pid).map_err(|_| "Invalid process ID")?;
        let pid = RustixPid::from_raw(raw).ok_or("Invalid process ID")?;
        pidfd_open(pid, PidfdFlags::empty())
            .map_err(|error| format!("Cannot open process: {error}"))?
    };

    let pid = Pid::from_u32(selected.pid);
    let mut system = System::new();
    system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
    let process = system.process(pid).ok_or("Process has exited")?;
    let command = process
        .cmd()
        .iter()
        .map(|arg| arg.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ");
    if process.start_time() != selected.start_time_secs
        || process.name().to_string_lossy() != selected.name
        || (!selected.cmd.is_empty() && command != selected.cmd)
    {
        return Err("Process has changed; refresh and select it again".into());
    }

    #[cfg(target_os = "linux")]
    {
        rustix::process::pidfd_send_signal(handle, rustix::process::Signal::TERM)
            .map_err(|error| format!("Cannot terminate process: {error}"))
    }
    #[cfg(not(target_os = "linux"))]
    {
        #[cfg(windows)]
        let sent = process.kill();
        #[cfg(not(windows))]
        let sent = process.kill_with(sysinfo::Signal::Term).unwrap_or(false);
        if sent {
            Ok(())
        } else {
            Err("Could not terminate process (permission denied or process exited)".into())
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};

    #[test]
    fn only_the_matching_process_is_terminated() {
        let mut child = Command::new("sleep").arg("60").spawn().unwrap();
        let pid = Pid::from_u32(child.id());
        let mut system = System::new();
        system.refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        let process = system.process(pid).unwrap();
        let selected = ProcessInfo {
            pid: child.id(),
            start_time_secs: process.start_time(),
            name: process.name().to_string_lossy().into_owned(),
            ..ProcessInfo::default()
        };
        let mut stale = selected.clone();
        stale.start_time_secs += 1;
        assert!(terminate(&stale).is_err());
        assert!(child.try_wait().unwrap().is_none());
        terminate(&selected).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                panic!("termination request did not stop the child process");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
