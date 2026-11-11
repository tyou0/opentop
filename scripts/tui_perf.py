#!/usr/bin/env python3
"""PTY performance harness for opentop.

Runs a release binary inside a pseudo terminal, drains its output (optionally
throttled to simulate a slow transport), injects timestamped keys, and reads
the timing counters the runtime writes to `--perf-log`.

Scenarios:
  slow-collector  Fixture collector blocks inside sample for sample 2;
                  measures key-to-visible-change latency (help overlay).
  idle            Fixed screen, no input; counts draw calls and bytes over
                  --duration seconds after a settle period.
  slow-transport  Process navigation (j) at 5 keys/second over a throttled
                  PTY; measures key-to-output latency.
  startup         Fixture collector blocks its first sample; measures first
                   UI and quit after the fixture releases the gate.
  blocked-plugin  Fills all four plugin worker slots with blocked synthetic
                    children; measures key latency, release, and shutdown.
  terminal-panic  Panics after terminal setup and verifies restoration output.
  plugin-ui       Enables the repository example through the actual Widgets UI.
  smoke           Runs any binary briefly; records startup and idle bytes.

Examples:
  python3 scripts/tui_perf.py --binary target/release/examples/perf_fixture \\
      --scenario slow-collector --assert-responsive
  python3 scripts/tui_perf.py --binary target/release/examples/perf_fixture \\
      --scenario idle --assert-idle
  python3 scripts/tui_perf.py --binary target/release/examples/perf_fixture \\
      --scenario blocked-plugin --assert-responsive
  python3 scripts/tui_perf.py --binary target/release/opentop --scenario plugin-ui \\
      --app-arg=--plugin-dir --app-arg=./examples/plugins --assert-responsive
  python3 scripts/tui_perf.py --binary target/release/opentop --scenario smoke

The reported budgets mirror docs/IMPLEMENTATION_PLAN.md: slow-collector p95
<= 50 ms and max <= 100 ms; idle zero draws/bytes after the first frame;
slow-transport p95 <= 200 ms at 32 KiB/s after warmup.
"""

from __future__ import annotations

import argparse
import fcntl
import math
import os
import pathlib
import pty
import re
import select
import shutil
import struct
import subprocess
import sys
import tempfile
import termios
import time

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
ANSI_CSI = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")
FIXTURE_SCENARIOS = {
    "slow-collector", "idle", "slow-transport", "startup", "blocked-plugin", "terminal-panic"
}
GATED_SAMPLES = range(2, 3)
IDLE_TAIL = b"\x1b[39m\x1b[49m\x1b[59m\x1b[0m\x1b[?25l"


