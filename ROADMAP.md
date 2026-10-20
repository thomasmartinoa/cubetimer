# Roadmap

What comes after 0.1.0, in phases. Each phase is built on its own branch, merged into
`dev` once it passes every check below, and `dev` is merged into `main` only for a
release.

## How work flows

```
main      ── releases only (v0.1.0, v0.2.0, ...) ─────────────────────────●──
dev       ── integration ──●──────────●──────────●──────────●──────────╯
feat/...  ─────────────────╯ feat/puzzles ╯ feat/stats ╯ feat/3d ╯ ...
```

- Start each phase from the latest `dev`: `git switch dev && git pull && git switch -c feat/<name>`.
- Commit in small steps on the feature branch. CI runs on every push to `feat/**`, `dev`
  and `main`.
- Merge a finished feature into `dev` with a pull request (or `git merge --no-ff`) after
  the merge checklist passes. Never commit straight to `main`.
- A release is a pull request from `dev` to `main`, then a tag and the AUR update.
- Commit messages and pull requests are plain descriptions of the change.

### Merge checklist (every feature branch into `dev`)

1. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.
2. `python3 scripts/pty_check.py target/release/cubetimer` (also runs in CI).
3. If input or drawing changed: `scripts/terminal_smoke.sh` with the desktop left alone
   (kitty, Alacritty, foot, Ghostty, WezTerm with real key events).
4. Screenshots of every new or changed screen, looked at, at a big and a small window size.
5. README and the `Unreleased` section of CHANGELOG.md updated.
6. CI green on the branch.

## Where cubetimer stands

