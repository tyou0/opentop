# Opentop SSH performance and extensibility implementation plan

Make keyboard interaction responsive while system metrics update in the background. Add a keyboard driven widget picker and discover executable plugins in known folders. Complete the numbered tasks in order, one task per coding session. Each task defines its scope and the evidence required to finish it.

This is an implementation handoff, not a performance diagnosis. The observations below come from the current source. Task 01 must establish a reproducible performance failure before the executor selects a cause or claims an improvement.

## Current code and constraints

| Observation | Code to inspect | Consequence to verify |
| --- | --- | --- |
| `spawn` takes the shared write lock before `collector.collect` | `crates/opentop-core/src/monitor.rs` | A slow collector can delay every reader and other collectors. |
| CPU/process priming sleeps for 300 ms; network/disk priming sleeps for 400 ms inside `collect` | `crates/opentop-core/src/collectors/{cpu_memory,processes,network,disks}.rs` | Startup sampling can block rendering. |
| Sensors call platform sampling, including external commands and macmon sampling | `crates/opentop-core/src/collectors/sensors.rs`, `platform/{macos,nvidia,lm_sensors}.rs` | Collection can take much longer than a frame. |
| `dashboard::render` renders inside `monitor.read` | `crates/opentop-app/src/dashboard.rs` | Rendering holds the shared read lock; fetching data can block the UI. |
| Each 100 ms tick marks the UI dirty; elapsed 33 ms also independently permits drawing | `crates/opentop-app/src/{main,state}.rs` | Redundant render work; the 33 ms condition is not a reliable frame cap. Measure terminal bytes separately: Ratatui already diffs buffers. |
| Startup waits up to three seconds before entering the event loop | `crates/opentop-app/src/main.rs` | The initial UI cannot accept keys during this wait. |
| Rendering creates a fresh `ProcessTableState` | `crates/opentop-app/src/dashboard.rs` | Selection and scrolling in `App` do not drive the displayed table. |
| Layout is fixed; focus includes header/footer and has no visible focus treatment | `crates/opentop-app/src/{dashboard,state,input}.rs` | Widget selection requires a shared catalog and layout rectangles. |
| `PluginRegistry` only stores name/version/description/enabled | `crates/opentop-core/src/plugins.rs` | Discovery, execution, output handling, and rendering all need implementation. |

The initial `cargo test --workspace --offline` run passed on 2026-10-20, with existing compiler warnings. The repository currently has no commits and its source files are untracked. Do not clean, reset, or overwrite this working tree. Do not run broad automatic fixes or commit unless asked.

## Fixed product decisions

- Keep Ratatui and the existing collector threads. Use owned collector results and short publication locks. Do not add an async runtime merely for this work.
- Separate data sampling cadence from keyboard responsiveness. CPU/network remain at one second and processes at two seconds by default. A key press must not wait for the next sample.
- Draw on a real UI change or a coalesced data update. Cap data driven refreshes at 10 per second and input driven frames at 30 per second, retaining pending changes. Idle synthetic data must not cause redraws.
- Offer one Widgets overlay opened with `w`. It selects which panels are visible and their order; it also includes discovered plugins. `Tab`/`BackTab` moves focus among visible dashboard panels. `t` changes theme. `?` opens help.
- Keep header/footer fixed. Start with all 14 currently rendered content panels enabled, in their current order. Header/footer are not selectable panels. Additional existing widget adapters such as `MemSparkline` remain outside this first catalog.
- Use executable plugins, launched directly with arguments and returning JSON. Plugin code runs as the current user in a separate process; it is not sandboxed. Discovery reads manifests only. Newly discovered plugins start disabled; enabling one in Widgets loads it.
- Version one plugins render text lines in a standard panel. Arbitrary terminal output, dynamic Rust libraries, scripting engines, and custom plugin drawing are outside this scope.
- Hiding a built in widget stops rendering it. Built in collectors keep their existing cadences in this iteration. Hiding a plugin panel disables its execution.

