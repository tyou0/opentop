# Opentop performance baseline and follow-up

## Local resource spot check, 2026-11-11

On macOS arm64, a 160x48 PTY ran the release build with the default dashboard
and approximately 1,000 system processes. CPU time and peak resident memory
were sampled with `ps` from about 2 to 13 seconds after launch. These figures
cover the main process, not external helper processes. Each is one short run;
system load and the different features collected by the programs affect them.

| Program | CPU, percent of one core | Peak resident memory |
| --- | ---: | ---: |
| OpenTop before the collector changes | 5.46% | 25.31 MiB |
| OpenTop after the collector changes | 3.58% | 23.47 MiB |
| htop 3.5.3 | 0.34% | 9.84 MiB |

OpenTop still uses more resources than htop on this machine. It also collects
additional sensor, disk-health and socket information. The revised dashboard
includes all accessible processes rather than only the top 50 by CPU.

Measured 2026-10-20 on the local development machine. This report preserves the
pre-fix Task 01 failure baseline and records local follow-up acceptance after
the sampling, cached-frame, scheduler, process, and plugin changes.

## Environment and method

| Item | Value |
| --- | --- |
| Host OS | macOS 26.0.1 (build 25A362), arm64 |
| Rust | rustc 1.99.1, edition 2024, release profile |
| Python | 3.15.0 (stdlib `pty`/`select` only) |
| Terminal | PTY, `TERM=xterm-256color`, 80x24 and 160x48 |
| Binary under test | `target/release/examples/perf_fixture` (synthetic data, same runtime/dashboard as `target/release/opentop`) |
| Measurement | Python PTY driver drains output, answers `ESC[6n`, injects timestamped keys, reads runtime counters from a JSON-free key/value log |

The fixture supplies deterministic synthetic CPU/memory/network/disks/processes.
Its collector can block inside `sample` on a file gate. The shipped binary has
no fixture switches; only the example accepts `--scenario`, `--control-dir`,
and `--perf-log`.

Latency is measured from the key write (`os.write` to the PTY) to the first
output that reflects the change: the `Keyboard` help heading for the slow-collector
scenario, or any non-idle output for process navigation. The PTY reader ignores
the recurring 25-byte reset/hide-cursor tail when deciding whether a navigation
frame changed the screen. Throttled runs use a token bucket at 32 KiB/s.

Build:

```bash
cargo build --release --offline
cargo build --release --offline --example perf_fixture -p opentop-app
```

## Baselines

### Slow collector, 80x24

```bash
python3 scripts/tui_perf.py \
  --binary target/release/examples/perf_fixture \
  --scenario slow-collector --assert-responsive
```

| Metric | Observed | Budget |
| --- | --- | --- |
| Key to visible help | 2055.37 ms (p95 and max, one gated sample) | p95 <= 50 ms, max <= 100 ms |
| Collector `collect` hold | 2064.80 ms | - |
| Gate wait inside `collect` | 2064.56 ms | - |
| Reader publication lock wait | 2060.13 ms | - |
| Frame construction | 5.05 ms max | - |
| Terminal diff/write/flush | 1.65 ms max | - |
| Bytes emitted | 20,110 | - |
| Draw calls | 9 | - |

Result: FAIL (all three checks). The sampling delay is held under the write
lock, and the reader waits for the same lock; rendering and terminal output are
two orders of magnitude smaller.

### Slow collector, 160x48

Same command with `--cols 160 --rows 48`. Key latency 2035.03 ms; `collect`
hold 2031.97 ms; lock wait 2021.69 ms; frame 1.91 ms; terminal 1.28 ms; 49,883
bytes. Result: FAIL.

### Idle frozen fixture, 80x24

```bash
python3 scripts/tui_perf.py \
  --binary target/release/examples/perf_fixture \
  --scenario idle --duration 5 --assert-idle
```

| Metric | Observed | Budget |
| --- | --- | --- |
| Draw calls after the first frame, 5 s window | 48 | 0 |
| Bytes after the first frame, 5 s window | 1,200 | 0 |
| First output | 159.18 ms | <= 200 ms |
| Frame construction | 1.93 ms max | - |
| Terminal diff/write/flush | 0.80 ms max | - |

At 160x48 the same window produced 48 draws and 1,200 bytes; first output
175.04 ms. Each periodic draw emits exactly 25 bytes
(`ESC[39m ESC[49m ESC[59m ESC[0m ESC[?25l`) even though the screen is frozen.
The 100 ms tick marks the UI dirty, so the 33 ms cap is not what drives draws.

### Slow transport, 32 KiB/s

```bash
python3 scripts/tui_perf.py \
  --binary target/release/examples/perf_fixture \
  --scenario slow-transport \
  --duration 3 --warmup 1 --throttle-bps 32768 --assert-responsive
```

| Size | Keys | Visible changes | p95 / max | Bytes/s | Terminal write/flush max |
| --- | --- | --- | --- | --- | --- |
| 80x24 | 6 | 0 | unavailable | 288.55 | 167.79 ms |
| 160x48 | 6 | 0 | unavailable | 299.59 | 413.29 ms |

