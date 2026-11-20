//! Shared event loop for the interactive TUI.
//!
//! The shipped binary and the release-built performance fixture run this
//! module, so timing data describes the real scheduler and renderer rather
//! than a copy.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyEventKind};
use opentop_core::MonitorHandle;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::dashboard;
use crate::state::App;

/// Timing sink used by the performance fixture. The shipped binary runs with
/// no recorder, so it performs only the `Option` checks.
pub struct PerfRecorder {
    start: Instant,
    file: Mutex<BufWriter<File>>,
}

impl PerfRecorder {
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            start: Instant::now(),
            file: Mutex::new(BufWriter::new(File::create(path)?)),
        })
    }

    pub fn record(&self, event: &str, fields: &[(&str, u64)]) {
        let mut file = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        let _ = write!(
            file,
            "t_us={} event={event}",
            self.start.elapsed().as_micros()
        );
        for (name, value) in fields {
            let _ = write!(file, " {name}={value}");
        }
        let _ = writeln!(file);
        let _ = file.flush();
    }
}

/// Scheduler settings for [`run`].
#[derive(Clone)]
pub struct RuntimeOptions {
    pub data_interval: Duration,
    pub input_frame_interval: Duration,
    pub max_poll: Duration,
    pub perf: Option<Arc<PerfRecorder>>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            data_interval: Duration::from_millis(250),
            input_frame_interval: Duration::from_millis(33),
            max_poll: Duration::from_millis(50),
            perf: None,
        }
    }
}

pub fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    monitor: &MonitorHandle,
    options: &RuntimeOptions,
) -> io::Result<()> {
    let now = Instant::now();
    let mut last_data_fetch = now.checked_sub(options.data_interval).unwrap_or(now);
    let mut last_draw = now.checked_sub(options.input_frame_interval).unwrap_or(now);
    let mut initial_draw = true;
    let mut force_draw = false;
    let mut draw_seq = 0u64;

    loop {
        if app.should_exit() {
            break;
        }
        app.poll_plugins();
        app.poll_process_action();

        let now = Instant::now();
        let next_data = last_data_fetch + options.data_interval;
        let next_input = if app.needs_render() || force_draw {
            last_draw + options.input_frame_interval
        } else {
            now + options.max_poll
        };
        let deadline = next_data.min(next_input).min(now + options.max_poll);
        let timeout = deadline.saturating_duration_since(now);

        if event::poll(timeout)? {
            let mut drained = 0;
            while drained < 64 {
                match event::read()? {
                    Event::Key(key)
                        if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) =>
                    {
                        let started = Instant::now();
                        let should_exit = app.handle_key(key);
                        if let Some(perf) = &options.perf {
                            perf.record("key", &[("handler_us", duration_us(started.elapsed()))]);
                        }
                        if should_exit {
                            break;
                        }
                    }
                    Event::Mouse(mouse) => app.handle_mouse(mouse),
                    Event::Resize(..) => {
                        app.invalidate();
                        force_draw = true;
                    }
                    _ => {}
                }
                drained += 1;
                if app.should_exit() || !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }

        if app.should_exit() {
            break;
        }

        let now = Instant::now();
        let data_ready = if now >= last_data_fetch + options.data_interval {
            let started = Instant::now();
            let changed = app.update_frame(monitor);
            if let Some(perf) = &options.perf {
                perf.record(
                    "frame",
                    &[
                        ("fetch_us", duration_us(started.elapsed())),
                        ("changed", u64::from(changed)),
                        ("revision", app.frame.revision),
                    ],
                );
            }
            last_data_fetch = now;
            changed
        } else {
            false
        };

        let now = Instant::now();
        let input_ready = app.needs_render()
            && (force_draw || now.duration_since(last_draw) >= options.input_frame_interval);
        if initial_draw || input_ready || data_ready {
            let theme = app.theme().clone();
            let started = Instant::now();
            let mut frame_time = Duration::ZERO;
            terminal.draw(|frame| {
                let frame_started = Instant::now();
                dashboard::draw(frame, app, &theme);
                frame_time = frame_started.elapsed();
            })?;

            let total = started.elapsed();
            let terminal_time = total.saturating_sub(frame_time);
            if let Some(perf) = &options.perf {
                perf.record(
                    "draw",
                    &[
                        ("draw", draw_seq),
                        ("lock_wait_us", 0),
                        ("frame_us", duration_us(frame_time)),
                        ("terminal_us", duration_us(terminal_time)),
                        ("total_us", duration_us(total)),
                        ("revision", app.frame.revision),
                    ],
                );
            }

            draw_seq += 1;
            last_draw = Instant::now();
            initial_draw = false;
            force_draw = false;
            app.mark_rendered();
        }
    }

    Ok(())
}

fn duration_us(duration: Duration) -> u64 {
    duration.as_micros().try_into().unwrap_or(u64::MAX)
}