## Performance acceptance criteria

Use release builds and report terminal dimensions, host OS, workload, and measurement method. These are proposed budgets to validate, not current measurements.

| Scenario | Required result |
| --- | --- |
| First UI and ability to quit | Within 200 ms on the controlled fixture, without waiting for collectors; real hardware startup reported separately. |
| Key delivery to a frame reflecting the change | Local PTY p95 at most 50 ms and maximum at most 100 ms during a deliberately blocked collector. Measure from injected key to the corresponding terminal output, not just handler execution. |
| Frozen fixture, no input, fixed size | After initial frame, zero further `draw` calls over five virtual seconds and no recurring terminal output. |
| Frozen fixture with process navigation | Only relevant cells change; no periodic whole screen clear, no stale queued frames. |
| Sample with a collector blocked for two seconds | Existing data remains navigable; the slow sample neither blocks input nor overwrites unrelated sections. |
| Normal data, no input | At most 10 data driven draws per second; individual collector cadences remain unchanged. |
| Slow terminal transport | Compare the same 80x24 and 160x48 fixtures before/after at 32 KiB/s; for process navigation at five keys/second, p95 key to visible change at most 200 ms after warmup. Measure theme/resize redraws separately because they change much more of the screen. Record bytes/sec, p95, maximum, and CPU. If it fails, locate the blocked operation and reduce output before declaring completion. |

An actual SSH connection adds network delay. Report a separate real SSH smoke result against `top`/`htop` using the same host, terminal, and link; do not subtract an assumed RTT or claim equivalence from local measurements alone. Read the fleet rules in `~/Documents/Obsidian/tyou/11-DevOps/CLAUDE.md` before any server access. Follow their approval requirements for remote changes. No server changes are part of this plan.

## Task 01 Build the performance feedback loop

**Change:** Add `scripts/tui_perf.py`, deterministic metric fixtures, and the smallest internal injection seam needed to drive the actual scheduler and renderer without real OS collection. Put the loop in a library module such as `crates/opentop-app/src/runtime.rs` so tests exercise the same code as the binary. Keep fixture controls out of the normal product interface.

**Steps:**

1. Build the release binary and establish the existing test baseline. Use offline mode when dependencies are cached.
2. Add a PTY driver that sets size and TERM, drains output continuously, injects timestamped keys, and recognizes output reflecting a visible theme/help/navigation change. A successful write to stdin is not a rendered response.
3. Add a fake collector that signals sampling has started and waits on a test controlled gate. Inject a key only after that signal. Always release the gate in cleanup.
4. Record draw calls, collector sampling time, publication lock wait/hold time, frame construction time, terminal write/flush time, input latency, and bytes emitted. Logs contain timing/counters only, never raw snapshots, process commands, environment values, or terminal captures containing real host data.
5. Add an output throttle in the PTY reader and record the slow transport baseline. Use synthetic host/process names.
6. Save results in `docs/PERFORMANCE.md`, including the exact commands. Expose a reproducible command such as `python3 scripts/tui_perf.py --binary target/release/opentop --scenario slow-collector --assert-responsive`. Pin that command's final syntax in the script help and document it.

**Done when:** One unattended command drives the actual UI path and fails on the observed slow behavior, without hanging. The report distinguishes sampling, lock contention, rendering, and terminal output. This task may leave the new performance assertion red; existing tests must still pass. Do not change sampling or refresh behavior yet. Do not use `history_probe.rs` as the harness: it nests monitor read locks. If the PTY scenarios need a fixture executable, add a release-built Cargo example using the same runtime and renderer, document its build command, and pass that executable to `--binary`; do not add hidden fixture switches to the shipped binary. Keep a separate core test exercising the actual monitor spawn/publication path with the gated collector, so a fake data source cannot conceal a real monitor stall.

## Task 02 Separate sampling from publication