def percentile(values: list[float], fraction: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    position = (len(ordered) - 1) * fraction
    low = math.floor(position)
    high = math.ceil(position)
    if low == high:
        return ordered[int(position)]
    return ordered[low] * (high - position) + ordered[high] * (position - low)


class Throttle:
    def __init__(self, bytes_per_second: int) -> None:
        self.rate = bytes_per_second
        self.tokens = 0.0
        self.last = time.monotonic()

    def available(self) -> int:
        if self.rate <= 0:
            return 4096
        now = time.monotonic()
        self.tokens = min(self.tokens + (now - self.last) * self.rate, self.rate)
        self.last = now
        if self.tokens < 1.0:
            return 0
        allowed = min(4096, int(self.tokens))
        self.tokens -= allowed
        return allowed


class PtySession:
    def __init__(
        self,
        argv: list[str],
        cols: int,
        rows: int,
        throttle_bps: int,
        env_overrides: dict[str, str] | None = None,
        env_remove: tuple[str, ...] = (),
    ) -> None:
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        env = os.environ.copy()
        env["TERM"] = "xterm-256color"
        env["COLUMNS"] = str(cols)
        env["LINES"] = str(rows)
        for name in env_remove:
            env.pop(name, None)
        if env_overrides:
            env.update(env_overrides)
        self.proc = subprocess.Popen(
            argv,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            close_fds=True,
            start_new_session=True,
            env=env,
            cwd=str(REPO_ROOT),
        )
        os.close(slave)
        self.master = master
        self.spawn_time = time.monotonic()
        self.throttle = Throttle(throttle_bps)
        self.buf = bytearray()
        self.bytes_total = 0
        self.eof = False
        self.dsr_answered = 0

    def pump(self, timeout: float = 0.0) -> int:
        deadline = time.monotonic() + timeout
        received = 0
        while True:
            if timeout <= 0:
                wait = 0.0
            else:
                wait = deadline - time.monotonic()
                if wait <= 0:
                    break
            readable, _, _ = select.select([self.master], [], [], min(wait, 0.05))
            if not readable:
                if timeout <= 0:
                    break
                continue
            allowed = self.throttle.available()
            if allowed <= 0:
                time.sleep(0.005)
                continue
            try:
                data = os.read(self.master, allowed)
            except OSError:
                self.eof = True
                break
            if not data:
                self.eof = True
                break
            self.buf += data
            self.bytes_total += len(data)
            received += len(data)
            self.answer_cursor_queries()
            if timeout <= 0:
                break
        return received

    def answer_cursor_queries(self) -> None:
        queries = self.buf.count(b"\x1b[6n")
        while self.dsr_answered < queries:
            try:
                os.write(self.master, b"\x1b[1;1R")
            except OSError:
                return
            self.dsr_answered += 1

    def send(self, data: bytes) -> float:
        timestamp = time.monotonic()
        try:
            os.write(self.master, data)
        except OSError:
            pass
        return timestamp

    def wait_for(self, pattern: bytes, timeout: float, since: int = 0):
        started = time.monotonic()
        while time.monotonic() - started < timeout:
            index = self.buf.find(pattern, since)
            if index != -1:
                return time.monotonic() - started, index
            if self.eof:
                return None, -1
            self.pump(0.02)
        index = self.buf.find(pattern, since)
        if index != -1:
            return time.monotonic() - started, index
        return None, -1

    def wait_for_output(self, timeout: float, since: int):
        started = time.monotonic()
        while time.monotonic() - started < timeout:
            if len(self.buf) > since:
                return time.monotonic() - started
            if self.eof:
                return None
            self.pump(0.02)
        return None

    def wait_for_change(self, timeout: float, since: int):
        started = time.monotonic()
        while time.monotonic() - started < timeout:
            if self.buf[since:].replace(IDLE_TAIL, b""):
                return time.monotonic() - started
            if self.eof:
                return None
            self.pump(0.02)
        return None

    def drain_for(self, seconds: float) -> None:
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            self.pump(min(0.05, deadline - time.monotonic()))

    def write_control(self, path: pathlib.Path, content: str = "1") -> None:
        try:
            path.write_text(content)
        except OSError:
            pass

    def release_all(self, control_dir: pathlib.Path) -> None:
        for sample in [1, *GATED_SAMPLES]:
            self.write_control(control_dir / f"release-{sample}")
        for plugin in range(4):
            self.write_control(control_dir / f"plugin-release-{plugin}")

    def close(self, control_dir: pathlib.Path | None) -> None:
        if control_dir is not None:
            self.release_all(control_dir)
        self.send(b"q")
        deadline = time.monotonic() + 1.0
        while time.monotonic() < deadline and self.proc.poll() is None:
            self.pump(0.05)
        if self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=1.0)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait(timeout=2.0)
        try:
            os.close(self.master)
        except OSError:
            pass


def wait_for_file(path: pathlib.Path, timeout: float, session: PtySession) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if path.exists():
            return True
        session.pump(0.02)
    return path.exists()


def read_events(path: pathlib.Path) -> list[dict]:
    try:
        text = path.read_text()
    except OSError:
        return []
    events = []
    for line in text.splitlines():
        fields = {}
        for token in line.split():
            if "=" in token:
                key, value = token.split("=", 1)
                fields[key] = value
        if "event" in fields:
            events.append(fields)
    return events


def integer(event: dict, key: str) -> int:
    try:
        return int(event[key])
    except (KeyError, ValueError):
        return 0


