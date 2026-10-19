# cubetimer: full-screen ASCII speedcube timer for kitty (Rust)

## Goal
A full-screen terminal timer for 3x3 solves that feels like csTimer or a Stackmat:
large ASCII digits (the figlet "colossal" style, as in the reference screenshot), the
WCA 15-second inspection, hold-and-release start, and live ao5/ao12 stats. Built for
kitty, but it should still work in other terminals with a fallback.

## Tech choices
- **Rust (2024 edition)** as a single binary crate.
- **ratatui** for layout, buffer diffing (no flicker) and resize handling.
  - Use the crossterm that ratatui re-exports (`ratatui::crossterm`), so the two
    crates never end up on mismatched versions.
- **crossterm** for raw mode, the alternate screen and the kitty keyboard protocol.
- **serde + serde_json** for saved sessions, **dirs** for the XDG data path, and
  **rand** for scrambles.
- Rendering: the event loop calls `event::poll` with a timeout of about 16 ms while
  the timer runs. When idle it blocks on input instead (or ticks at about 10 fps
  during inspection).
- Colors: `Color::Indexed(0..=15)` and the named ANSI colors only, so the timer
  automatically picks up the user's kitty or Orrery theme.
- Timing: `std::time::Instant`, stored internally in milliseconds.

## Key input (the hard part)
kitty keyboard protocol through crossterm:
- Detect support with `crossterm::terminal::supports_keyboard_enhancement()`.
- If it's supported, run `execute!(PushKeyboardEnhancementFlags(...))` with these
  flags:
  - `DISAMBIGUATE_ESCAPE_CODES`
  - `REPORT_EVENT_TYPES`
  - `REPORT_ALL_KEY_EVENTS_AS_ESCAPE_CODES`. This one is needed, or space does not
    send release events.
- `KeyEvent.kind` tells you whether the event is `Press`, `Repeat` or `Release`.
  - Ignore `Repeat`.
  - Hold to start means `Press` → `Release` of `KeyCode::Char(' ')`.
- Always restore the terminal: `PopKeyboardEnhancementFlags`, leave the alternate
  screen, show the cursor, and disable raw mode.
  - Put the restore in a `Drop` guard struct, and also call it from a panic hook
    (`std::panic::set_hook`).
  - A panic must never leave kitty in a broken state. `ratatui::init()` and
    `ratatui::restore()` handle the basics, but the keyboard flags need popping on
    top of that.
- **Fallback** when the protocol isn't supported: a space *press* starts and stops
  the timer, with no hold step. Show "basic mode" in the footer.

## State machine (`enum State`)
```
Idle ──space press──▶ Inspection{start} (15→0 countdown, big digits)
Inspection ──space press──▶ Holding{since} (red while <300ms, then green = ready)
Holding ──release before ready──▶ back to Inspection (or Idle if inspection off)
Holding(ready) ──release──▶ Running{start} (see "Time display format", big digits)
Running ──any key press──▶ Stopped → solve saved → Idle (shows final time)
```
- Inspection follows WCA rules. Show "8s!" and "12s!" warnings, with an optional
  terminal bell (`\x07`). Starting after 15 s adds **+2**, and going past 17 s is
  **DNF**.
- `i` toggles inspection on or off. `Esc` cancels inspection or a hold.

## Screen layout (ratatui `Layout`, responsive, centered)
```
┌ scramble (wrapped, top) ───────────────────────────────┐
│                                                        │
│            ██  BIG  ASCII  TIME / COUNTDOWN  ██        │
│                 state hint (hold… / ready / +2)        │
│                                                        │
├ stats ─────────────────┬ recent times ─────────────────┤
│ best  ao5  ao12  ao100 │ 12. 14.21                     │
│ mean  count  session   │ 11. 13.88 (+2)  10. DNF ...   │
└────────────────────────┴───────────────────────────────┘
```
- Big font: a built-in "colossal"-style glyph map in `font.rs` covering
  `0-9 . : + - D N F ( ) s`.
  - Each glyph is `&[&str]` with a fixed height, and `render(text) -> Vec<Line>`
    joins them side by side.
  - Include a smaller fallback font for when the terminal is too narrow.
  - `tui-big-text` is not used, because its 8x8 pixel font looks different from
    the colossal style.
- Color the time by state:
  - white when idle
  - yellow during inspection, turning orange or red after the 8 s and 12 s
    warnings
  - red or green while holding
  - the accent color while running
  - a brief flash for a new personal best

## Time display format (`fmt.rs`, matches WCA, csTimer and the Stackmat)
These rules follow WCA Regulation 9f (truncate to hundredths) and csTimer's
defaults. Store milliseconds internally and format only at the edges.
- **Single solves are truncated, not rounded, to centiseconds.**
  - `12_349 ms → 12.34`
  - Use integer math, `cs = ms / 10`. Never format through floats.