**Change:** `opentop-core/src/collectors/*.rs`, `monitor.rs`, and `smart.rs` as needed.

**Design:** Replace the mutating collector operation with `sample(&mut self, interval: Duration) -> Option<CollectorUpdate>`. Add a typed `CollectorUpdate` enum for CPU/memory, network, disks, processes, sensors, system, battery, SMART, and connections. CPU/memory are one update because one sampler supplies both. `None` means an internal worker has no new result.

**Steps:**

1. Each collector builds only its owned result, entirely outside shared state. Keep platform sampler construction inside its worker thread because macmon handles need not be `Send`.
2. The monitor samples first, then briefly locks shared state to apply the relevant update, append its history samples, update timestamp, and increment a monotonic revision.
3. History mutation moves into `CollectorUpdate::apply` or an equivalent private function. An update changes only its owned sections. Never copy an entire stale snapshot back into shared state.
4. Preserve rate calculations and priming intervals outside the lock. SMART/connections retain their existing worker arrangement initially; publish only on a newly received result, rather than repeatedly adding unchanged worker output.
5. Update collector comments and all callers, including the CLI. Retain public `MonitorHandle::snapshot/history/read` behavior for existing callers.

**Verify:** Block a collector using a gate and prove other collectors can publish and readers can obtain their data before the gate opens. Apply interleaved updates and verify unrelated fields and history survive. Verify each history sample is appended once per produced sample. The Task 01 contention check must improve; final input budgets also depend on Tasks 03–04.

## Task 03 Give the UI a cached nonblocking frame

**Change:** `monitor.rs`, app `runtime.rs`, `state.rs`, `dashboard.rs`.

**Steps:**

1. Add an owned `MonitorFrame { revision, snapshot, history }` and `try_frame_since(revision) -> Option<MonitorFrame>`. Use `try_read`: return `None` when unchanged or busy. Clone a consistent snapshot/history pair only for a new revision. Keep frames bounded by existing history/process limits.
2. Cache the last frame in the app. Fetch once on a data deadline; render from that owned frame after dropping all monitor locks. Never call `monitor.read` inside drawing.
3. Display empty/collecting data immediately. Remove `wait_ready` from interactive startup; leave it available to the CLI and other callers.
4. A missed fetch keeps the current frame and retries on a later deadline. Do not spin, clear the screen, or blank old metrics.
5. Document that revision is publication metadata, not a wall clock timestamp. Sampling clocks and graph histories stay independent from redraws.

**Verify:** A deliberately held write lock does not stall input or rendering from the cached frame. Busy reads preserve the previous display. Rendering never holds a monitor lock. No UI action clones data unless a new monitor revision is obtained.

## Task 04 Fix scheduling and terminal lifetime

**Change:** app `main.rs`, `runtime.rs`, `state.rs`; add `terminal.rs` if useful.

**Steps:**

1. Remove unconditional 33 ms redraws and the tick that always sets dirty. Track UI dirtiness, data revision, resize, and pending frame deadlines explicitly.
2. Render immediately when permitted after input. Limit successive input frames to one per 33 ms; retain pending changes. Coalesce data to at most one frame per 100 ms. Do not gate input on the metric refresh deadline.
3. Poll with the nearest deadline, capped at 50 ms for noticing published data. Drain a bounded batch of events, then allow deadlines to run so key repeats cannot starve sampling or drawing. Accept key press/repeat, ignore release.
4. Keep one terminal instance and Ratatui's buffer diffing. Buffer backend writes and flush once per frame. Never clear on a normal tick or queue historical frames for output. Resize may require backend clearing.
5. Introduce a terminal guard for raw mode, alternate screen, hidden cursor, optional mouse capture, and restoration on normal exit/error/panic. Install a panic hook that restores the terminal before delegating. Normal quit must restore the terminal before joining workers.
6. Check Ctrl-C through modifiers, handle `BackTab` explicitly, and route Esc to close the active overlay before quitting. Remove bare `c` as an undocumented quit key. Ensure setup failures undo completed terminal setup.