def perf_summary(events: list[dict]) -> dict:
    draws = [e for e in events if e["event"] == "draw"]
    collects = [e for e in events if e["event"] == "collect"]
    summary = {
        "draws": len(draws),
        "collect_max_us": max((integer(e, "collect_us") for e in collects), default=0),
        "gate_wait_max_us": max((integer(e, "gate_wait_us") for e in collects), default=0),
        "lock_wait_max_us": max((integer(e, "lock_wait_us") for e in draws), default=0),
        "frame_max_us": max((integer(e, "frame_us") for e in draws), default=0),
        "terminal_max_us": max((integer(e, "terminal_us") for e in draws), default=0),
    }
    return summary


def scenario_slow_collector(session: PtySession, args, control_dir: pathlib.Path, events_path: pathlib.Path):
    metrics: dict = {"scenario": "slow-collector"}
    first = session.wait_for(b"perf-fixture", 10.0)[0]
    metrics["first_output_ms"] = first * 1000 if first is not None else None

    latencies: list[float] = []
    missing = 0
    for sample in GATED_SAMPLES:
        if not wait_for_file(control_dir / f"started-{sample}", 10.0, session):
            metrics["gated_samples"] = sample - 2
            break
        before = len(session.buf)
        started = session.send(b"?")
        elapsed, _ = session.wait_for(b"Keyboard", args.key_timeout, since=before)
        session.write_control(control_dir / f"release-{sample}")
        if elapsed is None:
            missing += 1
            session.wait_for(b"Keyboard", 3.0, since=before)
        latencies.append((time.monotonic() - started) * 1000)
        session.drain_for(0.3)
        session.send(b"?")
        session.drain_for(0.2)

    metrics["gated_samples"] = len(latencies)
    metrics["latencies_ms"] = latencies
    metrics["p95_ms"] = percentile(latencies, 0.95)
    metrics["max_ms"] = max(latencies, default=None)
    metrics["missing_changes"] = missing
    return metrics


def scenario_idle(session: PtySession, args, control_dir: pathlib.Path, events_path: pathlib.Path):
    metrics: dict = {"scenario": "idle"}
    first = session.wait_for(b"perf-fixture", 10.0)[0]
    metrics["first_output_ms"] = first * 1000 if first is not None else None
    session.drain_for(0.5)

    draws_before = len([e for e in read_events(events_path) if e["event"] == "draw"])
    bytes_before = session.bytes_total
    session.drain_for(args.duration)
    draws_after = len([e for e in read_events(events_path) if e["event"] == "draw"])

    metrics["draw_calls_in_window"] = draws_after - draws_before
    metrics["bytes_in_window"] = session.bytes_total - bytes_before
    metrics["window_s"] = args.duration
    return metrics


def scenario_slow_transport(session: PtySession, args, control_dir: pathlib.Path, events_path: pathlib.Path):
    metrics: dict = {"scenario": "slow-transport", "throttle_bps": args.throttle_bps}
    first = session.wait_for(b"perf-fixture", 10.0)[0]
    metrics["first_output_ms"] = first * 1000 if first is not None else None
    session.drain_for(args.warmup)

    latencies: list[float] = []
    missing = 0
    bytes_before = session.bytes_total
    started = time.monotonic()
    while time.monotonic() - started < args.duration:
        before = len(session.buf)
        key_time = session.send(b"j")
        elapsed = session.wait_for_change(0.5, since=before)
        if elapsed is None:
            missing += 1
        else:
            latencies.append((time.monotonic() - key_time) * 1000)
        sleep_left = 0.2 - (time.monotonic() - key_time)
        if sleep_left > 0:
            session.drain_for(sleep_left)

    elapsed = time.monotonic() - started
    metrics["keys"] = len(latencies) + missing
    metrics["latencies_ms"] = latencies
    metrics["p95_ms"] = percentile(latencies, 0.95)
    metrics["max_ms"] = max(latencies, default=None)
    metrics["missing_changes"] = missing
    metrics["bytes_per_second"] = (session.bytes_total - bytes_before) / elapsed if elapsed > 0 else 0
    return metrics


