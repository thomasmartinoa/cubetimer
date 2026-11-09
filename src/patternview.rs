//! The patterns screen (`c`): famous cube patterns, letters and hearts,
//! the ones you saved, and an editor to paint your own.
//!
//! Every pattern is shown held white on top and green in front, the way you
//! hold the cube to make it. The editor works out the moves for whatever you
//! paint: stickers you leave open are filled in for you.

use std::cell::Cell;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};

use crate::alg;
use crate::cube::{Cube, Move, SOLVED};
use crate::nxn::Nxn;
use crate::patterns::{self, ANY, COLOR_NAMES, Group};
use crate::render3d::Camera;
use crate::store::{Data, SavedPattern};
use crate::ui::{STICKER_COLORS, dim, net_grid_of, net_lines};
use crate::view3d::{self, Gfx, Look};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Classic,
    Letters,
    Mine,
    Create,
}

impl Tab {
    const ALL: [Tab; 4] = [Tab::Classic, Tab::Letters, Tab::Mine, Tab::Create];

    fn name(self) -> &'static str {
        match self {
            Tab::Classic => "classic",
            Tab::Letters => "letters & hearts",
            Tab::Mine => "mine",
            Tab::Create => "create",
        }
    }
}

/// A pattern in one of the lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub name: String,
    pub alg: String,
    pub about: String,
    pub state: Cube,
}

impl Item {
    fn moves(&self) -> usize {
        alg::steps(&self.alg).map_or(0, |s| s.len())
    }
}

/// The patterns of a list tab.
pub fn items(tab: Tab, saved: &[SavedPattern]) -> Vec<Item> {
    let preset = |g| {
        patterns::group(g)
            .into_iter()
            .map(|p| Item {
                name: p.name.into(),
                alg: p.alg.into(),
                about: p.about.into(),
                state: p.state(),
            })
            .collect()
    };
    match tab {
        Tab::Classic => preset(Group::Classic),
        Tab::Letters => preset(Group::Letters),
        Tab::Mine => saved
            .iter()
            .map(|s| Item {
                name: s.name.clone(),
                alg: s.alg.clone(),
                about: "One you made.".into(),
                state: alg::effect(&s.alg).unwrap_or(SOLVED),
            })
            .collect(),
        Tab::Create => Vec::new(),
    }
}

/// What the editor knows about the painted stickers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// A real cube, and the moves that make it from solved.
    Cube { state: Cube, moves: Vec<Move> },
    /// Why no real cube looks like this.
    Impossible(String),
}

/// How the editor's net was last drawn, so a mouse click finds its sticker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Geometry {
    x: u16,
    y: u16,
    /// Columns and rows from one sticker to the next.
    pitch_x: u16,
    pitch_y: u16,
    /// Columns and rows a sticker covers.
    width: u16,
    height: u16,
}

impl Geometry {
    /// The screen column and row of the top-left of a net cell.
    fn origin(&self, row: usize, col: usize) -> (u16, u16) {
        // one extra column and row between faces
        let x = self.x + col as u16 * self.pitch_x + (col / 3) as u16;
        let y = self.y + row as u16 * self.pitch_y + (row / 3) as u16;
        (x, y)
    }

    /// The net cell under a screen position, if it is on a sticker.
    fn cell_at(&self, column: u16, row: u16) -> Option<(usize, usize)> {
        (0..9)
            .flat_map(|r| (0..12).map(move |c| (r, c)))
            .find(|&(r, c)| {
                let (x, y) = self.origin(r, c);
                face_at(r, c).is_some()
                    && (x..x + self.width).contains(&column)
                    && (y..y + self.height).contains(&row)
            })
    }
}

/// The net as 9 rows by 12 columns of stickers: which face a cell shows.
fn face_at(row: usize, col: usize) -> Option<usize> {
    match (row / 3, col / 3) {
        (0, 1) => Some(0), // U
        (1, 0) => Some(4), // L
        (1, 1) => Some(2), // F
        (1, 2) => Some(1), // R
        (1, 3) => Some(5), // B
        (2, 1) => Some(3), // D
        _ => None,
    }
}

fn sticker_at(row: usize, col: usize) -> Option<usize> {
    face_at(row, col).map(|f| f * 9 + (row % 3) * 3 + col % 3)
}

fn cell_of(sticker: usize) -> (usize, usize) {
    let (face, k) = (sticker / 9, sticker % 9);
    let (band, slot) = match face {
        0 => (0, 1),
        4 => (1, 0),
        2 => (1, 1),
        1 => (1, 2),
        5 => (1, 3),
        _ => (2, 1),
    };
    (band * 3 + k / 3, slot * 3 + k % 3)
}