**Verify:** Run all Task 01 latency/idle scenarios, rapid key repeat, resize, and quit while a collector is blocked. A fixed fixture causes no idle draws; input still changes the screen promptly. PTY tests confirm raw mode/cursor/alternate screen restoration on normal exit and injected failure. Also measure process exit time: restoring the display while leaving shutdown hung on a child is not a passing quit result. Bound existing `smartctl`, `sensors`, and `nvidia-smi` invocations with cancellation/deadlines using a reusable subprocess helper in core, and test a hung replacement executable. Reuse that helper where practical in Task 10. Document any platform sampler that cannot be canceled rather than claiming bounded shutdown for it. Do not promise that terminal writes can never block under an indefinitely stalled transport.

## Task 05 Connect process navigation to rendering

**Change:** app `state.rs`, `input.rs`, `dashboard.rs`; widgets `processes.rs` if needed.

**Steps:**

1. Use `App.process_state` in `render_stateful_widget`. Remove the redundant `process_selected` and `process_scroll` fields after migrating their callers.
2. Route j/k, arrows, Home/End, and PageUp/PageDown through this one state. Page size comes from the actual table's visible row count; PageUp and PageDown are symmetric.
3. Clamp safely for empty/shrinking process lists and End. Use saturating arithmetic; never increment an unchecked `usize::MAX` sentinel.
4. Add PID tie breakers to every sort for stable ordering. Store selected PID when data changes so a re-sort preserves selection when that PID remains; otherwise clamp the previous index. Page/navigation uses the same sorted order as rendering.

**Verify:** Input plus rendering tests with named fixture PIDs prove the highlighted row moves and scrolling follows. Cover empty data, fewer rows after refresh, sorting, End followed by Down, and the selected process disappearing. Measure rendering costs before adding caches; do not optimize unrelated widgets speculatively.

## Task 06 Add the widget catalog and layout configuration

**Change:** new app `widgets.rs`, `config.rs`; app manifest dependencies; `state.rs`.

**Design:** A stable `WidgetId` is `builtin:<name>` or `plugin:<manifest-id>`. A descriptor supplies ID, label, description, source, availability/status, and minimum height. Built in rendering remains an explicit match using existing widget implementations; reuse the dashboard context where useful without rebuilding the widget crate.

**Configuration contract:**

```toml
schema_version = 1
widgets = ["builtin:cpu_graph", "builtin:memory", "builtin:processes"]
plugin_dirs = []
```

Use these built in names in default order: `cpu_graph`, `cpu_cores`, `memory`, `swap`, `sensors`, `power`, `cpu_tiers`, `battery`, `network_graph`, `disk_usage`, `disk_health`, `processes`, `connections`, `network_table`.

**Steps:**

1. UNIX config path: `$XDG_CONFIG_HOME/opentop/config.toml`, falling back to `~/.config/opentop/config.toml`, including macOS. Windows: `%APPDATA%/opentop/config.toml`. Add `--config <path>` as an explicit override; use a path resolver rather than shell expansion.
2. Missing config uses defaults. Invalid schema/TOML shows a readable in-app error and uses defaults without overwriting the file. Unknown widget IDs remain persisted as missing entries; do not silently delete plugin choices. Deduplicate IDs preserving first occurrence.
3. Save atomically with a temporary sibling file and rename. Report save failure and retain the in-memory layout. Do not dump config or environment values in logs.
4. `widgets` is the ordered enabled list. `[]` deliberately shows an empty dashboard with a prompt to press `w`; it must not restore defaults automatically.

**Verify:** Default, custom, empty, duplicate, malformed, unsupported-version, missing-plugin, unwritable-file, and save/reload cases in temporary directories. Existing user files must survive failed writes.

## Task 07 Render layouts from the catalog

