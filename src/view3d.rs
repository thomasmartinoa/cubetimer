//! The cube view (`v`): the scrambled cube in 3D, which you can turn around
//! and watch being scrambled move by move, or as a flat net.
//!
//! The cube is drawn as a real image where the terminal can show one (kitty
//! graphics in kitty and Ghostty, sixel in foot, the iTerm2 protocol in
//! WezTerm) and with half-block characters everywhere else.

use std::cell::RefCell;
use std::time::Instant;

use image::{DynamicImage, RgbaImage};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Flex, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Paragraph, Wrap};
use ratatui_image::FontSize;
use ratatui_image::StatefulImage;
use ratatui_image::picker::{Capability, Picker, ProtocolType};
use ratatui_image::protocol::kitty::StatefulKitty;
use ratatui_image::protocol::{StatefulProtocol, StatefulProtocolType};

use crate::anim::Player;
use crate::nxn::{self, Nxn};
use crate::render3d::{self, Camera, Turning};
use crate::ui;

/// How much one arrow press turns the cube.
const TURN_DEGREES: f64 = 15.0;
/// The largest image (pixels per side). Big enough to fill a high-DPI
/// screen; sixel is held smaller because encoding it grows with the area and
/// is what limits foot's frame rate.
const MAX_PIXELS: usize = 1200;
const MAX_PIXELS_SIXEL: usize = 900;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Cube3d,
    Net,
}

/// What the view shows; owned by the app.
#[derive(Debug, Clone)]
pub struct CubeView {
    /// Shown in the title: "3x3 scramble", "solve 18", ...
    pub title: String,
    pub size: usize,
    pub player: Player,
    pub camera: Camera,
    pub mode: Mode,
    /// Where a mouse drag last was, while one is in progress.
    drag_from: Option<(u16, u16)>,
}

impl CubeView {
    /// A view of `scramble` on a cube of `size`. `None` if the scramble can't
    /// be read (an empty one shows a solved cube).
    pub fn new(title: impl Into<String>, size: usize, scramble: &str) -> Option<CubeView> {
        let moves = nxn::parse(scramble)?;
        Some(CubeView {
            title: title.into(),
            size,
            player: Player::new(size, moves),
            camera: Camera::default(),
            mode: Mode::Cube3d,
            drag_from: None,
        })
    }

    /// Whether the view needs redrawing on its own (a turn is animating).
    pub fn animating(&self) -> bool {
        self.player.is_playing()
    }

    pub fn tick(&mut self, now: Instant) {
        self.player.advance(now);
    }

    /// Handle a key; `false` means close the view.
    pub fn key(&mut self, code: KeyCode, now: Instant) -> bool {
        let turn = |c: Camera, yaw: f64, pitch: f64| c.turned(yaw, pitch);
        match code {
            KeyCode::Esc | KeyCode::Char('v' | 'q') => return false,
            // the arrows move the front of the cube in their direction
            KeyCode::Left | KeyCode::Char('h') => {
                self.camera = turn(self.camera, -TURN_DEGREES, 0.0)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.camera = turn(self.camera, TURN_DEGREES, 0.0)
            }
            KeyCode::Up | KeyCode::Char('k') => self.camera = turn(self.camera, 0.0, -TURN_DEGREES),
            KeyCode::Down | KeyCode::Char('j') => {
                self.camera = turn(self.camera, 0.0, TURN_DEGREES)
            }
            KeyCode::Char('r') => self.camera = Camera::default(),
            KeyCode::Char(' ') => self.player.toggle(now),
            KeyCode::Char('.') => self.player.step(true),
            KeyCode::Char(',') => self.player.step(false),
            KeyCode::Home => self.player.rewind(),
            KeyCode::End => self.player.fast_forward(),
            KeyCode::Char('+' | '=') => self.player.faster(),
            KeyCode::Char('-') => self.player.slower(),
            KeyCode::Tab => {
                self.mode = match self.mode {
                    Mode::Cube3d => Mode::Net,
                    Mode::Net => Mode::Cube3d,
                }
            }
            _ => {}
        }
        true
    }

