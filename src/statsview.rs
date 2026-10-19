//! The statistics screen (`s`) and the time chart it shares with the history
//! browser.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::canvas::{Canvas, Line as CanvasLine, Points};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::chart::{self, Range};
use crate::fmt::{self, Precision};
use crate::stats::{self, Report, TimeResult};

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// The statistics screen's state: which solves the chart covers, and the
/// summary, computed once when the screen opens (the averages over 1000 solves
/// are too slow to redo for every frame).
#[derive(Debug, Clone)]
pub struct StatsView {
    pub range: Range,
    pub report: Report,
    pub plus2: usize,
    pub dnf: usize,
}

impl StatsView {
    pub fn new(app: &App) -> StatsView {
        use crate::stats::Penalty;
        let solves = app.solves();
        StatsView {
            range: Range::default(),
            report: stats::report(&app.results()),
            plus2: solves
                .iter()
                .filter(|s| s.penalty == Penalty::Plus2)
                .count(),
            dnf: solves.iter().filter(|s| s.penalty == Penalty::Dnf).count(),
        }
    }
}

/// Colors of the chart's series, so the legend and the screen agree.
const AO5: Color = Color::Cyan;
const AO12: Color = Color::LightMagenta;
const BEST: Color = Color::Green;
const SLOW: Color = Color::Yellow;
const DNF: Color = Color::Red;
const SELECTED: Color = Color::White;

