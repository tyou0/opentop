# Plugins

OpenTop discovers plugins from one-level-deep folders containing `plugin.toml`.
Discovery reads manifests only. A plugin executable runs only after its widget is
enabled in the Widgets overlay and the configuration is applied.

## Example

Python 3 is required for the repository example. From the repository root:

```bash
cargo run -p opentop-app -- --plugin-dir ./examples/plugins
```

Open Widgets with `w`, move to **Hello**, press Space, then Enter. The executable
must have its executable bit set on Unix:

```bash
chmod +x examples/plugins/hello/hello.py
```

The plugin panel renders the JSON `lines` values as plain text. It receives no
system snapshot, credentials, or environment values from OpenTop.

To verify selection, persistence, and rendering through the real TUI without
installing the example globally:

```bash
cargo build --release --offline
python3 scripts/tui_perf.py --binary target/release/opentop --scenario plugin-ui \
  --app-arg=--plugin-dir --app-arg=./examples/plugins --assert-responsive
```

## Manifest

Each plugin directory contains one `plugin.toml` file, limited to 64 KiB:

```toml
protocol_version = 1
id = "example.hello"
name = "Hello"
version = "0.1.0"
description = "Example plugin"
executable = "hello.py"
args = []
interval_ms = 5000
timeout_ms = 1000
min_height = 5
```

IDs match `[a-z0-9][a-z0-9._-]{0,63}`. Intervals are 1000-3600000 ms,
timeouts are 50-10000 ms and must be shorter than the interval, and minimum
panel height is 3-30 rows. The executable path must be relative and resolve
inside the plugin directory. Traversal and symlink escapes are rejected.

The executable runs directly, without a shell, with its plugin directory as the
working directory. It runs with the same operating-system privileges as
OpenTop; this is process isolation, not a sandbox. The child environment is
cleared except for a minimal `PATH`; no host environment variables or metric
data are passed. On Windows the current build shows an unsupported status
instead of executing plugins.

## Discovery order

The first valid or broken entry for an ID wins. Later duplicates remain visible
as diagnostics and are not executed:

1. Repeated `--plugin-dir <path>` flags, in argument order.
2. `plugin_dirs` from the config file, in listed order. Relative entries are
   relative to the config file's directory.
3. `OPENTOP_PLUGIN_PATH`, parsed as a platform path list.
4. User data plugins: `$XDG_DATA_HOME/opentop/plugins` or
   `~/.local/share/opentop/plugins` on Unix; `%LOCALAPPDATA%/opentop/plugins`
   on Windows.
5. System plugins: `/usr/local/lib/opentop/plugins` then
   `/usr/lib/opentop/plugins` on Unix; `%PROGRAMDATA%/opentop/plugins` on
   Windows.

Missing roots are ignored. Each root is scanned one level deep, with child
folders sorted for deterministic results. Relative CLI and environment paths
resolve against the startup working directory.

## Configuration and controls

The default config is `$XDG_CONFIG_HOME/opentop/config.toml` or
`~/.config/opentop/config.toml` on Unix, and `%APPDATA%/opentop/config.toml`
on Windows. Override it with `--config <path>`. A layout may include:

```toml
schema_version = 1
widgets = ["builtin:cpu_graph", "builtin:processes", "plugin:example.hello"]
plugin_dirs = ["plugins"]
```

Use `--plugin-dir ./examples/plugins` to load this repository's example without
installing it globally. New discoveries stay disabled until enabled and applied
in Widgets. `Esc` or `q` cancels the draft; Enter applies and atomically saves
it. F5 rescans manifests. Removing an enabled plugin leaves its configured
widget as a missing placeholder and stops its process.

## Runtime limits

- At most four plugin processes run concurrently; overdue samples are
  coalesced, and one plugin ID never overlaps itself.
- At most 32 plugins may be enabled.
- Each invocation has its manifest timeout; stdout and stderr are independently
  capped at 64 KiB and read concurrently.
- stdout must be one UTF-8 JSON object with `protocol_version: 1` and at most
  200 lines, each at most 512 Unicode scalar values.
- Terminal controls are stripped. Raw stderr is not shown or logged.
- Failures show a safe status category and retain the last successful output as
  stale. Rescan stops replaced or removed executables before scheduling their
  replacements.

## Troubleshooting

| Status | Likely cause | Fix |
| --- | --- | --- |
| Missing | Configured ID has no discovered manifest | Restore its folder or disable the widget |
| Invalid manifest | TOML, bounds, protocol, or executable path failed validation | Correct `plugin.toml` and press F5 |
| Duplicate plugin ID | A higher-priority root already owns the ID | Remove or rename the lower-priority copy |
| Unsupported | Plugin execution is not implemented for this platform | Run on Unix or disable the panel |
| Timed out | The executable exceeded `timeout_ms` | Reduce work or increase the manifest timeout below the interval |
| Output limit exceeded | stdout or stderr exceeded 64 KiB | Return smaller output |
| Invalid output | stdout was not one valid protocol object | Emit one UTF-8 JSON object followed by whitespace |
