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

/// How the cube is drawn. `m` cycles through them; the choice is saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Look {
    /// A real image where the terminal can show one, else `Blocks`.
    #[default]
    Image,
    /// Half-block characters: colored pixels, two per cell.
    Blocks,
    /// ASCII characters in the stickers' colors; the character's density
    /// carries the shading, like the classic spinning ASCII donut.
    Ascii,
    /// The same ASCII art without any color.
    Mono,
}

impl Look {
    pub const ALL: [Look; 4] = [Look::Image, Look::Blocks, Look::Ascii, Look::Mono];

    pub fn name(self) -> &'static str {
        match self {
            Look::Image => "image",
            Look::Blocks => "blocks",
            Look::Ascii => "ascii",
            Look::Mono => "mono",
        }
    }

    /// The look called `name`; anything else (a typo, a file from a newer
    /// version) is the default.
    pub fn from_name(name: &str) -> Look {
        Look::ALL
            .into_iter()
            .find(|l| l.name() == name)
            .unwrap_or_default()
    }

    /// The next look, skipping `Image` when the terminal can't show images.
    pub fn next(self, has_images: bool) -> Look {
        let i = Look::ALL.iter().position(|&l| l == self).unwrap_or(0);
        (1..=Look::ALL.len())
            .map(|k| Look::ALL[(i + k) % Look::ALL.len()])
            .find(|&l| l != Look::Image || has_images)
            .unwrap_or(self)
    }
}

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
    pub look: Look,
    /// Whether the terminal can show images (set by the app from `Gfx`), so
    /// cycling the look knows whether `Image` is a real choice.
    pub has_images: bool,
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
            look: Look::default(),
            has_images: false,
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
            KeyCode::Char('m') => self.look = self.look.next(self.has_images),
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

    /// Whether the terminal shows real images (not just half blocks).
    pub fn has_images(&self) -> bool {
        self.picker.is_some()
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
        let how = match view.look {
            Look::Image => gfx.protocol_name(),
            Look::Blocks => "half blocks",
            Look::Ascii => "ascii",
            Look::Mono => "mono ascii",
        };
        text.push_str(&format!("[m: {how}] "));
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
    match view.look {
        Look::Blocks => return draw_halfblocks(frame, area, view, cube, turning),
        Look::Ascii | Look::Mono => return draw_ascii(frame, area, view, cube, turning),
        Look::Image => {}
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

/// How much of its square a sticker fills in the text looks.
const TEXT_FILL: f64 = 0.8;
/// Lighting contrast for the text looks. The character is chosen from it, so
/// the three visible faces of a cube get clearly different densities.
const TEXT_SHADE_RANGE: f64 = 0.55;

/// How much the text looks are zoomed in. At the default view the cube just
/// fits at 1.4 and touches the edge at 1.5; turned to other angles a corner
/// can clip a little.
const TEXT_ZOOM: f64 = 1.35;

/// Characters from sparse to dense. Sticker brightness picks the character.
const RAMP: &[u8] = b":-=+*#%@";
/// Pixels darker than this (by luminance) are the cube's black plastic
/// (luma 18). The dimmest sticker, blue at the lowest lighting, is about 32.
const PLASTIC_LUMA: f64 = 26.0;
/// The lighting range that maps onto `RAMP`: a face turned fully away from
/// the light (1 - `TEXT_SHADE_RANGE`) up to a face lit straight on (1.0).
const RAMP_FROM: f64 = 1.0 - TEXT_SHADE_RANGE;
const RAMP_TO: f64 = 1.0;

/// How brightly lit a sticker pixel is: its brightest channel against the
/// brightest channel of the sticker color it is closest to, 0 to 1.
fn lighting(rgb: [f64; 3]) -> f64 {
    let peak = rgb.iter().cloned().fold(1.0, f64::max);
    let direction = |c: [f64; 3]| {
        let len = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt().max(1.0);
        [c[0] / len, c[1] / len, c[2] / len]
    };
    let d = direction(rgb);
    let base = render3d::STICKER_RGB
        .iter()
        .map(|&c| c.map(f64::from))
        .min_by(|a, b| {
            let dist = |c: &[f64; 3]| {
                let e = direction(*c);
                (0..3).map(|k| (e[k] - d[k]).powi(2)).sum::<f64>()
            };
            dist(a).total_cmp(&dist(b))
        })
        .unwrap_or([255.0; 3]);
    let base_peak = base.iter().cloned().fold(1.0, f64::max);
    (peak / base_peak).clamp(0.0, 1.0)
}

/// The brightness of a pixel, 0 to 255.
fn luma(p: [u8; 4]) -> f64 {
    0.299 * p[0] as f64 + 0.587 * p[1] as f64 + 0.114 * p[2] as f64
}

/// One cell of ASCII art for a pixel pair (the two halves of the cell):
/// the character and its color (`None` for no color), or `None` for empty.
///
/// A cell is either black plastic or a sticker, never a blend: blending the
/// anti-aliased pixels along every sticker edge makes in-between characters
/// that read as noise. A cell where plastic meets a sticker is plastic, which
/// also keeps the gaps between stickers visible at this coarse resolution.
fn ascii_cell(top: [u8; 4], bottom: [u8; 4], color: bool) -> Option<(char, Option<Color>)> {
    let solid = |p: [u8; 4]| p[3] >= 128;
    let (top_in, bottom_in) = (solid(top), solid(bottom));
    if !top_in && !bottom_in {
        return None;
    }
    let plastic = Some(('.', color.then_some(Color::Indexed(240))));
    let pixels: Vec<[u8; 4]> = [top, bottom].into_iter().filter(|&p| solid(p)).collect();
    if pixels.iter().any(|&p| luma(p) < PLASTIC_LUMA) {
        return plastic;
    }
    // what is left is sticker (one half, or both of the same sticker)
    let n = pixels.len() as f64;
    let rgb = [0, 1, 2].map(|k| pixels.iter().map(|p| p[k] as f64).sum::<f64>() / n);
    let t = ((lighting(rgb) - RAMP_FROM) / (RAMP_TO - RAMP_FROM)).clamp(0.0, 1.0);
    let ch = RAMP[(t * (RAMP.len() - 1) as f64).round() as usize] as char;
    // the character carries the brightness, so show the hue at full strength
    let peak = rgb.iter().cloned().fold(1.0, f64::max);
    let fg = color.then(|| {
        let scale = 255.0 / peak;
        Color::Rgb(
            (rgb[0] * scale).round() as u8,
            (rgb[1] * scale).round() as u8,
            (rgb[2] * scale).round() as u8,
        )
    });
    Some((ch, fg))
}

/// The cube as ASCII art: each cell is two pixels stacked, as with half
/// blocks, but drawn with a character whose density shows the shading.
fn draw_ascii(
    frame: &mut Frame,
    area: Rect,
    view: &CubeView,
    cube: &Nxn,
    turning: Option<Turning>,
) {
    let (rect, pw, ph) = square_rect(area, 1, 2, MAX_PIXELS);
    // Text is coarse: zoomed in to fill the window, with wide gaps and little
    // smoothing, so the cells read as a grid of squares.
    let opts = render3d::Options {
        ss: 2,
        sticker_fill: TEXT_FILL,
        zoom: TEXT_ZOOM,
        shade_range: TEXT_SHADE_RANGE,
    };
    let img = render3d::render_opts(cube, view.camera, turning, pw, ph, opts);
    let color = view.look == Look::Ascii;
    let lines: Vec<Line> = (0..rect.height as usize)
        .map(|row| {
            let spans: Vec<Span> = (0..rect.width as usize)
                .map(|x| {
                    match ascii_cell(img.pixel(x, 2 * row), img.pixel(x, 2 * row + 1), color) {
                        Some((ch, Some(fg))) => Span::styled(ch.to_string(), Style::new().fg(fg)),
                        Some((ch, None)) => Span::raw(ch.to_string()),
                        None => Span::raw(" "),
                    }
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

    #[test]
    fn looks_have_unique_names_that_round_trip() {
        for l in Look::ALL {
            assert_eq!(Look::from_name(l.name()), l);
        }
        assert!(
            Look::ALL
                .iter()
                .enumerate()
                .all(|(i, a)| Look::ALL[..i].iter().all(|b| a.name() != b.name()))
        );
        assert_eq!(Look::from_name("from-the-future"), Look::default());
        assert_eq!(Look::from_name(""), Look::default());
        assert_eq!(Look::default(), Look::Image);
    }

    #[test]
    fn cycling_the_look_visits_every_style_and_skips_images_when_unavailable() {
        let mut seen = vec![];
        let mut l = Look::Image;
        for _ in 0..4 {
            seen.push(l);
            l = l.next(true);
        }
        assert_eq!(seen, Look::ALL.to_vec());
        assert_eq!(l, Look::Image, "wraps around");
        // no images: Blocks -> Ascii -> Mono -> Blocks, never Image
        let mut l = Look::Blocks;
        for want in [Look::Ascii, Look::Mono, Look::Blocks, Look::Ascii] {
            l = l.next(false);
            assert_eq!(l, want);
        }
        // a saved Image look on a terminal without images moves on cleanly
        assert_eq!(Look::Image.next(false), Look::Blocks);
    }

    fn ascii_view(look: Look, scramble: &str) -> CubeView {
        let mut v = view(scramble);
        v.look = look;
        v
    }

    #[test]
    fn m_cycles_the_look() {
        let t = Instant::now();
        let mut v = view("R U");
        v.has_images = true;
        v.key(KeyCode::Char('m'), t);
        assert_eq!(v.look, Look::Blocks);
        v.key(KeyCode::Char('m'), t);
        assert_eq!(v.look, Look::Ascii);
        v.key(KeyCode::Char('m'), t);
        v.key(KeyCode::Char('m'), t);
        assert_eq!(v.look, Look::Image);
        v.has_images = false;
        v.look = Look::Mono;
        v.key(KeyCode::Char('m'), t);
        assert_eq!(v.look, Look::Blocks, "no image step when none can be shown");
    }

    #[test]
    fn ascii_cells_are_characters_with_density_for_shading() {
        let px = |r, g, b| [r, g, b, 255];
        // empty space and the edges of the cube
        assert_eq!(ascii_cell([0; 4], [0; 4], true), None);
        assert_eq!(
            ascii_cell([200, 0, 0, 30], [200, 0, 0, 30], true),
            None,
            "mostly empty"
        );
        // black plastic is a dim dot
        assert_eq!(
            ascii_cell(px(18, 18, 18), px(18, 18, 18), true),
            Some(('.', Some(Color::Indexed(240))))
        );
        assert_eq!(
            ascii_cell(px(18, 18, 18), px(18, 18, 18), false),
            Some(('.', None))
        );
        // brighter stickers use denser characters
        let rank = |c: char| RAMP.iter().position(|&b| b as char == c).unwrap();
        let white = ascii_cell(px(245, 245, 245), px(245, 245, 245), true)
            .unwrap()
            .0;
        let shaded = ascii_cell(px(160, 160, 160), px(160, 160, 160), true)
            .unwrap()
            .0;
        let dark = ascii_cell(px(70, 70, 70), px(70, 70, 70), true).unwrap().0;
        assert!(
            rank(white) > rank(shaded) && rank(shaded) > rank(dark),
            "{white} {shaded} {dark}"
        );
        assert_eq!(white, '@');
    }

    #[test]
    fn ascii_color_keeps_the_hue_at_full_strength() {
        let red = ascii_cell([140, 20, 20, 255], [140, 20, 20, 255], true)
            .unwrap()
            .1
            .unwrap();
        assert_eq!(red, Color::Rgb(255, 36, 36));
        let blue = ascii_cell([0, 60, 150, 255], [0, 60, 150, 255], true)
            .unwrap()
            .1
            .unwrap();
        assert_eq!(blue, Color::Rgb(0, 102, 255));
        // two pixels of different coverage mix by how much of the cell they fill
        let edge = ascii_cell([200, 0, 0, 255], [0, 0, 200, 40], true)
            .unwrap()
            .1
            .unwrap();
        let Color::Rgb(r, _, b) = edge else { panic!() };
        assert!(r > 200 && b < 100, "mostly the opaque pixel: {edge:?}");
    }

    #[test]
    fn the_ascii_cube_is_drawn_in_text_without_blocks_or_images() {
        let v = ascii_view(Look::Ascii, "R U R' U'");
        let (text, buf) = render(&v, 100, 40);
        assert!(text.contains("[m: ascii]"), "{text}");
        assert!(!text.contains('\u{2580}') && !text.contains('\u{2584}'));
        // plastic dots, and at least three different densities for the three
        // visible faces
        assert!(text.contains('.'));
        let densities = RAMP.iter().filter(|&&b| text.contains(b as char)).count();
        assert!(densities >= 3, "{densities} densities");
        // sticker characters carry their color
        let colored = (0..40u16)
            .flat_map(|y| (0..100u16).map(move |x| (x, y)))
            .filter(|&(x, y)| matches!(buf[(x, y)].style().fg, Some(Color::Rgb(..))))
            .count();
        assert!(colored > 300, "{colored}");
        // the corners of the view are empty
        assert_eq!(buf[(2, 2)].symbol(), " ");
    }

    #[test]
    fn mono_has_no_color_at_all_and_still_shows_the_cube() {
        let v = ascii_view(Look::Mono, "R U R' U'");
        let (text, buf) = render(&v, 100, 40);
        assert!(text.contains("[m: mono ascii]"));
        let colored = (0..40u16)
            .flat_map(|y| (0..100u16).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let fg = buf[(x, y)].style().fg;
                matches!(fg, Some(Color::Rgb(..) | Color::Indexed(_)))
            })
            .count();
        // only the title and footer (outside the cube) use the default colors too
        assert_eq!(colored, 0);
        let art: usize = text.chars().filter(|c| ":-=+*#%@".contains(*c)).count();
        assert!(art > 250, "{art}");
    }

    #[test]
    fn the_ascii_cube_shows_its_stickers_as_a_grid() {
        // a solved 3x3 seen head on has 9 stickers per face; the plastic
        // between them must show as '.' so the grid reads
        let mut v = ascii_view(Look::Mono, "");
        v.camera = Camera {
            yaw: 0.0,
            pitch: 0.0,
        };
        let (text, _) = render(&v, 100, 50);
        let dots = text.chars().filter(|&c| c == '.').count();
        assert!(dots > 80, "{dots}");
    }

    #[test]
    fn the_ascii_cube_turns_animates_and_plays() {
        let t = Instant::now();
        let mut v = ascii_view(Look::Ascii, "R U F");
        let (before, _) = render(&v, 100, 40);
        v.key(KeyCode::Left, t);
        let (turned, _) = render(&v, 100, 40);
        assert_ne!(before, turned);
        v.key(KeyCode::Char(' '), t);
        let (playing, _) = render(&v, 100, 40);
        assert!(playing.contains("pause"));
    }

    #[test]
    fn every_look_renders_at_every_size_and_window() {
        for look in Look::ALL {
            for size in 2..=7 {
                let mut v = CubeView::new("x", size, "R U").unwrap();
                v.look = look;
                for (w, h) in [(1, 1), (5, 3), (30, 10), (80, 24), (200, 60)] {
                    render(&v, w, h);
                }
            }
        }
    }

    #[test]
    fn the_character_follows_the_lighting_not_the_color() {
        // the same lighting on different colors gives the same character
        let lit = |base: [u8; 3], light: f64| {
            let p = base.map(|c| (c as f64 * light).round() as u8);
            ascii_cell([p[0], p[1], p[2], 255], [p[0], p[1], p[2], 255], false)
                .unwrap()
                .0
        };
        for light in [1.0, 0.8, 0.6, 0.45] {
            let chars: Vec<char> = render3d::STICKER_RGB
                .iter()
                .map(|&c| lit(c, light))
                .collect();
            assert!(
                chars.windows(2).all(|w| w[0] == w[1]),
                "light {light}: {chars:?}"
            );
        }
        // and brighter light gives a denser character
        let rank = |c: char| RAMP.iter().position(|&b| b as char == c).unwrap();
        let ranks: Vec<usize> = [0.45, 0.6, 0.8, 1.0]
            .iter()
            .map(|&l| rank(lit([214, 40, 40], l)))
            .collect();
        assert!(ranks.windows(2).all(|w| w[0] < w[1]), "{ranks:?}");
    }

    #[test]
    fn the_three_visible_faces_use_different_characters() {
        let mut v = ascii_view(Look::Mono, "");
        v.camera = Camera::default();
        let (text, _) = render(&v, 120, 50);
        // a solved cube: each face is one color, so a face is one density; the
        // top (lit most), front and side must all differ
        let used: Vec<char> = RAMP
            .iter()
            .map(|&b| b as char)
            .filter(|c| text.contains(*c))
            .collect();
        assert!(used.len() >= 3, "{used:?}");
    }

    #[test]
    fn the_zoomed_text_cube_fits_at_the_default_view_for_every_size() {
        let opts = render3d::Options {
            ss: 1,
            sticker_fill: TEXT_FILL,
            zoom: TEXT_ZOOM,
            shade_range: TEXT_SHADE_RANGE,
        };
        for size in 2..=7 {
            let img =
                render3d::render_opts(&Nxn::solved(size), Camera::default(), None, 200, 200, opts);
            for i in 0..200 {
                for (x, y) in [(i, 0), (i, 199), (0, i), (199, i)] {
                    assert_eq!(
                        img.pixel(x, y)[3],
                        0,
                        "{size}x{size} touches the edge at {x},{y}"
                    );
                }
            }
        }
    }
}