/// Draw the time chart of `results[from..]` in `area`. `selected` marks one
/// solve (by index). Draws a short message instead when there is nothing to
/// plot.
///
/// It is drawn on a canvas so that the axis labels, the gridlines and the data
/// all go through one coordinate transform and cannot disagree about where a
/// value is.
pub fn draw_chart(
    frame: &mut Frame,
    area: Rect,
    results: &[TimeResult],
    from: usize,
    selected: Option<usize>,
    prec: Precision,
    title: &str,
) {
    let mut block = Block::bordered()
        .title(format!(" {title} "))
        .border_style(dim());
    let inner = block.inner(area);
    let max_ticks = ((inner.height as usize).saturating_sub(2) / 2).clamp(2, 10);
    let Some(series) = chart::series(results, from, max_ticks) else {
        frame.render_widget(
            Paragraph::new("nothing to plot until a solve is finished")
                .style(dim())
                .block(block),
            area,
        );
        return;
    };
    if series.times.len() + series.slow.len() < 2 {
        frame.render_widget(
            Paragraph::new("a chart needs two finished solves")
                .style(dim())
                .block(block),
            area,
        );
        return;
    }
    if inner.width < 12 || inner.height < 4 {
        frame.render_widget(block, area);
        return;
    }
    let (x0, x1) = series.x;
    let axis = &series.axis;
    let labels: Vec<String> = axis
        .ticks
        .iter()
        .map(|&t| fmt::fmt_time((t * 1000.0).round() as u64, prec))
        .collect();
    let label_w = labels.iter().map(|l| l.len()).max().unwrap_or(0) + 1;
    // The canvas covers the whole inner area, so reserve room for the labels
    // on the left and one row below the data for the solve numbers.
    let plot_cols = (inner.width as usize).saturating_sub(label_w + 1).max(1) as f64;
    let per_col = (x1 - x0) / plot_cols;
    let x_lo = x0 - per_col * (label_w + 1) as f64;
    let rows = inner.height as f64;
    let per_row = (axis.hi - axis.lo) / (rows - 2.0).max(1.0);
    let y_lo = axis.lo - per_row * 1.5;
    let y_hi = axis.hi + per_row * 0.5;

    let marked: Option<(f64, f64)> = selected
        .and_then(|i| series.times.iter().find(|p| p.0 == (i + 1) as f64))
        .copied();
    let grid = Color::Indexed(238);
    let text = |t: &str, color: Color| Span::styled(t.to_string(), Style::new().fg(color));

    // the legend only lists what is on the chart, and only what fits
    let key: [(&str, Color, &str, bool); 6] = [
        ("\u{b7}", Color::Gray, "time", !series.times.is_empty()),
        ("\u{2500}", AO5, "ao5", !series.ao5.is_empty()),
        ("\u{2500}", AO12, "ao12", !series.ao12.is_empty()),
        ("\u{25c6}", BEST, "best", !series.personal_bests.is_empty()),
        ("\u{25b2}", SLOW, "slower", !series.slow.is_empty()),
        ("\u{d7}", DNF, "DNF", !series.dnfs.is_empty()),
    ];
    let mut legend = vec![Span::styled(" ", dim())];
    let mut used = 1;
    for (glyph, color, name, present) in key {
        // glyph, space, name and two spaces after
        let width = 1 + 1 + name.len() + 2;
        if !present || used + width > inner.width as usize {
            continue;
        }
        used += width;
        legend.push(text(glyph, color));
        legend.push(Span::styled(format!(" {name}  "), dim()));
    }
    block = block.title_bottom(Line::from(legend));

    let canvas = Canvas::default()
        .block(block)
        .marker(Marker::Braille)
        .x_bounds([x_lo, x1])
        .y_bounds([y_lo, y_hi])
        .paint(|ctx| {
            // gridlines and the axis
            for &t in &axis.ticks[1..axis.ticks.len() - 1] {
                ctx.draw(&CanvasLine {
                    x1: x0,
                    y1: t,
                    x2: x1,
                    y2: t,
                    color: grid,
                });
            }
            ctx.draw(&CanvasLine {
                x1: x0,
                y1: axis.lo,
                x2: x0,
                y2: axis.hi,
                color: grid,
            });
            ctx.layer();
            ctx.draw(&Points {
                coords: &series.times,
                color: Color::Gray,
            });
            // the smooth ao12 goes on top of the jagged ao5, or it disappears in it
            for (points, color) in [(&series.ao5, AO5), (&series.ao12, AO12)] {
                for pair in points.windows(2) {
                    ctx.draw(&CanvasLine {
                        x1: pair[0].0,
                        y1: pair[0].1,
                        x2: pair[1].0,
                        y2: pair[1].1,
                        color,
                    });
                }
            }
            ctx.layer();
            // text goes on top: the labels, then the markers
            for (t, label) in axis.ticks.iter().zip(&labels) {
                ctx.print(x_lo, *t, Line::from(Span::styled(label.clone(), dim())));
            }
            ctx.print(
                x0,
                y_lo,
                Line::from(Span::styled(format!("{}", x0 as usize), dim())),
            );
            let last = format!("{}", x1 as usize);
            ctx.print(
                x1 - per_col * last.len() as f64,
                y_lo,
                Line::from(Span::styled(last, dim())),
            );
            for &(x, y) in &series.personal_bests {
                ctx.print(x, y, Line::from(text("\u{25c6}", BEST)));
            }
            for &(x, y) in &series.slow {
                ctx.print(x, y, Line::from(text("\u{25b2}", SLOW)));
            }
            for &(x, y) in &series.dnfs {
                ctx.print(x, y, Line::from(text("\u{d7}", DNF)));
            }
            if let Some((x, y)) = marked {
                ctx.print(
                    x,
                    y,
                    Line::from(Span::styled(
                        "\u{25cf}",
                        Style::new().fg(SELECTED).add_modifier(Modifier::BOLD),
                    )),
                );
            }
        });
    frame.render_widget(canvas, area);
}

