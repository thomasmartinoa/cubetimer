//! The algorithm browser (`a`): every CFOP case as a diagram with its
//! algorithm, playable on the 3D cube, with progress marks.
//!
//! Diagrams are computed from each algorithm's own case (the inverse of what
//! it does), never drawn by hand, so a picture can't disagree with its
//! algorithm. Cases are shown as cubers learn them: yellow on top, green in
//! front.

use std::cell::Cell;
use std::collections::BTreeMap;

use ratatui::Frame;
use ratatui::crossterm::event::KeyCode;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::algs::{Case, Set};
use crate::cube::{CORNER_FACELETS, EDGE_FACELETS};
use crate::fmt::{self, Precision};
use crate::nxn::Nxn;
use crate::render3d::{Camera, GRAY};
use crate::store::{Data, Solve};
use crate::trainer;
use crate::ui::STICKER_COLORS;
use crate::view3d::{self, Gfx, Look};

/// The color each face's stickers are shown in: the model's white-top
/// scheme turned over so yellow is on top and green in front (so orange is
/// on the right). Gray stays gray.
pub const DISPLAY: [u8; 7] = [3, 4, 2, 0, 1, 5, 6];
const YELLOW: u8 = 3;

/// How far you are with a case. Saved by case key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Progress {
    #[default]
    New,
    Learning,
    Learned,
}

impl Progress {
    pub fn name(self) -> &'static str {
        match self {
            Progress::New => "new",
            Progress::Learning => "learning",
            Progress::Learned => "learned",
        }
    }

    pub fn from_name(s: &str) -> Progress {
        match s {
            "learning" => Progress::Learning,
            "learned" => Progress::Learned,
            _ => Progress::New,
        }
    }

    pub fn next(self) -> Progress {
        match self {
            Progress::New => Progress::Learning,
            Progress::Learning => Progress::Learned,
            Progress::Learned => Progress::New,
        }
    }

    fn color(self) -> Color {
        match self {
            Progress::New => Color::DarkGray,
            Progress::Learning => Color::Yellow,
            Progress::Learned => Color::Green,
        }
    }
}

/// The browser's state; owned by the app.
#[derive(Debug, Clone)]
pub struct AlgBrowser {
    pub set: Set,
    pub selected: usize,
    /// Tiles per row as last drawn, so up and down move by a row.
    columns: Cell<usize>,
    /// The first row of tiles shown.
    scroll: Cell<usize>,
}

/// What a key in the browser asks the app to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Close,
    /// Show this case on the 3D cube.
    Show(&'static Case),
    /// Cycle this case's progress mark.
    Mark(&'static Case),
    /// Start the trainer on this set.
    Train(Set),
}

impl Default for AlgBrowser {
    fn default() -> Self {
        AlgBrowser {
            set: Set::Pll,
            selected: 0,
            columns: Cell::new(1),
            scroll: Cell::new(0),
        }
    }
}

impl AlgBrowser {
    pub fn case(&self) -> &'static Case {
        let cases = self.set.cases();
        &cases[self.selected.min(cases.len() - 1)]
    }

    pub fn key(&mut self, code: KeyCode) -> Action {
        let n = self.set.cases().len();
        let cols = self.columns.get().max(1);
        match code {
            KeyCode::Esc | KeyCode::Char('a' | 'q') => return Action::Close,
            KeyCode::Tab => self.switch(1),
            KeyCode::BackTab => self.switch(-1),
            KeyCode::Right | KeyCode::Char('l') => self.selected = (self.selected + 1).min(n - 1),
            KeyCode::Left | KeyCode::Char('h') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected + cols < n {
                    self.selected += cols;
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected >= cols {
                    self.selected -= cols;
                }
            }
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => self.selected = n - 1,
            KeyCode::Enter | KeyCode::Char('v' | ' ') => return Action::Show(self.case()),
            KeyCode::Char('m') => return Action::Mark(self.case()),
            KeyCode::Char('t') => return Action::Train(self.set),
            _ => {}
        }
        Action::None
    }

    fn switch(&mut self, step: isize) {
        let all = Set::ALL;
        let i = all.iter().position(|&s| s == self.set).unwrap_or(0) as isize;
        self.set = all[(i + step).rem_euclid(all.len() as isize) as usize];
        self.selected = 0;
        self.scroll.set(0);
    }

    /// Keys that make sense held down.
    pub fn repeats(code: KeyCode) -> bool {
        matches!(
            code,
            KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Char('h' | 'j' | 'k' | 'l')
        )
    }
}

