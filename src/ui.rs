//! Rendering. Colors are ANSI only, so the timer follows the terminal theme.

use std::time::Instant;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::app::{App, Tone};
use crate::fmt;
use crate::font::{self, Font};

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
const FULL_MIN: (u16, u16) = (60, 17);
const COMPACT_MIN_HEIGHT: u16 = 8;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width >= FULL_MIN.0 && area.height >= FULL_MIN.1 {
        let [scramble, main, stats, footer] = Layout::vertical([
            Constraint::Length(4),
            Constraint::Min(0),
            Constraint::Length(6),
            Constraint::Length(1),
        ])
        .areas(area);
        draw_scramble(frame, scramble, app);
        draw_time(frame, main, app);
        draw_stats(frame, stats, app);
        draw_footer(frame, footer, app);
    } else if area.height >= COMPACT_MIN_HEIGHT {
        // The scramble gets a second line when it won't fit on one.
        let scramble_h = if area.width as usize >= app.scramble.len() + 2 { 1 } else { 2 };
        let [scramble, main, stats, footer] = Layout::vertical([
            Constraint::Length(scramble_h),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);
        frame.render_widget(
            Paragraph::new(app.scramble.as_str()).alignment(Alignment::Center).wrap(Wrap { trim: true }),
            scramble,
        );
        draw_time(frame, main, app);
        draw_stats_line(frame, stats, app);
        draw_footer(frame, footer, app);
    } else {
        draw_time(frame, area, app);
    }
}

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

fn draw_scramble(frame: &mut Frame, area: Rect, app: &App) {
    let p = Paragraph::new(app.scramble.as_str())
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true })
        .block(Block::bordered().title(" scramble ").border_style(dim()));
    frame.render_widget(p, area);
}

fn draw_time(frame: &mut Frame, area: Rect, app: &App) {
    let view = app.view(Instant::now());
    let color = tone_color(view.tone);

    // Big digits when they fit, then the small font, then plain text (for
    // strings the fonts have no glyphs for, like "solving...").
    let font = if area.height as usize >= font::BIG.height + 2 {
        font::pick(&view.text, area.width as usize)
    } else {
        &font::SMALL
    };
    let font: Option<&Font> = Some(font).filter(|f| f.supports(&view.text));
    let lines: Vec<Line> = match font {
        Some(f) => f.render(&view.text),
        None => vec![Line::from(view.text.as_str())],
    };

    // The hint only gets the rows left over after the digits.
    let hint_h = area.height.saturating_sub(lines.len() as u16).min(2);
    let [time, hint] = Layout::vertical([Constraint::Length(lines.len() as u16), Constraint::Length(hint_h)])
        .flex(Flex::Center)
        .areas(area);
    let style = Style::new().fg(color).add_modifier(Modifier::BOLD);
    frame.render_widget(Paragraph::new(lines).style(style).alignment(Alignment::Center), time);
    frame.render_widget(
        Paragraph::new(vec![Line::default(), Line::from(view.hint)])
            .style(Style::new().fg(color))
            .alignment(Alignment::Center),
        hint,
    );
}

fn draw_stats(frame: &mut Frame, area: Rect, app: &App) {
    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);
    let prec = app.precision;
    let s = app.summary();

    let stat = |name: &'static str, value: String| -> Vec<Span<'static>> {
        vec![Span::styled(format!("{name} "), dim()), Span::raw(format!("{value:<9}"))]
    };
    let row = |items: Vec<Vec<Span<'static>>>| Line::from(items.concat());
    let lines = vec![
        row(vec![
            stat("best", fmt::fmt_single(s.best, prec)),
            stat("ao5", fmt::fmt_avg(s.ao5, prec)),
        ]),
        row(vec![
            stat("ao12", fmt::fmt_avg(s.ao12, prec)),
            stat("ao100", fmt::fmt_avg(s.ao100, prec)),
        ]),
        row(vec![
            stat("best ao5", fmt::fmt_avg(s.best_ao5, prec)),
            stat("best ao12", fmt::fmt_avg(s.best_ao12, prec)),
        ]),
        row(vec![
            stat("mean", s.mean.map_or("-".into(), |m| fmt::fmt_avg_ms(m, prec))),
            stat("count", s.count.to_string()),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(format!(" stats - session {}/{} ", app.data.current + 1, app.data.sessions.len())).border_style(dim())),
        left,
    );

    // Newest first, numbered by solve index.
    let mut recent = Vec::new();
    for (i, solve) in app.solves().iter().enumerate().rev() {
        // Non-breaking space: a wrap may split entries, never a number from its time.
        recent.push(Span::styled(format!("{}.\u{a0}", i + 1), dim()));
        recent.push(Span::raw(format!("{}   ", fmt::fmt_solve_list(solve.ms, solve.penalty, prec))));
    }
    frame.render_widget(
        Paragraph::new(Line::from(recent))
            .wrap(Wrap { trim: false })
            .block(Block::bordered().title(" recent times ").border_style(dim())),
        right,
    );
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
    frame.render_widget(Paragraph::new(text).style(dim()).alignment(Alignment::Center), area);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let on = |b: bool| if b { "on" } else { "off" };
    let mut text = format!(
        " space go  +/d/o mark  bksp delete  n [ ] session  i inspect:{}  p display  b bell:{}  q quit",
        on(app.inspection_on),
        on(app.bell)
    );
    if !app.enhanced {
        text.push_str("  | basic mode");
    }
    let mut spans = vec![Span::styled(text, dim())];
    if let Some(status) = &app.status {
        spans.push(Span::styled(format!("  | {status}"), Style::new().fg(Color::LightRed)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Solve;
    use crate::stats::Penalty;
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
            app.data.solves_mut().push(Solve::new(ms, Penalty::Ok, String::new()));
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
        for want in ["scramble", "stats - session 1/1", "recent times", "ao100", "q quit", ".d8888b."] {
            assert!(out.contains(want), "missing {want:?}");
        }
    }

    #[test]
    fn recent_times_wrap_between_entries_not_inside_them() {
        let mut app = App::new(true);
        for i in 0..40 {
            app.data.solves_mut().push(Solve::new(12_000 + i * 137, Penalty::Ok, String::new()));
        }
        for w in 60..=140 {
            let out = render(&app, w, 30);
            for row in out.lines().filter(|r| r.contains('\u{2502}')) {
                // text of the right-hand panel, between its borders
                let inner = row.rsplit('\u{2502}').nth(1).unwrap_or("").trim_end_matches('\u{a0}').trim();
                let last = inner.rsplit(' ').next().unwrap_or("");
                let dangling = last.strip_suffix('.').is_some_and(|n| n.parse::<u32>().is_ok());
                assert!(!dangling, "width {w}: row ends with a bare number: {row:?}");
            }
        }
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
}