**Change:** new app `layout.rs`, refactor `dashboard.rs`, update focus state.

**Steps:**

1. Produce ordered `(WidgetId, Rect)` entries from visible IDs and terminal size. Reserve one row each for header/footer. Use a maximum of three columns, with at least 40 columns per panel; use one column below 80 columns. Use row major order and each row's maximum requested minimum height.
2. For vertical overflow, maintain a viewport of whole rows. Focus movement scrolls the focused row into view. If a single row exceeds available height, clamp that row to the viewport. Tiny terminals must avoid arithmetic underflow and zero-area rendering.
3. Focus only enabled panels. Draw a visible focused border. When the focused widget is hidden/missing, select the nearest remaining available panel, or no panel for an empty layout.
4. Use the same stored rectangles for drawing and mouse hit testing. Remove hardcoded row/column guesses. Process-row clicks use the table body origin and current scroll offset; clicking header/border only focuses the panel.
5. Temporarily unavailable built ins stay enabled with their existing unavailable message. Missing plugins retain a placeholder and cannot execute.

**Verify:** Test 40x12, 80x24, 120x40, 160x48, and a tiny 1x1 terminal, plus reorder/hide/empty layouts. Focus and click tests must use the generated rectangles. No enabled widget becomes permanently unreachable because of terminal height.

## Task 08 Implement the Widgets overlay

**Change:** new app `widget_picker.rs`, `input.rs`, `state.rs`, `dashboard.rs`, footer/help.

**Behavior:**

| Key | Widgets overlay action |
| --- | --- |
| `w` | Open Widgets; initialize a draft from the active configuration. |
| Up/Down or j/k | Select a catalog entry. |
| Space | Toggle the selected entry; show a plugin execution notice in its description. |
| `+` / `-` | Move an enabled entry up/down in the draft enabled order. |
| `r` | Restore the default built in layout in the draft. |
| Enter | Apply draft, save configuration, close. |
| Esc or q | Cancel draft and close. |
| F5 | Rescan plugin manifests without executing them; integrated in Task 11. |

Show enabled checkboxes, current order, source, availability/error, and a description for the selected entry. Enabled entries appear in layout order; disabled entries follow sorted by label and ID. Keep selection by ID while toggling/reordering. Use an explicit overlay enum so help and Widgets do not both consume keys. Clear the modal rectangle before drawing it. Modal keys never navigate the underlying dashboard.

**Verify:** Open/toggle/reorder/apply/restart, cancel without changes, empty layout recovery, long lists, small terminal, save failure, and modal key isolation. Update help/footer to the implemented bindings; remove the currently advertised unimplemented `d` action.

## Task 09 Discover and validate plugin manifests

**Change:** split the metadata stub into `opentop-core/src/plugins/{mod,manifest,discovery}.rs` and update registry tests. Preserve old exports with a compatibility wrapper if practical; otherwise migrate all workspace callers and explain the change.

**Folder contract:** Each search root contains immediate child plugin directories, each with `plugin.toml`. Scan one level only. Highest priority wins for a duplicate ID; roots in this order:

1. Repeated `--plugin-dir <path>` flags, in supplied order.
2. `plugin_dirs` from config, in listed order; relative paths resolve against the config file's directory.
3. `OPENTOP_PLUGIN_PATH`, parsed with the platform path-list parser, in listed order.
4. UNIX user `$XDG_DATA_HOME/opentop/plugins` or `~/.local/share/opentop/plugins`; Windows `%LOCALAPPDATA%/opentop/plugins`.
5. UNIX `/usr/local/lib/opentop/plugins`, then `/usr/lib/opentop/plugins`; Windows `%PROGRAMDATA%/opentop/plugins`.

CLI/env relative roots resolve against startup working directory; none are implicitly added from the current directory. Canonicalize and deduplicate existing roots; sort child paths for deterministic discovery. Missing roots are normal. Report unreadable roots/malformed entries individually and continue with other plugins. Keep duplicate diagnostics instead of silently replacing entries. An invalid higher-priority candidate with a readable ID remains a broken entry rather than executing a lower-priority duplicate.

