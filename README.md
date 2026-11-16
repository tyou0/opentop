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

### npm and Bun

Install the prebuilt binaries directly from GitHub; no Rust toolchain, registry
account, or manual source checkout is needed. Git must be installed:

```bash
npm install -g git+https://github.com/tyou0/opentop.git
# or
bun install -g git+https://github.com/tyou0/opentop.git
```

Run `opentop` for the dashboard or `opentop-cli` for snapshots. The package
supports Apple Silicon and Intel macOS, and x64 and ARM64 Linux with glibc
2.35 or newer. On other systems, including Windows, build from source below.
The npm package named `opentop` is a different project.

To update, rerun the installation command. To remove this installation:

```bash
npm uninstall -g opentop-rs
# or, if installed with Bun
bun remove -g opentop-rs
```

### Install from source

Install from a source checkout with a stable Rust toolchain and a C compiler/linker.
On Windows, the Rust MSVC toolchain also requires the Visual Studio C++ build
tools. See [rustup](https://rustup.rs/) for platform-specific setup.

First, obtain a checkout of this repository:

```bash
git clone https://github.com/tyou0/opentop.git
cd opentop
```

Run the following commands from the checkout root.

### Cargo

With Rust installed through rustup or another provider, install both binaries:

```bash
cargo install --locked --path crates/opentop-app
cargo install --locked --path crates/opentop-cli
```

Cargo installs `opentop` and `opentop-cli` into `~/.cargo/bin` by default
(`%USERPROFILE%\.cargo\bin` on Windows). Add that directory to your `PATH` if
your shell cannot find the commands. A custom Cargo installation root or
`CARGO_HOME` may use a different `bin` directory.

### Homebrew

Homebrew can provide the Rust toolchain on macOS or Linux:

```bash
brew install rust
```

Then run the two [Cargo commands](#cargo) above. This repository does not
provide a Homebrew formula for opentop itself.

### mise

Use mise to install Rust and run Cargo without changing your shell's active
toolchain:

```bash
mise install rust@stable
mise exec rust@stable -- cargo install --locked --path crates/opentop-app
mise exec rust@stable -- cargo install --locked --path crates/opentop-cli
```

The binaries go into Cargo's installation directory as described above.

### Build without installing

To build the binaries in the checkout instead:

```bash
cargo build --release --locked
```

By default, the binaries are `target/release/opentop` and
`target/release/opentop-cli`. On Windows, the executable names end in `.exe`.

### Update or uninstall

After updating the checkout, rerun the Cargo or mise installation commands above
with `--force` to replace the installed binaries. To remove a Cargo installation:

```bash
cargo uninstall opentop-app
cargo uninstall opentop-cli
```

## Usage

### Interactive TUI

Launch the interactive dashboard:

```bash
opentop
```

The process table includes every accessible process. On a wide panel it also
shows command lines; zoom the panel with `f` or `z` to give it more room.
The list stays sorted as values change and keeps a selected process on the
same screen row. Press `r` to stop following it and return to the top.

Use `--config <path>` to override the config file. Repeated
`--plugin-dir <path>` flags add plugin roots. See [docs/PLUGINS.md](docs/PLUGINS.md)
for discovery, limits, and the opt-in example plugin.

## Keyboard controls

**Global controls (when not in a widget panel or overlay):**
- `Tab` / `BackTab` - Focus the next/previous enabled widget
- `↑↓` / `j k` - Navigate processes when the Processes widget is focused
- `PgUp/PgDn`, `Home/End` - Page or jump through the process list
- `s` - Cycle process sort when Processes is focused
- `r` - Stop following the selected process and return to the top
- `x` - Open process termination confirmation; `y` confirms, `n` / `Esc` cancels
- `[` / `]` - Decrease/increase the number of widget columns
- `Shift` + arrows - Move the focused widget; `Alt` + arrows resize it
- `0` - Reset column count and pane sizes; `f` / `z` zooms the focused widget
- `t` - Toggle dark/light theme
- `i` - Show hardware information; use left/right arrows to change tabs and Esc to close
- `w` - Open Widgets; Space toggles, `+`/`-` reorders, Enter applies, Esc cancels
- `?` - Show help; Esc closes the top overlay
- `q` / `Esc` - Quit when no overlay is open; Ctrl-C also quits
- Mouse clicks focus panels and select visible process rows

**Widgets overlay controls (when opened with `w`):**
- `↑` / `↓` / `j` / `k` - Move selection
- `Space` - Toggle the selected widget
- `+` / `-` - Move enabled widget up/down in enabled order
- `r` - Restore default built-in layout in draft
- `Enter` - Apply draft, save configuration, close
- `F5` - Rescan plugin manifests without executing them
- `Esc` / `q` - Cancel draft and close

**Plugin panels (when enabled and focused):**
- Shows cached plugin output lines; no input handling needed
- Status categories: waiting, ready, stale, missing, error categories

**Help overlay (when opened with `?`):**
- `Esc`, `q`, or `?` - Close help

**Hardware information (when opened with `i`):**
- `←` / `→` - Switch CPU, Memory, GPU, Storage, and System tabs
- `↑` / `↓` - Scroll longer details
- `Esc` / `q` - Close the popup

## Configuration file

OpenTop uses a TOML configuration file located at:
- `$XDG_CONFIG_HOME/opentop/config.toml` or `~/.config/opentop/config.toml` on Unix
- `%APPDATA%/opentop/config.toml` on Windows

Override with `--config <path>`:

```bash
opentop --config /path/to/my/config.toml
```

### Config format

```toml
schema_version = 1
widgets = ["builtin:cpu_graph", "builtin:memory", "builtin:processes"]
plugin_dirs = []
```

**`schema_version`**: Currently must be `1`. Unknown versions show readable errors and use defaults.

**`widgets`**: Ordered list of enabled widgets. `[]` shows an empty dashboard with a prompt to press `w`. Widgets fill the available dashboard area; when there is not enough space for every row, focusing a widget scrolls it into view. Column count, pane sizes and widget order can be changed with the keyboard and are saved in the config file.

Panels are arranged in rows. Horizontal resizing shares space with a neighboring panel; vertical resizing changes the height of the whole row.

The process action sends SIGTERM on Unix and force-terminates on Windows. The app checks the selected process identity again before acting and reports permission or exit errors. On Linux, it requires kernel `pidfd` support and fails safely if that is unavailable. A successful signal does not guarantee that the process exits immediately.

**`plugin_dirs`**: Additional directories to search for plugins alongside the standard search paths. Relative paths resolve against the config file's directory.

### Configuration persistence

Changes applied in the Widgets overlay are saved atomically using a temporary file sibling and rename. Failed saves keep the in-memory layout and show a readable error. Existing user files survive failed writes.

Configuration supports:
- Default values for missing fields
- Duplicate widget IDs (first occurrence wins)
- Unknown widget IDs (preserved as missing placeholders)
- Plugin directory scanning limits and precedence rules

## Plugin system

OpenTop discovers executable plugins from one-level-deep folders containing `plugin.toml`.

### Example plugin setup

```bash
# From the repository root:
cargo run -p opentop-app -- --plugin-dir ./examples/plugins
```

Open Widgets with `w`, move to **Hello**, press Space, then Enter. The executable
must have its executable bit set on Unix:

```bash
chmod +x examples/plugins/hello/hello.py
```

### Plugin execution limits

- Maximum 4 plugin processes run concurrently
- Maximum 32 plugins may be enabled
- Each invocation has its manifest timeout; stdout and stderr are independently
capped at 64 KiB and read concurrently.
- stdout must be one UTF-8 JSON object with `protocol_version: 1` and at most
200 lines, each at most 512 Unicode scalar values.
- Terminal controls are stripped; raw stderr is not shown or logged.
- Failures show safe status categories and retain last successful output as stale.

### Performance considerations

For PTY performance measurements, use:

```bash
python3 scripts/tui_perf.py \
  --binary target/release/opentop \
  --scenario plugin-ui \
  --app-arg=--plugin-dir \
  --app-arg=./examples/plugins \
  --assert-responsive
```

Platform support:
- Built and tested on macOS arm64 (aarch64-apple-darwin)
- The npm package build checks global installs on macOS and Linux for arm64 and x64
- Windows requires further testing
- Real SSH comparison pending fleet authorization

## CLI

Dump current snapshot:

```bash
opentop-cli dump
```

JSON output:

```bash
opentop-cli dump --json
opentop-cli dump --json --pretty
opentop-cli dump --json --top 25
```

JSON snapshots include process command-line arguments, which can contain
sensitive information. Review snapshots before sharing them.

Watch mode (stream every N seconds):

```bash
opentop-cli dump --watch 2
opentop-cli dump --json --watch 1  # NDJSON streaming
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

To update the binaries used by Git-based npm and Bun installs, keep the versions
in `Cargo.toml` and `package.json` in sync, then run the **Build npm package**
GitHub Actions workflow. It builds and smoke-tests each native target and
produces an `opentop-rs-npm` artifact. Extract its `npm/native` directory and
commit the updated binaries to the repository.

## License

MIT License - see [LICENSE](LICENSE) for details.
