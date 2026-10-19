//! Rendering. Colors are ANSI only, so the timer follows the terminal theme.

use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Axis, Block, Chart, Clear, Dataset, GraphType, LegendPosition, Padding, Paragraph, Wrap,
};

use crate::app::{App, Tone};
use crate::cube;
use crate::fmt;
use crate::font;
use crate::stats::{self, Penalty, TimeResult};

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
    if area.width >= FULL_MIN.0 && area.height >= FULL_MIN.1 {
        // One line for the scramble when it fits, two when it has to wrap.
        let scramble_lines = if app.scramble.len() + 4 <= area.width as usize {
            1
        } else {
            2
        };
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
        // The scramble gets a second line when it won't fit on one.
        let scramble_h = if area.width as usize >= app.scramble.len() + 2 {
            1
        } else {
            2
        };
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
        draw_preview(frame, p, &app.scramble, "preview");
    }
    draw_recent(frame, right, app);
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
    let title = format!(
        " stats - session {}/{} ",
        app.data.current + 1,
        app.data.sessions.len()
    );
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(title).border_style(dim())),
        area,
    );
}

/// WCA color scheme, U R F D L B. Real cube colors rather than the terminal
/// theme's, since they have to match the cube in your hands.
const STICKER_COLORS: [Color; 6] = [
    Color::Rgb(245, 245, 245),
    Color::Rgb(214, 40, 40),
    Color::Rgb(0, 168, 84),
    Color::Rgb(255, 213, 0),
    Color::Rgb(255, 128, 0),
    Color::Rgb(0, 82, 204),
];

