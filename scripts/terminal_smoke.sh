#!/usr/bin/env bash
# Real-keyboard smoke test in every installed terminal (local only).
#
# Usage: scripts/terminal_smoke.sh [binary] [screenshot-dir]
#
# Needs a Hyprland session with wtype and grim. For each terminal it opens
# the timer, types a physical key sequence, and checks the saved solves:
#   tap space (inspection), hold space 1.2 s and release (start), stop with
#   space and keep holding 1.2 s (auto-repeat must not start a new attempt),
#   Shift+= (+2), Shift+/ (help), Esc, q.
# Expected in every terminal: exactly one solve of about 1.0 s, marked plus2.
# Every keystroke is sent only if the test window still has focus; if focus
# moves (say you click another window), that terminal is abandoned at once so
# no test keys land anywhere else. Don't use the desktop while it runs.

set -u
BIN=$(realpath "${1:-target/release/cubetimer}")
SHOTS=${2:-$(mktemp -d)}
mkdir -p "$SHOTS"
failed=0

focused() {
  hyprctl activewindow -j | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("class",""), d.get("title",""))'
}
region() {
  hyprctl activewindow -j | python3 -c 'import sys,json; d=json.load(sys.stdin); print("%d,%d %dx%d" % (*d["at"], *d["size"]))'
}

run() { # name, focus-match, command...
  local name=$1 match=$2; shift 2
  command -v "$1" >/dev/null || { echo "skip $name (not installed)"; return; }
  local data; data=$(mktemp -d)
  env XDG_DATA_HOME="$data" "$@" >/dev/null 2>&1 &
  local pid=$!
  sleep 3.5
  if [[ "$(focused)" != *"$match"* ]]; then
    echo "skip $name (focus is elsewhere)"
    kill "$pid" 2>/dev/null
    return
  fi
  # each step re-checks focus before typing
  send() {
    if [[ "$(focused)" != *"$match"* ]]; then
      echo "FAIL $name: focus moved away, stopped sending keys"
      failed=1
      kill "$pid" 2>/dev/null # close the test window
      return 1
    fi
    wtype "$@"
  }
  send -k space && sleep 0.5 &&
    send -P space -s 1200 -p space && sleep 1.0 &&
    send -P space -s 1200 -p space && sleep 0.4 &&
    grim -g "$(region)" "$SHOTS/$name-stopped.png" &&
    send -M shift -k equal -m shift && sleep 0.3 &&
    send -M shift -k slash -m shift && sleep 0.4 &&
    grim -g "$(region)" "$SHOTS/$name-help.png" &&
    send -k Escape && sleep 0.2 &&
    send -k q && sleep 0.8 || return
  python3 - "$data/cubetimer/sessions.json" "$name" <<'EOF' || failed=1
import json, sys
path, name = sys.argv[1], sys.argv[2]
try:
    solves = json.load(open(path))["sessions"][0]["solves"]
except Exception as e:
    print(f"FAIL {name}: no saved data ({e})"); sys.exit(1)
ok = len(solves) == 1 and solves[0]["penalty"] == "plus2" and 800 <= solves[0]["ms"] <= 1500
print(f"{'ok  ' if ok else 'FAIL'} {name}: {[(s['ms'], s['penalty']) for s in solves]}")
sys.exit(0 if ok else 1)
EOF
}

run kitty     cubetimer-smoke kitty --title cubetimer-smoke "$BIN"
run alacritty cubetimer-smoke alacritty --title cubetimer-smoke -e "$BIN"
run foot      cubetimer-smoke foot --title cubetimer-smoke "$BIN"
run ghostty   cubetimer-smoke ghostty --title=cubetimer-smoke -e "$BIN"
run wezterm   wezterm wezterm --config enable_kitty_keyboard=true start --always-new-process -- "$BIN"

echo "screenshots: $SHOTS"
exit $failed