const FACE_NAMES: [&str; 6] = ["top", "right", "front", "bottom", "left", "back"];
const SPOT_NAMES: [&str; 9] = [
    "top left",
    "top",
    "top right",
    "left",
    "center",
    "right",
    "bottom left",
    "bottom",
    "bottom right",
];

/// The pattern editor: 54 stickers, each a color or open.
#[derive(Debug)]
pub struct Editor {
    pub stickers: [u8; 54],
    pub cursor: usize,
    /// The color `space` paints.
    pub brush: u8,
    /// The answer for the current stickers, once known.
    pub found: Option<Found>,
    /// A search running in the background, for these stickers.
    job: Option<([u8; 54], Receiver<Found>)>,
    geometry: Cell<Option<Geometry>>,
}

/// Every sticker open except the centers.
fn blank() -> [u8; 54] {
    let mut f = [ANY; 54];
    for face in 0..6 {
        f[face * 9 + 4] = face as u8;
    }
    f
}

/// How long the editor looks for a shorter way to make a pattern.
const SHORTEN_FOR: Duration = Duration::from_millis(800);

impl Default for Editor {
    fn default() -> Self {
        let mut e = Editor {
            stickers: blank(),
            cursor: 18, // front, top left
            brush: 1,
            found: None,
            job: None,
            geometry: Cell::new(None),
        };
        e.changed();
        e
    }
}

impl Editor {
    pub fn load(&mut self, stickers: [u8; 54]) {
        self.stickers = stickers;
        self.changed();
    }

    /// The stickers changed: check them now, and find the moves in the
    /// background.
    fn changed(&mut self) {
        self.found = patterns::quick_check(&self.stickers).map(Found::Impossible);
        if self.found.is_none() && self.job.is_none() {
            self.start_job();
        }
    }

    fn start_job(&mut self) {
        let f = self.stickers;
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let found = match patterns::complete(&f) {
                Ok(state) => Found::Cube {
                    state,
                    moves: patterns::moves_to_make(&state, SHORTEN_FOR),
                },
                Err(why) => Found::Impossible(why),
            };
            let _ = tx.send(found);
        });
        self.job = Some((f, rx));
    }

    /// Pick up a finished search; start another if the stickers changed
    /// while it ran.
    pub fn poll(&mut self) {
        let Some((f, rx)) = &self.job else {
            return;
        };
        let found = match rx.try_recv() {
            Ok(found) => Some(found),
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => None,
        };
        let current = *f == self.stickers;
        self.job = None;
        if current {
            self.found = found;
        } else if self.found.is_none() {
            self.start_job();
        }
    }

    /// Whether moves are still being looked for.
    pub fn working(&self) -> bool {
        self.found.is_none()
    }

    /// Wait for the search to finish (tests).
    #[cfg(test)]
    pub fn wait(&mut self) {
        while self.working() {
            std::thread::sleep(Duration::from_millis(5));
            self.poll();
        }
    }

    fn paint(&mut self, color: u8) {
        if self.cursor % 9 == 4 || self.stickers[self.cursor] == color {
            return;
        }
        self.stickers[self.cursor] = color;
        self.changed();
    }

    /// The next sticker of the same face in reading order, skipping the
    /// center: typing colors fills a face row by row.
    fn advance(&mut self) {
        let (face, mut k) = (self.cursor / 9, self.cursor % 9);
        k = (k + 1) % 9;
        if k == 4 {
            k = 5;
        }
        self.cursor = face * 9 + k;
    }

    fn step(&mut self, dr: isize, dc: isize) {
        let (mut r, mut c) = cell_of(self.cursor);
        for _ in 0..12 {
            let nr = r as isize + dr;
            if !(0..9).contains(&nr) {
                return;
            }
            r = nr as usize;
            // the middle band wraps round, like the cube
            let nc = c as isize + dc;
            c = if r / 3 == 1 {
                nc.rem_euclid(12) as usize
            } else if (0..12).contains(&nc) {
                nc as usize
            } else {
                return;
            };
            if let Some(s) = sticker_at(r, c) {
                self.cursor = s;
                return;
            }
        }
    }

    fn fill_face(&mut self, color: u8) {
        let face = self.cursor / 9;
        for k in (0..9).filter(|&k| k != 4) {
            self.stickers[face * 9 + k] = color;
        }
        self.changed();
    }

    /// The moves, once found.
    pub fn alg(&self) -> Option<String> {
        match &self.found {
            Some(Found::Cube { moves, .. }) if !moves.is_empty() => Some(patterns::format(moves)),
            _ => None,
        }
    }

    fn mouse(&mut self, kind: MouseEventKind, column: u16, row: u16) {
        let Some(g) = self.geometry.get() else {
            return;
        };
        let paint =
            match kind {
                MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left) => self.brush,
                MouseEventKind::Down(MouseButton::Right)
                | MouseEventKind::Drag(MouseButton::Right) => ANY,
                _ => return,
            };
        if let Some(s) = g.cell_at(column, row).and_then(|(r, c)| sticker_at(r, c)) {
            self.cursor = s;
            self.paint(paint);
        }
    }
}