def scenario_startup(session: PtySession, args, control_dir: pathlib.Path, events_path: pathlib.Path):
    metrics: dict = {"scenario": "startup"}
    wait_started = time.monotonic()
    elapsed, _ = session.wait_for(b"unknown host", args.startup_timeout)
    if elapsed is None:
        metrics["first_ui_ms"] = None
    else:
        metrics["first_ui_ms"] = (wait_started - session.spawn_time + elapsed) * 1000
    first_draw = next(
        (line for line in events_path.read_text(encoding="utf-8").splitlines() if " event=draw" in line),
        None,
    )
    metrics["first_draw_ms"] = (
        int(first_draw.split("t_us=", 1)[1].split()[0]) / 1000 if first_draw else None
    )

    while time.monotonic() - session.spawn_time < args.startup_release_delay:
        session.pump(0.05)
    session.write_control(control_dir / "release-1")
    session.wait_for(b"perf-fixture", args.startup_timeout)

    quit_sent = session.send(b"q")
    deadline = time.monotonic() + 2.0
    while time.monotonic() < deadline and session.proc.poll() is None:
        session.pump(0.05)
    metrics["quit_ms"] = (time.monotonic() - quit_sent) * 1000 if session.proc.poll() is not None else None
    session.pump(0.05)
    metrics["terminal_restored"] = (
        b"\x1b[?1049l" in session.buf and b"\x1b[?25h" in session.buf
    )
    return metrics


def scenario_blocked_plugin(session: PtySession, args, control_dir: pathlib.Path, events_path: pathlib.Path):
    metrics: dict = {"scenario": "blocked-plugin"}
    first = session.wait_for(b"perf-fixture", 10.0)[0]
    metrics["first_output_ms"] = first * 1000 if first is not None else None
    started = all(
        wait_for_file(control_dir / f"plugin-started-{index}", 5.0, session)
        for index in range(4)
    )
    metrics["all_workers_started"] = started
    before = len(session.buf)
    key_time = session.send(b"?")
    response, _ = session.wait_for(b"Keyboard", args.key_timeout, since=before)
    metrics["key_ms"] = (time.monotonic() - key_time) * 1000 if response is not None else None
    metrics["missing_changes"] = int(response is None)
    session.release_all(control_dir)
    session.send(b"\x1b")
    visible, _ = session.wait_for(b"plugin-0", 3.0)
    metrics["plugin_visible"] = visible is not None
    quit_sent = session.send(b"q")
    deadline = time.monotonic() + 2.0
    while time.monotonic() < deadline and session.proc.poll() is None:
        session.pump(0.05)
    metrics["quit_ms"] = (time.monotonic() - quit_sent) * 1000 if session.proc.poll() is not None else None
    return metrics


def scenario_terminal_panic(session: PtySession, args, control_dir, events_path):
    session.wait_for_output(3.0, 0)
    deadline = time.monotonic() + 3.0
    while time.monotonic() < deadline and session.proc.poll() is None:
        session.pump(0.05)
    session.pump(0.05)
    return {
        "terminal_restored": b"\x1b[?1049l" in session.buf and b"\x1b[?25h" in session.buf,
        "exit_code": session.proc.returncode,
    }