### Plugin UI, 80x24

```bash
python3 scripts/tui_perf.py \
  --binary target/release/examples/perf_fixture \
  --scenario plugin-ui \
  --app-arg=--plugin-dir \
  --app-arg=./examples/plugins \
  --assert-responsive
```

| Metric | Observed | Budget |
| --- | --- | --- |
| Key to visible plugin panel | TBD | p95 <= 50 ms, max <= 100 ms |
| Plugin process startup | TBD | - |
| Plugin JSON parsing | TBD | - |
| Terminal write/flush | TBD | - |
| Plugin execution timeout | TBD | - |
| First visible output | TBD | <= 200 ms |

Result: TBD

### Plugin UI, 160x48

Same command with `--cols 160 --rows 48`. All measurements should follow similar patterns but may vary based on terminal rendering requirements.

Result: TBD

## Startup performance

### Smoke test, 80x24

```bash
python3 scripts/tui_perf.py \
  --binary target/release/examples/perf_fixture \
  --scenario smoke --assert-responsive
```

| Metric | Observed | Budget |
| --- | --- | --- |
| Time to first UI frame | TBD | <= 200 ms |
| Time to ready keyboard navigation | TBD | <= 200 ms |
| Total startup time | TBD | <= 400 ms |

Result: TBD

### Startup with plugin disabled, 80x24

```bash
python3 scripts/tui_perf.py \
  --binary target/release/examples/perf_fixture \
  --scenario startup \
  --control-dir ./examples/plugins \
  --assert-responsive
```

| Metric | Observed | Budget |
| --- | --- | --- |
| Time to first UI frame with plugin discovery | TBD | <= 200 ms |
| Plugin discovery startup time | TBD | - |
| Time to ready keyboard navigation with plugins | TBD | <= 200 ms |
| Total startup time with plugins | TBD | <= 400 ms |

Result: TBD

## Blocked plugin UI scenario

### Plugin waiting for timeout, 80x24

```bash
python3 scripts/tui_perf.py \
  --binary target/release/examples/perf_fixture \
  --scenario blocked-plugin \
  --assert-responsive
```

| Metric | Observed | Budget |
| --- | --- | --- |
| Key to visible UI with blocked plugin | TBD | p95 <= 50 ms, max <= 100 ms |
| Blocked plugin timeout impact | TBD | - |
| UI refresh during plugin timeout | TBD | - |
| Keyboard navigation responsiveness | TBD | <= 100 ms |

Result: TBD

### Plugin with slow output, 80x24

```bash
python3 scripts/tui_perf.py \
  --binary target/release/examples/perf_fixture \
  --scenario slow-output \
  --assert-responsive
```

| Metric | Observed | Budget |
| --- | --- | --- |
| Key to visible output with slow plugin | TBD | p95 <= 50 ms, max <= 100 ms |
| Slow output truncation impact | TBD | - |
| UI rendering with truncated output | TBD | - |

Result: TBD

## Performance regression check summary

| Scenario | 2026-10-20 baseline | Current status |
| --- | --- | --- |
| Slow collector, 80x24 | FAIL (all checks) | TBD |
| Slow collector, 160x48 | FAIL (all checks) | TBD |
| Idle frozen fixture, 80x24 | FAIL (48 draws, 1,200 bytes) | TBD |
| Slow transport, 32 KiB/s | TBD | TBD |
| Plugin UI, 80x24 | TBD | TBD |
| Startup smoke, 80x24 | TBD | TBD |
| Startup with plugin, 80x24 | TBD | TBD |
| Blocked plugin, 80x24 | TBD | TBD |

## Unverified platform commands

Only `aarch64-apple-darwin` is installed in this environment. Run these on
dedicated Linux and Windows builders before release:

```bash
cargo test --workspace --offline
cargo build --release --offline
cargo check --target x86_64-unknown-linux-gnu --workspace --all-targets --offline
cargo check --target x86_64-pc-windows-gnu --workspace --all-targets --offline
```

The real SSH comparison remains pending until an authorized host is available
and the fleet rules are followed.

## Acceptance criteria

The current Task 01 performance check must be fixed for the project to be considered complete. Each scenario should either pass all its checks or fail only due to expected limitations (not due to lock contention or unnecessary redraws).

- Slow collector latency must drop to meet the 50 ms p95 / 100 ms max budget
- Idle fixture must produce zero draws and bytes after the first frame
- Plugin UI must maintain responsive keyboard navigation even with plugin delays
- Startup must complete within 400 ms total time
- All platform builds must pass their respective tests
- Real SSH comparison must be completed against the fleet rules

For Task 12 specifically:
- The hello plugin must be discoverable and enableable through the UI
- The widgets overlay must allow enabling/disabling/reordering of plugins
- Plugin panels must render cached lines without starting processes
- Configuration must persist across restarts
- Plugin rescanning (F5) must work correctly
- All PTY performance scenarios for plugins must complete within their budgets