- **No leading minutes or zeros under 60 s.**
  - `7.05`, `14.21`
  - At 60 s and over: `1:02.34`
  - At 10 min and over: `10:03.00`
- **Averages and means are rounded to the nearest hundredth.**
  - `(sum_ms + 5) / 10` in centiseconds
  - Displayed with the same format.
- **+2 solves** show the penalized time with a trailing plus. For example, a raw
  12.34 shows as `14.34+`, both in the big display and in the lists. This is the
  csTimer style.
- **DNF** shows as `DNF` on the big display. Lists show `DNF(12.34)`, keeping the
  raw time.
- **While the timer runs**, the display updates live at 0.01 precision. `p` cycles
  the running display between three modes:
  - `0.01`
  - whole seconds (some cubers prefer `12` while solving)
  - `solving...` (time hidden until the stop)

  The final time always shows in full `0.01` precision.
- **Inspection** counts down in whole seconds: `15, 14 … 1`.
  - From 15 to 17 s it shows `+2`, and after 17 s it shows `DNF`.
  - The warnings fire at 8 s and 12 s elapsed.
- Show averages that don't have enough solves yet as `-`, the way csTimer does.
- **Optional setting:** 0.001 precision (`12.345`), for cubers who use
  thousandths-precision timers.
- Unit-test the formatter at the boundaries: `59.999 → 59.99`, `60.000 → 1:00.00`,
  `599.999 → 9:59.99`, `0.009 → 0.00`, and rounding vs truncation for aoN.

## Stats (`stats.rs`, pure functions, unit-tested)
- `aoN`: drop the best 5% and worst 5% of solves (at least one of each) and take
  the mean of the rest.
  - A DNF counts as the worst possible solve.
  - If there are more DNFs than solves dropped, the average is DNF.
- `mo3` is a plain mean.
- Represent a result as `enum TimeResult { Ms(u64), Dnf }` with `Ord`, so sorting
  just works.
- Track the session best, best ao5 and best ao12, the mean, and the solve count.

## Scrambles (`scramble.rs`)
WCA-style random-move 3x3 scramble:
- 20 moves, using the faces URFDLB with the modifiers `'` and `2`.
- Never repeat a face twice in a row, and avoid sequences like `R L R` (the same
  axis three times).
- Generate the next scramble as soon as a solve is saved.

## Keys
| key | action |
|---|---|
| space | inspect, then hold and release to start, then stop |
| `+` / `d` / `o` | mark the last solve +2, DNF, or OK |
| `Backspace` | delete the last solve (asks for confirmation) |
| `i` | toggle inspection |
| `n` / `[` `]` | new session, previous or next session |
| `q` / `Ctrl-C` | quit (`Ctrl-C` arrives as a key event in raw mode, so handle it) |

## Persistence (`store.rs`)
- Store sessions in `dirs::data_dir()/cubetimer/sessions.json`.
- Each solve is `{ms, penalty, scramble, ts}`, with the penalty serialized as
  `"ok" | "plus2" | "dnf"`.
- After every solve, write to a temp file and rename it (an atomic write).

## Crate layout
```
Cargo.toml        # ratatui, serde(derive), serde_json, dirs, rand
src/
  main.rs         # init terminal + guard, run app, restore
  term.rs         # enhancement detect/push/pop, Drop guard, panic hook
  app.rs          # App struct, State machine, key handling, tick
  ui.rs           # ratatui draw(): layout, big time, stats, recent list
  fmt.rs          # time formatting (+ tests)
  font.rs         # big glyphs + render
  stats.rs        # aoN, mean, best (+ #[cfg(test)] tests)
  scramble.rs     # (+ tests: no same-face repeats, correct length)
  store.rs        # load/save sessions
examples/keytest.rs  # prints Press/Repeat/Release events — step-1 sanity check
```

## Build order
1. `cargo init`, `term.rs` and `examples/keytest.rs`. Run them in kitty to confirm
   space Press and Release events arrive.
2. `fmt.rs`, `font.rs` and `stats.rs`, with unit tests (`cargo test`).
3. `app.rs` and `ui.rs`: the state machine and the layout.
4. `store.rs`, the scramble generator, and the penalty keys.
5. Polish:
   - colors and the PB flash
   - the small-terminal fallback
   - a release build (`cargo install --path .`)
   - an optional kitty launcher (`kitty --start-as=fullscreen cubetimer`) and a
     Hyprland keybind

## Token-saving tips for the build session
- Use `cargo build -q` and `cargo clippy -q --message-format short` to keep the
  compiler output short.
- Use `cargo test -q` for tests.
- Don't paste full `cargo` output back unless an error needs it.

## Out of scope for v1
- Other puzzles
- Bluetooth smart cubes
- Graphs
- kitty's OSC 66 text-sizing protocol (worth trying later as an alternative to
  ASCII digits)
