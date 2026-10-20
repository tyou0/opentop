//! opentop interactive TUI.
//!
//! A ratatui frontend that composes every widget from `opentop-widgets`
//! into a single dashboard, driven by a `crossterm` event loop with full
//! keyboard and mouse support. The sampling engine runs in the background;
//! the UI thread only reads the latest snapshot/history on each tick.

use std::io;

use opentop_app::runtime::{self, RuntimeOptions};
use opentop_app::state::App;
use opentop_app::terminal::{TerminalSession, install_panic_hook};
use opentop_core::{MonitorConfig, MonitorHandle};

fn main() -> io::Result<()> {
    install_panic_hook();
    let mut session = TerminalSession::new()?;
    let mut app = App::new();
    let monitor = MonitorHandle::start(MonitorConfig::default());

    let result = runtime::run(
        session.terminal_mut(),
        &mut app,
        &monitor,
        &RuntimeOptions::default(),
    );

    drop(session);
    drop(monitor);
    result
}