/// The full-screen statistics view.
pub fn draw(frame: &mut Frame, area: Rect, app: &App, view: &StatsView) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    // title and the range selector
    let mut spans = vec![Span::styled(
        format!(" statistics - {}   ", app.puzzle().name()),
        Style::new().add_modifier(Modifier::BOLD),
    )];
    for r in Range::ALL {
        let style = if r == view.range {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            dim()
        };
        spans.push(Span::styled(format!(" {} ", r.label()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), header);
    frame.render_widget(Paragraph::new(" r range   esc close").style(dim()), footer);

    let results = app.results();
    let prec = app.precision;
    let wide = body.width >= 90;
    let [left, right] = if wide {
        Layout::horizontal([Constraint::Min(0), Constraint::Length(38)]).areas(body)
    } else {
        [body, Rect::default()]
    };
    // the distribution gets about a third of the height once the chart can
    // spare it
    let tall = left.height >= 22;
    let histogram_rows = (left.height / 3).clamp(7, 13);
    let [chart_area, histogram_area] = if tall {
        Layout::vertical([Constraint::Min(0), Constraint::Length(histogram_rows)]).areas(left)
    } else {
        [left, Rect::default()]
    };
    let from = view.range.start(results.len());
    draw_chart(
        frame,
        chart_area,
        &results,
        from,
        None,
        prec,
        view.range.label(),
    );
    if tall {
        draw_histogram(frame, histogram_area, &results[from..], prec);
    }
    if wide {
        draw_summary(frame, right, view, prec);
    }
}

/// Horizontal bars: how many solves fall in each time bucket.
fn draw_histogram(frame: &mut Frame, area: Rect, results: &[TimeResult], prec: Precision) {
    let block = Block::bordered()
        .title(" distribution ")
        .border_style(dim());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let times: Vec<f64> = results
        .iter()
        .filter_map(|r| match r {
            TimeResult::Ms(ms) => Some(*ms as f64 / 1000.0),
            TimeResult::Dnf => None,
        })
        .collect();
    let buckets = chart::histogram(&times, inner.height.max(1) as usize);
    let most = buckets.iter().map(|b| b.count).max().unwrap_or(0).max(1);
    let label = |t: f64| fmt::fmt_time((t * 1000.0).round() as u64, prec);
    let label_w = buckets
        .iter()
        .map(|b| label(b.lo).len() + label(b.hi).len() + 1)
        .max()
        .unwrap_or(0);
    let bar_room = (inner.width as usize).saturating_sub(label_w + 8).max(1);
    let lines: Vec<Line> = buckets
        .iter()
        .map(|b| {
            let len = (b.count * bar_room).div_ceil(most).min(bar_room);
            let len = if b.count == 0 { 0 } else { len.max(1) };
            Line::from(vec![
                Span::styled(
                    format!("{:>label_w$} ", format!("{}-{}", label(b.lo), label(b.hi))),
                    dim(),
                ),
                Span::styled("\u{2588}".repeat(len), Style::new().fg(AO5)),
                Span::raw(format!(" {}", b.count)),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_summary(frame: &mut Frame, area: Rect, view: &StatsView, prec: Precision) {
    let r = &view.report;
    let single = |ms: Option<u64>| ms.map_or("-".into(), |m| fmt::fmt_time(m, prec));
    let row = |name: &str, value: String| {
        Line::from(vec![
            Span::styled(format!("{name:<10}"), dim()),
            Span::raw(value),
        ])
    };
    let mut lines = vec![
        row("solves", format!("{}", r.count)),
        row("  +2", format!("{}", view.plus2)),
        row("  DNF", format!("{}", view.dnf)),
        Line::default(),
        row("best", fmt::fmt_single(r.best, prec)),
        row("worst", single(r.worst)),
        row(
            "mean",
            r.mean.map_or("-".into(), |m| fmt::fmt_avg_ms(m, prec)),
        ),
        row(
            "std dev",
            r.std_dev_ms
                .map_or("-".into(), |d| fmt::fmt_avg_ms(d.round() as u64, prec)),
        ),
        Line::default(),
        Line::from(vec![
            Span::styled(format!("{:<10}", ""), dim()),
            Span::styled(format!("{:<10}", "current"), dim()),
            Span::styled("best", dim()),
        ]),
    ];
    for (n, current, best) in &r.averages {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<10}", format!("ao{n}")), dim()),
            Span::raw(format!("{:<10}", fmt::fmt_avg(*current, prec))),
            Span::raw(fmt::fmt_avg(*best, prec)),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" summary ").border_style(dim())),
        area,
    );
}
