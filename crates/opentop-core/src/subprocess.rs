use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant};

const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

#[derive(Debug)]
pub enum RunError {
    Spawn(io::Error),
    Timeout,
    Cancelled,
    OutputLimit,
    Wait(io::Error),
}

#[derive(Debug)]
pub struct RunOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub fn run_bounded(
    command: &mut Command,
    timeout: Duration,
    stop: Option<&AtomicBool>,
) -> Result<RunOutput, RunError> {
    run_bounded_with_limit(command, timeout, stop, MAX_OUTPUT_BYTES)
}

pub fn run_bounded_with_limit(
    command: &mut Command,
    timeout: Duration,
    stop: Option<&AtomicBool>,
    max_output_bytes: usize,
) -> Result<RunOutput, RunError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command.spawn().map_err(RunError::Spawn)?;
    let pid = child.id();
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let (out_tx, out_rx) = mpsc::sync_channel(1);
    let (err_tx, err_rx) = mpsc::sync_channel(1);
    let out_overflow = Arc::new(AtomicBool::new(false));
    let err_overflow = Arc::new(AtomicBool::new(false));
    let out_overflow_reader = Arc::clone(&out_overflow);
    let err_overflow_reader = Arc::clone(&err_overflow);
    let out_reader = std::thread::spawn(move || {
        let _ = out_tx.send(read_bounded(stdout, max_output_bytes, &out_overflow_reader));
    });
    let err_reader = std::thread::spawn(move || {
        let _ = err_tx.send(read_bounded(stderr, max_output_bytes, &err_overflow_reader));
    });

    let deadline = Instant::now() + timeout;
    let mut status = None;
    let mut stdout = None;
    let mut stderr = None;
    let result = loop {
        if stop.is_some_and(|flag| flag.load(Ordering::Relaxed)) {
            break Err(RunError::Cancelled);
        }
        if Instant::now() >= deadline {
            break Err(RunError::Timeout);
        }
        if out_overflow.load(Ordering::Relaxed) || err_overflow.load(Ordering::Relaxed) {
            break Err(RunError::OutputLimit);
        }
        if status.is_none() {
            match child.try_wait() {
                Ok(result) => status = result,
                Err(error) => {
                    break Err(RunError::Wait(error));
                }
            }
        }
        receive(&out_rx, &mut stdout);
        receive(&err_rx, &mut stderr);
        if status.is_some() && stdout.is_some() && stderr.is_some() {
            break Ok(status.take().expect("status checked"));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    if result.is_err() {
        terminate(&mut child, pid);
    }
    // Give the OS a moment to close pipes after the process group is killed so
    // blocked reader threads see EOF and can be joined promptly.
    std::thread::sleep(Duration::from_millis(10));
    let _ = out_reader.join();
    let _ = err_reader.join();
    let status = result?;
    let (stdout, stdout_overflow) = stdout.unwrap_or_default();
    let (stderr, stderr_overflow) = stderr.unwrap_or_default();
    if stdout_overflow || stderr_overflow {
        return Err(RunError::OutputLimit);
    }
    Ok(RunOutput {
        status,
        stdout,
        stderr,
    })
}

fn receive(receiver: &Receiver<(Vec<u8>, bool)>, result: &mut Option<(Vec<u8>, bool)>) {
    if result.is_some() {
        return;
    }
    match receiver.try_recv() {
        Ok(value) => *result = Some(value),
        Err(TryRecvError::Disconnected) => *result = Some((Vec::new(), true)),
        Err(TryRecvError::Empty) => {}
    }
}

fn read_bounded(
    mut reader: impl Read,
    max_output_bytes: usize,
    overflow_flag: &AtomicBool,
) -> (Vec<u8>, bool) {
    let mut output = Vec::new();
    let mut total = 0usize;
    let mut buffer = [0u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                total = total.saturating_add(n);
                if total > max_output_bytes {
                    overflow_flag.store(true, Ordering::Relaxed);
                }
                let remaining = max_output_bytes.saturating_sub(output.len());
                output.extend_from_slice(&buffer[..n.min(remaining)]);
            }
        }
    }
    (output, total > max_output_bytes)
}

fn terminate(child: &mut Child, pid: u32) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-KILL", &format!("-{pid}")])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn timeout_covers_a_descendant_holding_pipes_open() {
        let started = Instant::now();
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "(sleep 3) & exit 0"]);
        let result = run_bounded(&mut command, Duration::from_millis(100), None);
        assert!(matches!(result, Err(RunError::Timeout)));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[cfg(unix)]
    #[test]
    fn caps_output_while_draining_both_pipes() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "yes x | head -c 2200000; yes y | head -c 2200000 >&2"]);
        assert!(matches!(
            run_bounded(&mut command, Duration::from_secs(3), None),
            Err(RunError::OutputLimit)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn stop_flag_cancels_a_running_child() {
        let stop = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let mut command = Command::new("/bin/sh");
                command.args(["-c", "sleep 3"]);
                run_bounded(&mut command, Duration::from_secs(5), Some(&stop))
            });
            std::thread::sleep(Duration::from_millis(50));
            stop.store(true, Ordering::Relaxed);
            assert!(matches!(worker.join().unwrap(), Err(RunError::Cancelled)));
        });
    }
}
