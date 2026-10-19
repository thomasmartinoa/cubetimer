# Changelog

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