def scenario_plugin_ui(session: PtySession, args, control_dir, events_path):
    metrics: dict = {"scenario": "plugin-ui"}
    first, _ = session.wait_for(b"up ", 10.0)
    metrics["first_ui_ms"] = first * 1000 if first is not None else None
    session.send(b"w")
    picker, _ = session.wait_for(b"Widgets", 3.0)
    if picker is None:
        metrics.update(plugin_selected=False, plugin_visible=False, config_saved=False)
        return metrics

    selection_start = len(session.buf)
    selected = False
    for _ in range(100):
        if b"Plugin: runs" in session.buf[selection_start:]:
            selected = True
            break
        session.send(b"j")
        session.pump(0.05)
    metrics["plugin_selected"] = selected
    metrics["plugin_label_seen"] = b"Hello" in session.buf[selection_start:]
    if not selected:
        metrics.update(plugin_visible=False, config_saved=False)
        return metrics

    session.send(b" ")
    session.send(b"\r")
    panel_start = len(session.buf)
    expected_words = (b"Hello", b"from", b"OpenTop")
    deadline = time.monotonic() + 5.0
    while time.monotonic() < deadline and not all(
        word in session.buf[panel_start:] for word in expected_words
    ):
        session.pump(0.02)
    visible_text = ANSI_CSI.sub(b"", session.buf[panel_start:])
    metrics["plugin_visible"] = all(word in visible_text for word in expected_words)
    panel_output = session.buf[panel_start:]
    status_markers = [b"updated", b"timed out", b"exit code", b"invalid output", b"output limit exceeded", b"missing plugin", b"waiting"]
    metrics["plugin_status"] = next(
        (marker.decode("ascii") for marker in status_markers if marker in panel_output),
        "unseen",
    )
    metrics["plugin_words_seen"] = [
        marker.decode("ascii")
        for marker in (b"Hello", b"from", b"OpenTop")
        if marker in panel_output
    ]
    metrics["config_saved"] = (control_dir.parent / "config.toml").exists()
    quit_sent = session.send(b"q")
    deadline = time.monotonic() + 2.0
    while time.monotonic() < deadline and session.proc.poll() is None:
        session.pump(0.05)
    metrics["quit_ms"] = (time.monotonic() - quit_sent) * 1000 if session.proc.poll() is not None else None

    restart = PtySession(
        [str(args.binary_path), *args.app_arg, "--config", str(control_dir.parent / "config.toml")],
        cols=args.cols,
        rows=args.rows,
        throttle_bps=0,
        env_overrides={
            "HOME": str(control_dir.parent / "home"),
            "XDG_DATA_HOME": str(control_dir.parent / "xdg-data"),
        },
        env_remove=("OPENTOP_PLUGIN_PATH",),
    )
    try:
        restart_deadline = time.monotonic() + 5.0
        expected = (b"from", b"OpenTop")
        while time.monotonic() < restart_deadline and not all(
            word in restart.buf for word in expected
        ):
            restart.pump(0.05)
        metrics["restart_plugin_visible"] = all(word in restart.buf for word in expected)
    finally:
        restart.close(None)
    return metrics


def scenario_smoke(session: PtySession, args, control_dir, events_path):
    metrics: dict = {"scenario": "smoke"}
    first = session.wait_for(b"up ", 10.0)[0]
    metrics["first_output_ms"] = first * 1000 if first is not None else None
    session.drain_for(args.duration)
    quit_sent = session.send(b"q")
    deadline = time.monotonic() + 2.0
    while time.monotonic() < deadline and session.proc.poll() is None:
        session.pump(0.05)
    metrics["quit_ms"] = (time.monotonic() - quit_sent) * 1000 if session.proc.poll() is not None else None
    return metrics


SCENARIOS = {
    "slow-collector": scenario_slow_collector,
    "idle": scenario_idle,
    "slow-transport": scenario_slow_transport,
    "startup": scenario_startup,
    "blocked-plugin": scenario_blocked_plugin,
    "terminal-panic": scenario_terminal_panic,
    "plugin-ui": scenario_plugin_ui,
    "smoke": scenario_smoke,
}


