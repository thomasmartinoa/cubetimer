#!/usr/bin/env python3
"""End-to-end checks of the real binary on a pseudo-terminal.

Usage: python3 scripts/pty_check.py target/release/cubetimer

Runs the timer as a terminal would, without a desktop, and checks:
  - basic mode (no kitty keyboard protocol): inspect, start, stop, quit
  - a terminal that never answers queries still starts (after a timeout)
  - a second copy refuses to start while one is running
  - SIGTERM and SIGHUP restore the terminal and exit with 128 + signal
Exits non-zero if any check fails. Uses a throwaway data directory.
"""

import fcntl
import os
import pty
import re
import select
import signal
import struct
import sys
import tempfile
import termios
import time

BINARY = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else "target/release/cubetimer"
DATA = tempfile.mkdtemp(prefix="cubetimer-pty-")
ENV = dict(os.environ, TERM="xterm-256color", XDG_DATA_HOME=DATA)
failures = []


def check(name, ok, detail=""):
    print(f"{'ok  ' if ok else 'FAIL'} {name}{' (' + detail + ')' if detail and not ok else ''}")
    if not ok:
        failures.append(name)


def spawn():
    pid, fd = pty.fork()
    if pid == 0:
        os.execve(BINARY, [BINARY], ENV)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
    return pid, fd


def pump(fd, secs, answer_da1=False):
    """Read output for `secs`; optionally answer the device-attributes query
    like xterm does (so the protocol check fails fast instead of timing out)."""
    out, answered, end = b"", False, time.time() + secs
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.05)
        if not r:
            continue
        try:
            data = os.read(fd, 65536)
        except OSError:
            break
        out += data
        if answer_da1 and not answered and b"\x1b[c" in out:
            os.write(fd, b"\x1b[?62;c")
            answered = True
    return out


def text(raw):
    return re.sub(rb"\x1b\[[0-9;?<>]*[A-Za-z]", b"", raw).decode("utf8", "replace")


def wait_exit(pid, secs=3):
    end = time.time() + secs
    while time.time() < end:
        done, status = os.waitpid(pid, os.WNOHANG)
        if done:
            return os.waitstatus_to_exitcode(status)
        time.sleep(0.05)
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
    return None


# Basic mode, as in a terminal without the kitty keyboard protocol.
pid, fd = spawn()
t0 = time.time()
screen = pump(fd, 2.5, answer_da1=True)
check("starts quickly when the terminal answers", b"scramble" in screen and time.time() - t0 < 3)
check("falls back to basic mode", "basic mode" in text(screen))
os.write(fd, b" ")
check("space starts inspection", "space: start" in text(pump(fd, 0.6)))
os.write(fd, b" ")
check("space starts the timer", "start inspection" not in text(pump(fd, 0.8)))
os.write(fd, b"x")
check("any key stops it", "space: start inspection" in text(pump(fd, 0.6)))

# A second copy while the first is running.
pid2, fd2 = spawn()
message = text(pump(fd2, 1.5))
code2 = wait_exit(pid2)
check("a second copy refuses to start", "already running" in message and code2 == 1, f"exit {code2}")

# SIGTERM restores the terminal.
check("raw mode while running", not termios.tcgetattr(fd)[3] & termios.ECHO)
os.kill(pid, signal.SIGTERM)
tail = pump(fd, 1.0)
code = wait_exit(pid)
lflag = termios.tcgetattr(fd)[3]
check("SIGTERM exits with 143", code == 143, f"exit {code}")
check("SIGTERM leaves the alternate screen", b"\x1b[?1049l" in tail)
check("SIGTERM shows the cursor", b"\x1b[?25h" in tail)
check("SIGTERM restores echo and line mode", bool(lflag & termios.ECHO) and bool(lflag & termios.ICANON))

# The lock is released, and SIGHUP (window closed) also exits cleanly.
pid3, fd3 = spawn()
check("starts again after the first copy ends", b"scramble" in pump(fd3, 2.5, answer_da1=True))
os.kill(pid3, signal.SIGHUP)
code3 = wait_exit(pid3)
check("SIGHUP exits with 129", code3 == 129, f"exit {code3}")

# A terminal that never answers: startup waits for the query timeout, then runs.
pid4, fd4 = spawn()
t0 = time.time()
screen = pump(fd4, 4.0)
check("starts even if the terminal never answers", b"scramble" in screen)
os.write(fd4, b"q")
code4 = wait_exit(pid4)
check("q quits with exit code 0", code4 == 0, f"exit {code4}")

print(f"\n{len(failures)} failed" if failures else "\nall passed")
sys.exit(1 if failures else 0)