    /// Dragging with the left button turns the cube, like holding it. A cell
    /// is twice as tall as it is wide, so a row counts double.
    pub fn mouse(&mut self, kind: MouseEventKind, column: u16, row: u16) {
        const DEGREES_PER_COLUMN: f64 = 2.5;
        match kind {
            MouseEventKind::Down(MouseButton::Left) => self.drag_from = Some((column, row)),
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some((c0, r0)) = self.drag_from {
                    let dx = column as f64 - c0 as f64;
                    let dy = row as f64 - r0 as f64;
                    self.camera = self
                        .camera
                        .turned(dx * DEGREES_PER_COLUMN, dy * 2.0 * DEGREES_PER_COLUMN);
                }
                self.drag_from = Some((column, row));
            }
            MouseEventKind::Up(_) => self.drag_from = None,
            _ => {}
        }
    }

    /// Keys that make sense held down (auto-repeat).
    pub fn repeats(code: KeyCode) -> bool {
        matches!(
            code,
            KeyCode::Left
                | KeyCode::Right
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Char('h' | 'j' | 'k' | 'l' | ',' | '.')
        )
    }
}

/// How the terminal can show images; set up the first time the view opens
/// (it has to ask the terminal), cached for the rest of the run.
#[derive(Default)]
pub struct Gfx {
    picker: Option<Picker>,
    queried: bool,
    cache: RefCell<Option<(CacheKey, StatefulProtocol)>>,
    /// Images made so far, to alternate the kitty ids.
    frames: std::cell::Cell<u64>,
}

/// What a cached image was drawn for.
#[derive(Debug, Clone, PartialEq)]
struct CacheKey {
    rect: Rect,
    camera: Camera,
    pos: usize,
    size: usize,
    moves: usize,
}

impl Gfx {
    /// Work out how this terminal shows images. Call it outside of drawing,
    /// while nothing else reads input.
    ///
    /// This doesn't use `Picker::from_query_stdio`: when a terminal doesn't
    /// answer, that gives up after two seconds but leaves a thread blocked
    /// reading stdin, which then swallows keypresses for the rest of the run.
    /// Here the protocol comes from the environment, and the cell size from
    /// the window size, or (foot reports no pixel size) from one short query
    /// read directly, with a timeout and no thread left behind.
    pub fn ensure_picker(&mut self) {
        if self.queried {
            return;
        }
        self.queried = true;
        let env = |k: &str| std::env::var(k).unwrap_or_default();
        let Some(protocol) = detect_protocol(
            &env("CUBETIMER_GRAPHICS"),
            &env("TERM"),
            &env("TERM_PROGRAM"),
        ) else {
            return;
        };
        let Some(font) = cell_size_from_window().or_else(query_cell_size) else {
            return;
        };
        // Deprecated in favour of `from_query_stdio`, whose leftover thread is
        // the problem described above; it still works in the pinned 11.1.
        #[allow(deprecated)]
        let mut picker = Picker::from_fontsize(font);
        picker.set_protocol_type(protocol);
        self.picker = Some(picker);
    }

    /// The protocol in use, for the footer.
    pub fn protocol_name(&self) -> &'static str {
        match self.picker.as_ref().map(Picker::protocol_type) {
            Some(ProtocolType::Kitty) => "kitty graphics",
            Some(ProtocolType::Sixel) => "sixel",
            Some(ProtocolType::Iterm2) => "iTerm2 images",
            _ => "half blocks",
        }
    }
}

