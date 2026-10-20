# Changelog

## Unreleased

- 2x2, 4x4, 5x5, 6x6 and 7x7 next to the 3x3: `e` picks the puzzle, and every puzzle has
  its own sessions, stats and scrambles. Files saved by 0.1.0 open unchanged as 3x3.
- 2x2 scrambles are random-state (at least 4 moves, as the WCA requires); 4x4 to 7x7 use the
  WCA's random-move lengths and move sets.
- The scramble preview works at every size.
- Long scrambles (up to 100 moves) wrap across as many lines as they need.
- `--export` has a puzzle column.
- Statistics screen (`s`): a time chart with rolling ao5 and ao12, personal bests, DNFs and
  slow solves marked, a range of the last 50, 100, 500 or all solves, a histogram and a
  summary with standard deviation and ao50 and ao1000.
- The chart is drawn so that axis labels and gridlines line up exactly, uses round axis
  values, and a few terrible solves no longer flatten the rest. The history browser uses it.
- A 3D view of the scrambled cube (`v`, also from the history): drag it or use the arrows
  to turn it, and play the scramble move by move with animated turns. Drawn as an image in
  kitty, Ghostty, foot and WezTerm, with half blocks elsewhere.

## 0.1.0

First release.

- Full-screen timer with big digits in seven fonts, sized to the window by default.
- WCA inspection: 15 s countdown, 8 s and 12 s warnings, +2 from 15 s and DNF from 17 s.
- Hold-and-release start through the kitty keyboard protocol, with a basic mode for
  terminals without it. Tested in kitty, foot, Alacritty, Ghostty and WezTerm.
- WCA-style random-state scrambles (uniform random state, two-phase solver, at most 21
  moves) with a colored preview of the scrambled cube.
- Stats: best, mean, mo3, ao5, ao12, ao100, best ao5 and best ao12, with WCA rounding.
- History browser with a trend chart; mark or delete any solve.
- Sessions, saved atomically after every change; `--export` to CSV.