def evaluate(args, metrics: dict) -> list[tuple[str, bool, str]]:
    checks: list[tuple[str, bool, str]] = []
    latencies = metrics.get("latencies_ms") or []

    if args.assert_responsive and args.scenario == "slow-collector":
        p95 = metrics.get("p95_ms")
        worst = metrics.get("max_ms")
        checks.append(("p95 <= 50 ms", p95 is not None and p95 <= 50, f"p95={fmt(p95)} ms"))
        checks.append(("max <= 100 ms", worst is not None and worst <= 100, f"max={fmt(worst)} ms"))
        checks.append(
            ("visible change for every gated key",
             metrics.get("missing_changes", 0) == 0,
             f"missing={metrics.get('missing_changes')}"),
        )
    if args.assert_responsive and args.scenario == "slow-transport":
        p95 = metrics.get("p95_ms")
        checks.append(("p95 <= 200 ms", p95 is not None and p95 <= 200, f"p95={fmt(p95)} ms"))
        checks.append(
            ("visible change for every key",
             metrics.get("missing_changes", 0) == 0,
             f"missing={metrics.get('missing_changes')}"),
        )
    if args.assert_responsive and args.scenario == "startup":
        first = metrics.get("first_ui_ms")
        quit_ms = metrics.get("quit_ms")
        checks.append(("first UI <= 200 ms", first is not None and first <= 200, f"first={fmt(first)} ms"))
        checks.append(("quit <= 200 ms", quit_ms is not None and quit_ms <= 200, f"quit={fmt(quit_ms)} ms"))
        checks.append((
            "terminal restored on quit",
            metrics.get("terminal_restored") is True,
            f"restored={metrics.get('terminal_restored')}",
        ))
    if args.assert_responsive and args.scenario == "blocked-plugin":
        key_ms = metrics.get("key_ms")
        checks.append(("all four worker slots started", metrics.get("all_workers_started") is True,
                       f"started={metrics.get('all_workers_started')}"))
        checks.append(("key <= 50 ms", key_ms is not None and key_ms <= 50,
                       f"key={fmt(key_ms)} ms"))
        checks.append(("visible plugin result after release", metrics.get("plugin_visible") is True,
                       f"visible={metrics.get('plugin_visible')}"))
        checks.append(("quit <= 200 ms", metrics.get("quit_ms") is not None and metrics["quit_ms"] <= 200,
                       f"quit={fmt(metrics.get('quit_ms'))} ms"))
    if args.assert_responsive and args.scenario == "terminal-panic":
        checks.append(("panic exits nonzero", metrics.get("exit_code") not in (None, 0),
                       f"exit_code={metrics.get('exit_code')}"))
        checks.append(("terminal restored during panic", metrics.get("terminal_restored") is True,
                       f"restored={metrics.get('terminal_restored')}"))
    if args.assert_responsive and args.scenario == "plugin-ui":
        checks.append(("example selected in picker", metrics.get("plugin_selected") is True,
                       f"selected={metrics.get('plugin_selected')}"))
        checks.append(("plugin output visible", metrics.get("plugin_visible") is True,
                       f"visible={metrics.get('plugin_visible')}"))
        checks.append(("config saved", metrics.get("config_saved") is True,
                       f"saved={metrics.get('config_saved')}"))
        checks.append(("plugin restored on restart", metrics.get("restart_plugin_visible") is True,
                       f"restart_visible={metrics.get('restart_plugin_visible')}"))
    if args.assert_idle and args.scenario == "idle":
        checks.append(
            ("zero draws in window",
             metrics.get("draw_calls_in_window") == 0,
             f"draws={metrics.get('draw_calls_in_window')}"),
        )
        checks.append(
            ("zero bytes in window",
             metrics.get("bytes_in_window") == 0,
             f"bytes={metrics.get('bytes_in_window')}"),
        )
    return checks