/// The image protocol for this terminal, from `CUBETIMER_GRAPHICS` (kitty,
/// sixel, iterm2 or halfblocks) if set, else from what the terminal says it
/// is. `None` means half blocks. `KITTY_WINDOW_ID` isn't used: it leaks into
/// every terminal started from inside kitty.
fn detect_protocol(force: &str, term: &str, term_program: &str) -> Option<ProtocolType> {
    match force.to_ascii_lowercase().as_str() {
        "kitty" => return Some(ProtocolType::Kitty),
        "sixel" => return Some(ProtocolType::Sixel),
        "iterm2" => return Some(ProtocolType::Iterm2),
        "halfblocks" | "none" | "off" => return None,
        _ => {}
    }
    if term == "xterm-kitty" || term == "xterm-ghostty" || term_program == "ghostty" {
        Some(ProtocolType::Kitty)
    } else if term_program == "WezTerm" {
        Some(ProtocolType::Iterm2)
    } else if term == "foot" || term.starts_with("foot-") {
        Some(ProtocolType::Sixel)
    } else {
        None
    }
}

/// The pixel size of one cell from the window size, if the terminal reports
/// pixels (kitty, Ghostty, WezTerm and Alacritty do; foot doesn't).
fn cell_size_from_window() -> Option<FontSize> {
    let w = ratatui::crossterm::terminal::window_size().ok()?;
    if w.width == 0 || w.height == 0 || w.columns == 0 || w.rows == 0 {
        return None;
    }
    Some(FontSize::new(w.width / w.columns, w.height / w.rows))
}

/// Ask the terminal for its cell size (`CSI 16 t`), followed by a device
/// attributes request that every terminal answers, so we know when to stop.
/// Reads straight from stdin with a 300 ms limit and no helper thread.
fn query_cell_size() -> Option<FontSize> {
    use rustix::event::{PollFd, PollFlags, poll};
    use std::io::Write;
    use std::os::fd::AsFd;
    let mut out = std::io::stdout();
    out.write_all(b"\x1b[16t\x1b[c").ok()?;
    out.flush().ok()?;
    let stdin = std::io::stdin();
    let fd = stdin.as_fd();
    let deadline = Instant::now() + std::time::Duration::from_millis(300);
    let mut reply = Vec::new();
    while !has_device_attributes(&reply) {
        let left = deadline
            .saturating_duration_since(Instant::now())
            .as_millis() as i32;
        if left <= 0 {
            break;
        }
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        if poll(&mut fds, left).ok()? == 0 {
            break;
        }
        let mut buf = [0u8; 256];
        let n = rustix::io::read(fd, &mut buf).ok()?;
        if n == 0 {
            break;
        }
        reply.extend_from_slice(&buf[..n]);
    }
    parse_cell_size(&reply)
}

/// Whether `reply` contains a device attributes answer (`ESC [ ? ... c`).
fn has_device_attributes(reply: &[u8]) -> bool {
    reply.windows(3).any(|w| w == b"\x1b[?") && reply.ends_with(b"c")
}

/// The cell size from a `CSI 6 ; height ; width t` answer.
fn parse_cell_size(reply: &[u8]) -> Option<FontSize> {
    let text = std::str::from_utf8(reply).ok()?;
    let start = text.find("\x1b[6;")? + 4;
    let end = start + text[start..].find('t')?;
    let mut parts = text[start..end].split(';');
    let (h, w): (u16, u16) = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    (h > 0 && w > 0).then(|| FontSize::new(w, h))
}

fn dim() -> Style {
    Style::new().fg(Color::DarkGray)
}

/// The full-screen cube view.
pub fn draw(frame: &mut Frame, area: Rect, view: &CubeView, gfx: &Gfx) {
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(format!(" {} ", view.title))
        .title_bottom(Line::from(footer(view, gfx)).right_aligned());
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let moves_height = moves_lines(view, inner.width).len().min(6) as u16;
    let [cube_area, moves_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(moves_height)]).areas(inner);
    let now = Instant::now();
    let (cube, turning) = view.player.frame(now);
    match view.mode {
        Mode::Cube3d => draw_cube(frame, cube_area, view, &cube, turning, gfx),
        Mode::Net => draw_net(frame, cube_area, &cube),
    }
    frame.render_widget(
        Paragraph::new(moves_lines(view, moves_area.width)).wrap(Wrap { trim: true }),
        moves_area,
    );
}