/// The case's 54 stickers as they are shown: yellow on top, and for F2L the
/// last-layer pieces gray, so the pair and the slot stand out.
pub fn display_facelets(case: &Case) -> Vec<u8> {
    let state = case.state();
    let mut f: Vec<u8> = state.facelets().to_vec();
    if case.set == Set::F2l {
        for (piece, stickers) in state.cp.iter().zip(&CORNER_FACELETS) {
            if *piece < 4 {
                for &k in stickers {
                    f[k] = GRAY;
                }
            }
        }
        for (piece, stickers) in state.ep.iter().zip(&EDGE_FACELETS) {
            if *piece < 4 {
                for &k in stickers {
                    f[k] = GRAY;
                }
            }
        }
        f[4] = GRAY; // the top center belongs to the last layer too
    }
    f.iter().map(|&c| DISPLAY[c as usize]).collect()
}

/// The case as a cube for the 3D player.
pub fn start_cube(case: &Case) -> Nxn {
    Nxn::from_facelets(3, &display_facelets(case))
}

/// Which stickers to draw in a diagram, and in what color.
fn shown(set: Set, sticker: u8) -> Color {
    match set {
        // OLL is about which stickers are yellow, nothing else
        Set::Oll | Set::TwoLookOll if sticker != YELLOW => STICKER_COLORS[GRAY as usize],
        _ => STICKER_COLORS[sticker as usize],
    }
}

/// Pixel coordinates of the three stickers across a face: each is 2 pixels
/// wide with a 1-pixel gap, after a 1-pixel border and gap.
const STICKER_AT: [usize; 3] = [2, 5, 8];
/// The width and height of the last-layer diagram, in pixels.
const LL_PIXELS: usize = 12;

/// The diagram as a grid of colored pixels (`None` is empty): the last layer
/// seen from above with the back at the top, and the side stickers of the
/// top layer as thin strips around it. For F2L, the front face's two lower
/// rows and the right face's front column follow below, where the slot is.
///
/// A pixel is one column by half a row, so a 2x2-pixel sticker is square on
/// screen and the 1-pixel gaps keep the grid readable.
pub fn diagram_pixels(case: &Case) -> Vec<Vec<Option<Color>>> {
    let f = display_facelets(case);
    let set = case.set;
    let c = |k: usize| Some(shown(set, f[k]));
    let (u, r, front, l, b) = (0, 9, 18, 36, 45);
    let height = if set == Set::F2l {
        LL_PIXELS + 6
    } else {
        LL_PIXELS
    };
    let mut px = vec![vec![None; LL_PIXELS]; height];
    let mut square = |x: usize, y: usize, color: Option<Color>| {
        for dy in 0..2 {
            for dx in 0..2 {
                px[y + dy][x + dx] = color;
            }
        }
    };
    for (row, &y) in STICKER_AT.iter().enumerate() {
        for (col, &x) in STICKER_AT.iter().enumerate() {
            square(x, y, c(u + row * 3 + col));
        }
    }
    if set == Set::F2l {
        // the front face's lower rows, below the top layer
        for row in 1..3 {
            for (col, &x) in STICKER_AT.iter().enumerate() {
                square(x, LL_PIXELS + 1 + (row - 1) * 3, c(front + row * 3 + col));
            }
        }
    }
    // the thin strips: back above (its right sticker on the left, as seen
    // from above), front below, left and right at the sides
    for (i, &at) in STICKER_AT.iter().enumerate() {
        let (x, y) = (at, at);
        for d in 0..2 {
            px[0][x + d] = c(b + 2 - i);
            px[LL_PIXELS - 1][x + d] = c(front + i);
            px[y + d][0] = c(l + i);
            px[y + d][LL_PIXELS - 1] = c(r + 2 - i);
        }
    }
    if set == Set::F2l {
        for row in 1..3 {
            let y = LL_PIXELS + 1 + (row - 1) * 3;
            for d in 0..2 {
                px[y + d][LL_PIXELS - 1] = c(r + row * 3);
            }
        }
    }
    px
}

