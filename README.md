# opentop

A modern, cross-platform terminal system monitor built in Rust. opentop provides both an interactive TUI dashboard and a CLI for exporting system metrics as JSON.

## Features

- **Interactive TUI**: Real-time system dashboard using [ratatui](https://github.com/ratatui/ratatui)
- **CLI export**: Dump snapshots as human-readable text or JSON (with watch mode for streaming)
- **Comprehensive metrics**: CPU (total, per-core, cluster tiers), memory/swap, network I/O, disks, processes, sensors (temps/fans/power), battery, disk health (SMART), and network connections
- **Cross-platform**: Works on macOS, Linux, and Windows (with platform-specific optimizations)
- **Graceful degradation**: Missing features (battery, SMART tools, etc.) are marked as unavailable instead of crashing
- **Multi-threaded collection**: Each collector runs in its own thread with configurable sampling intervals

## Architecture

- **opentop-core**: Backend engine with data model, collectors, sampling, and history
- **opentop-widgets**: Reusable ratatui widgets for rendering metrics
- **opentop-theme**: Theme system (dark/light)
- **opentop-app**: Interactive TUI binary (`opentop`)
- **opentop-cli**: CLI harness binary (`opentop-cli`)

## Installation

### From source

```bash
git clone https://github.com/tyou/opentop.git
cd opentop
cargo build --release
```

Binaries will be available at:
- `target/release/opentop` - Interactive TUI
- `target/release/opentop-cli` - CLI tool

### Using cargo install

```bash
cargo install --path .
```

## Usage

### Interactive TUI

Launch the interactive dashboard:

```bash
opentop
```

**Key bindings:**
- `q` / `Esc` - Quit
- `↑↓` / `j k` - Navigate processes
- `s` - Cycle process sort (CPU/mem/PID/name)
- `t` / `Tab` - Toggle dark/light theme
- `PgUp/PgDn` - Scroll process list
- `Home/End` - Jump to top/bottom
- `?` - Show help
- Mouse support for selecting rows and scrolling

### CLI

Dump current snapshot:

```bash
opentop-cli dump
```

JSON output:

```bash
opentop-cli dump --json
opentop-cli dump --json --pretty
```

Watch mode (stream every N seconds):

```bash
opentop-cli dump --watch 2
opentop-cli dump --json --watch 1  # NDJSON streaming
```

Limit top processes in text output:

```bash
opentop-cli dump --top 20
```

## Dependencies

- [sysinfo](https://github.com/GuillaumeGomez/sysinfo) - System information
- [ratatui](https://github.com/ratatui/ratatui) + [crossterm](https://github.com/crossterm-rs/crossterm) - TUI
- [starship-battery](https://github.com/starship/rust-battery) - Battery monitoring
- [netstat2](https://github.com/ohadravid/netstat2) - Network connections (optional)
- [macmon](https://github.com/KYDronePilot/macmon) - macOS sensor monitoring

## Building

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Run tests
cargo test --workspace

# Run clippy
cargo clippy --workspace -- -D warnings

# Format code
cargo fmt --all
```

## License

MIT License - see [LICENSE](LICENSE) for details.
