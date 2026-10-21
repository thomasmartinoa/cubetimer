//! Rendering. Colors are ANSI only, so the timer follows the terminal theme.

use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph, Wrap};

use crate::algview;
use crate::app::{App, Tone};
use crate::fmt;
use crate::font;
use crate::nxn;
use crate::stats::{Penalty, TimeResult};
use crate::statsview;
use crate::view3d;

fn tone_color(tone: Tone) -> Color {
    match tone {
        Tone::Idle => Color::White,
        Tone::Inspect => Color::Yellow,
        Tone::Warn8 => Color::LightRed,
        Tone::Warn12 | Tone::HoldWait => Color::Red,
        Tone::HoldReady => Color::Green,
        Tone::Running => Color::Cyan,
        Tone::Pb => Color::Magenta,
        Tone::PbAlt => Color::LightMagenta,
    }
}

/// Full layout needs room for the boxed panels; below it the layout shrinks
/// to a few text lines, and below that to just the time.
const FULL_MIN: (u16, u16) = (60, 20);
const COMPACT_MIN_HEIGHT: u16 = 8;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if let Some(view) = &app.cube_view {
        view3d::draw(frame, area, view, &app.gfx);
        return;
    }
    if let Some(browser) = &app.algs {
        algview::draw(frame, area, browser, &app.data, app.look, &app.gfx);
        return;
    }
    if let Some(view) = &app.stats {
        statsview::draw(frame, area, app, view);
        return;
    }
    if area.width >= FULL_MIN.0 && area.height >= FULL_MIN.1 {
        // As many lines as the scramble needs once wrapped (100 moves on 7x7).
        let scramble_lines = wrapped_lines(&app.scramble, (area.width as usize).saturating_sub(4));
        let [scramble, main, stats, footer] = Layout::vertical([
            Constraint::Length(scramble_lines + 2),
            Constraint::Min(0),
            Constraint::Length(8),
            Constraint::Length(1),
        ])
        .areas(area);
        draw_scramble(frame, scramble, app);
        draw_main(frame, main, app);
        draw_stats(frame, stats, app);
        draw_footer(frame, footer, app);
    } else if area.height >= COMPACT_MIN_HEIGHT {
        let scramble_h = wrapped_lines(&app.scramble, area.width as usize);
        let [scramble, main, stats, footer] = Layout::vertical([
            Constraint::Length(scramble_h),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(scramble_text(app))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
            scramble,
        );
        draw_main(frame, main, app);
        draw_stats_line(frame, stats, app);
        draw_footer(frame, footer, app);
    } else {
        draw_main(frame, area, app);
    }
    if app.show_help {
        draw_help(frame, area);
    }
    if let Some(selected) = app.picker {
        draw_picker(frame, area, app, selected);
    }
    if let Some(row) = app.settings_row {
        draw_settings(frame, area, app, row);
    }
    if let Some(input) = &app.input {
        draw_input(frame, area, input);
    }
}

/// The settings screen: one row per setting, left and right change it.
fn draw_settings(frame: &mut Frame, area: Rect, app: &App, selected: usize) {
    let on = |b: bool| if b { "on" } else { "off" };
    let rows: [(&str, String); App::SETTINGS_ROWS] = [
        ("hold time", format!("{} ms", app.hold.as_millis())),
        ("inspection", on(app.inspection_on).into()),
        (
            "precision",
            match app.precision {
                fmt::Precision::Centi => "0.01".into(),
                fmt::Precision::Milli => "0.001".into(),
            },
        ),
        ("inspection bell", on(app.bell).into()),
        ("digit font", font::name_at(app.font).into()),
        ("3D look", app.look.name().into()),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .enumerate()
        .map(|(i, (name, value))| {
            let style = if i == selected {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            };
            Line::from(Span::styled(format!(" {name:<16}  < {value:^8} > "), style))
        })
        .collect();
    let width = 40.min(area.width);
    let height = (lines.len() as u16 + 4).min(area.height);
    let [popup] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [popup] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" settings ")
                .title_bottom(Line::from(" up/down pick, left/right change ").right_aligned())
                .padding(Padding::new(0, 0, 1, 0)),
        ),
        popup,
    );
}

/// The line being typed (a session name or a comment), along the bottom.
fn draw_input(frame: &mut Frame, area: Rect, input: &crate::app::TextInput) {
    let label = match input.kind {
        crate::app::InputKind::SessionName => "session name",
        crate::app::InputKind::Comment(_) => "comment",
    };
    let [_, line] = Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(area);
    frame.render_widget(Clear, line);
    frame.render_widget(
        Paragraph::new(format!("{}\u{2588}", input.text)).block(
            Block::bordered().title(format!(" {label} ")).title_bottom(
                Line::from(" enter saves, esc cancels, empty clears ").right_aligned(),
            ),
        ),
        line,
    );
}

/// The big time, or the history browser when it is open.
fn draw_main(frame: &mut Frame, area: Rect, app: &App) {
    match app.history {
        Some(selected) => draw_history(frame, area, app, selected),
        None => draw_time(frame, area, app),
    }
}

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// Lines `text` takes when wrapped at word boundaries to `width` columns,
/// between 1 and `MAX_SCRAMBLE_LINES`.
fn wrapped_lines(text: &str, width: usize) -> u16 {
    let width = width.max(1);
    let (mut lines, mut used) = (1u16, 0usize);
    for word in text.split_whitespace() {
        let len = word.chars().count();
        if used == 0 {
            used = len;
        } else if used + 1 + len <= width {
            used += 1 + len;
        } else {
            lines += 1;
            used = len;
        }
    }
    lines.min(MAX_SCRAMBLE_LINES)
}

/// A 7x7 scramble of 100 moves needs about four lines in a normal window.
const MAX_SCRAMBLE_LINES: u16 = 8;

/// The scramble, or a placeholder while the first one is being made.
fn scramble_text(app: &App) -> Span<'_> {
    if app.scramble.is_empty() {
        Span::styled("generating scramble...", dim())
    } else {
        Span::raw(app.scramble.as_str())
    }
}

fn draw_scramble(frame: &mut Frame, area: Rect, app: &App) {
    let p = Paragraph::new(scramble_text(app))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title(" scramble ").border_style(dim()));
    frame.render_widget(p, area);
}

fn draw_time(frame: &mut Frame, area: Rect, app: &App) {
    let view = app.view(Instant::now());
    let color = tone_color(view.tone);

    // The font the user picked if it fits, else the next smaller one that does.
    let font = font::pick(
        &view.text,
        area.width as usize,
        area.height as usize,
        app.font,
    );
    let lines = font.render(&view.text);

    // The hint only gets the rows left over after the digits.
    let hint_h = area.height.saturating_sub(lines.len() as u16).min(2);
    let [time, hint] = Layout::vertical([
        Constraint::Length(lines.len() as u16),
        Constraint::Length(hint_h),
    ])
    .flex(Flex::Center)
    .areas(area);
    let style = Style::new().fg(color).add_modifier(Modifier::BOLD);
    frame.render_widget(
        Paragraph::new(lines)
            .style(style)
            .alignment(Alignment::Center),
        time,
    );
    frame.render_widget(
        Paragraph::new(vec![Line::default(), Line::from(view.hint)])
            .style(Style::new().fg(color))
            .alignment(Alignment::Center),
        hint,
    );
}