/// The scrambled cube as a net (U on top, L F R B across, D below), each
/// sticker a square half a character tall.
fn draw_preview(frame: &mut Frame, area: Rect, scramble: &str, title: &str) {
    let block = Block::bordered()
        .title(format!(" {title} "))
        .border_style(dim());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    // nothing to show yet (or a solve saved without one)
    if scramble.is_empty() {
        return;
    }
    let Some(moves) = cube::parse(scramble) else {
        return;
    };
    let stickers = cube::SOLVED.apply_all(&moves).facelets();

    // Sticker grid: 15 columns (four faces with gaps), 11 half-rows.
    let at = |row: usize, col: usize| -> Option<Color> {
        let (band, r) = (row / 4, row % 4);
        let (slot, c) = (col / 4, col % 4);
        if r == 3 || c == 3 {
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
        Some(STICKER_COLORS[stickers[face * 9 + r * 3 + c] as usize])
    };
    let lines: Vec<Line> = (0..6)
        .map(|y| {
            let spans: Vec<Span> = (0..15)
                .map(|x| match (at(2 * y, x), at(2 * y + 1, x)) {
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
        .collect();
    let [net] = Layout::vertical([Constraint::Length(6)])
        .flex(Flex::Center)
        .areas(inner);
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), net);
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
    let text = format!(
        "best {}  ao5 {}  ao12 {}  mean {}  n={}  s{}/{}",
        fmt::fmt_single(s.best, prec),
        fmt::fmt_avg(s.ao5, prec),
        fmt::fmt_avg(s.ao12, prec),
        s.mean.map_or("-".into(), |m| fmt::fmt_avg_ms(m, prec)),
        s.count,
        app.data.current + 1,
        app.data.sessions.len(),
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
    let parts: [(u8, String); 11] = [
        (1, "space go".into()),
        (6, "+/d/o mark".into()),
        (8, "bksp delete".into()),
        (7, "h history".into()),
        (9, "n [ ] session".into()),
        (5, format!("i inspect:{}", on(app.inspection_on))),
        (4, format!("f font:{}", font::name_at(app.font))),
        (10, "p display".into()),
        (11, format!("b bell:{}", on(app.bell))),
        (3, "? help".into()),
        (2, "q quit".into()),
    ];
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by_key(|&i| parts[i].0);
    let mut keep = [false; 11];
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
const HELP: [(&str, &str); 18] = [
    ("space", "inspect, hold and release to start, stop"),
    ("any key", "stop a running solve"),
    ("esc", "cancel inspection or a hold"),
    ("+ / d / o", "mark the last solve +2, DNF or OK"),
    ("backspace", "delete the last solve (asks y / n)"),
    ("h", "history: browse, mark and delete any solve"),
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
    ("in history", "up/down j/k pgup/pgdn home/end move"),
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
    if let Some(i) = app.confirm_delete {
        lines.push(Line::default());
        lines.push(Line::from(Span::styled(
            format!("delete solve {}? y / n", i + 1),
            Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    }
    let keys = "up/down move   + d o mark   bksp delete   esc close";
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
        draw_preview(frame, p, &solve.scramble, "state");
    }
    if chart.width > 0 {
        draw_trend(frame, chart, app, selected);
    }
    frame.render_widget(Paragraph::new(keys).style(dim()), help);
}

/// Every solve of the session as a line (DNFs left out), the rolling ao5 over
/// it, and the selected solve marked.
fn draw_trend(frame: &mut Frame, area: Rect, app: &App, selected: usize) {
    let results = app.results();
    let secs = |ms: u64| ms as f64 / 1000.0;
    let times: Vec<(f64, f64)> = results
        .iter()
        .enumerate()
        .filter_map(|(i, r)| match r {
            TimeResult::Ms(ms) => Some(((i + 1) as f64, secs(*ms))),
            TimeResult::Dnf => None,
        })
        .collect();
    let block = Block::bordered().title(" trend ").border_style(dim());
    if times.len() < 2 {
        frame.render_widget(
            Paragraph::new("a trend needs two finished solves")
                .style(dim())
                .block(block),
            area,
        );
        return;
    }
    let ao5: Vec<(f64, f64)> = (5..=results.len())
        .filter_map(|end| match stats::ao(&results[..end], 5) {
            Some(TimeResult::Ms(ms)) => Some((end as f64, secs(ms))),
            _ => None,
        })
        .collect();
    let mark: Vec<(f64, f64)> = times
        .iter()
        .copied()
        .filter(|&(x, _)| x == (selected + 1) as f64)
        .collect();

    let (lo, hi) = times
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), &(_, y)| {
            (lo.min(y), hi.max(y))
        });
    let pad = ((hi - lo) * 0.1).max(0.05);
    let (lo, hi) = ((lo - pad).max(0.0), hi + pad);
    let prec = app.precision;
    let label = |v: f64| Span::styled(fmt::fmt_time((v * 1000.0) as u64, prec), dim());
    let n = results.len() as f64;

    let mut datasets = vec![
        Dataset::default()
            .name("time")
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .data(&times),
    ];
    if !ao5.is_empty() {
        datasets.push(
            Dataset::default()
                .name("ao5")
                .marker(Marker::Braille)
                .graph_type(GraphType::Line)
                .style(Style::new().fg(Color::Cyan))
                .data(&ao5),
        );
    }
    datasets.push(
        Dataset::default()
            .marker(Marker::Block)
            .graph_type(GraphType::Scatter)
            .style(Style::new().fg(Color::Yellow))
            .data(&mark),
    );
    let chart = Chart::new(datasets)
        .block(block)
        .x_axis(
            Axis::default()
                .bounds([1.0, n.max(2.0)])
                .labels([
                    Span::styled("1", dim()),
                    Span::styled(format!("{}", results.len()), dim()),
                ])
                .style(dim()),
        )
        .y_axis(
            Axis::default()
                .bounds([lo, hi])
                .labels([label(lo), label(hi)])
                .style(dim()),
        )
        .legend_position(Some(LegendPosition::TopRight))
        // the default hides the legend in charts this short
        .hidden_legend_constraints((Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)));
    frame.render_widget(chart, area);
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
    use crate::stats::Penalty;
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
            "stats - session 1/1",
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
        for c in STICKER_COLORS {
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
        let mut terminal = Terminal::new(TestBackend::new(60, 12)).unwrap();
        terminal.draw(|f| draw_trend(f, f.area(), &app, 3)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let chart: String = (0..12u16)
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
        assert!(render(&app, 140, 44).contains("a trend needs two finished solves"));
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
}