/// The browser's state; owned by the app.
#[derive(Debug)]
pub struct PatternBrowser {
    pub tab: Tab,
    pub selected: usize,
    pub editor: Editor,
    /// Asked "delete this pattern?" and waiting for y.
    pub confirm_delete: bool,
    /// A short message for the footer.
    pub note: Option<String>,
    columns: Cell<usize>,
    scroll: Cell<usize>,
}

impl Default for PatternBrowser {
    fn default() -> Self {
        PatternBrowser {
            tab: Tab::Classic,
            selected: 0,
            editor: Editor::default(),
            confirm_delete: false,
            note: None,
            columns: Cell::new(1),
            scroll: Cell::new(0),
        }
    }
}

/// What a key asks the app to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Close,
    /// Play these moves on the 3D cube, from solved.
    Play {
        title: String,
        alg: String,
    },
    Copy(String),
    /// Ask for a name and save the editor's pattern.
    Save,
    /// Ask for moves to load into the editor.
    AskMoves,
    /// Delete saved pattern `i`.
    Delete(usize),
}

impl PatternBrowser {
    pub fn key(&mut self, code: KeyCode, saved: &[SavedPattern]) -> Action {
        self.note = None;
        if std::mem::take(&mut self.confirm_delete) {
            if code == KeyCode::Char('y') && self.selected < saved.len() {
                return Action::Delete(self.selected);
            }
            return Action::None;
        }
        match code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Tab => {
                self.switch(1);
                return Action::None;
            }
            KeyCode::BackTab => {
                self.switch(-1);
                return Action::None;
            }
            _ => {}
        }
        if self.tab == Tab::Create {
            return self.editor_key(code);
        }
        let list = items(self.tab, saved);
        let n = list.len();
        let cols = self.columns.get().max(1);
        match code {
            KeyCode::Char('q' | 'c') => return Action::Close,
            KeyCode::Right | KeyCode::Char('l') => {
                self.selected = (self.selected + 1).min(n.saturating_sub(1))
            }
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
            KeyCode::End | KeyCode::Char('G') => self.selected = n.saturating_sub(1),
            _ => {}
        }
        let Some(item) = list.get(self.selected) else {
            return Action::None;
        };
        match code {
            KeyCode::Enter | KeyCode::Char('v' | ' ') => Action::Play {
                title: format!("{}   {}", item.name, item.alg),
                alg: item.alg.clone(),
            },
            KeyCode::Char('y') => Action::Copy(item.alg.clone()),
            KeyCode::Char('e') => {
                self.editor.load(item.state.facelets());
                self.tab = Tab::Create;
                self.note = Some(format!("editing a copy of {}", item.name));
                Action::None
            }
            KeyCode::Char('d' | 'x') | KeyCode::Delete | KeyCode::Backspace
                if self.tab == Tab::Mine =>
            {
                self.confirm_delete = true;
                Action::None
            }
            _ => Action::None,
        }
    }

    fn editor_key(&mut self, code: KeyCode) -> Action {
        let e = &mut self.editor;
        let color = |c: char| match c {
            'w' | '1' => Some(0),
            'r' | '2' => Some(1),
            'g' | '3' => Some(2),
            'y' | '4' => Some(3),
            'o' | '5' => Some(4),
            'b' | '6' => Some(5),
            _ => None,
        };
        match code {
            KeyCode::Char('q') => return Action::Close,
            KeyCode::Char(c) if color(c).is_some() => {
                let c = color(c).unwrap_or(0);
                e.brush = c;
                if e.cursor % 9 == 4 {
                    self.note = Some("centers can't change: they set each face's color".into());
                } else {
                    e.paint(c);
                    e.advance();
                }
            }
            KeyCode::Char(' ') => {
                if e.cursor % 9 == 4 {
                    self.note = Some("centers can't change: they set each face's color".into());
                }
                e.paint(e.brush);
            }
            KeyCode::Char('x') | KeyCode::Backspace | KeyCode::Delete => e.paint(ANY),
            KeyCode::Up | KeyCode::Char('k') => e.step(-1, 0),
            KeyCode::Down | KeyCode::Char('j') => e.step(1, 0),
            KeyCode::Left | KeyCode::Char('h') => e.step(0, -1),
            KeyCode::Right | KeyCode::Char('l') => e.step(0, 1),
            KeyCode::Char('f') => e.fill_face(e.brush),
            KeyCode::Char('F') => e.fill_face(ANY),
            KeyCode::Char('e') => e.load(blank()),
            KeyCode::Char('n') => e.load(SOLVED.facelets()),
            KeyCode::Char('a') => return Action::AskMoves,
            KeyCode::Enter | KeyCode::Char('v') => match e.alg() {
                Some(alg) => {
                    return Action::Play {
                        title: format!("your pattern   {alg}"),
                        alg,
                    };
                }
                None => self.note = Some(self.not_ready().into()),
            },
            KeyCode::Char('s') => match e.alg() {
                Some(_) => return Action::Save,
                None => self.note = Some(self.not_ready().into()),
            },
            KeyCode::Char('Y') => match e.alg() {
                Some(alg) => return Action::Copy(alg),
                None => self.note = Some(self.not_ready().into()),
            },
            _ => {}
        }
        Action::None
    }

    fn not_ready(&self) -> &'static str {
        match &self.editor.found {
            None => "still finding the moves",
            Some(Found::Impossible(_)) => "no real cube looks like this yet",
            Some(Found::Cube { .. }) => "that's a solved cube: paint something first",
        }
    }

    fn switch(&mut self, step: isize) {
        let all = Tab::ALL;
        let i = all.iter().position(|&t| t == self.tab).unwrap_or(0) as isize;
        self.tab = all[(i + step).rem_euclid(all.len() as isize) as usize];
        self.selected = 0;
        self.scroll.set(0);
    }

    /// Keep the selection on the list after it shrank.
    pub fn clamp(&mut self, len: usize) {
        self.selected = self.selected.min(len.saturating_sub(1));
    }

    pub fn mouse(&mut self, kind: MouseEventKind, column: u16, row: u16) {
        if self.tab == Tab::Create {
            self.editor.mouse(kind, column, row);
        }
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

/// The full-screen browser.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    browser: &PatternBrowser,
    data: &Data,
    look: Look,
    gfx: &Gfx,
) {
    let [tabs, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);
    let mut spans = vec![Span::styled(
        " patterns  ",
        Style::new().add_modifier(Modifier::BOLD),
    )];
    for tab in Tab::ALL {
        let style = if tab == browser.tab {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            dim()
        };
        spans.push(Span::styled(format!(" {} ", tab.name()), style));
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), tabs);

    let list = items(browser.tab, &data.patterns);
    let keys = if browser.confirm_delete {
        match list.get(browser.selected) {
            Some(item) => format!(" delete {}? y deletes, any other key keeps it", item.name),
            None => String::new(),
        }
    } else if let Some(note) = &browser.note {
        format!(" {note}")
    } else if browser.tab == Tab::Create {
        " arrows move  w r g y o b paint  x open  enter watch in 3D  s save  Y copy  tab next  esc close"
            .into()
    } else if browser.tab == Tab::Mine {
        " arrows move  enter watch in 3D  y copy moves  e edit  d delete  tab next  esc close"
            .into()
    } else {
        " arrows move  enter watch in 3D  y copy moves  e edit a copy  tab next  esc close".into()
    };
    let footer_style = if browser.confirm_delete || browser.note.is_some() {
        Style::new().fg(Color::Yellow)
    } else {
        dim()
    };
    frame.render_widget(Paragraph::new(keys).style(footer_style), footer);

    let body = Rect {
        y: body.y + 1,
        height: body.height.saturating_sub(1),
        ..body
    };
    let look = match look {
        Look::Image if !gfx.has_images() => Look::Blocks,
        look => look,
    };
    if browser.tab == Tab::Create {
        draw_editor(frame, body, &browser.editor, look, gfx);
        return;
    }
    if list.is_empty() {
        let text = vec![
            Line::from("No patterns of your own yet."),
            Line::default(),
            Line::from(Span::styled(
                "Paint one in create (tab), or press e on any pattern to change it,",
                dim(),
            )),
            Line::from(Span::styled("then press s to save it here.", dim())),
        ];
        frame.render_widget(
            Paragraph::new(text),
            body.inner(ratatui::layout::Margin::new(2, 1)),
        );
        return;
    }
    let wide = body.width >= 70;
    let [grid, details] = if wide {
        Layout::horizontal([Constraint::Min(0), Constraint::Length(36)]).areas(body)
    } else {
        [body, Rect::default()]
    };
    draw_grid(frame, grid, browser, &list);
    if wide && let Some(item) = list.get(browser.selected.min(list.len() - 1)) {
        draw_details(frame, details, item, look, gfx);
    }
}