/// Width of the scramble preview box: a 15-column cube net plus borders and
/// a column of padding each side.
const PREVIEW_WIDTH: u16 = 19;
/// Inner width of the two-column stats grid.
const STATS_GRID: usize = 40;

fn draw_stats(frame: &mut Frame, area: Rect, app: &App) {
    let wide = area.width as usize >= STATS_GRID + 2 + PREVIEW_WIDTH as usize + 20;
    let (left, preview, right) = if wide {
        let [l, p, r] = Layout::horizontal([
            Constraint::Length(STATS_GRID as u16 + 2),
            Constraint::Length(PREVIEW_WIDTH),
            Constraint::Min(0),
        ])
        .areas(area);
        (l, Some(p), r)
    } else {
        let [l, r] = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .areas(area);
        (l, None, r)
    };
    draw_stats_grid(frame, left, app);
    if let Some(p) = preview {
        draw_preview(frame, p, &app.scramble, app.puzzle().size(), "preview");
    }
    draw_recent(frame, right, app);
}

/// This session's place among the sessions of the same puzzle: (2, 3) is the
/// second of three.
fn session_position(app: &App) -> (usize, usize) {
    let of_puzzle = app.data.sessions_of(app.puzzle());
    let at = of_puzzle
        .iter()
        .position(|&i| i == app.data.current)
        .unwrap_or(0);
    (at + 1, of_puzzle.len())
}

fn draw_stats_grid(frame: &mut Frame, area: Rect, app: &App) {
    let prec = app.precision;
    let s = app.summary();
    // Left column first: with room for only one, these are the ones to keep.
    let left = [
        ("best", fmt::fmt_single(s.best, prec)),
        ("ao5", fmt::fmt_avg(s.ao5, prec)),
        ("ao12", fmt::fmt_avg(s.ao12, prec)),
        (
            "mean",
            s.mean.map_or("-".into(), |m| fmt::fmt_avg_ms(m, prec)),
        ),
        ("count", s.count.to_string()),
    ];
    let right = [
        ("mo3", fmt::fmt_avg(s.mo3, prec)),
        ("ao100", fmt::fmt_avg(s.ao100, prec)),
        ("best ao5", fmt::fmt_avg(s.best_ao5, prec)),
        ("best ao12", fmt::fmt_avg(s.best_ao12, prec)),
    ];
    let two_columns = area.width as usize >= STATS_GRID + 2;
    let cell = |(name, value): &(&'static str, String)| {
        vec![
            Span::styled(format!("{name:<10}"), dim()),
            Span::raw(format!("{value:<9}")),
        ]
    };
    let lines: Vec<Line> = left
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let mut spans = cell(item);
            if let Some(r) = right.get(i).filter(|_| two_columns) {
                spans.push(Span::raw("  "));
                spans.extend(cell(r));
            }
            Line::from(spans)
        })
        .collect();
    let title = match &app.drill {
        Some(d) => format!(" {} trainer - {} cases ", d.set.name(), d.cases.len()),
        None => {
            let (number, count) = session_position(app);
            let name = match &app.data.sessions[app.data.current].name {
                Some(n) => format!(" \"{n}\""),
                None => String::new(),
            };
            format!(
                " stats - {} - session {number}/{count}{name} ",
                app.puzzle().name()
            )
        }
    };
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(title).border_style(dim())),
        area,
    );
}

/// WCA color scheme, U R F D L B. Real cube colors rather than the terminal
/// theme's, since they have to match the cube in your hands.
pub const STICKER_COLORS: [Color; 7] = [
    Color::Rgb(245, 245, 245),
    Color::Rgb(214, 40, 40),
    Color::Rgb(0, 168, 84),
    Color::Rgb(255, 213, 0),
    Color::Rgb(255, 128, 0),
    Color::Rgb(0, 82, 204),
    Color::Rgb(92, 92, 92),
];

/// The scrambled cube as a net (U on top, L F R B across, D below), each
/// sticker a square half a character tall.
fn draw_preview(frame: &mut Frame, area: Rect, scramble: &str, size: usize, title: &str) {
    let block = Block::bordered()
        .title(format!(" {title} "))
        .border_style(dim());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // nothing to show yet (or a solve saved without one)
    let Some(grid) = net_grid(scramble, size) else {
        return;
    };
    let (rows, cols) = (net_rows(&grid), grid[0].len());
    if inner.height as usize >= rows && inner.width as usize >= cols {
        let [net] = Layout::vertical([Constraint::Length(rows as u16)])
            .flex(Flex::Center)
            .areas(inner);
        frame.render_widget(
            Paragraph::new(net_lines(&grid, 1)).alignment(Alignment::Center),
            net,
        );
    } else {
        // too big for this box: point at the full-screen view
        frame.render_widget(
            Paragraph::new("v: full view")
                .style(dim())
                .alignment(Alignment::Center),
            inner,
        );
    }
}

/// The net of the cube after `scramble` as a grid of sticker colors (`None`
/// for the gaps), `3 * size + 2` half-rows by `4 * size + 3` columns.
/// `None` if there is no scramble or it can't be read.
fn net_grid(scramble: &str, size: usize) -> Option<Vec<Vec<Option<Color>>>> {
    if scramble.is_empty() {
        return None;
    }
    let moves = nxn::parse(scramble)?;
    let mut cube = nxn::Nxn::solved(size);
    cube.apply_all(&moves);
    Some(net_grid_of(&cube))
}

/// The net of `cube` (see `net_grid`).
pub fn net_grid_of(cube: &nxn::Nxn) -> Vec<Vec<Option<Color>>> {
    let size = cube.size();
    let stickers = cube.facelets();
    let pitch = size + 1;
    (0..3 * pitch - 1)
        .map(|row| {
            (0..4 * pitch - 1)
                .map(|col| {
                    let (band, r) = (row / pitch, row % pitch);
                    let (slot, c) = (col / pitch, col % pitch);
                    if r == size || c == size {
                        return None;
                    }
                    let face = match (band, slot) {
                        (0, 1) => 0, // U
                        (1, 0) => 4, // L
                        (1, 1) => 2, // F
                        (1, 2) => 1, // R
                        (1, 3) => 5, // B
                        (2, 1) => 3, // D
                        _ => return None,
                    };
                    Some(STICKER_COLORS[stickers[face * size * size + r * size + c] as usize])
                })
                .collect()
        })
        .collect()
}

/// Text rows the net takes (two half-rows each).
pub fn net_rows(grid: &[Vec<Option<Color>>]) -> usize {
    grid.len().div_ceil(2)
}