**Manifest version one:**

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

Validate ID as `[a-z0-9][a-z0-9._-]{0,63}`, bounded metadata strings, protocol exactly 1, interval 1000–3600000 ms, timeout 50–10000 ms and strictly below interval, min height 3–30. Require a relative executable path that canonicalizes inside the canonical plugin directory; reject absolute paths, traversal, escaping symlinks, missing files, and on UNIX non-executable files. Bound manifest size to 64 KiB. Discovery must not spawn executables. Missing/disabled/error states are separate from the persisted enabled choice.

**Verify:** Temporary directory fixtures cover precedence, duplicate roots/IDs, spaces/Unicode in paths, missing roots, permissions where supported, malformed TOML, unsupported protocol, bounds, and path escapes. A marker executable must prove discovery never runs code.

## Task 10 Run plugins with bounded resources

**Change:** `plugins/runtime.rs`, `plugins/protocol.rs`; app integration follows in Task 11.

**Protocol:** Start one child per scheduled sample. Set working directory to its plugin directory; run the validated executable with manifest args directly, without a shell. Set stdin to null. stdout is one UTF-8 JSON object, followed only by whitespace; stderr is separate. No real snapshot or secrets are supplied by the host.

```json
{"protocol_version":1,"lines":["Status: OK","Jobs: 3"]}
```

**Steps:**

1. Run children off the UI thread. Execute only enabled plugins; no overlapping invocations for the same ID. Use a bounded worker pool, maximum four concurrent children, and coalesce overdue work instead of accumulating a queue. Default hard limit: 32 enabled plugins; show a clear error if exceeded.
2. Bound stdout and stderr separately to 64 KiB while reading concurrently. A child filling stderr must not deadlock stdout. Enforce the deadline while reading, including children holding pipes open; do not call unbounded `output()` and check size afterward.
3. Timeout, overflow, disable, rescan replacement, and shutdown cancel work and reap children. On UNIX launch a process group and terminate that group so descendants cannot retain pipes after cancellation. Cover the equivalent process-tree behavior on Windows or explicitly feature-gate execution there with a visible unsupported status until implemented.
4. Validate protocol and at most 200 lines of at most 512 Unicode scalar values each. Strip terminal control characters including ESC, C0/C1 controls, and DEL; the host renders plain text with Ratatui, never raw escape sequences. Each item is one logical line.
5. Publish a latest-only bounded result per plugin with data revision, last success time, and a typed status. Keep the previous successful output on failure and mark it stale. Never push plugin results into built in snapshot JSON or an unbounded channel.
6. Errors show safe categories such as timeout, invalid output, or exit code. Do not display/log raw stderr by default; plugins can accidentally emit secrets. Retry at the configured interval. Cancellation and config errors are not crashes of the host.

**Verify:** Test success, nonzero exit, invalid UTF-8/JSON/schema, multiple JSON objects, huge stdout, huge stderr, hanging child, child with a pipe-holding descendant, disable during execution, and simultaneous plugins. Prove limits hold, children are reaped, and the UI remains responsive while all worker slots are busy. Fixtures produce only synthetic data.

## Task 11 Integrate plugin panels and rescanning

**Change:** app catalog/config/picker/dashboard/runtime, new `opentop-widgets/src/plugin.rs` if appropriate.

**Steps:**