fn footer(view: &CubeView, gfx: &Gfx) -> Vec<Span<'static>> {
    let play = if view.player.is_playing() {
        "pause"
    } else {
        "play"
    };
    let mode = match view.mode {
        Mode::Cube3d => "net",
        Mode::Net => "3D",
    };
    let mut text = format!(
        " space {play}  , . step  +/- speed ({} ms)  drag or arrows turn  r reset  tab {mode}  esc close ",
        view.player.speed_ms()
    );
    if view.mode == Mode::Cube3d {
        text.push_str(&format!("[{}] ", gfx.protocol_name()));
    }
    vec![Span::styled(text, dim())]
}

/// The scramble with the moves done so far dimmed and the one turning now
/// highlighted.
fn moves_lines(view: &CubeView, _width: u16) -> Vec<Line<'static>> {
    let pos = view.player.pos();
    let playing = view.player.is_playing();
    let mut spans = vec![Span::styled(
        format!("{}/{}  ", pos, view.player.moves().len()),
        dim(),
    )];
    for (i, m) in view.player.moves().iter().enumerate() {
        let style = if i < pos {
            dim()
        } else if i == pos && playing {
            Style::new()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if i == pos {
            Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::new()
        };
        spans.push(Span::styled(m.to_string(), style));
        spans.push(Span::raw(" "));
    }
    vec![Line::from(spans)]
}

/// The largest square (in pixels) that fits `area`, as a centered cell
/// rectangle, with the pixel size of one cell.
fn square_rect(area: Rect, cell_w: usize, cell_h: usize, max: usize) -> (Rect, usize, usize) {
    let side = (area.width as usize * cell_w)
        .min(area.height as usize * cell_h)
        .min(max.max(cell_w.max(cell_h)));
    let cols = (side / cell_w).max(1) as u16;
    let rows = (side / cell_h).max(1) as u16;
    let [r] = Layout::horizontal([Constraint::Length(cols)])
        .flex(Flex::Center)
        .areas(area);
    let [r] = Layout::vertical([Constraint::Length(rows)])
        .flex(Flex::Center)
        .areas(r);
    (r, r.width as usize * cell_w, r.height as usize * cell_h)
}

fn draw_cube(
    frame: &mut Frame,
    area: Rect,
    view: &CubeView,
    cube: &Nxn,
    turning: Option<Turning>,
    gfx: &Gfx,
) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let Some(picker) = &gfx.picker else {
        draw_halfblocks(frame, area, view, cube, turning);
        return;
    };
    let font = picker.font_size();
    let max = match picker.protocol_type() {
        ProtocolType::Sixel => MAX_PIXELS_SIXEL,
        _ => MAX_PIXELS,
    };
    let (rect, pw, ph) = square_rect(area, font.width as usize, font.height as usize, max);
    let key = CacheKey {
        rect,
        camera: view.camera,
        pos: view.player.pos(),
        size: view.size,
        moves: view.player.moves().len(),
    };
    let mut cache = gfx.cache.borrow_mut();
    // a still cube is drawn once; a turning one every frame, a little rougher
    let fresh = turning.is_some() || cache.as_ref().is_none_or(|(k, _)| *k != key);
    if fresh {
        // A turning cube is redrawn every frame, so it gets fewer samples per
        // pixel on big images (jagged edges don't show while it moves); the
        // cube at rest is always drawn smooth.
        let quality = match turning {
            None => 3,
            Some(_) if pw * ph > 700 * 700 => 1,
            Some(_) => 2,
        };
        let img = render3d::render_with(cube, view.camera, turning, pw, ph, quality);
        let Some(rgba) = RgbaImage::from_raw(img.width as u32, img.height as u32, img.rgba) else {
            return;
        };
        let frame = gfx.frames.get();
        gfx.frames.set(frame + 1);
        let proto = new_protocol(picker, DynamicImage::ImageRgba8(rgba), frame);
        *cache = Some((key, proto));
    }
    if let Some((_, proto)) = cache.as_mut() {
        frame.render_stateful_widget(StatefulImage::default(), rect, proto);
    }
}

