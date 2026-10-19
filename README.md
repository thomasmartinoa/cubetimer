# cubetimer

A full-screen speedcubing timer for your terminal. Big ASCII digits, the WCA 15-second
inspection, hold-and-release start like a Stackmat, WCA-style random-state scrambles with a
preview of the scrambled cube, and live ao5 / ao12 / ao100 stats.

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
- **Random-state scrambles.** Made the way the WCA's official scrambler (TNoodle) makes
  them: a uniformly random cube state, solved with the two-phase algorithm in at most 21
  moves. See [Scrambles](#scrambles).
- **Scramble preview.** A colored net of the scrambled cube, so you can check you
  scrambled it right.
- **Live stats.** Best, mean, mo3, ao5, ao12, ao100, best ao5 and best ao12. The average
  drops the best and worst 5% (at least one each) and a DNF counts as the worst result.
- **History.** Press `h` to browse every solve: its time, when it was done, its scramble
  and cube, and a trend chart of the session with the rolling ao5. Mark any solve +2, DNF
  or OK, or delete it.
- **Sessions.** Everything is saved after every change; start a new session, flip
  between them, or delete one.
- **Your data is yours.** `cubetimer --export` prints every solve as CSV for a
  spreadsheet or another timer.
- **Personal best flash.** Beat your best single and the time blinks.
- **Digits sized to your window.** The default `auto` mode keeps the time at about half
  of the available height: not huge, not tiny, at any window size. Press `f` to
  pick a specific style instead, see [Digit fonts](#digit-fonts).
- **Fits any window.** If the font you picked doesn't fit, the next smaller one is used,
  and the layout shrinks to a few text lines in very small windows.
- **Remembers your setup.** The font, inspection and bell settings are saved.
- **Safe with your data.** Saves are atomic, a damaged file is set aside instead of
  overwritten, only one copy can run at a time, and the terminal is restored however the
  program ends (quit, crash, `kill`, closed window).

| Inspection (8 s warning) | Ready to start | Running |
|---|---|---|
| ![inspection countdown with the 8s warning](docs/screenshots/inspection.png) | ![hold space until the digits turn green](docs/screenshots/ready.png) | ![the running timer](docs/screenshots/running.png) |

![the history browser: solve list, the selected solve's details, scramble and cube](docs/screenshots/history.png)

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
| `h` | History: browse, mark and delete any solve |
| `n` | New session (not while the current one is empty) |
| `[` / `]` | Previous / next session |
| `X` | Delete the current session and its solves (asks `y` / `n`) |
| `i` | Turn inspection on or off |
| `f` / `F` | Next / previous digit font (`auto` first) |
| `p` | Cycle the running display: `0.01`, whole seconds, hidden |
| `b` | Turn the inspection bell on or off |
| `?` or `F1` | Show all keys |
| `q` / `Ctrl-C` | Quit (`q` stops the timer if one is running; `Ctrl-C` always quits) |

The mark, delete, history, session, `i` and `?` keys work only when the timer is idle.
The font, `p` and bell keys work any time except while the clock is running, when any key
stops it.

In the history browser, `↑`/`↓` (or `k`/`j`), `PgUp`/`PgDn` and `Home`/`End` move, `+` `d`
`o` mark the selected solve, `Backspace` deletes it (asks first) and `Esc` closes.

`cubetimer --help` and `cubetimer --version` work as usual. `cubetimer --export > solves.csv`
saves every solve (session, number, time, penalty, result, scramble, UTC date) as CSV. Starting a second copy while
one is running prints `already running` and exits, so two copies can't overwrite each
other's solves.

Solves are stored in `$XDG_DATA_HOME/cubetimer/sessions.json` (usually
`~/.local/share/cubetimer/sessions.json`) and written atomically after every change. If
the file can't be read it is moved aside to `sessions.json.corrupt` rather than
overwritten, and a warning shows in the footer.

## Digit fonts

`f` cycles through `auto` and the fonts below, tallest first. Your choice is saved.
`colossal`, `standard` and `mini` are ASCII art and work in any terminal. `pixel-xl`,
`pixel` and `half` use Unicode block characters (`█ ▀ ▄`), which every common terminal
font has. `plain` is ordinary text.

`auto` picks from `colossal`, `standard` and `mini` (all ASCII), so it works everywhere.

```
pixel-xl (10 rows)
    ████        ████████████            ████████████    ████    ████
    ████        ████████████            ████████████    ████    ████
████████                ████            ████            ████    ████
████████                ████            ████            ████    ████
    ████        ████████████            ████████████    ████████████
    ████        ████████████            ████████████    ████████████
    ████                ████                    ████            ████
    ████                ████                    ████            ████
████████████    ████████████    ████    ████████████            ████
████████████    ████████████    ████    ████████████            ████

colossal (8 rows)
  d888     .d8888b.        888888888       d8888
 d8888    d88P  Y88b       888            d8P888
   888         .d88P       888           d8P 888
   888        8888"        8888888b.    d8P  888
   888         "Y8b.            "Y88b  d88   888
   888    888    888              888  8888888888
   888    Y88b  d88P  888  Y88b  d88P        888
 8888888   "Y8888P"   888   "Y8888P"         888

standard (5 rows)
 _   _____       ____    _  _
/ | |___ /      | ___|  | || |
| |   |_ \      |___ \  | || |_
| |  ___) |  _   ___) | |__   _|
|_| |____/  (_) |____/     |_|

pixel (5 rows)
  ██    ██████      ██████  ██  ██
████        ██      ██      ██  ██
  ██    ██████      ██████  ██████
  ██        ██          ██      ██
██████  ██████  ██  ██████      ██

mini (3 rows)
     _     _
  |  _|   |_  |_|
  |  _| .  _|   |

half (3 rows)
▄█  ▀▀█   █▀▀ █ █
 █  ▀▀█   ▀▀█ ▀▀█
▀▀▀ ▀▀▀ ▀ ▀▀▀   ▀

plain (1 row)
13.54
```

Run `cargo run --example fonts` to print them, or `cargo run --example fonts -- 1:02.34+`
to try other text.

## Scrambles

WCA Regulation 4b3 asks for "a random state from all states that require at least 2 moves
to solve". cubetimer follows the method of TNoodle, the WCA's official scramble program:

1. pick a cube state uniformly at random from all 43 quintillion legal states;
2. throw it away if it is solved or one move from solved;
3. solve it with Kociemba's two-phase algorithm, in at most 21 moves;
4. show the inverse of that solution as the scramble.

So every state is equally likely, which random-move scrambles can't promise. Scrambles are
usually 19 to 21 moves. The solver's tables take about 0.15 s to build when the program
starts; the timer shows at once and the first scramble appears a moment later. After that
the next scramble is prepared in the background while you solve.

This is the same method, not the same program: for practice it is equivalent, but official
competitions must use TNoodle itself (Regulation 4b).

## Other terminals

cubetimer needs key *release* events for hold-to-start, which come from the
[kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). It checks
for support at startup:

- **Supported**: hold and release to start.
- **Not supported**: it falls back to **basic mode**. A space press starts the timer and
  another stops it, and the footer says `basic mode`. (A terminal that never answers the
  capability query makes startup wait about two seconds before falling back.)

| Terminal | Status |
|---|---|
| kitty 0.49 | Tested: hold-to-start |
| foot 1.28 | Tested: hold-to-start |
| Alacritty 0.17 | Tested: hold-to-start |
| Ghostty 1.3 | Tested: hold-to-start |
| WezTerm | Tested: basic mode by default. For hold-to-start add `config.enable_kitty_keyboard = true` to `wezterm.lua` |
| Any terminal without the protocol | Tested on a bare pty: basic mode works end to end |

Each was checked end to end with the same physical key sequence: inspection,
hold-and-release start, stopping while holding the key, Shift+`=` for +2 and Shift+`/`
for help. Terminals disagree on some details (Ghostty and WezTerm send a held key's
auto-repeat as new presses; WezTerm leaves out Shift on shifted keys), and cubetimer
handles each of them.

If hold-to-start doesn't behave, run the key tester and check that you see `Press`,
`Repeat` and `Release` lines for space:

```sh
cargo run --example keytest
```

(`KEYTEST_LOG=file` also writes the events to a file.)

The first line should say `keyboard enhancement: ON`.

## Limitations

- 3x3 only.
- No graphs, Bluetooth smart cubes, csTimer import/export, or session names.
- The scramble preview uses true color (24-bit), which every terminal listed above
  supports.

## Development

```sh
cargo test                 # cube model, solver, scrambles, formatting, stats, fonts, state machine, layouts
cargo clippy --all-targets
cargo run --example keytest
cargo run --release --example scramblebench -- 2000   # solver speed and scramble lengths
```

The code is split by job: `cube.rs` (the cube at the piece level), `twophase.rs` (the
solver), `scramble.rs`, `fmt.rs` (time formatting), `stats.rs` (averages), `font.rs`
(digit fonts), `store.rs` (sessions on disk), `app.rs` (state machine) and `ui.rs`
(drawing). `term.rs` owns raw mode and the keyboard protocol, and restores the terminal on
exit, on panic and on signals.

## License

[MIT](LICENSE)