Other terminal cube timers: [lazytimer](https://github.com/xyzyx4546/lazytimer) (Rust;
2x2 to 5x5, Skewb, Pyraminx, a stats graph), [cube-tui](https://docs.rs/crate/cube-tui)
(Rust; sessions, a web dashboard, a GAN Bluetooth timer),
[TuiTimerX](https://github.com/llyram/TuiTimerX) (Python, early) and
[sctt](https://pypi.org/project/sctt) (Python, needs root for key events).

cubetimer already leads on WCA accuracy (exact inspection penalties, truncation and
rounding, TNoodle-method random-state scrambles), hold-to-start that works the same in
five terminals, and the scramble preview. It is behind on puzzle choice and statistics.
None of the timers above has a 3D cube or a CFOP algorithm trainer, so those would make
cubetimer stand out.

## Phase 1: more cubes (2x2, 4x4 to 7x7) — `feat/puzzles` — done (merged into dev)

**Goal:** time any cube from 2x2 to 7x7, each with its own sessions and the scramble type
the WCA uses for it.

**Data model**

- `src/puzzle.rs`: `enum Puzzle { Cube2, Cube3, Cube4, Cube5, Cube6, Cube7 }` with WCA
  event ids (`222`, `333`, `444`, `555`, `666`, `777`) and display names.
- `Session` gains `puzzle: String`, `#[serde(default = "333")]`, so 0.1.0 files load
  unchanged as 3x3 sessions.
- Session keys work within the current puzzle. `e` opens a puzzle picker; choosing a
  puzzle goes to its most recent session, creating one if there is none.

**Cube model for any size**

- `src/nxn.rs`: a sticker-level model of an N×N×N cube (`[[u8; N*N]; 6]`), moves with a
  depth (`R`, `Rw` = 2 layers, `3Rw` = 3 layers) and `'`/`2`, parsing and printing in
  WCA notation.
- Used for scrambling big cubes and for drawing previews of any size. The 3x3 solver keeps
  using `cube.rs`.

**Scrambles** (WCA Regulation 4b3 and what TNoodle does)

| Puzzle | Method | Detail |
|---|---|---|
| 2x2 | Random state | Distance table over all 3,674,160 states (moves U, R, F with the DBL corner fixed). Reject states under 4 moves (Reg. 4b3b). The scramble is the inverse of an optimal solution, at most 11 moves. |
| 4x4 | Random moves (first) | 40 moves from U D L R F B plus Uw Rw Fw, TNoodle's move set. TNoodle uses random state for 4x4; that needs a three-phase reduction solver and is a stretch goal (Phase 6). Label it honestly in the README. |
| 5x5 | Random moves | 60 moves, outer and wide turns on all faces (this is the WCA method). |
| 6x6 | Random moves | 80 moves, adds 3Rw-type turns on R, U, F. |
| 7x7 | Random moves | 100 moves, 3Rw-type turns on all faces. |

Random-move rules: never the same face and depth twice in a row, and a fixed order for
moves on one axis so no sequence cancels or commutes into a shorter one.

**Interface**

- The scramble box wraps long scrambles (100 moves on 7x7 is about 5 lines).
- Preview: the net for 2x2 to 4x4 fits in the bottom panel. For 5x5 to 7x7, `v` opens a
  full-screen preview (Phase 3 replaces it with the 3D view).
- The footer and stats title show the puzzle: `stats - 4x4 - session 2/3`.

**Tests**

- `nxn` with N=3 gives exactly the same stickers as `cube.rs` for 1,000 random sequences.
- Orders: `R`, `Rw`, `3Rw` have order 4 on every size; `R U` has order 105 on 3x3; on 4x4,
  `Rw` moves the right two layers and nothing else.
- 2x2 distance table: the number of states at each distance matches the known counts
  (1, 9, 54, 321, 1847, 9992, 50136, 227536, 870072, 1887748, 623800, 2644).
- Every scramble applied to a solved cube gives the chosen state (2x2) or a valid move
  sequence of the right length and move set (4x4 to 7x7).
- A 0.1.0 `sessions.json` loads with every session as 3x3.

**Done when:** all six cubes can be timed with correct scrambles and previews, old data
loads, and the merge checklist passes. Suggested model: Sonnet (clear spec).

## Phase 2: statistics screen and better graphs — `feat/stats` — done (merged into dev)

**Goal:** a proper statistics view; the current trend chart is small, spiky and hard to
read.

**Problems with the current chart:** one outlier stretches the y-axis and flattens
everything else; there are no gridlines or tick values between min and max; DNFs and
personal bests aren't shown; it only fits in the history panel.

**Stats screen** (`s`, full screen)

- **Main chart:** one dot per solve, ao5 and ao12 lines, personal-best singles marked,
  DNFs as red ticks along the top. Y ticks at round values (1-2-5 steps: every 0.5 s, 1 s,
  2 s, 5 s) with faint gridlines. Y range from the fastest time to the 98th percentile,
  with slower outliers drawn as a marker on the top edge. A range selector for the last
  50, 100, 500 or all solves.
- **Distribution:** a histogram of times in automatic buckets (0.5 s or 1 s), as
  horizontal bars with counts.
- **Summary:** solve count with +2 and DNF counts, best and worst single, mean, standard
  deviation, current and best ao5, ao12, ao50, ao100 and ao1000.
- The small trend chart in the history view reuses the same chart code and fixes.

**Tests:** tick generation (round values, sensible counts for any range), percentiles,
histogram buckets, ao50/ao1000, rendering at large and small sizes with outliers, all
DNFs, one solve, identical times.

**Done when:** a 500-solve session with outliers reads clearly at 80x24 and fullscreen.
Suggested model: Sonnet.

## Phase 3: 3D cube view — `feat/3d` — done (merged into dev)

**Is it practical?** Yes. Terminals can show real images: kitty, Ghostty and WezTerm
through the kitty graphics protocol, foot through sixel. The
[ratatui-image](https://docs.rs/ratatui-image) crate picks the right protocol and falls
back to half-block characters, so Alacritty gets a coarser version rather than nothing.

**Plan**

1. **Spike first (decide before building):** render one static cube image and show it with
   ratatui-image in all five terminals. Measure the binary size and build-time cost.
   Decide whether the 3D view is a default feature or a cargo feature.
2. `src/render3d.rs`: a small software renderer. The cube is made of N³ cubies and each
   sticker is a quad, with a perspective camera, sorting back to front (cubies are
   convex, so this is enough), and light shading by face direction. The output is an RGBA
   buffer.
3. **View:** `v` toggles the 3D view of the current scramble in place of the flat preview.
   Arrow keys rotate it, and auto-spin can be turned on or off.
4. **Move animation:** a `Player` that animates a move sequence, turning each layer over
   about 150 ms, with play, pause, step and speed controls. Phase 4 uses it.
5. Render on a background thread (ratatui-image's thread protocol) so the timer never
   stutters, and draw nothing while the cube is still.

**Tests:** projection maths; for a solved cube seen from the front, the center pixels of
each visible face have the right colors; after `R` the right column of the top face is
green; the fallback renders without panicking at any size.

**Done when:** the 3D view shows the right state for every size from 2x2 to 7x7 in all five
terminals (images in four, half-blocks in Alacritty) and costs no timer accuracy. Suggested
model: Opus for the spike and renderer, Sonnet for the view and controls.

## Phase 4: CFOP algorithms and trainer — `feat/algs`

**Goal:** look up any CFOP algorithm you forgot, watch it performed on the 3D cube, and
drill the cases you are learning.

**Library**

- Data files embedded in the binary: `algs/pll.toml` (21 cases), `algs/oll.toml` (57),
  `algs/f2l.toml` (41), plus the 2-look OLL and PLL subsets for beginners. Each case has an
  id, a common name (`T-perm`, `OLL 21 (H)`), a group, a main algorithm and alternatives.
- **Every algorithm is checked by the test suite against the cube model**, so a typo can't
  ship:
  - PLL: undoing the algorithm from solved leaves F2L solved and the last layer oriented;
    the algorithm solves that state (up to a final U turn); the 21 cases are all different
    and together cover every PLL.
  - OLL: likewise, with all 57 orientation cases distinct and covering every OLL.
  - F2L: the case leaves the cross and other three slots solved; the 41 cases cover every
    case for one slot.

**Screens** (`a` opens it)

- Tabs for PLL, OLL, F2L and the 2-look subsets, with a grid of cases. Each case shows a
  small top-view diagram (the last-layer pattern with side stickers).
- Selecting a case shows its algorithms, a large diagram and the 3D cube playing the
  algorithm (Phase 3) with play, pause and step.
- Mark each case as not started, learning or learned; this is saved.

**Trainer**

- Choose cases (for example all PLLs, or only the ones marked learning). The timer then
  gives scrambles that each end in a random chosen case with a random U turn. Scrambles
  come from the two-phase solver, so they look like normal scrambles and can't be
  recognised by their ending.
- Times are kept per case, so you can see your slowest cases.

**Done when:** every algorithm is verified, browsing and playback work at 80x24, and the
trainer's scrambles always produce the chosen case. Suggested model: Sonnet (the tests
catch wrong algorithms; switch to Opus if the case-coverage checks are hard to satisfy).

## Phase 5: everyday improvements — `feat/qol`

- **Settings screen** (`,`): hold time (0 to 1 s, now fixed at 0.3 s), inspection per
  puzzle, 0.01 or 0.001 precision, the bell, and the font.
- **Session names** (`r` to rename) and **solve comments** (`c` in history).
- **Average details:** an ao5 or ao12 written out with the dropped times in brackets,
  ready to copy (`(12.10) 13.20 14.00 (16.43) 13.80 = 13.67`).
- **Copy to clipboard** with OSC 52 (`y` copies the scramble or a selected average).
- **Import from csTimer:** `cubetimer --import export.txt`, with a summary and a dry-run
  option. The csTimer export format must be checked against a real export file, which
  then becomes a test fixture.
- **Multi-phase timing** (optional): split a solve into phases (for example cross, F2L,
  OLL, PLL) by pressing space at each step, with stats per phase.

Suggested model: Sonnet.

## Phase 6: other WCA events (stretch) — one branch each, `feat/event-<name>`

| Event | Scramble (WCA) | Work |
|---|---|---|
| Pyraminx | Random state, at least 6 moves, plus random tips | Small state space: distance table |
| Skewb | Random state, at least 7 moves | Small state space: distance table |
| Megaminx | Random moves (Pochmann style, 7 lines) | Generator and a net preview |
| Clock | Random pins and dials | Generator and a dial preview |
| Square-1 | Random state, at least 11 moves | Hardest: needs a two-phase Square-1 solver |
| 4x4 random state | Like TNoodle | A three-phase reduction solver |

Suggested model: Sonnet for Pyraminx, Skewb, Megaminx and Clock; Opus for Square-1 and
4x4 random state.

## Phase 7: release 0.2.0 — pull request `dev` → `main`

1. Full merge checklist on `dev`, including `terminal_smoke.sh` and new screenshots.
2. Bump the version in Cargo.toml and PKGBUILD; move `Unreleased` in CHANGELOG to `0.2.0`.
3. README: screenshots and sections for every new screen.
4. `makepkg` plus `namcap` on the exact tree, then merge, tag `v0.2.0`, push the tag,
   and update the AUR package (`pkgver`, `updpkgsums`, `.SRCINFO`).

## Not planned

- Bluetooth smart cubes and timers: a large separate project (BLE on Linux, a protocol for
  each brand).
- A Stackmat connected through audio input: needs signal decoding and real hardware to test.