/// The two kitty image ids the view takes turns with. `new_resize_protocol`
/// picks a random id per image, so kitty would keep every frame of an
/// animation until its quota pushed them out. One fixed id doesn't work
/// either: replacing an image drops its placement, and as the placeholder
/// cells on screen don't change, nothing redraws them and the cube vanishes
/// while it turns. Alternating two ids changes the cells every frame and
/// keeps at most two images in the terminal.
const KITTY_IMAGE_IDS: [u32; 2] = [0x00c0_be3d, 0x00c0_be3e];

fn new_protocol(picker: &Picker, image: DynamicImage, frame: u64) -> StatefulProtocol {
    if picker.protocol_type() != ProtocolType::Kitty {
        return picker.new_resize_protocol(image);
    }
    let in_tmux = std::env::var_os("TMUX").is_some_and(|v| !v.is_empty());
    let compress = picker
        .capabilities()
        .contains(&Capability::KittyCompression);
    let id = KITTY_IMAGE_IDS[(frame % 2) as usize];
    let kitty = StatefulKitty::new(id, in_tmux, compress);
    StatefulProtocol::new(
        image,
        picker.font_size(),
        None,
        StatefulProtocolType::Kitty(kitty),
    )
}

/// The cube drawn with half blocks: each cell is two pixels stacked, with the
/// background left alone where the image is empty.
fn draw_halfblocks(
    frame: &mut Frame,
    area: Rect,
    view: &CubeView,
    cube: &Nxn,
    turning: Option<Turning>,
) {
    let (rect, pw, ph) = square_rect(area, 1, 2, MAX_PIXELS);
    let img = render3d::render(cube, view.camera, turning, pw, ph);
    let color = |x: usize, y: usize| {
        let p = img.pixel(x, y);
        (p[3] >= 128).then_some(Color::Rgb(p[0], p[1], p[2]))
    };
    let lines: Vec<Line> = (0..rect.height as usize)
        .map(|row| {
            let spans: Vec<Span> = (0..rect.width as usize)
                .map(|x| match (color(x, 2 * row), color(x, 2 * row + 1)) {
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
    frame.render_widget(Paragraph::new(lines), rect);
}

fn draw_net(frame: &mut Frame, area: Rect, cube: &Nxn) {
    let grid = ui::net_grid_of(cube);
    let (rows, cols) = (grid.len(), grid[0].len());
    let scale = (1..=6usize)
        .rev()
        .find(|&k| {
            cols * k <= area.width as usize && (rows * k).div_ceil(2) <= area.height as usize
        })
        .unwrap_or(1);
    let lines = ui::net_lines(&grid, scale);
    let height = (lines.len() as u16).min(area.height);
    let width = ((cols * scale) as u16).min(area.width);
    let [r] = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .areas(area);
    let [r] = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .areas(r);
    frame.render_widget(Paragraph::new(lines), r);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::time::Duration;

    fn view(scramble: &str) -> CubeView {
        CubeView::new("3x3 scramble", 3, scramble).unwrap()
    }

    fn render(view: &CubeView, w: u16, h: u16) -> (String, ratatui::buffer::Buffer) {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let gfx = Gfx::default();
        terminal.draw(|f| draw(f, f.area(), view, &gfx)).unwrap();
        let buf = terminal.backend().buffer().clone();
        let text = (0..h)
            .map(|y| (0..w).map(|x| buf[(x, y)].symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        (text, buf)
    }

    #[test]
    fn a_bad_scramble_gives_no_view() {
        assert!(CubeView::new("x", 3, "not moves").is_none());
        assert!(
            CubeView::new("x", 3, "").is_some(),
            "empty is a solved cube"
        );
    }

    #[test]
    fn keys_turn_the_cube_play_step_and_close() {
        let t = Instant::now();
        let mut v = view("R U F");
        let start = v.camera;
        assert!(v.key(KeyCode::Left, t));
        assert!(v.camera.yaw != start.yaw);
        v.key(KeyCode::Char('r'), t);
        assert_eq!(v.camera, Camera::default());
        v.key(KeyCode::Up, t);
        assert!(v.camera.pitch < start.pitch, "up shows more of the bottom");
        v.key(KeyCode::Char(' '), t);
        assert!(v.player.is_playing());
        assert_eq!(v.player.pos(), 0, "plays the scramble from solved");
        v.key(KeyCode::Char(' '), t);
        assert!(!v.player.is_playing());
        v.key(KeyCode::Char('.'), t);
        assert_eq!(v.player.pos(), 1);
        v.key(KeyCode::Char(','), t);
        v.key(KeyCode::End, t);
        assert_eq!(v.player.pos(), 3);
        v.key(KeyCode::Home, t);
        assert_eq!(v.player.pos(), 0);
        let speed = v.player.speed_ms();
        v.key(KeyCode::Char('+'), t);
        assert!(v.player.speed_ms() < speed);
        v.key(KeyCode::Tab, t);
        assert_eq!(v.mode, Mode::Net);
        for close in [KeyCode::Esc, KeyCode::Char('v'), KeyCode::Char('q')] {
            assert!(!view("R").key(close, t));
        }
    }

    #[test]
    fn animation_runs_on_ticks() {
        let t = Instant::now();
        let mut v = view("R U");
        v.key(KeyCode::Char(' '), t);
        assert!(v.animating());
        v.tick(t + Duration::from_secs(5));
        assert!(!v.animating());
        assert_eq!(v.player.pos(), 2);
    }

    #[test]
    fn without_image_support_the_cube_is_drawn_in_half_blocks() {
        let v = view("R U R' U'");
        let (text, buf) = render(&v, 100, 40);
        assert!(text.contains("3x3 scramble"));
        assert!(text.contains("half blocks"));
        // all six colors can't be seen at once, but the three faces' colors are there
        let colors: std::collections::HashSet<_> = (0..40u16)
            .flat_map(|y| (0..100u16).map(move |x| (x, y)))
            .filter(|&(x, y)| buf[(x, y)].symbol() == "\u{2580}")
            .filter_map(|(x, y)| buf[(x, y)].style().fg)
            .collect();
        assert!(
            colors.len() > 20,
            "shaded and anti-aliased: {}",
            colors.len()
        );
        // the corners of the view are the terminal's own background
        assert_eq!(buf[(1, 1)].style().bg, Some(Color::Reset));
    }

    #[test]
    fn the_moves_line_marks_progress() {
        let t = Instant::now();
        let mut v = view("R U F");
        v.key(KeyCode::Home, t);
        v.key(KeyCode::Char('.'), t);
        let (text, _) = render(&v, 100, 30);
        assert!(text.contains("1/3  R U F"), "{text}");
    }

    #[test]
    fn the_net_mode_shows_the_current_state() {
        let mut v = view("R");
        v.key(KeyCode::Tab, Instant::now());
        let (text, _) = render(&v, 100, 40);
        assert!(text.contains('\u{2580}'));
        assert!(text.contains("tab 3D"));
        assert!(
            !text.contains("half blocks"),
            "no protocol shown for the net"
        );
    }

    #[test]
    fn every_size_and_window_renders() {
        for size in 2..=7 {
            let v = CubeView::new("x", size, "R U").unwrap();
            for (w, h) in [(1, 1), (5, 3), (30, 10), (80, 24), (200, 60)] {
                render(&v, w, h);
            }
            let mut net = v.clone();
            net.mode = Mode::Net;
            for (w, h) in [(1, 1), (30, 10), (200, 60)] {
                render(&net, w, h);
            }
        }
    }

    #[test]
    fn square_rects_are_centered_and_capped() {
        let area = Rect::new(0, 0, 200, 60);
        let (r, pw, ph) = square_rect(area, 10, 20, MAX_PIXELS);
        assert_eq!((pw, ph), (r.width as usize * 10, r.height as usize * 20));
        assert!(pw <= MAX_PIXELS && ph <= MAX_PIXELS);
        assert!(
            (pw as i64 - ph as i64).abs() <= 20,
            "about square: {pw}x{ph}"
        );
        assert!(r.x > 0 && r.x + r.width < 200, "centered");
    }

    #[test]
    fn protocols_come_from_what_the_terminal_says_it_is() {
        use ProtocolType::*;
        // what the five tested terminals set
        assert_eq!(detect_protocol("", "xterm-kitty", ""), Some(Kitty));
        assert_eq!(detect_protocol("", "xterm-ghostty", "ghostty"), Some(Kitty));
        assert_eq!(
            detect_protocol("", "xterm-256color", "WezTerm"),
            Some(Iterm2)
        );
        assert_eq!(detect_protocol("", "foot", ""), Some(Sixel));
        assert_eq!(detect_protocol("", "foot-extra", ""), Some(Sixel));
        assert_eq!(detect_protocol("", "alacritty", ""), None);
        assert_eq!(detect_protocol("", "xterm-256color", ""), None);
        assert_eq!(detect_protocol("", "", ""), None);
        // the override wins, either way
        assert_eq!(detect_protocol("SIXEL", "alacritty", ""), Some(Sixel));
        assert_eq!(detect_protocol("halfblocks", "xterm-kitty", ""), None);
        assert_eq!(detect_protocol("nonsense", "xterm-kitty", ""), Some(Kitty));
    }

    #[test]
    fn cell_size_answers_are_parsed() {
        // FontSize has no PartialEq, so compare (width, height)
        let size = |reply: &[u8]| parse_cell_size(reply).map(|f| (f.width, f.height));
        assert_eq!(size(b"\x1b[6;29;13t\x1b[?62;4c"), Some((13, 29)));
        assert_eq!(size(b"junk\x1b[6;20;10t"), Some((10, 20)));
        assert_eq!(size(b"\x1b[?62c"), None);
        assert_eq!(size(b"\x1b[6;0;0t"), None);
        assert_eq!(size(b"\x1b[6;x;yt"), None);
        assert!(has_device_attributes(b"\x1b[6;29;13t\x1b[?62;4c"));
        assert!(!has_device_attributes(b"\x1b[6;29;13t"));
    }

    #[test]
    fn dragging_turns_the_cube_the_way_the_mouse_moves() {
        let mut v = view("R U");
        let start = v.camera;
        v.mouse(MouseEventKind::Moved, 10, 10); // no button: nothing
        assert_eq!(v.camera, start);
        v.mouse(MouseEventKind::Down(MouseButton::Left), 10, 10);
        v.mouse(MouseEventKind::Drag(MouseButton::Left), 14, 10); // 4 columns right
        // yaw wraps around at 360
        let turned = (v.camera.yaw - start.yaw).rem_euclid(360.0);
        assert!((turned - 10.0).abs() < 1e-9, "{:?}", v.camera);
        assert_eq!(v.camera.pitch, start.pitch);
        v.mouse(MouseEventKind::Drag(MouseButton::Left), 14, 12); // 2 rows down
        assert!((v.camera.pitch - (start.pitch + 10.0)).abs() < 1e-9);
        v.mouse(MouseEventKind::Up(MouseButton::Left), 14, 12);
        let after = v.camera;
        v.mouse(MouseEventKind::Drag(MouseButton::Left), 30, 30); // no drag in progress
        assert_eq!(v.camera, after);
        // right button drags are ignored
        v.mouse(MouseEventKind::Down(MouseButton::Right), 0, 0);
        v.mouse(MouseEventKind::Drag(MouseButton::Right), 9, 9);
        assert_eq!(v.camera, after);
    }
}
