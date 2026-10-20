# Opentop performance baseline (Task 01)

Measured 2026-10-20 on the local development machine. This report establishes a
reproducible failure before any sampling or scheduling fix. The new assertions
are expected to stay red until Tasks 02-05 land; existing tests pass.

## Environment and method

| Item | Value |
| --- | --- |
| Host OS | macOS 26.0.1 (build 25A362), arm64 |
| Rust | rustc 1.99.1, edition 2024, release profile |
| Python | 3.15.0 (stdlib `pty`/`select` only) |
| Terminal | PTY, `TERM=xterm-256color`, 80x24 and 160x48 |
| Binary under test | `target/release/examples/perf_fixture` (synthetic data, same runtime/dashboard as `target/release/opentop`) |
| Measurement | Python PTY driver drains output, answers `ESC[6n`, injects timestamped keys, reads runtime counters from a JSON-free key/value log |

The fixture supplies a deterministic snapshot (host `perf-fixture`, synthetic
CPU/memory/network/disks/processes). Its collector can block inside `collect`
on a file gate. Because the monitor takes the shared write lock before calling
`collect`, a gated sample exercises the real publication path rather than a
mocked monitor. The shipped binary has no fixture switches; only the example
accepts `--scenario`, `--control-dir`, and `--perf-log`.

Latency is measured from the key write (`os.write` to the PTY) to the first
output that reflects the change: `opentop help` for the slow-collector
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

Result: FAIL. Process navigation currently produces no visible change because
`dashboard::render` creates a fresh `ProcessTableState` for every frame, so
`App` selection/scroll do not reach the table. The latency check cannot pass
until Task 05 connects navigation to rendering; the unavailable p95 is not a
pass. The bytes/s figure is the idle reset/hide-cursor tail, and the large
terminal write time confirms that throttled output is a separate bottleneck
from lock contention.

### Startup with a blocked first sample

```bash
python3 scripts/tui_perf.py \
  --binary target/release/examples/perf_fixture \
  --scenario startup --assert-responsive
```

The fixture calls `monitor.wait_ready(3 s)` exactly like the shipped binary,
then the harness releases the first-sample gate after 3.3 s.

| Metric | Observed | Budget |
| --- | --- | --- |
| First UI | 3459.44 ms | <= 200 ms |
| Quit after first UI | 82.37 ms | <= 200 ms |
| `collect` hold / gate wait | 3322.27 ms / 3322.07 ms | - |

Result: FAIL on first UI. Quit passes once the UI exists. Task 03 removes the
interactive `wait_ready`; rerun this scenario then.

### Real binary smoke (not an acceptance measurement)

```bash
python3 scripts/tui_perf.py --binary target/release/opentop --scenario smoke --duration 2
```

First output was 1714.21 ms in the saved run and 3189.02 ms in an earlier run;
quit was not observed within 2 s in the saved run (474.20 ms earlier). The
shipped binary has no perf recorder, so draw counters are unavailable. The
spread is consistent with real macOS sensor sampling holding the write lock;
this is a local hardware observation, not the controlled-fixture acceptance
result. No SSH host was used.

## Where the time goes

- **Sampling:** the fixture's `collect_us` includes the gate wait. In the
  slow-collector run, 2,064.56 ms of 2,064.80 ms was the gate.
- **Publication lock:** the monitor holds the write lock for the whole
  `collect`; the UI reader waited 2,060.13 ms before it could render.
- **Rendering:** frame construction stayed at 1-8 ms and is not the latency
  source in these runs.
- **Terminal output:** 1-3 ms unthrottled at both sizes; 168-413 ms for the
  initial screen when the reader is throttled to 32 KiB/s, which is a separate
  transport cost.
- **Idle churn:** 48 draws per 5 s (one per 100 ms tick) with 25 bytes each,
  even with frozen data.

## Core regression pin

```bash
cargo test -p opentop-core --offline --test monitor_stall
```

`gated_collector_stalls_publication_and_readers` drives the real
`MonitorHandle` spawn/publication path with a gated collector and a fast
collector. It asserts the current stall (fast samples and readers do not
progress while the gate is held) and that publication resumes after release.
Task 02 must invert those assertions.

## Reproduce and inspect

The script prints every counter and the perf-log path. `--perf-log <path>`
keeps the raw timing log; without it the temporary directory is removed. Log
lines contain only timings and counters:

```text
t_us=... event=draw draw=... lock_wait_us=... frame_us=... terminal_us=... total_us=...
t_us=... event=collect sample=... collect_us=... gate_wait_us=...
t_us=... event=key handler_us=...
```

No snapshots, process command lines, environment values, or terminal captures
are logged. All fixture names are synthetic.

## Limits

- Local PTY latency is not an SSH measurement. A real SSH comparison is still
  pending and must follow the fleet rules before any server access.
- The slow-collector baseline uses one gated sample per process run; p95 and
  max are therefore the same value. Later acceptance runs should repeat the
  scenario to build a distribution.
- Numbers are single saved runs. First-output and startup timings vary by
  hundreds of milliseconds with host load, while the gated key latency stays
  near the gate duration. Re-run before comparing a fix.
- The throttled process-navigation check is blocked on Task 05, not on
  transport, because navigation does not reach the rendered table yet.
- `cargo fmt --all -- --check` reports pre-existing formatting differences in
  files untouched by this task; the new `runtime.rs`, `perf_fixture.rs`, and
  `monitor_stall.rs` are rustfmt-clean. Pre-existing Clippy warnings remain in
  `plugins.rs`, `input.rs`, and the CLI.