def fmt(value) -> str:
    if value is None:
        return "n/a"
    if isinstance(value, float):
        return f"{value:.2f}"
    return str(value)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="PTY performance harness for opentop",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument("--binary", required=True, help="release binary or fixture executable")
    parser.add_argument("--scenario", choices=sorted(SCENARIOS), default="slow-collector")
    parser.add_argument("--cols", type=int, default=80)
    parser.add_argument("--rows", type=int, default=24)
    parser.add_argument("--duration", type=float, default=5.0, help="measurement window in seconds")
    parser.add_argument("--warmup", type=float, default=2.0, help="slow-transport warmup seconds")
    parser.add_argument("--throttle-bps", type=int, default=32768, help="PTY read throttle (0 disables)")
    parser.add_argument("--key-timeout", type=float, default=2.0, help="seconds to wait for a visible change")
    parser.add_argument("--startup-release-delay", type=float, default=3.3,
                        help="startup scenario: seconds before releasing the first-sample gate")
    parser.add_argument("--startup-timeout", type=float, default=6.0)
    parser.add_argument("--perf-log", type=pathlib.Path, default=None)
    parser.add_argument("--app-arg", action="append", default=[],
                        help="extra argument passed to opentop in plugin-ui scenario")
    parser.add_argument("--assert-responsive", action="store_true")
    parser.add_argument("--assert-idle", action="store_true")
    args = parser.parse_args()

    binary = pathlib.Path(args.binary)
    if not binary.is_absolute():
        binary = (REPO_ROOT / binary).resolve()
    if not binary.exists():
        print(f"error: binary not found: {binary}", file=sys.stderr)
        return 2
    args.binary_path = binary
    if args.scenario in FIXTURE_SCENARIOS and "perf_fixture" not in binary.name:
        print(
            f"error: scenario {args.scenario} needs the perf_fixture example "
            f"(got {binary.name}); build it with "
            f"`cargo build --release --offline --example perf_fixture -p opentop-app`",
            file=sys.stderr,
        )
        return 2

    workdir = pathlib.Path(tempfile.mkdtemp(prefix="tui-perf-"))
    control_dir = workdir / "control"
    control_dir.mkdir()
    events_path = args.perf_log or (workdir / "perf.log")
    if args.scenario == "plugin-ui":
        (workdir / "config.toml").write_text("schema_version = 1\nwidgets = []\n", encoding="utf-8")

    argv = [str(binary), "--scenario", args.scenario, "--control-dir", str(control_dir),
            "--perf-log", str(events_path)] if args.scenario in FIXTURE_SCENARIOS else [str(binary)]
    if args.scenario == "plugin-ui":
        argv.extend(args.app_arg)
        argv.extend(["--config", str(workdir / "config.toml")])

    session = None
    try:
        session = PtySession(
            argv,
            cols=args.cols,
            rows=args.rows,
            throttle_bps=args.throttle_bps if args.scenario == "slow-transport" else 0,
            env_overrides={
                "HOME": str(workdir / "home"),
                "XDG_DATA_HOME": str(workdir / "xdg-data"),
            } if args.scenario == "plugin-ui" else None,
            env_remove=("OPENTOP_PLUGIN_PATH",) if args.scenario == "plugin-ui" else (),
        )
        metrics = SCENARIOS[args.scenario](session, args, control_dir, events_path)
        events = read_events(events_path)
        output_bytes = session.bytes_total
    except Exception as exc:
        print(f"error: scenario {args.scenario} failed: {exc}", file=sys.stderr)
        return 1
    finally:
        if session is not None:
            session.close(control_dir if args.scenario in FIXTURE_SCENARIOS else None)

    summary = perf_summary(events)
    print(f"binary={binary}")
    print(f"scenario={args.scenario} size={args.cols}x{args.rows} term=xterm-256color")
    print(f"host={sys.platform} python={sys.version.split()[0]}")
    print(f"throttle_bps={args.throttle_bps if args.scenario == 'slow-transport' else 0}")
    for key, value in metrics.items():
        if key != "latencies_ms":
            print(f"{key}={fmt(value)}")
    if metrics.get("latencies_ms"):
        print("latencies_ms=" + ",".join(f"{v:.2f}" for v in metrics["latencies_ms"]))
    print("--- perf counters (from runtime) ---")
    for key, value in summary.items():
        print(f"{key}={value}")
    print(f"output_bytes={output_bytes}")
    print(f"perf_log={events_path}")

    checks = evaluate(args, metrics)
    for name, ok, detail in checks:
        print(f"CHECK {'PASS' if ok else 'FAIL'}: {name} ({detail})")
    failed = [name for name, ok, _ in checks if not ok]
    if checks and failed:
        print(f"RESULT: FAIL ({len(failed)}/{len(checks)} checks failed)")
        if not args.perf_log:
            shutil.rmtree(workdir, ignore_errors=True)
        return 1
    print("RESULT: PASS" if checks else "RESULT: NO ASSERTIONS")
    if not args.perf_log:
        shutil.rmtree(workdir, ignore_errors=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
