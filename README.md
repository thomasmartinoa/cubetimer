# cubetimer

[![CI](https://github.com/thomasmartinoa/cubetimer/actions/workflows/ci.yml/badge.svg)](https://github.com/thomasmartinoa/cubetimer/actions/workflows/ci.yml)

A speedcubing timer that lives in your terminal. Hold space, release, solve. It has the WCA
inspection, real scrambles for 2x2 to 7x7, a scrambled cube you can turn in 3D, and a CFOP
algorithm trainer, in a single small Rust binary.

![cubetimer: scramble, big time, stats, recent solves](docs/screenshots/idle.png)

## What you get

**Timing that follows the rules.** Hold space until the digits turn green, release to start,
press any key to stop. Inspection counts down 15 seconds with the 8 s and 12 s warnings, then
+2 and DNF at 15 s and 17 s. Times are truncated and averages rounded the way the WCA and
csTimer do it. Best, mean, mo3, ao5, ao12 and ao100 update live, and new personal bests blink.

**Scrambles you can trust.** 3x3 and 2x2 use random-state scrambles made the way TNoodle, the
official WCA scrambler, makes them (a uniformly random state, solved in at most 21 moves). 4x4
to 7x7 use the WCA's random-move lengths. A colored net shows the scrambled cube so you can
check your scramble.

**A 3D cube.** Press `v` to see the scramble on a cube you can drag around, and play it move
by move. It is a real image in kitty, Ghostty, foot and WezTerm and falls back to half blocks
or ASCII art everywhere else, so it works over ssh too.

![the scrambled cube in 3D, mid-turn](docs/screenshots/cube3d.png)

**CFOP algorithms.** Press `a` for every PLL, OLL and F2L case with a standard algorithm,
drawn as a diagram and playable on the 3D cube. Mark what you are learning, then train it:
the trainer gives scrambles that set up exactly those cases and tells you which one you just
solved. Every algorithm is checked by the tests against the cube model.

![the algorithm browser: PLL cases with progress marks, and the T-perm on a cube](docs/screenshots/algs.png)

**Statistics.** Press `s` for a chart of your times with rolling ao5 and ao12, personal bests
marked, a histogram, and a full summary down to ao1000.

![the statistics screen: time chart, distribution and summary](docs/screenshots/stats.png)

And: sessions per puzzle with names, a history of every solve you can comment on, mark or
delete, a settings screen (hold time, precision and more), copying scrambles and averages to
the clipboard, CSV export, and import from csTimer.

## Install

Arch Linux (AUR):

```sh
COMING SOON
```

From source (Rust 1.90 or newer):

```sh
git clone https://github.com/thomasmartinoa/cubetimer
cd cubetimer
cargo install --path . --root ~/.local     # installs ~/.local/bin/cubetimer
```

Then run `cubetimer`, ideally full screen: `kitty --start-as=fullscreen cubetimer`.

## Using it

Press `?` in the timer for every key. The ones you need first:

| Key | |
|---|---|
| `space` | inspect, hold and release to start, any key stops |
| `+` `d` `o` | mark the last solve +2, DNF or OK (`Backspace` deletes it) |
| `h` | history: browse every solve, comment (`c`), mark, delete |
| `s` | statistics |
| `v` | the scramble in 3D (`m` changes how it is drawn) |
| `a` | algorithms and the trainer |
| `e` | choose the puzzle, 2x2 to 7x7 |
| `n` `[` `]` | new session, previous, next |
| `,` | settings |
| `q` | quit |

Your solves are saved after every change in `~/.local/share/cubetimer/sessions.json`.

```sh
cubetimer --export > solves.csv                    # every solve as CSV
cubetimer --import cstimer.txt --dry-run           # check a csTimer export first
cubetimer --import cstimer.txt                     # then add its sessions
```

## Terminals

Hold-to-start needs key release events, which come from the
[kitty keyboard protocol](https://sw.kovidgoyal.net/kitty/keyboard-protocol/). Tested end to
end with real key events:

| Terminal | Hold to start | 3D cube |
|---|---|---|
| kitty, Ghostty | yes | image |
| foot | yes | image (sixel) |
| WezTerm | with `config.enable_kitty_keyboard = true` | image |
| Alacritty | yes | half blocks / ASCII |
| any other terminal | tap to start and stop ("basic mode") | half blocks / ASCII |

`CUBETIMER_GRAPHICS=kitty|sixel|iterm2|halfblocks` overrides how the cube is drawn.

## Good to know

- Cubes only: no Pyraminx, Skewb, Megaminx, Clock or Square-1.
- 4x4 scrambles are random-move, not TNoodle's random-state. For practice that is fine;
  competitions must use TNoodle itself anyway.
- One algorithm per case, CFOP only; the trainer assumes a white cross and yellow last layer.
- The 3D view and the cube preview use true color.

## Contributing

`cargo test` runs about 400 tests (cube models, solvers, scrambles, algorithms, layouts, the
state machine). `scripts/pty_check.py` drives the real binary on a pseudo-terminal, and
`scripts/terminal_smoke.sh` checks the five terminals above with real key events (Hyprland,
`wtype` and `grim`). Work goes on `feat/*` branches into `dev`; `main` is for releases.
See [CHANGELOG.md](CHANGELOG.md) and [ROADMAP.md](ROADMAP.md).

## License

[MIT](LICENSE)