/// The net as lines of half blocks, every sticker `scale` columns wide and
/// `scale` half-rows tall. From scale 3 up, each sticker leaves its last
/// column and half-row empty, so neighbours of the same color stay apart.
pub fn net_lines(grid: &[Vec<Option<Color>>], scale: usize) -> Vec<Line<'static>> {
    let (rows, cols) = (grid.len() * scale, grid[0].len() * scale);
    let gap = scale >= 3;
    let cell = |row: usize, col: usize| -> Option<Color> {
        if row >= rows || (gap && (row % scale == scale - 1 || col % scale == scale - 1)) {
            return None;
        }
        grid[row / scale][col / scale]
    };
    (0..rows.div_ceil(2))
        .map(|y| {
            let spans: Vec<Span> = (0..cols)
                .map(|x| match (cell(2 * y, x), cell(2 * y + 1, x)) {
                    (Some(top), Some(bottom)) => {
                        Span::styled("\u{2580}", Style::new().fg(top).bg(bottom))
                    }
                    (Some(top), None) => Span::styled("\u{2580}", Style::new().fg(top)),
                    (None, Some(bottom)) => Span::styled("\u{2584}", Style::new().fg(bottom)),
                    (None, None) => Span::raw(" "),
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// The puzzle picker (`e`).
fn draw_picker(frame: &mut Frame, area: Rect, app: &App, selected: usize) {
    let current = app.puzzle();
    let lines: Vec<Line> = crate::puzzle::ALL
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mark = if *p == current { "*" } else { " " };
            let style = if i == selected {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                Style::new()
            };
            let used = app
                .data
                .sessions_of(*p)
                .iter()
                .map(|&s| app.data.sessions[s].solves.len())
                .sum::<usize>();
            Line::from(Span::styled(
                format!(" {mark} {}  {used:>5} solves ", p.name()),
                style,
            ))
        })
        .collect();
    let height = (lines.len() as u16 + 4).min(area.height);
    let width = 28.min(area.width);
    let [popup] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [popup] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(popup);
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" puzzle ")
                .title_bottom(Line::from(" enter picks, 2-7 jump ").right_aligned())
                .padding(Padding::vertical(1)),
        ),
        popup,
    );
}