1. Discover on startup using the resolved roots; add plugin descriptors as `plugin:<id>`. Do manifest I/O on a worker and show discovery status rather than blocking initial input. Missing roots are harmless.
2. The enabled widget list controls execution. Persisted enabled plugins load automatically after validation on subsequent starts. Newly found plugins remain disabled until the user enables them and applies the picker draft.
3. Render latest lines in a standard host-owned panel with title, focus border, clipped/wrapped content, and concise waiting/error/stale status. Rendering reads cached results and never starts a process or reads plugin files.
4. F5 performs a worker rescan and reconciles by stable ID. Preserve built in state/order. If an enabled manifest or canonical executable changes, cancel old work, revalidate, and schedule the replacement. If it disappears, stop it and retain a missing placeholder/config choice. Invalid replacement must never leave old code running indefinitely.
5. Coalesce plugin data with the same data render deadlines; plugin errors cannot cause a redraw loop. Applying/canceling the picker must start/stop exactly the intended plugins.

**Verify:** Install fixture into a temporary root, launch with `--plugin-dir`, enable via keyboard, see its text, restart, remove it, rescan, and see the missing state. Repeat while a child is blocked and while changing the manifest. Canceling the picker must never execute a newly toggled plugin.

## Task 12 Ship an example and complete acceptance

**Change:** `examples/plugins/hello/`, `README.md`, `docs/PLUGINS.md`, `docs/PERFORMANCE.md`.

**Steps:**

1. Add a self-contained Python 3 executable example plus manifest. Document the Python requirement and executable permissions. This repository example is loaded only with an explicit plugin root; do not install it into user/system folders automatically.
2. Document every search folder and precedence rule, config syntax, IDs, controls, process privileges, output limits, failures, rescan semantics, and a copy/paste example using `--plugin-dir ./examples/plugins`. Do not modify the install script to install arbitrary plugins.
3. Re-run the Task 01 release benchmarks for idle, normal metrics, blocked collector, blocked plugins, and throttled output at both sizes. Save before/after numbers and exact reproduction commands. Fix budget failures rather than hiding them by slowing keyboard handling or removing metric coverage.
4. Run the workspace tests, formatting check, release build, and Clippy. Record pre-existing lint failures separately; do not reformat/fix unrelated code merely to make the report green. Run Linux and macOS checks; if one is unavailable, mark it unverified and supply the command. Check Windows feature gating does not break compilation.
5. Perform the real SSH comparison only on an authorized host under the fleet rules. If no host is available, leave that check explicitly pending; local PTY success does not prove real SSH acceptance.

**Commands:**

```bash
cargo test --workspace --offline
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo build --release --offline
python3 scripts/tui_perf.py --help
```

**Done when:** All supported local acceptance scenarios pass; the example is discovered/enabled/rendered through the actual UI; existing CLI output still works; terminal restoration is verified; documentation names any remaining platform/SSH verification gaps.

## Prompt to give the implementation model

```text
Read docs/IMPLEMENTATION_PLAN.md and the applicable AGENTS.md instructions.
Implement Task NN only, including its verification. Read the prerequisite task
results and relevant source before editing. Follow the fixed contracts in the
plan; do not redesign plugin formats, key bindings, or folder precedence.
Preserve unrelated working tree files and changes. Never print config/env
values, real process command lines, or raw plugin stderr in diagnostics.
Do not access servers, install plugins globally, commit, or push.

If a prerequisite is missing, report the exact missing prerequisite instead of
silently implementing later tasks. If an acceptance check fails, investigate
within this task and report the command/output summary. Do not weaken a check
to claim success. For Task 01, the new performance regression check is expected
to be red before the fixes.

Finish with: files changed, behavior implemented, commands run and their
results, unresolved issues, and the next task number. Update the execution
record in this plan with evidence; never mark a task done on code inspection
alone. Stop after this task.
```

## Execution record

Plan prepared 2026-10-20. No application changes were implemented by the planning task. Initial verification before implementation: `cargo test --workspace --offline` passed with existing warnings. Task 01 baselines are now recorded in `docs/PERFORMANCE.md`; actual SSH behavior is still unmeasured.