/// The diagram as half-block text: 12 columns by 6 rows (9 for F2L).
pub fn diagram(case: &Case) -> Vec<Line<'static>> {
    crate::ui::net_lines(&diagram_pixels(case), 1)
}

/// The width of a diagram, in columns.
pub const DIAGRAM_WIDTH: u16 = LL_PIXELS as u16;

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// The tile size: the diagram, its name and its progress mark.
fn tile_size(set: Set) -> (u16, u16) {
    let rows = if set == Set::F2l { 9 } else { 6 };
    (DIAGRAM_WIDTH + 4, rows + 3)
}

/// The full-screen browser.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    browser: &AlgBrowser,
    data: &Data,
    look: Look,
    gfx: &Gfx,
) {
    let progress = &data.progress;
    let [tabs, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    // the set tabs
    let mut spans = vec![Span::styled(
        " algorithms  ",
        Style::new().add_modifier(Modifier::BOLD),
    )];
    for set in Set::ALL {
        let style = if set == browser.set {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            dim()
        };
        spans.push(Span::styled(format!(" {} ", set.name()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), tabs);
    frame.render_widget(
        Paragraph::new(" arrows move  enter show in 3D  m mark  t train  tab next set  esc close")
            .style(dim()),
        footer,
    );

    // a blank row between the tabs and the first row of tiles
    let body = Rect {
        y: body.y + 1,
        height: body.height.saturating_sub(1),
        ..body
    };
    let wide = body.width >= 70;
    let [grid, details] = if wide {
        Layout::horizontal([Constraint::Min(0), Constraint::Length(34)]).areas(body)
    } else {
        [body, Rect::default()]
    };
    draw_grid(frame, grid, browser, progress);
    if wide {
        // your training times, from this set's drill session if there is one
        let drill: &[Solve] = data
            .sessions
            .iter()
            .find(|s| s.drill.as_deref() == Some(browser.set.name()))
            .map_or(&[], |s| s.solves.as_slice());
        draw_details(frame, details, browser, progress, drill, look, gfx);
    }
}

fn progress_of(progress: &BTreeMap<String, String>, case: &Case) -> Progress {
    progress
        .get(&case.key())
        .map_or(Progress::New, |s| Progress::from_name(s))
}

fn draw_grid(
    frame: &mut Frame,
    area: Rect,
    browser: &AlgBrowser,
    progress: &BTreeMap<String, String>,
) {
    let cases = browser.set.cases();
    let (tw, th) = tile_size(browser.set);
    let cols = ((area.width / tw) as usize).max(1);
    let rows_visible = ((area.height / th) as usize).max(1);
    browser.columns.set(cols);
    // keep the selected tile's row on screen
    let sel_row = browser.selected / cols;
    let mut scroll = browser.scroll.get();
    if sel_row < scroll {
        scroll = sel_row;
    } else if sel_row >= scroll + rows_visible {
        scroll = sel_row + 1 - rows_visible;
    }
    browser.scroll.set(scroll);

    for (i, case) in cases.iter().enumerate() {
        let (row, col) = (i / cols, i % cols);
        if row < scroll || row >= scroll + rows_visible {
            continue;
        }
        let x = area.x + (col as u16) * tw;
        let y = area.y + ((row - scroll) as u16) * th;
        if x + tw > area.x + area.width || y + th > area.y + area.height {
            continue;
        }
        let tile = Rect::new(x, y, tw, th);
        let mark = progress_of(progress, case);
        let selected = i == browser.selected;
        let mut lines = diagram(case);
        let name_style = if selected {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
        };
        lines.push(Line::default());
        let name = if case.set == Set::TwoLookOll || case.set == Set::TwoLookPll {
            case.name
                .split(": ")
                .last()
                .unwrap_or(case.name)
                .to_string()
        } else {
            case.name.to_string()
        };
        lines.push(Line::from(vec![Span::styled(
            format!("{name:^w$}", w = DIAGRAM_WIDTH as usize),
            name_style,
        )]));
        lines.push(Line::from(Span::styled(
            format!(
                "{:^w$}",
                if mark == Progress::New {
                    ""
                } else {
                    mark.name()
                },
                w = DIAGRAM_WIDTH as usize
            ),
            Style::new().fg(mark.color()),
        )));
        frame.render_widget(
            Paragraph::new(lines),
            Rect {
                x: tile.x + 1,
                width: tile.width - 1,
                ..tile
            },
        );
    }
    // more rows above or below
    let total_rows = cases.len().div_ceil(cols);
    if scroll > 0 || scroll + rows_visible < total_rows {
        let hint = format!(
            " rows {}-{} of {} ",
            scroll + 1,
            (scroll + rows_visible).min(total_rows),
            total_rows
        );
        let w = hint.len() as u16;
        if area.width > w && area.height > 0 {
            frame.render_widget(
                Paragraph::new(hint).style(dim()),
                Rect::new(area.x + area.width - w, area.y + area.height - 1, w, 1),
            );
        }
    }
}

fn draw_details(
    frame: &mut Frame,
    area: Rect,
    browser: &AlgBrowser,
    progress: &BTreeMap<String, String>,
    drill: &[Solve],
    look: Look,
    gfx: &Gfx,
) {
    let case = browser.case();
    let block = Block::bordered()
        .title(format!(" {} ", case.title()))
        .border_style(dim());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mark = progress_of(progress, case);
    let mut lines = vec![];
    lines.push(Line::from(Span::styled(case.group_name(), dim())));
    lines.push(Line::from(vec![
        Span::styled("progress ", dim()),
        Span::styled(mark.name(), Style::new().fg(mark.color())),
    ]));
    let times = trainer::case_times(drill, &case.key());
    if times.count > 0 {
        let show =
            |ms: Option<u64>| ms.map_or("-".into(), |m| fmt::fmt_avg_ms(m, Precision::Centi));
        lines.push(Line::from(vec![
            Span::styled("trained ", dim()),
            Span::raw(format!(
                "{}x  mean {}  best {}",
                times.count,
                show(times.mean),
                show(times.best)
            )),
        ]));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        case.alg,
        Style::new().add_modifier(Modifier::BOLD),
    )));
    let text_height = 6 + case.alg.len() as u16 / inner.width.max(1);
    let [text, picture] =
        Layout::vertical([Constraint::Length(text_height), Constraint::Min(0)]).areas(inner);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text);
    // the case on the 3D cube, drawn like the 3D view (a real image where
    // the terminal can show one)
    let look = match look {
        Look::Image if !gfx.has_images() => Look::Blocks,
        look => look,
    };
    view3d::draw_cube(
        frame,
        picture,
        &start_cube(case),
        None,
        Camera::default(),
        look,
        gfx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algs::{self, F2L, OLL, PLL};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(browser: &AlgBrowser, progress: &BTreeMap<String, String>, w: u16, h: u16) -> String {
        let data = Data {
            progress: progress.clone(),
            ..Data::default()
        };
        render_data(browser, &data, w, h)
    }

    fn render_data(browser: &AlgBrowser, data: &Data, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|f| draw(f, f.area(), browser, data, Look::Blocks, &Gfx::default()))
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The diagram's stickers, row by row, as letters: the back strip,
    /// three rows of left strip + top face + right strip, the front strip,
    /// and for F2L the front face's two lower rows with the right strip.
    fn picture(case: &Case) -> Vec<String> {
        let letter = |c: Option<Color>| match c {
            Some(Color::Rgb(255, 213, 0)) => 'Y',
            Some(Color::Rgb(245, 245, 245)) => 'W',
            Some(Color::Rgb(0, 168, 84)) => 'G',
            Some(Color::Rgb(0, 82, 204)) => 'B',
            Some(Color::Rgb(214, 40, 40)) => 'R',
            Some(Color::Rgb(255, 128, 0)) => 'O',
            Some(Color::Rgb(92, 92, 92)) => '.',
            _ => ' ',
        };
        let px = diagram_pixels(case);
        let at = |x: usize, y: usize| letter(px[y][x]);
        let last = LL_PIXELS - 1;
        let mut rows = vec![STICKER_AT.iter().map(|&x| at(x, 0)).collect::<String>()];
        for &y in &STICKER_AT {
            let mut r = String::new();
            r.push(at(0, y));
            r.extend(STICKER_AT.iter().map(|&x| at(x, y)));
            r.push(at(last, y));
            rows.push(r);
        }
        rows.push(STICKER_AT.iter().map(|&x| at(x, last)).collect());
        if case.set == Set::F2l {
            for row in 0..2 {
                let y = LL_PIXELS + 1 + row * 3;
                let mut r: String = STICKER_AT.iter().map(|&x| at(x, y)).collect();
                r.push(at(last, y));
                rows.push(r);
            }
        }
        rows
    }

    #[test]
    fn cases_are_shown_yellow_on_top_green_in_front_orange_on_the_right() {
        // a solved last layer: the PLL solved state, drawn
        let solved = Case {
            set: Set::Pll,
            name: "-",
            group: "",
            alg: "",
        };
        // red on the left, orange on the right
        assert_eq!(picture(&solved), ["BBB", "RYYYO", "RYYYO", "RYYYO", "GGG"]);
        let f = display_facelets(&solved);
        assert_eq!(
            (f[4], f[13], f[22]),
            (3, 4, 2),
            "yellow top, orange right, green front"
        );
        assert_eq!(
            (f[31], f[40], f[49]),
            (0, 1, 5),
            "white bottom, red left, blue back"
        );
    }

    #[test]
    fn the_t_perm_has_headlights_on_the_left() {
        // the T-perm case as everyone draws it: the left side's two corner
        // stickers match (headlights), and the right side's don't
        let t = PLL.iter().find(|c| c.name == "T").unwrap();
        let p = picture(t);
        let left: Vec<char> = p[1..4].iter().map(|r| r.chars().next().unwrap()).collect();
        let right: Vec<char> = p[1..4].iter().map(|r| r.chars().last().unwrap()).collect();
        assert_eq!(left[0], left[2], "headlights on the left: {p:?}");
        assert_ne!(right[0], right[2], "{p:?}");
        // every top sticker is yellow: PLL cases are oriented
        assert!(p[1..4].iter().all(|r| r[1..4].chars().all(|c| c == 'Y')));
    }

    #[test]
    fn oll_diagrams_show_only_which_stickers_are_yellow() {
        for case in OLL {
            let p = picture(case);
            assert!(
                p.iter()
                    .flat_map(|r| r.chars())
                    .all(|c| c == 'Y' || c == '.'),
                "{}: {p:?}",
                case.title()
            );
            // 9 yellow stickers in all, counting the side ones
            let yellow = p
                .iter()
                .flat_map(|r| r.chars())
                .filter(|&c| c == 'Y')
                .count();
            assert_eq!(yellow, 9, "{}", case.title());
        }
        // the dot case: only the center is yellow on top
        let dot = OLL.iter().find(|c| c.name == "2").unwrap();
        let top: String = picture(dot)[1..4].iter().map(|r| &r[1..4]).collect();
        assert_eq!(top, "....Y....");
        // OLL 21 (H): all four edges oriented, corners all twisted sideways
        let h = OLL.iter().find(|c| c.name == "21").unwrap();
        let top: String = picture(h)[1..4].iter().map(|r| &r[1..4]).collect();
        assert_eq!(top, ".Y.YYY.Y.");
    }

    #[test]
    fn f2l_diagrams_gray_the_last_layer_and_show_the_slot() {
        for case in F2L {
            let p = picture(case);
            assert_eq!(p.len(), 7, "{}", case.title()); // 5 rows of the top view + 2 of the front
            // the front face below: its two left columns are always green
            for row in &p[5..7] {
                assert_eq!(&row[..2], "GG", "{}: {p:?}", case.title());
            }
            // nothing yellow: the yellow pieces are all last-layer, so gray
            assert!(!p.iter().any(|r| r.contains('Y')), "{}", case.title());
        }
        // F2L 4 (R U R'): the pair is solved by an insert, so before it the
        // slot is empty of its pieces
        let four = F2L.iter().find(|c| c.name == "4").unwrap();
        let p = picture(four);
        assert_ne!(
            &p[6][2..3],
            "G",
            "the slot's front sticker isn't green yet: {p:?}"
        );
    }

    #[test]
    fn every_case_has_a_diagram_of_the_right_size() {
        for case in algs::all_cases() {
            let lines = diagram(case);
            assert_eq!(lines.len(), if case.set == Set::F2l { 9 } else { 6 });
            for l in &lines {
                assert!(l.width() <= DIAGRAM_WIDTH as usize, "{}", case.title());
            }
        }
    }

    #[test]
    fn the_case_cube_is_the_case() {
        // playing the algorithm on the shown cube solves it (in display
        // colors, up to how the cube ends up held)
        for case in [&PLL[15], &OLL[20], &F2L[3]] {
            let mut cube = start_cube(case);
            let steps = crate::alg::steps(case.alg).unwrap();
            for s in &steps {
                cube.apply_all(&s.moves);
            }
            let f = cube.facelets();
            for face in 0..6 {
                let side = &f[face * 9..face * 9 + 9];
                // every non-gray sticker on a face matches the face's center
                let center = side[4];
                assert!(
                    side.iter()
                        .all(|&c| c == center || c == GRAY || center == GRAY),
                    "{}: face {face} {side:?}",
                    case.title()
                );
            }
        }
    }

    #[test]
    fn keys_move_switch_sets_and_ask_for_actions() {
        let mut b = AlgBrowser::default();
        b.columns.set(4);
        assert_eq!(b.key(KeyCode::Right), Action::None);
        assert_eq!(b.selected, 1);
        b.key(KeyCode::Down);
        assert_eq!(b.selected, 5);
        b.key(KeyCode::Up);
        b.key(KeyCode::Up);
        assert_eq!(b.selected, 1, "can't go above the first row");
        b.key(KeyCode::End);
        assert_eq!(b.selected, 20);
        b.key(KeyCode::Right);
        assert_eq!(b.selected, 20, "stops at the last case");
        assert_eq!(b.key(KeyCode::Enter), Action::Show(&PLL[20]));
        assert_eq!(b.key(KeyCode::Char('m')), Action::Mark(&PLL[20]));
        assert_eq!(b.key(KeyCode::Char('t')), Action::Train(Set::Pll));
        b.key(KeyCode::Tab);
        assert_eq!((b.set, b.selected), (Set::Oll, 0));
        b.key(KeyCode::BackTab);
        b.key(KeyCode::BackTab);
        assert_eq!(b.set, Set::TwoLookPll, "wraps around");
        assert_eq!(b.key(KeyCode::Esc), Action::Close);
    }

    #[test]
    fn progress_cycles_and_round_trips() {
        let mut p = Progress::New;
        for want in [Progress::Learning, Progress::Learned, Progress::New] {
            p = p.next();
            assert_eq!(p, want);
            assert_eq!(Progress::from_name(p.name()), p);
        }
        assert_eq!(Progress::from_name("from-the-future"), Progress::New);
    }

    #[test]
    fn the_browser_draws_tiles_details_and_marks() {
        let b = AlgBrowser::default();
        let mut progress = BTreeMap::new();
        progress.insert("PLL:Aa".to_string(), "learned".to_string());
        let out = render(&b, &progress, 140, 44);
        for want in [
            "algorithms",
            "PLL",
            "OLL",
            "F2L",
            "2-look OLL",
            "Aa-perm",
            "x R' U R' D2",
            "learned",
            "Corners only",
        ] {
            assert!(out.contains(want), "missing {want:?}");
        }
        // scrolling hint on a short screen
        let b = AlgBrowser {
            set: Set::Oll,
            ..AlgBrowser::default()
        };
        assert!(render(&b, &progress, 100, 30).contains("rows 1-"));
    }

    #[test]
    fn the_browser_survives_every_set_and_size() {
        for set in Set::ALL {
            let b = AlgBrowser {
                set,
                selected: set.cases().len() - 1,
                ..AlgBrowser::default()
            };
            for (w, h) in [(1, 1), (10, 5), (40, 12), (80, 24), (200, 60)] {
                render(&b, &BTreeMap::new(), w, h);
            }
        }
    }

    /// Yellow stickers on the top face of an OLL case's diagram.
    fn yellow_on_top(case: &Case) -> String {
        picture(case)[1..4]
            .iter()
            .map(|r| r[1..4].to_string())
            .collect()
    }

    // Patterns every OLL chart shows, to pin the numbers to the right cases.
    #[test]
    fn oll_numbers_match_the_standard_patterns() {
        let oll = |n: &str| OLL.iter().find(|c| c.name == n).unwrap();
        let count = |n: &str| yellow_on_top(oll(n)).chars().filter(|&c| c == 'Y').count();
        // the cross cases: 5 yellow on top plus however many corners are up
        for (n, top) in [
            ("21", 5),
            ("22", 5),
            ("23", 7),
            ("24", 7),
            ("25", 7),
            ("26", 6),
            ("27", 6),
        ] {
            assert_eq!(count(n), top, "OLL {n}: {}", yellow_on_top(oll(n)));
        }
        // dot cases: OLL 1 and 2 have only the center up, 3 and 4 also one
        // corner
        for n in ["1", "2"] {
            assert_eq!(yellow_on_top(oll(n)), "....Y....", "OLL {n}");
        }
        for n in ["3", "4"] {
            let top = yellow_on_top(oll(n));
            assert_eq!(count(n), 2, "OLL {n}: {top}");
            assert_eq!(&top[4..5], "Y");
            assert!(
                [0, 2, 6, 8].iter().any(|&k| &top[k..k + 1] == "Y"),
                "OLL {n}: {top}"
            );
        }
        // OLL 20, the X: four corners and the center
        assert_eq!(yellow_on_top(oll("20")), "Y.Y.Y.Y.Y");
        // corners oriented, two edges flipped: everything but two edges
        for n in ["28", "57"] {
            let top = yellow_on_top(oll(n));
            assert_eq!(count(n), 7, "OLL {n}: {top}");
            for corner in [0, 2, 6, 8] {
                assert_eq!(&top[corner..corner + 1], "Y", "OLL {n}: {top}");
            }
        }
        // Sune and Antisune are mirror images: one corner up, the rest twisted
        // opposite ways, so their diagrams differ
        assert_ne!(picture(oll("26")), picture(oll("27")));
    }

    #[test]
    fn stickers_are_square_with_gaps_between_them() {
        let px = diagram_pixels(&PLL[0]);
        // a sticker fills 2x2 pixels, and the pixel after it is a gap
        for &x in &STICKER_AT {
            for &y in &STICKER_AT {
                assert!(px[y][x].is_some() && px[y + 1][x + 1].is_some());
                assert!(
                    px[y + 2][x].is_none() && px[y][x + 2].is_none(),
                    "gap after ({x},{y})"
                );
            }
        }
        // the corners of the diagram are empty
        assert!(px[0][0].is_none() && px[0][LL_PIXELS - 1].is_none());
    }

    #[test]
    fn the_details_show_your_training_times_for_the_case() {
        let mut data = Data::default();
        let i = data.drill_session("PLL");
        for ms in [2_000, 2_400] {
            let mut s = Solve::new(ms, crate::stats::Penalty::Ok, String::new());
            s.case = Some("PLL:Aa".into());
            data.sessions[i].solves.push(s);
        }
        let out = render_data(&AlgBrowser::default(), &data, 140, 44);
        assert!(out.contains("trained 2x  mean 2.20  best 2.00"), "{out}");
        // another case has none
        let b = AlgBrowser {
            selected: 1,
            ..AlgBrowser::default()
        };
        assert!(!render_data(&b, &data, 140, 44).contains("trained"));
    }
}