/// A tile: the pattern's net and its name.
const TILE: (u16, u16) = (19, 8);

fn draw_grid(frame: &mut Frame, area: Rect, browser: &PatternBrowser, list: &[Item]) {
    let (tw, th) = TILE;
    let cols = ((area.width / tw) as usize).max(1);
    let rows_visible = ((area.height / th) as usize).max(1);
    browser.columns.set(cols);
    let sel_row = browser.selected / cols;
    let mut scroll = browser.scroll.get();
    if sel_row < scroll {
        scroll = sel_row;
    } else if sel_row >= scroll + rows_visible {
        scroll = sel_row + 1 - rows_visible;
    }
    browser.scroll.set(scroll);
    for (i, item) in list.iter().enumerate() {
        let (row, col) = (i / cols, i % cols);
        if row < scroll || row >= scroll + rows_visible {
            continue;
        }
        let x = area.x + col as u16 * tw;
        let y = area.y + (row - scroll) as u16 * th;
        if x + tw > area.x + area.width || y + th > area.y + area.height {
            continue;
        }
        let mut lines: Vec<Line> = net_lines(&net_grid_of(&nxn_of(&item.state)), 1)
            .into_iter()
            .map(|l| {
                let mut spans = vec![Span::raw(" ")];
                spans.extend(l.spans);
                Line::from(spans)
            })
            .collect();
        let style = if i == browser.selected {
            Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
        } else {
            Style::new()
        };
        let room = tw as usize - 2;
        let name: String = if item.name.chars().count() > room {
            let mut n: String = item.name.chars().take(room - 1).collect();
            n.push('\u{2026}');
            n
        } else {
            item.name.clone()
        };
        lines.push(Line::from(Span::styled(
            format!("{name:^w$}", w = tw as usize - 2),
            style,
        )));
        frame.render_widget(Paragraph::new(lines), Rect::new(x, y, tw - 1, th - 1));
    }
    let total_rows = list.len().div_ceil(cols);
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

fn nxn_of(state: &Cube) -> Nxn {
    Nxn::from_facelets(3, &state.facelets())
}

fn draw_details(frame: &mut Frame, area: Rect, item: &Item, look: Look, gfx: &Gfx) {
    let block = Block::bordered()
        .title(format!(" {} ", item.name))
        .border_style(dim());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = vec![
        Line::from(item.about.as_str()),
        Line::default(),
        Line::from(Span::styled(
            format!("{} moves, white on top, green in front:", item.moves()),
            dim(),
        )),
        Line::from(Span::styled(
            item.alg.as_str(),
            Style::new().add_modifier(Modifier::BOLD),
        )),
    ];
    let width = inner.width.max(1) as usize;
    let wrapped = |s: &str| (s.len() / width) as u16 + 1;
    let text_height = wrapped(&item.about) + 2 + wrapped(&item.alg) + 1;
    let [text, picture] =
        Layout::vertical([Constraint::Length(text_height), Constraint::Min(0)]).areas(inner);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), text);
    view3d::draw_cube(
        frame,
        picture,
        &nxn_of(&item.state),
        None,
        Camera::default(),
        look,
        gfx,
    );
}