/// Recent solves, newest first, in aligned columns. +2 and DNF are colored and
/// the session best is highlighted.
fn draw_recent(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered()
        .title(" recent times ")
        .border_style(dim());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let prec = app.precision;
    let solves = app.solves();
    let best = app.summary().best;
    let num_w = solves.len().to_string().len() + 1;
    let entries: Vec<(usize, String)> = solves
        .iter()
        .enumerate()
        .rev()
        .map(|(i, s)| (i, fmt::fmt_solve_list(s.ms, s.penalty, prec)))
        .collect();
    let time_w = entries.iter().map(|(_, t)| t.len()).max().unwrap_or(0);
    let cell_w = num_w + 1 + time_w + 3;
    let per_row = (inner.width as usize / cell_w.max(1)).max(1);
    let lines: Vec<Line> = entries
        .chunks(per_row)
        .take(inner.height as usize)
        .map(|row| {
            let mut spans = Vec::new();
            for (i, text) in row {
                let solve = &solves[*i];
                let style = penalty_style(solve, best);
                spans.push(Span::styled(
                    format!("{:>num_w$} ", format!("{}.", i + 1)),
                    dim(),
                ));
                spans.push(Span::styled(format!("{text:<time_w$}"), style));
                spans.push(Span::raw("   "));
            }
            Line::from(spans)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

/// One-line stats for the compact layout.
fn draw_stats_line(frame: &mut Frame, area: Rect, app: &App) {
    let prec = app.precision;
    let s = app.summary();
    let (number, count) = session_position(app);
    let label = match &app.drill {
        Some(d) => format!("{} trainer", d.set.name()),
        None => app.puzzle().name().to_string(),
    };
    let text = format!(
        "{} best {}  ao5 {}  ao12 {}  mean {}  n={}  s{number}/{count}",
        label,
        fmt::fmt_single(s.best, prec),
        fmt::fmt_avg(s.ao5, prec),
        fmt::fmt_avg(s.ao12, prec),
        s.mean.map_or("-".into(), |m| fmt::fmt_avg_ms(m, prec)),
        s.count,
    );
    frame.render_widget(
        Paragraph::new(text)
            .style(dim())
            .alignment(Alignment::Center),
        area,
    );
}

/// The key hints, most important first when the window is too narrow for all
/// of them. Returns the text to show, in display order.
fn footer_text(app: &App, width: usize) -> String {
    let on = |b: bool| if b { "on" } else { "off" };
    // (priority: lower stays longer, text), in display order
    let parts: [(u8, String); 15] = [
        (1, "space go".into()),
        (6, "+/d/o mark".into()),
        (8, "bksp delete".into()),
        (7, "h history".into()),
        (7, "s stats".into()),
        (7, "a algs".into()),
        (9, ", settings".into()),
        (9, "n [ ] session".into()),
        (5, format!("i inspect:{}", on(app.inspection_on))),
        (4, format!("f font:{}", font::name_at(app.font))),
        if app.drill.is_some() {
            (1, "t stop training".into())
        } else {
            (4, format!("e puzzle:{}", app.puzzle().name()))
        },
        (10, "p display".into()),
        (11, format!("b bell:{}", on(app.bell))),
        (3, "? help".into()),
        (2, "q quit".into()),
    ];
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by_key(|&i| parts[i].0);
    let mut keep = [false; 15];
    let mut used = 1;
    for i in order {
        let need = parts[i].1.chars().count() + 2;
        if used + need > width {
            break;
        }
        keep[i] = true;
        used += need;
    }
    let kept: Vec<&str> = parts
        .iter()
        .zip(keep)
        .filter(|(_, k)| *k)
        .map(|((_, t), _)| t.as_str())
        .collect();
    format!(" {}", kept.join("  "))
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let tag = if app.enhanced { "" } else { "  | basic mode" };
    let status = app
        .status
        .as_ref()
        .map(|s| format!("  | {s}"))
        .unwrap_or_default();
    let reserved = tag.chars().count() + status.chars().count();
    let keys = footer_text(app, (area.width as usize).saturating_sub(reserved));
    let mut spans = vec![Span::styled(format!("{keys}{tag}"), dim())];
    if !status.is_empty() {
        spans.push(Span::styled(status, Style::new().fg(Color::LightRed)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Every key, for the `?` overlay.
const HELP: [(&str, &str); 27] = [
    ("space", "inspect, hold and release to start, stop"),
    ("any key", "stop a running solve"),
    ("esc", "cancel inspection or a hold"),
    ("+ / d / o", "mark the last solve +2, DNF or OK"),
    ("backspace", "delete the last solve (asks y / n)"),
    ("h", "history: browse, mark and delete any solve"),
    ("s", "statistics: chart, distribution, summary"),
    ("a", "algorithms: PLL, OLL, F2L, with the 3D cube"),
    (",", "settings: hold time, precision, bell, font, 3D look"),
    ("r", "name the current session"),
    (
        "y",
        "copy the scramble (in history: the solve; in stats: ao5, Y ao12)",
    ),
    ("c", "in history: comment the selected solve"),
    ("e", "choose the puzzle: 2x2 to 7x7"),
    ("v", "the scrambled cube in 3D: turn it, watch the scramble"),
    ("n", "new session"),
    ("[ / ]", "previous / next session"),
    ("X", "delete the whole session (asks y / n)"),
    ("i", "inspection on / off"),
    ("f / F", "next / previous digit font"),
    ("p", "running display: 0.01, seconds, hidden"),
    ("b", "inspection bell on / off"),
    ("? / F1", "this help"),
    ("q / ctrl-c", "quit"),
    ("", ""),
    ("in stats", "r or 1-4 change the range, esc closes"),
    ("in history", "up/down j/k pgup/pgdn home/end move, v cube"),
    ("", "+ / d / o mark, bksp delete, esc close"),
];

fn draw_help(frame: &mut Frame, area: Rect) {
    let width = 62.min(area.width);
    let height = (HELP.len() as u16 + 4).min(area.height);
    let [popup] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [popup] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(popup);
    let lines: Vec<Line> = HELP
        .iter()
        .map(|(k, v)| {
            Line::from(vec![
                Span::styled(format!("{k:>12}  "), Style::new().fg(Color::Cyan)),
                Span::raw(*v),
            ])
        })
        .collect();
    frame.render_widget(Clear, popup);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(" keys ")
                .title_bottom(Line::from(" any key closes ").right_aligned())
                .padding(Padding::new(1, 1, 1, 0)),
        ),
        popup,
    );
}

/// The history browser: every solve of the session, newest first, with the
/// selected one's details and scramble.
fn draw_history(frame: &mut Frame, area: Rect, app: &App, selected: usize) {
    let solves = app.solves();
    let prec = app.precision;
    let wide = area.width >= 70;
    let [list_area, detail_area] = if wide {
        Layout::horizontal([Constraint::Length(28), Constraint::Min(0)]).areas(area)
    } else {
        [area, Rect::default()]
    };

    let block = Block::bordered()
        .title(format!(" history - {} solves ", solves.len()))
        .border_style(dim());
    let inner = block.inner(list_area);
    frame.render_widget(block, list_area);
    // newest first; scroll so the selection stays visible
    let position = solves.len().saturating_sub(1 + selected);
    let rows = inner.height.max(1) as usize;
    let offset = position.saturating_sub(rows - 1);
    let best = app.summary().best;
    let num_w = solves.len().to_string().len() + 1;
    let lines: Vec<Line> = solves
        .iter()
        .enumerate()
        .rev()
        .skip(offset)
        .take(rows)
        .map(|(i, s)| {
            let text = format!(
                " {:>num_w$} {}",
                format!("{}.", i + 1),
                fmt::fmt_solve_list(s.ms, s.penalty, prec)
            );
            let style = if i == selected {
                Style::new().add_modifier(Modifier::REVERSED)
            } else {
                penalty_style(s, best)
            };
            Line::from(Span::styled(
                format!("{text:<width$}", width = inner.width as usize),
                style,
            ))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);

    if !wide {
        return;
    }
    let Some(solve) = solves.get(selected) else {
        return;
    };
    let block = Block::bordered()
        .title(format!(" solve {} of {} ", selected + 1, solves.len()))
        .border_style(dim());
    let inner = block.inner(detail_area);
    frame.render_widget(block, detail_area);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let penalty = match solve.penalty {
        Penalty::Ok => "none",
        Penalty::Plus2 => "+2",
        Penalty::Dnf => "DNF",
    };
    let mut lines = vec![
        Line::from(vec![
            Span::styled("time      ", dim()),
            Span::styled(
                fmt::fmt_solve_list(solve.ms, solve.penalty, prec),
                penalty_style(solve, best),
            ),
        ]),
        Line::from(vec![Span::styled("penalty   ", dim()), Span::raw(penalty)]),
        Line::from(vec![
            Span::styled("when      ", dim()),
            Span::raw(fmt::fmt_ago(solve.ts, now)),
        ]),
        Line::default(),
        Line::from(Span::styled("scramble", dim())),
        Line::from(solve.scramble.clone()),
    ];
    if let Some(note) = &solve.comment {
        lines.push(Line::default());
        lines.push(Line::from(vec![
            Span::styled("comment   ", dim()),
            Span::styled(note.clone(), Style::new().fg(Color::Cyan)),
        ]));
    }
    if let Some(i) = app.confirm_delete {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("delete solve {}? y / n", i + 1),
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    let keys = "up/down move   + d o mark   c comment   y copy   bksp delete   v cube   esc close";
    let [content, help] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(inner);
    // The trend chart gets the right-hand side when there is room for it.
    let [info, chart] = if content.width >= 80 {
        Layout::horizontal([Constraint::Length(46), Constraint::Min(0)]).areas(content)
    } else {
        [content, Rect::default()]
    };
    let [text, preview] = Layout::vertical([Constraint::Min(0), Constraint::Length(8)]).areas(info);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text);
    if !solve.scramble.is_empty() && preview.width >= PREVIEW_WIDTH {
        let [p] = Layout::horizontal([Constraint::Length(PREVIEW_WIDTH)]).areas(preview);
        draw_preview(frame, p, &solve.scramble, app.puzzle().size(), "state");
    }
    if chart.width > 0 {
        draw_trend(frame, chart, app, selected);
    }
    frame.render_widget(Paragraph::new(keys).style(dim()), help);
}

/// The session's solves as a chart with the selected one marked (see
/// `statsview::draw_chart`).
fn draw_trend(frame: &mut Frame, area: Rect, app: &App, selected: usize) {
    statsview::draw_chart(
        frame,
        area,
        &app.results(),
        0,
        Some(selected),
        app.precision,
        "trend",
    );
}

/// +2 in yellow, DNF in red, the session best in bold green.
fn penalty_style(solve: &crate::store::Solve, best: Option<TimeResult>) -> Style {
    match solve.penalty {
        Penalty::Dnf => Style::new().fg(Color::Red),
        Penalty::Plus2 => Style::new().fg(Color::Yellow),
        Penalty::Ok if Some(solve.result()) == best => {
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
        }
        Penalty::Ok => Style::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::Range;
    use crate::stats::Penalty;
    use crate::statsview::StatsView;
    use crate::store::Solve;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(app: &App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn app() -> App {
        let mut app = App::new(true);
        for ms in [12_340, 11_000, 13_500] {
            app.data
                .solves_mut()
                .push(Solve::new(ms, Penalty::Ok, String::new()));
        }
        app
    }

    #[test]
    fn every_size_renders_without_panicking() {
        let app = app();
        for w in [1, 5, 10, 20, 40, 59, 60, 80, 120, 250] {
            for h in 1..=45 {
                render(&app, w, h);
            }
        }
    }

    #[test]
    fn full_layout_has_every_panel() {
        let out = render(&app(), 100, 30);
        for want in [
            "scramble",
            "stats - 3x3 - session 1/1",
            "recent times",
            "ao100",
            "q quit",
            "f font:auto",
        ] {
            assert!(out.contains(want), "missing {want:?}");
        }
    }

    #[test]
    fn recent_times_wrap_between_entries_not_inside_them() {
        let mut app = App::new(true);
        for i in 0..40 {
            app.data
                .solves_mut()
                .push(Solve::new(12_000 + i * 137, Penalty::Ok, String::new()));
        }
        for w in 60..=140 {
            let out = render(&app, w, 30);
            for row in out.lines().filter(|r| r.contains('\u{2502}')) {
                // text of the right-hand panel, between its borders
                let inner = row
                    .rsplit('\u{2502}')
                    .nth(1)
                    .unwrap_or("")
                    .trim_end_matches('\u{a0}')
                    .trim();
                let last = inner.rsplit(' ').next().unwrap_or("");
                let dangling = last
                    .strip_suffix('.')
                    .is_some_and(|n| n.parse::<u32>().is_ok());
                assert!(!dangling, "width {w}: row ends with a bare number: {row:?}");
            }
        }
    }

    #[test]
    fn every_font_is_drawn_when_it_fits() {
        for (i, f) in font::FONTS.iter().enumerate() {
            let mut app = app();
            app.font = i + 1;
            let out = render(&app, 140, 48);
            for row in f
                .rows("13.50")
                .iter()
                .map(|r| r.trim())
                .filter(|r| !r.is_empty())
            {
                assert!(out.contains(row), "{}: missing row {row:?}", f.name);
            }
            assert!(out.contains(&format!("f font:{}", f.name)), "{}", f.name);
        }
    }

    #[test]
    fn auto_font_grows_with_the_window() {
        let colossal = font::FONTS
            .iter()
            .find(|f| f.name == "colossal")
            .unwrap()
            .rows("13.50");
        let standard = font::FONTS
            .iter()
            .find(|f| f.name == "standard")
            .unwrap()
            .rows("13.50");
        let has = |out: &str, rows: &[String]| {
            rows.iter()
                .map(|r| r.trim())
                .filter(|r| !r.is_empty())
                .all(|r| out.contains(r))
        };
        let app = app(); // auto
        assert!(has(&render(&app, 120, 48), &colossal));
        // 24 rows leaves 13 for the time
        assert!(has(&render(&app, 120, 24), &standard));
        assert!(!has(&render(&app, 120, 24), &colossal));
    }

    #[test]
    fn a_too_big_font_steps_down_instead_of_overflowing() {
        let mut app = app();
        app.font = font::index_of("pixel-xl");
        // 30 rows leaves 19 for the time: xl (10 + hint) fits
        assert!(render(&app, 120, 30).contains('\u{2588}'));
        // 20 rows leaves 9: xl and colossal don't fit, so something shorter is used
        let out = render(&app, 120, 20);
        assert!(!out.contains("\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}\u{2588}"));
        assert!(out.contains("13.50") || out.contains("|_|"));
    }

    #[test]
    fn footer_drops_the_least_important_hints_first() {
        let app = app();
        let full = footer_text(&app, 300);
        for want in [
            "space go",
            "q quit",
            "f font:auto",
            "i inspect:on",
            "b bell:off",
            "n [ ] session",
        ] {
            assert!(full.contains(want), "{want}");
        }
        for width in [20, 30, 45, 60, 80, 100] {
            let text = footer_text(&app, width);
            assert!(text.chars().count() <= width, "{width}: {text:?}");
            assert!(text.contains("space go"), "{width}");
        }
        let narrow = footer_text(&app, 30);
        assert!(narrow.contains("q quit") && !narrow.contains("bell"));
        // a tiny width keeps nothing rather than overflowing
        assert!(footer_text(&app, 3).trim().is_empty());
    }

    #[test]
    fn footer_shows_basic_mode_and_status_without_losing_the_keys() {
        let mut app = app();
        app.enhanced = false;
        app.status = Some("save failed: disk full".into());
        let out = render(&app, 100, 30);
        assert!(out.contains("basic mode") && out.contains("save failed: disk full"));
        assert!(out.contains("space go") && out.contains("q quit"));
    }

    #[test]
    fn short_terminal_uses_compact_layout_and_small_font() {
        let out = render(&app(), 100, 12);
        assert!(!out.contains("recent times"));
        assert!(out.contains("best 11.00") && out.contains("n=3") && out.contains("q quit"));
        // time is 13.50 in the 3-row font: no colossal glyphs
        assert!(!out.contains("d88P"));
        assert!(out.contains("|_|") || out.contains("_|"));
    }

    #[test]
    fn narrow_terminal_falls_back_to_compact() {
        let out = render(&app(), 50, 30);
        assert!(!out.contains("recent times"));
        assert!(out.contains("n=3"));
    }

    #[test]
    fn tiny_terminal_shows_only_the_time() {
        let out = render(&app(), 30, 5);
        assert!(!out.contains("best") && !out.contains("quit"));
        assert!(out.contains("_|") || out.contains("|_|"));
    }

    #[test]
    fn narrow_width_picks_the_small_font() {
        // "13.50" in the big font is ~58 columns; 40 forces the small font
        let out = render(&app(), 40, 12);
        assert!(!out.contains("d88P"));
    }

    #[test]
    fn hidden_running_text_is_plain() {
        let mut app = app();
        app.run_display = fmt::RunDisplay::Hidden;
        app.state = crate::app::State::Running {
            start: Instant::now(),
            penalty: Penalty::Ok,
        };
        assert!(render(&app, 100, 30).contains("solving..."));
    }

    fn style_of(app: &App, w: u16, h: u16, needle: &str) -> Option<ratatui::style::Style> {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        for y in 0..h {
            let row: String = (0..w).map(|x| buf[(x, y)].symbol()).collect();
            if let Some(byte) = row.find(needle) {
                let x = row[..byte].chars().count() as u16;
                return Some(buf[(x, y)].style());
            }
        }
        None
    }

    #[test]
    fn preview_shows_the_scrambled_cube_when_there_is_room() {
        let mut app = app();
        app.scramble = "R U".into(); // any scramble shows all six centers
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let colored: Vec<_> = (0..40u16)
            .flat_map(|y| (0..120u16).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() == "\u{2580}")
            .map(|(x, y)| buf[(x, y)].style().fg)
            .collect();
        // a net has 54 stickers in 27 half-block cells (two per cell)...
        assert!(colored.len() >= 27, "{}", colored.len());
        // ...in all six colors
        for &c in &STICKER_COLORS[..6] {
            assert!(colored.contains(&Some(c)), "{c:?}");
        }
        assert!(render(&app, 120, 40).contains("preview"));
    }

    #[test]
    fn preview_is_left_out_when_narrow() {
        assert!(!render(&app(), 70, 30).contains("preview"));
        assert!(render(&app(), 70, 30).contains("recent times"));
    }

    #[test]
    fn recent_times_are_a_newest_first_grid() {
        let out = render(&app(), 120, 40);
        let row = out.lines().find(|l| l.contains("3. ")).unwrap();
        let (a, b, c) = (
            row.find("3. 13.50").unwrap(),
            row.find("2. 11.00").unwrap(),
            row.find("1. 12.34").unwrap(),
        );
        assert!(a < b && b < c, "{row}");
    }

    #[test]
    fn penalties_and_best_are_colored_in_the_list() {
        let mut app = app();
        app.data
            .solves_mut()
            .push(Solve::new(20_000, Penalty::Dnf, String::new()));
        app.data
            .solves_mut()
            .push(Solve::new(14_000, Penalty::Plus2, String::new()));
        assert_eq!(
            style_of(&app, 120, 40, "DNF(20.00)").unwrap().fg,
            Some(Color::Red)
        );
        assert_eq!(
            style_of(&app, 120, 40, "16.00+").unwrap().fg,
            Some(Color::Yellow)
        );
        // 11.00 (solve 2) is the session best; style_of finds the start of
        // the needle, so look past the "2. " number
        let at_time = |needle: &str| {
            let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
            terminal.draw(|f| draw(f, &app)).unwrap();
            let buf = terminal.backend().buffer().clone();
            (0..40u16)
                .find_map(|y| {
                    let row: String = (0..120u16).map(|x| buf[(x, y)].symbol()).collect();
                    row.find(needle)
                        .map(|b| (row[..b].chars().count() as u16 + 3, y))
                })
                .map(|(x, y)| buf[(x, y)].style().fg)
                .unwrap()
        };
        assert_eq!(at_time("2. 11.00"), Some(Color::Green));
        assert_eq!(at_time("3. 13.50"), Some(Color::Reset));
    }

    #[test]
    fn stats_grid_is_aligned() {
        let out = render(&app(), 120, 40);
        let col = |label: &str| {
            let line = out.lines().find(|l| l.contains(label)).unwrap();
            line.find(label).unwrap()
        };
        assert_eq!(col("best "), col("ao5 "));
        assert_eq!(col("mo3 "), col("ao100 "));
        assert!(out.contains("mo3"));
    }

    #[test]
    fn short_scramble_box_takes_one_line_long_takes_two() {
        let mut app = app();
        // 21 moves, 59 characters: fits in 140 columns, not in 60
        app.scramble = "R2 F2 R2 F2 D2 L2 U2 D L2 D R2 F U2 L' B2 U' B' D2 L' B2 U2".into();
        let one = render(&app, 140, 40);
        let lines: Vec<&str> = one.lines().collect();
        assert!(lines[2].starts_with('\u{2514}'), "{}", lines[2]); // box closes on row 3
        let two = render(&app, 60, 40);
        let lines: Vec<&str> = two.lines().collect();
        assert!(lines[3].starts_with('\u{2514}'), "{}", lines[3]);
    }

    #[test]
    fn history_shows_the_list_details_and_preview() {
        let mut app = app();
        app.data.solves_mut()[1].scramble = "R U R' U'".into();
        app.history = Some(1);
        let out = render(&app, 120, 40);
        assert!(out.contains("history - 3 solves"));
        assert!(out.contains("solve 2 of 3"));
        assert!(out.contains("R U R' U'"));
        assert!(out.contains("preview"));
        assert!(out.contains("just now"));
        // the big digits are replaced by the browser
        assert!(!out.contains("start inspection"));
        // the selected row is drawn reversed
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|f| draw(f, &app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let (x, y) = (0..40u16)
            .find_map(|y| {
                let row: String = (0..120u16).map(|x| buf[(x, y)].symbol()).collect();
                row.find("2. 11.00")
                    .map(|b| (row[..b].chars().count() as u16, y))
            })
            .unwrap();
        assert!(
            buf[(x, y)]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED)
        );
    }

    #[test]
    fn history_scrolls_to_keep_the_selection_visible() {
        let mut app = App::new(true);
        for i in 0..200 {
            app.data
                .solves_mut()
                .push(Solve::new(10_000 + i, Penalty::Ok, String::new()));
        }
        app.history = Some(0); // the oldest, far down the list
        // only the history column (the recent-times panel lists 200 too)
        let column: String = render(&app, 120, 30)
            .lines()
            .map(|l| l.chars().take(28).collect::<String>() + "\n")
            .collect();
        assert!(column.contains("  1. 10.00"), "{column}");
        assert!(!column.contains("200. "), "{column}");
    }

    #[test]
    fn history_works_in_small_windows() {
        let mut app = app();
        app.history = Some(2);
        for (w, h) in [(50, 12), (30, 6), (120, 40), (10, 3)] {
            render(&app, w, h);
        }
        assert!(render(&app, 50, 12).contains("3. 13.50"));
    }

    #[test]
    fn delete_prompt_shows_in_history() {
        let mut app = app();
        app.history = Some(0);
        app.confirm_delete = Some(0);
        assert!(render(&app, 120, 40).contains("delete solve 1? y / n"));
    }

    #[test]
    fn help_overlay_lists_the_keys() {
        let mut app = app();
        app.show_help = true;
        let out = render(&app, 120, 40);
        for want in [
            "keys",
            "history: browse",
            "next / previous digit font",
            "any key closes",
        ] {
            assert!(out.contains(want), "{want}");
        }
        // and it never panics, however small the window
        for (w, h) in [(20, 5), (5, 2), (60, 20)] {
            render(&app, w, h);
        }
    }

    #[test]
    fn footer_mentions_help_and_history() {
        let full = footer_text(&app(), 300);
        assert!(full.contains("? help") && full.contains("h history"));
        // help outlives the less important hints on a narrow screen
        let narrow = footer_text(&app(), 40);
        assert!(narrow.contains("? help") && !narrow.contains("history"));
    }

    #[test]
    fn placeholder_while_the_first_scramble_is_made() {
        let mut app = app();
        app.scramble = String::new();
        let out = render(&app, 120, 40);
        assert!(out.contains("generating scramble..."));
        // no preview net for a missing scramble
        assert!(!out.contains('\u{2580}'));
        assert!(render(&app, 50, 12).contains("generating scramble..."));
    }

    #[test]
    fn history_draws_a_trend_chart_with_ao5() {
        let mut app = App::new(true);
        for ms in [15_000, 14_000, 16_000, 13_000, 12_500, 14_200, 13_100] {
            app.data
                .solves_mut()
                .push(Solve::new(ms, Penalty::Ok, String::new()));
        }
        app.history = Some(3);
        // a short chart, like a 1600x900 fullscreen terminal gives it
        let out = render(&app, 140, 30);
        assert!(out.contains("trend"));
        // the legend, checked on the chart alone at the short height a
        // fullscreen 1600x900 terminal gives it (other panels say "ao5" too)
        let mut terminal = Terminal::new(TestBackend::new(60, 20)).unwrap();
        terminal.draw(|f| draw_trend(f, f.area(), &app, 3)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let chart: String = (0..20u16)
            .map(|y| (0..60u16).map(|x| buf[(x, y)].symbol()).collect::<String>() + "\n")
            .collect();
        assert!(chart.contains("ao5") && chart.contains("time"), "{chart}");
        // braille dots are drawn
        assert!(out.chars().any(|c| ('\u{2801}'..='\u{28ff}').contains(&c)));
        // y-axis labels are the padded range in timer format
        assert!(out.contains("12.") && out.contains("16."));
    }

    #[test]
    fn trend_needs_two_finished_solves() {
        let mut app = App::new(true);
        app.data
            .solves_mut()
            .push(Solve::new(10_000, Penalty::Ok, String::new()));
        app.data
            .solves_mut()
            .push(Solve::new(11_000, Penalty::Dnf, String::new()));
        app.history = Some(0);
        assert!(render(&app, 140, 44).contains("a chart needs two finished solves"));
    }

    #[test]
    fn trend_survives_odd_data_and_small_windows() {
        let mut app = App::new(true);
        for ms in [10_000, 10_000, 10_000] {
            app.data
                .solves_mut()
                .push(Solve::new(ms, Penalty::Ok, String::new())); // flat line
        }
        app.data
            .solves_mut()
            .push(Solve::new(1, Penalty::Plus2, String::new()));
        app.history = Some(3);
        for (w, h) in [(140, 44), (100, 30), (81, 22), (300, 80)] {
            render(&app, w, h);
        }
    }

    #[test]
    fn the_net_grid_has_the_expected_shape_for_every_size() {
        for n in 2..=7usize {
            let grid = net_grid("R U", n).unwrap();
            assert_eq!((grid.len(), grid[0].len()), (3 * n + 2, 4 * n + 3), "{n}");
            assert!(grid.iter().all(|r| r.len() == 4 * n + 3));
            // 6 faces of n x n stickers, nothing in the gaps
            let stickers: usize = grid.iter().map(|r| r.iter().flatten().count()).sum();
            assert_eq!(stickers, 6 * n * n, "{n}");
        }
        assert!(net_grid("", 3).is_none());
        assert!(net_grid("not a scramble", 3).is_none());
    }

    #[test]
    fn a_solved_net_has_n_squared_stickers_of_each_color() {
        for n in 2..=7usize {
            let grid = net_grid("U U'", n).unwrap(); // solved again
            for &color in &STICKER_COLORS[..6] {
                let count = grid
                    .iter()
                    .flatten()
                    .flatten()
                    .filter(|&&c| c == color)
                    .count();
                assert_eq!(count, n * n, "{n} {color:?}");
            }
        }
    }

    #[test]
    fn the_net_shows_the_right_colors_after_a_move() {
        // after R on a 3x3 the top face's right column is green (F's color)
        let grid = net_grid("R", 3).unwrap();
        let green = STICKER_COLORS[2];
        let white = STICKER_COLORS[0];
        // U face is at band 0, slot 1: columns 4..7, rows 0..3
        assert_eq!(grid[0][6], Some(green));
        assert_eq!(grid[1][6], Some(green));
        assert_eq!(grid[2][6], Some(green));
        assert_eq!(grid[0][4], Some(white));
    }

    #[test]
    fn every_size_renders_its_preview_in_a_normal_window() {
        for (digit, n) in [('2', 2), ('3', 3), ('4', 4), ('5', 5), ('6', 6), ('7', 7)] {
            let mut app = app();
            crate::app::tests_support::set_puzzle(&mut app, digit);
            app.scramble = "R U".into(); // switching clears it, so set it after
            let out = render(&app, 140, 44);
            assert!(
                out.contains(&format!("stats - {n}x{n} - session 1/1")),
                "{n}"
            );
            if n <= 3 {
                assert!(out.contains('\u{2580}'), "{n}: net in the panel");
            } else {
                assert!(out.contains("v: full view"), "{n}");
            }
        }
    }

    #[test]
    fn the_cube_view_replaces_the_screen_for_every_size() {
        for digit in ['2', '4', '7'] {
            let mut app = app();
            crate::app::tests_support::set_puzzle(&mut app, digit);
            app.scramble = "R U".into();
            app.cube_view =
                crate::view3d::CubeView::new("x scramble", app.puzzle().size(), &app.scramble);
            let out = render(&app, 120, 40);
            assert!(out.contains("x scramble"), "{digit}");
            assert!(out.contains("esc close"));
            assert!(!out.contains("recent times"), "full screen");
            let blocks = out
                .chars()
                .filter(|&c| c == '\u{2580}' || c == '\u{2584}')
                .count();
            assert!(blocks > 100, "{digit}: {blocks}");
        }
    }

    #[test]
    fn the_picker_lists_every_puzzle_and_marks_the_current_one() {
        let mut app = app();
        app.picker = Some(0);
        let out = render(&app, 100, 30);
        for name in ["2x2", "3x3", "4x4", "5x5", "6x6", "7x7"] {
            assert!(out.contains(name), "{name}");
        }
        assert!(out.contains("* 3x3"), "the current puzzle has a star");
        assert!(out.contains("enter picks"));
        for (w, h) in [(10, 3), (30, 8)] {
            render(&app, w, h);
        }
    }

    #[test]
    fn long_scrambles_wrap_into_as_many_lines_as_they_need() {
        assert_eq!(wrapped_lines("R U F", 80), 1);
        assert_eq!(wrapped_lines("", 80), 1);
        let hundred = vec!["Rw2"; 100].join(" "); // 399 characters
        assert_eq!(wrapped_lines(&hundred, 100), 4);
        assert_eq!(wrapped_lines(&hundred, 40), 8, "capped");
        // a word that's too wide for the line still gets one
        assert_eq!(wrapped_lines("abcdefghij", 3), 1);
        assert_eq!(wrapped_lines("ab cd ef", 5), 2);
        assert_eq!(wrapped_lines("ab cd ef", 4), 3);
    }

    #[test]
    fn a_100_move_scramble_is_fully_shown() {
        let mut app = app();
        let mut rng = <rand::rngs::StdRng as rand::SeedableRng>::seed_from_u64(7);
        crate::app::tests_support::set_puzzle(&mut app, '7');
        app.scramble = nxn::format(&nxn::random_moves(7, 100, &mut rng));
        let out = render(&app, 120, 44);
        for token in app.scramble.split_whitespace() {
            assert!(out.contains(token), "missing {token}");
        }
        // and the time display still has room
        assert!(out.contains("stats - 7x7"));
        render(&app, 80, 24);
        render(&app, 60, 20);
    }

    fn busy_app(n: usize) -> App {
        let mut app = App::new(true);
        for i in 0..n {
            // a wobble around 13 s, a drift, the odd +2 and DNF and a huge outlier
            let ms = 13_000 + ((i * 7919) % 1800) as u64 + (i as u64 * 3) % 400;
            let penalty = match i % 40 {
                7 => Penalty::Plus2,
                23 => Penalty::Dnf,
                _ => Penalty::Ok,
            };
            let ms = if i == n / 2 { 95_000 } else { ms };
            app.data
                .solves_mut()
                .push(Solve::new(ms, penalty, String::new()));
        }
        app
    }

    #[test]
    fn the_statistics_screen_shows_chart_distribution_and_summary() {
        let mut app = busy_app(300);
        app.stats = Some(StatsView::new(&app));
        let out = render(&app, 140, 44);
        for want in [
            "statistics - 3x3",
            "last 50",
            "last 100",
            "last 500",
            "all",
            "distribution",
            "summary",
            "solves",
            "std dev",
            "ao1000",
            "current",
            "esc close",
        ] {
            assert!(out.contains(want), "missing {want:?}");
        }
        assert!(out.contains("300"), "the solve count");
        // the 95 s outlier didn't stretch the axis: the top label is nowhere near it
        assert!(!out.contains("95.00") && !out.contains("90.00"), "{out}");
        // y labels are round numbers
        assert!(out.contains("13.00") || out.contains("14.00"));
        assert!(!out.contains("it's the timer"), "the timer is replaced");
    }

    #[test]
    fn the_range_selector_changes_what_is_drawn() {
        let mut app = busy_app(300);
        let mut view = StatsView::new(&app);
        view.range = Range::Last50;
        app.stats = Some(view);
        let out = render(&app, 140, 44);
        // the x axis starts at solve 251 when showing the last 50
        assert!(out.contains("251"), "{out}");
    }

    #[test]
    fn the_statistics_screen_survives_every_kind_of_data_and_size() {
        let mut datasets: Vec<App> = vec![App::new(true), busy_app(1), busy_app(2), busy_app(2000)];
        let mut dnfs = App::new(true);
        for _ in 0..5 {
            dnfs.data
                .solves_mut()
                .push(Solve::new(1_000, Penalty::Dnf, String::new()));
        }
        datasets.push(dnfs);
        let mut same = App::new(true);
        for _ in 0..30 {
            same.data
                .solves_mut()
                .push(Solve::new(10_000, Penalty::Ok, String::new()));
        }
        datasets.push(same);
        for mut app in datasets {
            for range in Range::ALL {
                let mut view = StatsView::new(&app);
                view.range = range;
                app.stats = Some(view);
                for (w, h) in [
                    (1, 1),
                    (10, 3),
                    (40, 10),
                    (80, 24),
                    (89, 29),
                    (90, 31),
                    (200, 60),
                ] {
                    render(&app, w, h);
                }
            }
        }
    }

    #[test]
    fn small_windows_drop_the_histogram_then_the_summary() {
        let mut app = busy_app(100);
        app.stats = Some(StatsView::new(&app));
        let big = render(&app, 140, 44);
        assert!(big.contains("distribution") && big.contains("summary"));
        // a normal 24-row terminal has room for the distribution; 20 rows doesn't
        assert!(render(&app, 140, 26).contains("distribution"));
        let short = render(&app, 140, 20);
        assert!(!short.contains("distribution") && short.contains("summary"));
        let narrow = render(&app, 80, 44);
        assert!(!narrow.contains("summary") && narrow.contains("distribution"));
    }

    #[test]
    fn nothing_to_plot_says_so() {
        let mut app = App::new(true);
        app.stats = Some(StatsView::new(&app));
        assert!(render(&app, 120, 40).contains("nothing to plot until a solve is finished"));
    }

    #[test]
    fn the_chart_marks_the_selected_solve_and_slow_ones() {
        let mut app = busy_app(60);
        app.history = Some(10);
        let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
        terminal
            .draw(|f| draw_trend(f, f.area(), &app, 10))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let colors: Vec<_> = (0..30u16)
            .flat_map(|y| (0..100u16).map(move |x| (x, y)))
            .filter_map(|(x, y)| buf[(x, y)].style().fg)
            .collect();
        assert!(colors.contains(&Color::White), "selected solve");
        assert!(
            colors.contains(&Color::Yellow),
            "the 95 s outlier at the top edge"
        );
        assert!(colors.contains(&Color::Cyan), "ao5");
        assert!(colors.contains(&Color::LightMagenta), "ao12");
        assert!(colors.contains(&Color::Red), "a DNF along the bottom");
    }

    #[test]
    fn the_legend_drops_entries_that_do_not_fit_instead_of_cutting_them() {
        let app = busy_app(80);
        for width in [30u16, 40, 50, 70, 100] {
            let mut terminal = Terminal::new(TestBackend::new(width, 16)).unwrap();
            terminal.draw(|f| draw_trend(f, f.area(), &app, 5)).unwrap();
            let buf = terminal.backend().buffer().clone();
            let bottom: String = (0..width).map(|x| buf[(x, 15)].symbol()).collect();
            // every legend word that appears is complete
            for word in ["tim", "ao", "bes", "slowe", "DN"] {
                for (at, _) in bottom.match_indices(word) {
                    let rest = &bottom[at..];
                    let full = ["time", "ao5", "ao12", "best", "slower", "DNF"]
                        .iter()
                        .any(|w| rest.starts_with(w));
                    assert!(full, "{width}: cut-off legend in {bottom:?}");
                }
            }
        }
    }

    #[test]
    fn axis_labels_sit_on_the_same_row_as_their_gridlines() {
        // flat-ish data with a known round range: 10 s .. 20 s
        let mut app = App::new(true);
        for ms in [10_000u64, 20_000, 10_500, 19_500, 12_000, 18_000] {
            app.data
                .solves_mut()
                .push(Solve::new(ms, Penalty::Ok, String::new()));
        }
        let mut terminal = Terminal::new(TestBackend::new(60, 24)).unwrap();
        terminal.draw(|f| draw_trend(f, f.area(), &app, 0)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let grid = Color::Indexed(238);
        let mut checked = 0;
        for y in 0..24u16 {
            let row: String = (0..60u16).map(|x| buf[(x, y)].symbol()).collect();
            // a label like "15.00" starting the row after the border
            let trimmed = row.trim_start_matches('\u{2502}');
            let Some(label) = trimmed
                .split_whitespace()
                .next()
                .filter(|l| l.contains('.') && l.parse::<f64>().is_ok())
            else {
                continue;
            };
            let value: f64 = label.parse().unwrap();
            if value <= 10.0 || value >= 20.0 {
                continue; // the edges have no gridline
            }
            // the gridline for this value is drawn in this row (not the one above or below)
            let has_line = (0..60u16).any(|x| buf[(x, y)].style().fg == Some(grid));
            assert!(
                has_line,
                "no gridline beside the {label} label (row {y}):\n{row}"
            );
            checked += 1;
        }
        assert!(checked >= 1, "no interior labels found");
    }

    #[test]
    fn big_nets_keep_same_colored_stickers_apart() {
        let grid = net_grid("U U'", 3).unwrap(); // solved: whole faces one color
        let lines = net_lines(&grid, 4);
        // the U face spans columns 16..28 at scale 4; a gap column after each sticker
        let row: Vec<String> = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(row[19], " ", "gap after the first U sticker");
        assert_ne!(row[18], " ");
        // small scales stay solid
        let small = net_lines(&grid, 2);
        let row: Vec<String> = small[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert_ne!(row[9], " ");
    }
}
