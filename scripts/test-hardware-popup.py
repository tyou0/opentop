#!/usr/bin/env python3
"""Exercise the installed dashboard's hardware popup in a real terminal."""

import sys
import time

from tui_perf import PtySession


def main() -> None:
    session = PtySession([sys.argv[1]], cols=200, rows=30, throttle_bps=0)
    try:
        if session.wait_for(b"up ", 10.0)[0] is None:
            raise AssertionError("dashboard did not start")

        def press_and_wait(key: bytes, expected: bytes) -> None:
            start = len(session.buf)
            session.send(key)
            if session.wait_for(expected, 5.0, since=start)[0] is None:
                raise AssertionError(f"expected {expected!r} after {key!r}")

        press_and_wait(b"i", b"Hardware")
        for title in (b"Memory", b"GPU", b"Storage", b"System"):
            press_and_wait(b"\x1b[C", title)
        press_and_wait(b"\x1b[D", b"Storage")
        session.send(b"\x1b")
        session.drain_for(0.2)
        press_and_wait(b"?", b"Keyboard")
        session.send(b"\x1b")
        session.drain_for(0.2)
        session.send(b"q")
        deadline = time.monotonic() + 5.0
        while session.proc.poll() is None and time.monotonic() < deadline:
            session.pump(0.05)
        if session.proc.poll() != 0:
            raise AssertionError("dashboard did not quit cleanly")
        print("hardware popup terminal check passed")
    finally:
        session.close(None)


if __name__ == "__main__":
    main()
