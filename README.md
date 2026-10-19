# cubetimer

A full-screen speedcubing timer for your terminal. Big ASCII digits, the WCA 15-second
inspection, hold-and-release start like a Stackmat, and live ao5 / ao12 / ao100 stats.

![cubetimer, idle: scramble, big last time, stats and recent solves](docs/screenshots/idle.png)

Built for [kitty](https://sw.kovidgoyal.net/kitty/) and written in Rust with
[ratatui](https://ratatui.rs). It only uses your terminal's ANSI colors, so it picks up
whatever theme you already run.

## Features

- **Hold to start.** Hold space until the display turns green, release to start the
  clock, press any key to stop. Needs the kitty keyboard protocol for release events;
  without it a space press starts and stops the timer (see [Other terminals](#other-terminals)).
- **WCA inspection.** A 15 to 1 countdown with warnings at 8 s and 12 s (and an optional
  terminal bell). Starting after 15 s is a **+2**, after 17 s a **DNF**.
- **Honest time formatting.** Single solves are truncated to hundredths and averages are
  rounded, as in the WCA regulations and csTimer. 12.349 shows as `12.34`; a +2 shows as
  `14.34+`; a DNF in the list keeps its raw time, `DNF(12.34)`.
- **Live stats.** Best, mean, ao5, ao12, ao100, best ao5 and best ao12. The average drops
  the best and worst 5% (at least one each) and a DNF counts as the worst result.
- **Scrambles.** A fresh 20-move scramble after every solve, with no repeated face and no
  `R L R` runs.
- **Sessions.** Everything is saved after every change; start a new session and flip
  between them.
- **Personal best flash.** Beat your best single and the time blinks.
- **Fits any window.** Big colossal-style digits, a compact 3-row font when the terminal
  is small, and a stripped-down layout below that.

| Inspection (8 s warning) | Ready to start | Running |
|---|---|---|
| ![inspection countdown with the 8s warning](docs/screenshots/inspection.png) | ![hold space until the digits turn green](docs/screenshots/ready.png) | ![the running timer](docs/screenshots/running.png) |

## Install

### Arch Linux (AUR)

```sh
yay -S cubetimer        # or paru -S cubetimer
```

### From source

You need Rust 1.89 or newer.

```sh
git clone https://github.com/thomasmartinoa/cubetimer
cd cubetimer
cargo install --path . --root ~/.local     # installs ~/.local/bin/cubetimer
```

Use `cargo install --path .` instead to install into `~/.cargo/bin`.

## Usage

Run `cubetimer`. For the full-screen experience:

```sh
kitty --start-as=fullscreen cubetimer
```

| Key | Action |
|---|---|
| `space` | Start inspection, then hold and release to start, then stop |
| any key | Stop a running solve |
| `Esc` | Cancel inspection or a hold |
| `+` (or `=`) / `d` / `o` | Mark the last solve +2, DNF or OK |
| `Backspace` | Delete the last solve (asks `y` / `n`) |
| `n` | New session (not while the current one is empty) |
| `[` / `]` | Previous / next session |
| `i` | Turn inspection on or off |
| `p` | Cycle the running display: `0.01`, whole seconds, hidden |
| `b` | Turn the inspection bell on or off |
| `q` / `Ctrl-C` | Quit (`q` stops the timer if one is running; `Ctrl-C` always quits) |

The mark, delete, session and `i` keys work only when the timer is idle.

Solves are stored in `$XDG_DATA_HOME/cubetimer/sessions.json` (usually
`~/.local/share/cubetimer/sessions.json`) and written atomically after every change. If
the file can't be read it is moved aside to `sessions.json.corrupt` rather than
overwritten, and a warning shows in the footer.

## Other terminals

cubetimer needs key *release* events for hold-to-start, which come from the
[kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). It checks
for support at startup:

- **Supported** (tested in kitty; other terminals that implement the protocol, such as
  foot, Ghostty, WezTerm and recent Alacritty, should work but are untested): hold and
  release to start.
- **Not supported**: it falls back to **basic mode**. A space press starts the timer and
  another stops it, and the footer says `basic mode`.

If hold-to-start doesn't behave, run the key tester and check that you see `Press`,
`Repeat` and `Release` lines for space:

```sh
cargo run --example keytest
```

The first line should say `keyboard enhancement: ON`.

## Limitations

- 3x3 only, with random-move scrambles (not the random-state scrambles used at WCA
  competitions).
- No graphs, Bluetooth smart cubes or other puzzles.

## Development

```sh
cargo test                 # 87 tests: formatting, stats, scrambles, state machine, layouts
cargo clippy
cargo run --example keytest
```

The code is small and split by job: `fmt.rs` (time formatting), `stats.rs` (averages),
`font.rs` (digit fonts), `scramble.rs`, `store.rs` (sessions on disk), `app.rs` (state
machine) and `ui.rs` (drawing). `term.rs` owns raw mode and the keyboard protocol, and
restores the terminal on exit and on panic.

## License

[MIT](LICENSE)