/// Text that reads well on a sticker of this color.
fn ink(color: u8) -> Color {
    match color {
        1 | 5 | 6 => Color::White,
        _ => Color::Black,
    }
}

fn draw_editor(frame: &mut Frame, area: Rect, e: &Editor, look: Look, gfx: &Gfx) {
    const PANE: u16 = 40;
    // Stickers are 4 or 2 columns wide, and 2 rows tall with the second row
    // half drawn, so stickers stand apart. On a very small screen they are
    // 1 row tall and touch the ones above and below.
    let big = Geometry {
        x: 0,
        y: 0,
        pitch_x: 5,
        pitch_y: 2,
        width: 4,
        height: 2,
    };
    let small = Geometry {
        pitch_x: 3,
        width: 2,
        ..big
    };
    let tiny = Geometry {
        pitch_y: 1,
        height: 1,
        ..small
    };
    let size = |g: &Geometry| (12 * g.pitch_x + 3, 9 * g.pitch_y + 2);
    let beside = |g: Geometry, pane: u16| {
        let (w, h) = size(&g);
        (area.width >= w + 2 + pane && area.height >= h).then(|| {
            let [n, _, p] = Layout::horizontal([
                Constraint::Length(w),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .areas(area);
            (g, n, p)
        })
    };
    let below = |g: Geometry| {
        let (w, h) = size(&g);
        (area.width >= w && area.height >= h + 4).then(|| {
            let [n, _, p] = Layout::vertical([
                Constraint::Length(h),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .areas(area);
            (g, n, p)
        })
    };
    let fits = beside(big, PANE)
        .or_else(|| beside(small, 30))
        .or_else(|| beside(tiny, 30))
        .or_else(|| below(tiny));
    let (mut g, net_area, pane) = if let Some(fit) = fits {
        fit
    } else {
        e.geometry.set(None);
        frame.render_widget(
            Paragraph::new("Make the window bigger to paint a pattern.").style(dim()),
            area,
        );
        return;
    };
    g.x = net_area.x + 1;
    g.y = net_area.y;
    e.geometry.set(Some(g));

    // open stickers show the color they were filled with, once known
    let filled = match &e.found {
        Some(Found::Cube { state, .. }) => Some(state.facelets()),
        _ => None,
    };
    let buf = frame.buffer_mut();
    for s in 0..54 {
        let (r, c) = cell_of(s);
        let (x, y) = g.origin(r, c);
        let painted = e.stickers[s];
        let shown = match (painted, filled) {
            (ANY, Some(f)) => f[s],
            (ANY, None) => 6,
            (p, _) => p,
        };
        let color = STICKER_COLORS[shown as usize];
        let mark = if s == e.cursor {
            if g.width >= 4 {
                " \u{25c0}\u{25b6} "
            } else {
                "\u{25c0}\u{25b6}"
            }
        } else if painted == ANY {
            if g.width >= 4 { " ?  " } else { "? " }
        } else if g.width >= 4 {
            "    "
        } else {
            "  "
        };
        let text_style = Style::new()
            .bg(color)
            .fg(ink(shown))
            .add_modifier(Modifier::BOLD);
        let open_style = Style::new().bg(color).fg(ink(shown));
        let style = if s == e.cursor {
            text_style
        } else {
            open_style
        };
        if y < buf.area.bottom() && x + g.width <= buf.area.right() {
            buf.set_string(x, y, mark, style);
        }
        if g.height == 2 && y + 1 < buf.area.bottom() && x + g.width <= buf.area.right() {
            // the lower half: colored on top, the gap below
            buf.set_string(
                x,
                y + 1,
                "\u{2580}".repeat(g.width as usize),
                Style::new().fg(color),
            );
        }
    }
    draw_editor_pane(frame, pane, e, look, gfx);
}

fn draw_editor_pane(frame: &mut Frame, area: Rect, e: &Editor, look: Look, gfx: &Gfx) {
    let swatch = |c: u8| Span::styled("  ", Style::new().bg(STICKER_COLORS[c as usize]));
    let key = |k: &'static str| Span::styled(k, Style::new().fg(Color::Cyan));
    let (face, spot) = (e.cursor / 9, e.cursor % 9);
    let mut lines = vec![
        Line::from(vec![
            Span::styled("brush   ", dim()),
            swatch(e.brush),
            Span::raw(format!(" {}", COLOR_NAMES[e.brush as usize])),
        ]),
        Line::from(vec![
            Span::styled("sticker ", dim()),
            Span::raw(format!("{}, {}", FACE_NAMES[face], SPOT_NAMES[spot])),
            Span::styled(if spot == 4 { "  (fixed)" } else { "" }, dim()),
        ]),
        Line::default(),
    ];
    let palette: [(&str, u8); 6] = [("w", 0), ("y", 3), ("r", 1), ("o", 4), ("g", 2), ("b", 5)];
    for pair in palette.chunks(2) {
        let mut spans = vec![];
        for &(k, c) in pair {
            spans.extend([
                key(k),
                Span::raw(" "),
                swatch(c),
                Span::raw(format!(" {:<9}", COLOR_NAMES[c as usize])),
            ]);
        }
        lines.push(Line::from(spans));
    }
    lines.extend([
        Line::from(vec![key("x"), Span::raw(" ? open: any color will do")]),
        Line::from(vec![
            key("space"),
            Span::raw(" paint  "),
            key("f"),
            Span::raw(" fill face  "),
            key("F"),
            Span::raw(" open face"),
        ]),
        Line::from(vec![
            key("e"),
            Span::raw(" open all  "),
            key("n"),
            Span::raw(" solved  "),
            key("a"),
            Span::raw(" type moves"),
        ]),
        Line::default(),
    ]);
    match &e.found {
        None => lines.push(Line::from(Span::styled(
            "finding the moves...",
            Style::new().fg(Color::Yellow),
        ))),
        Some(Found::Impossible(why)) => {
            lines.push(Line::from(Span::styled(
                "\u{2717} no real cube looks like this",
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(why.as_str()));
        }
        Some(Found::Cube { moves, .. }) if moves.is_empty() => {
            lines.push(Line::from(Span::styled(
                "That's a solved cube: paint the stickers you want, leave the rest open (?).",
                dim(),
            )));
        }
        Some(Found::Cube { moves, .. }) => {
            lines.push(Line::from(Span::styled(
                "\u{2713} a real cube",
                Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                format!(
                    "{} moves from solved, white on top, green in front:",
                    moves.len()
                ),
                dim(),
            )));
            lines.push(Line::from(Span::styled(
                patterns::format(moves),
                Style::new().add_modifier(Modifier::BOLD),
            )));
            if e.stickers.contains(&ANY) {
                lines.push(Line::from(Span::styled(
                    "Open stickers (?) show the colors they get.",
                    dim(),
                )));
            }
            lines.push(Line::from(vec![
                key("enter"),
                Span::raw(" watch in 3D  "),
                key("s"),
                Span::raw(" save  "),
                key("Y"),
                Span::raw(" copy"),
            ]));
        }
    }
    // the finished pattern on the cube, in the space left below
    let width = area.width.max(1) as usize;
    let used: usize = lines.iter().map(|l| l.width().max(1).div_ceil(width)).sum();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), area);
    let left = area.height.saturating_sub(used as u16 + 1);
    if let Some(Found::Cube { state, moves }) = &e.found
        && !moves.is_empty()
        && left >= 8
    {
        let picture = Rect {
            y: area.bottom() - left,
            height: left,
            ..area
        };
        view3d::draw_cube(
            frame,
            picture,
            &nxn_of(state),
            None,
            Camera::default(),
            look,
            gfx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::PATTERNS;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(browser: &PatternBrowser, data: &Data, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal
            .draw(|f| draw(f, f.area(), browser, data, Look::Blocks, &Gfx::default()))
            .unwrap();
        let buf = terminal.backend().buffer();
        (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_cell_maps_back_to_its_sticker() {
        for s in 0..54 {
            let (r, c) = cell_of(s);
            assert_eq!(sticker_at(r, c), Some(s));
        }
    }

    #[test]
    fn the_cursor_crosses_faces_and_wraps_round_the_middle() {
        let mut e = Editor::default();
        assert_eq!(e.cursor, 18, "starts on the front, top left");
        e.step(0, -1);
        assert_eq!(e.cursor, 36 + 2, "into the left face");
        e.step(0, -3);
        e.step(0, -1);
        e.step(0, -1);
        assert_eq!(e.cursor / 9, 5, "round to the back");
        e.cursor = 18;
        e.step(-1, 0);
        assert_eq!(e.cursor, 6, "up onto the top face");
        e.cursor = 0;
        e.step(0, -1);
        assert_eq!(e.cursor, 0, "nothing left of the top face");
    }

    #[test]
    fn typing_colors_fills_a_face_row_by_row_and_skips_the_center() {
        let mut b = PatternBrowser {
            tab: Tab::Create,
            ..Default::default()
        };
        b.editor.cursor = 18;
        // red corners, green edges
        for c in "rgrggrgr".chars() {
            b.key(KeyCode::Char(c), &[]);
        }
        let front: Vec<u8> = b.editor.stickers[18..27].to_vec();
        assert_eq!(front, vec![1, 2, 1, 2, 2, 2, 1, 2, 1]);
        assert_eq!(b.editor.cursor, 18, "back to the start of the face");
        b.editor.wait();
        assert!(matches!(b.editor.found, Some(Found::Cube { .. })));
    }

    #[test]
    fn a_painted_heart_gets_moves_that_make_it() {
        let mut b = PatternBrowser {
            tab: Tab::Create,
            ..Default::default()
        };
        b.editor.cursor = 18;
        for c in "grgggrgr".chars() {
            b.key(KeyCode::Char(c), &[]);
        }
        b.editor.wait();
        let alg = b.editor.alg().expect("moves");
        let state = alg::effect(&alg).unwrap();
        let f = state.facelets();
        assert_eq!(&f[18..27], &[2, 1, 2, 2, 2, 2, 1, 2, 1]);
        match b.key(KeyCode::Enter, &[]) {
            Action::Play { alg: a, .. } => assert_eq!(a, alg),
            other => panic!("{other:?}"),
        }
        assert_eq!(b.key(KeyCode::Char('s'), &[]), Action::Save);
    }

    #[test]
    fn an_impossible_painting_says_so_at_once() {
        let mut b = PatternBrowser {
            tab: Tab::Create,
            ..Default::default()
        };
        b.editor.load(SOLVED.facelets());
        b.editor.cursor = 7; // top, bottom middle: the top-front edge
        b.key(KeyCode::Char('y'), &[]);
        assert!(matches!(b.editor.found, Some(Found::Impossible(_))));
        b.key(KeyCode::Char('s'), &[]);
        assert!(b.note.is_some());
        let shown = render(&b, &Data::default(), 120, 40);
        assert!(shown.contains("no real cube looks like this"), "{shown}");
    }

    #[test]
    fn centers_cannot_be_painted() {
        let mut b = PatternBrowser {
            tab: Tab::Create,
            ..Default::default()
        };
        b.editor.cursor = 22;
        b.key(KeyCode::Char('w'), &[]);
        assert_eq!(b.editor.stickers[22], 2);
        assert!(b.note.as_deref().unwrap().contains("centers"));
    }

    #[test]
    fn lists_play_copy_edit_and_delete() {
        let saved = vec![SavedPattern {
            name: "mine".into(),
            alg: "R U".into(),
        }];
        let mut b = PatternBrowser::default();
        match b.key(KeyCode::Enter, &saved) {
            Action::Play { alg, .. } => assert_eq!(alg, PATTERNS[0].alg),
            other => panic!("{other:?}"),
        }
        assert_eq!(
            b.key(KeyCode::Char('y'), &saved),
            Action::Copy(PATTERNS[0].alg.into())
        );
        b.key(KeyCode::Char('e'), &saved);
        assert_eq!(b.tab, Tab::Create);
        assert_eq!(b.editor.stickers, PATTERNS[0].state().facelets());
        b.tab = Tab::Mine;
        b.key(KeyCode::Char('d'), &saved);
        assert!(b.confirm_delete);
        assert_eq!(b.key(KeyCode::Char('y'), &saved), Action::Delete(0));
        // anything else keeps it
        b.key(KeyCode::Char('d'), &saved);
        assert_eq!(b.key(KeyCode::Char('n'), &saved), Action::None);
        assert!(!b.confirm_delete);
    }

    #[test]
    fn draws_every_tab_at_any_size() {
        let mut data = Data::default();
        let mut b = PatternBrowser::default();
        for (w, h) in [(160, 50), (100, 30), (80, 24), (40, 12), (10, 5)] {
            for tab in Tab::ALL {
                b.tab = tab;
                let shown = render(&b, &data, w, h);
                if w >= 80 {
                    assert!(shown.contains("patterns"), "{tab:?} {w}x{h}");
                }
            }
        }
        b.tab = Tab::Mine;
        assert!(render(&b, &data, 100, 30).contains("No patterns of your own yet"));
        data.patterns.push(SavedPattern {
            name: "zigzag".into(),
            alg: "R U R' U'".into(),
        });
        assert!(render(&b, &data, 100, 30).contains("zigzag"));
        b.tab = Tab::Classic;
        let shown = render(&b, &data, 120, 40);
        assert!(shown.contains("Checkerboard") && shown.contains(PATTERNS[0].alg));
    }

    #[test]
    fn a_click_paints_the_sticker_under_it() {
        let mut b = PatternBrowser {
            tab: Tab::Create,
            ..Default::default()
        };
        render(&b, &Data::default(), 120, 40);
        let g = b.editor.geometry.get().expect("drawn");
        let (r, c) = cell_of(20);
        let (x, y) = g.origin(r, c);
        b.editor.brush = 5;
        b.mouse(MouseEventKind::Down(MouseButton::Left), x + 1, y);
        assert_eq!(b.editor.stickers[20], 5);
        b.mouse(MouseEventKind::Down(MouseButton::Right), x, y + 1);
        assert_eq!(b.editor.stickers[20], ANY);
    }
}