| Task | Status | Evidence |
| --- | --- | --- |
| 01 | Done (baseline red) | `docs/PERFORMANCE.md` records macOS 26.0.1 arm64 baselines at 80x24/160x48. `python3 scripts/tui_perf.py --binary target/release/examples/perf_fixture --scenario slow-collector --assert-responsive` exits 1 without hanging: 2055.37 ms key latency vs <=50 ms p95, with `lock_wait_us=2060132` vs `frame_us=5045` and `terminal_us=1650`. Idle scenario exits 1 with 48 draws/1200 bytes per 5 s; slow transport and startup exit 1 as expected. `cargo test --workspace --offline` passes; `cargo test -p opentop-core --offline --test monitor_stall` passes and pins the current stall for Task 02. Fixture builds via `cargo build --release --offline --example perf_fixture -p opentop-app`; shipped binary has no fixture switches. |
| 02 | Done | `cargo test -p opentop-core --test monitor_stall --offline` passes both lock-stall/history tests; collectors sample outside the shared lock and publish `CollectorUpdate` under a brief write lock. |
| 03 | Done (local; cold variance open) | `try_frame_since_never_waits_for_a_busy_writer` passes. Startup fixture passes at 80x24 and 160x48 (latest first UI 20.62/22.85 ms); several 80x24 wall-clock outliers at 396.93-570.08 ms remain documented for Task 12. |
| 04 | Done (local; macmon caveat) | Deadline-aware `run_bounded` replaces `Command::output()` for smartctl/sensors/nvidia-smi. Timeout, pipe-holding descendant, output cap and cancellation tests pass; PTY probes verify normal quit and panic restoration. Press/repeat/Ctrl-C/BackTab tests pass. Release real-binary quit took 479 ms in smoke and 949 ms in plugin UI because macmon cannot be interrupted mid-sample; that platform limit is documented. |
| 05 | Done | PID-stable selection, sort tie-breakers, symmetric page movement and table page size are wired. State tests cover sort/shrink/empty/End; TestBackend verifies the highlighted rendered row and pointer hit testing accounts for table offset. |
| 06 | Done | TOML config, default path/CLI overrides, de-duplication, empty layouts, unknown IDs, atomic save/reload and malformed-file preservation are implemented and tested. |
| 07 | Done (local) | Catalog-driven row-major layout, shared focus rectangles, mouse hit testing, focused border and 40x12/80x24/120x40/160x48/1x1 TestBackend coverage pass. |
| 08 | Done | Help/Widgets use an explicit overlay enum; picker toggle/reorder/apply/cancel, modal isolation, F5 rescan routing and config persistence are wired and tested. |
| 09 | Done (local) | Strict v1 manifest validation and ordered discovery are implemented. Tests cover unknown fields, bounds, oversize, symlink escape, precedence, malformed higher-priority ID, duplicate ID and discovery-never-executes. |
| 10 | Done (local Unix) | Supervisor uses max four workers/32 enabled, coalesced intervals, bounded concurrent stdout/stderr, timeout/cancel/process-group cleanup, protocol validation, sanitization, latest-only stale results; tests cover success/failure, pipe descendant, output caps, cancel/reap and concurrency. Windows execution is explicitly unsupported. |
| 11 | Done (local) | Async discovery, enabled-only execution, cached plugin panels, F5 rescan, missing placeholders and replacement cancellation are integrated. App test proves no execution before Apply and missing state after removal; blocked-plugin PTY acceptance passes at 80x24/160x48. |
| 12 | Partial (platform checks pending) | Python hello plugin, `docs/PLUGINS.md`, README controls, actual Widgets enable/save/restart PTY flow, and before/after PTY results are present. Workspace tests, release build, idle/slow-collector/slow-transport/blocked-plugin/panic probes pass on macOS. Startup passes most runs but several 80x24 wall-clock outliers exceed budget. Only `aarch64-apple-darwin` is installed; Linux/Windows commands are in `docs/PERFORMANCE.md`. Real SSH remains pending fleet authorization. Workspace formatting and Clippy remain red on formatting differences and existing warnings. |
