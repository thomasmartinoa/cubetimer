#!/usr/bin/env python3
"""End-to-end checks of the real binary on a pseudo-terminal.

Usage: python3 scripts/pty_check.py target/release/cubetimer

Runs the timer as a terminal would, without a desktop, and checks:
  - basic mode (no kitty keyboard protocol): inspect, start, stop, quit
  - a terminal that never answers queries still starts (after a timeout)
  - a second copy refuses to start while one is running
  - SIGTERM and SIGHUP restore the terminal and exit with 128 + signal
  - the cube view (v) opens quickly and keys keep working afterwards, even in
    a terminal that never answers queries
Exits non-zero if any check fails. Uses a throwaway data directory.
"""

import fcntl
import json
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


def spawn(term=None):
    pid, fd = pty.fork()
    if pid == 0:
        env = dict(ENV, TERM=term) if term else ENV
        os.execve(BINARY, [BINARY], env)
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

# The cube view in terminals that never answer: as plain xterm (no query is
# made) and as foot (which needs one cell-size query, given up after 300 ms).
# Keys typed afterwards must still reach the app.
for term in ("xterm-256color", "foot"):
    pid5, fd5 = spawn(term)
    pump(fd5, 2.5)
    os.write(fd5, b"v")
    t0 = time.time()
    shown = False
    while time.time() - t0 < 3:
        if "blocks]" in text(pump(fd5, 0.1)):  # "[half blocks]" in the footer
            shown = True
            break
    took = time.time() - t0
    check(f"cube view opens within 1 s ({term}, terminal never answers)", shown and took < 1.0, f"{took:.2f} s")
    os.write(fd5, b" ")  # play the scramble
    check(f"keys still reach the app after opening it ({term})", "pause" in text(pump(fd5, 0.6)))
    os.write(fd5, b"\x1b")
    pump(fd5, 0.3)
    os.write(fd5, b"q")
    code5 = wait_exit(pid5)
    check(f"q still quits afterwards ({term})", code5 == 0, f"exit {code5}")

# The cube look (m): with no image support the view starts as half blocks
# and every press must change what is drawn (ascii, mono, blocks, ascii...).
# The terminal only gets the cells that changed, so check that each press
# redraws most of the cube, then that the final choice was saved.
pid7, fd7 = spawn("xterm-256color")
pump(fd7, 2.5)
os.write(fd7, b"v")
opened = text(pump(fd7, 1.0))
check("the cube view starts as half blocks without image support", "[m: half blocks]" in opened)
for look in ("ascii", "mono", "blocks", "ascii"):
    os.write(fd7, b"m")
    redraw = pump(fd7, 0.6)
    check(f"m redraws the cube as {look}", len(redraw) > 1500, f"{len(redraw)} bytes")
os.write(fd7, b"\x1b")
pump(fd7, 0.3)
os.write(fd7, b"q")
check("q quits after cycling the look", wait_exit(pid7) == 0)
try:
    saved = json.load(open(os.path.join(DATA, "cubetimer", "sessions.json")))["settings"]["look"]
except Exception as e:
    saved = f"unreadable: {e}"
check("the chosen look is saved", saved == "ascii", saved)

# The algorithm browser (a), a case on the 3D cube (enter), and the trainer
# (t): its scrambles come in and a solve is saved tagged with its case.
pid8, fd8 = spawn("xterm-256color")
pump(fd8, 2.5)
os.write(fd8, b"a")
check("a opens the algorithm browser", "algorithms" in text(pump(fd8, 0.8)))
os.write(fd8, b"\r")
check("enter shows the case on the 3D cube", "Aa-perm" in text(pump(fd8, 1.0)))
os.write(fd8, b"\x1b")
pump(fd8, 0.4)
os.write(fd8, b"t")
trained = text(pump(fd8, 2.0))
check("t starts the PLL trainer", "PLL trainer" in trained)
os.write(fd8, b"i")  # inspection off, so space starts the clock (basic mode)
pump(fd8, 0.3)
os.write(fd8, b" ")
pump(fd8, 0.6)
os.write(fd8, b" ")
check("after a training solve it names the case", "that was" in text(pump(fd8, 0.8)))
os.write(fd8, b"q")
check("q quits from the trainer", wait_exit(pid8) == 0)
try:
    sessions = json.load(open(os.path.join(DATA, "cubetimer", "sessions.json")))["sessions"]
    drill = [x for x in sessions if x.get("drill") == "PLL"]
    tagged = drill and drill[0]["solves"] and drill[0]["solves"][-1].get("case", "").startswith("PLL:")
except Exception as e:
    tagged = False
check("the training solve is saved with its case", bool(tagged))

# Settings, names and the clipboard: a typed name is saved, the settings
# screen opens, and y sends an OSC 52 clipboard sequence to the terminal.
pid9, fd9 = spawn("xterm-256color")
pump(fd9, 2.5)
os.write(fd9, b",")
check(", opens the settings screen", "hold time" in text(pump(fd9, 0.6)))
os.write(fd9, b"\x1b")
pump(fd9, 0.3)
os.write(fd9, b"rwarm-up\r")
named = text(pump(fd9, 0.6))
check("r names the session", '"warm-up"' in named)
os.write(fd9, b"y")
copied = pump(fd9, 0.6)
check("y copies the scramble with OSC 52", b"\x1b]52;c;" in copied)
os.write(fd9, b"q")
check("q quits after settings and copying", wait_exit(pid9) == 0)

# Mouse: reporting is on only while the cube view is open, and a drag turns
# the cube (SGR mouse sequences, as kitty, foot, Ghostty and WezTerm send).
pid6, fd6 = spawn()
before_view = pump(fd6, 2.5, answer_da1=True)
check("mouse reporting is off on the timer screen", b"\x1b[?1000h" not in before_view)
os.write(fd6, b"v")
opened = pump(fd6, 0.8)
check("mouse reporting turns on with the cube view", b"\x1b[?1000h" in opened and b"\x1b[?1006h" in opened)
os.write(fd6, b"\x1b[<0;40;15M")            # left button down
pump(fd6, 0.1)
os.write(fd6, b"\x1b[<32;52;15M")           # drag 12 columns right
dragged = pump(fd6, 0.6)
os.write(fd6, b"\x1b[<0;52;15m")            # release
pump(fd6, 0.1)
check("a mouse drag redraws the cube", len(dragged) > 1000, f"{len(dragged)} bytes")
os.write(fd6, b"\x1b")
closed = pump(fd6, 0.6)
check("mouse reporting turns off when the view closes", b"\x1b[?1000l" in closed)
os.write(fd6, b"q")
check("q quits after using the mouse", wait_exit(pid6) == 0)

print(f"\n{len(failures)} failed" if failures else "\nall passed")
sys.exit(1 if failures else 0)
