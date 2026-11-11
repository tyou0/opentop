//! opentop interactive TUI.
//!
//! A ratatui frontend that composes every widget from `opentop-widgets`
//! into a single dashboard, driven by a `crossterm` event loop with full
//! keyboard and mouse support. The sampling engine runs in the background;
//! the UI thread only reads the latest snapshot/history on each tick.

use std::io;
use std::path::PathBuf;

use opentop_app::config::resolve_config_path;
use opentop_app::runtime::{self, RuntimeOptions};
use opentop_app::state::App;
use opentop_app::terminal::{TerminalSession, install_panic_hook};
use opentop_core::{MonitorConfig, MonitorHandle};

fn main() -> io::Result<()> {
    let (config_override, plugin_dirs) = parse_args()?;
    install_panic_hook();
    let mut session = TerminalSession::new()?;
    let config_path = resolve_config_path(config_override.as_deref());
    let mut app = App::load(config_path);
    app.cli_plugin_dirs = plugin_dirs;
    app.rescan_plugins();
    let monitor = MonitorHandle::start(MonitorConfig {
        top_processes: usize::MAX,
        ..MonitorConfig::default()
    });

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

fn parse_args() -> io::Result<(Option<PathBuf>, Vec<PathBuf>)> {
    parse_options(std::env::args().skip(1))
}

fn parse_options(
    args: impl Iterator<Item = String>,
) -> io::Result<(Option<PathBuf>, Vec<PathBuf>)> {
    let mut args = args;
    let mut config = None;
    let mut plugin_dirs = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => {
                config = Some(PathBuf::from(args.next().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "--config needs a path")
                })?));
            }
            "--plugin-dir" => plugin_dirs.push(PathBuf::from(args.next().ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidInput, "--plugin-dir needs a path")
            })?)),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "unknown argument",
                ));
            }
        }
    }
    Ok((config, plugin_dirs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_accepts_config_override_and_repeated_plugin_dirs() {
        let args = [
            "--config",
            "user.toml",
            "--plugin-dir",
            "one",
            "--plugin-dir",
            "two",
        ]
        .into_iter()
        .map(str::to_owned);
        let (config, plugins) = parse_options(args).unwrap();
        assert_eq!(config, Some(PathBuf::from("user.toml")));
        assert_eq!(plugins, [PathBuf::from("one"), PathBuf::from("two")]);
    }
}
