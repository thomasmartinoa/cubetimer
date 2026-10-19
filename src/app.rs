//! App state and the timer state machine.
//!
//! Handlers take `now: Instant` so the state machine can be tested without
//! sleeping.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::chart::Range;
use crate::fmt::{self, Precision, RunDisplay};
use crate::font;
use crate::puzzle::{self, Puzzle};
use crate::scramble::Scrambler;
use crate::stats::{self, Penalty, Summary, TimeResult};
use crate::statsview::StatsView;
use crate::store::{self, Data, Loaded, Session, Settings, Solve};
use crate::ui;

/// How long space must be held before the timer is ready.
pub const HOLD_READY: Duration = Duration::from_millis(300);

/// How long a new personal best flashes, and how fast it blinks.
const PB_FLASH: Duration = Duration::from_millis(1500);
const PB_BLINK_MS: u128 = 250;

const WARN_8S: u64 = 8_000;
const WARN_12S: u64 = 12_000;
const PLUS2_AFTER: u64 = 15_000;
const DNF_AFTER: u64 = 17_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    Inspection {
        start: Instant,
    },
    /// Space is held down; `inspection` is the start of the running inspection.
    Holding {
        since: Instant,
        inspection: Option<Instant>,
    },
    Running {
        start: Instant,
        penalty: Penalty,
    },
}

/// What the big display should look like; `ui` maps tones to colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Idle,
    Inspect,
    Warn8,
    Warn12,
    HoldWait,
    HoldReady,
    Running,
    /// New personal best; the two tones alternate to make it blink.
    Pb,
    PbAlt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub text: String,
    pub tone: Tone,
    pub hint: String,
}

/// The key as typed. With the kitty keyboard protocol a shifted key arrives
/// as its unshifted key plus SHIFT (Shift+/ is '/' with SHIFT, never '?'), so
/// map letters to capitals and the two symbols cubetimer uses. Other layouts
/// may put `?` and `+` elsewhere; F1 and `=` work everywhere.
fn typed(key: KeyEvent) -> KeyCode {
    match key.code {
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::SHIFT) => KeyCode::Char(match c {
            'a'..='z' => c.to_ascii_uppercase(),
            '/' => '?',
            '=' => '+',
            other => other,
        }),
        code => code,
    }
}

pub struct App {
    pub state: State,
    pub data: Data,
    /// Where sessions are saved; `None` keeps everything in memory (tests).
    path: Option<PathBuf>,
    /// A one-line message for the footer (save errors and the like).
    pub status: Option<String>,
    /// Waiting for y/n to delete this solve (an index into `solves()`).
    pub confirm_delete: Option<usize>,
    /// Waiting for y/n to delete the whole current session.
    pub confirm_session_delete: bool,
    /// The history browser is open with this solve selected.
    pub history: Option<usize>,
    /// The key help overlay is open.
    pub show_help: bool,
    /// The statistics screen, when open.
    pub stats: Option<StatsView>,
    /// The puzzle picker is open with this puzzle highlighted.
    pub picker: Option<usize>,
    /// The full-screen scramble preview is open.
    pub show_preview: bool,
    /// The key last pressed and not yet released (with release events only),
    /// to tell auto-repeat from a new press.
    down: Option<KeyCode>,
    /// When the last solve set a new best single; drives the flash.
    pb_flash: Option<Instant>,
    pub inspection_on: bool,
    /// True when the terminal reports key release events (kitty protocol).
    pub enhanced: bool,
    pub run_display: RunDisplay,
    pub precision: Precision,
    pub bell: bool,
    /// The digit font choice (0 is `auto`), see `font::name_at`.
    pub font: usize,
    /// The scramble to do next; empty while the first one is being made.
    pub scramble: String,
    scrambler: Scrambler,
    pub quit: bool,
    /// Inspection warnings already announced: 0 none, 1 at 8 s, 2 at 12 s.
    warned: u8,
}

impl App {
    pub fn new(enhanced: bool) -> Self {
        let mut app = Self {
            state: State::Idle,
            data: Data::default(),
            path: None,
            status: None,
            confirm_delete: None,
            history: None,
            confirm_session_delete: false,
            show_help: false,
            picker: None,
            stats: None,
            show_preview: false,
            down: None,
            pb_flash: None,
            inspection_on: true,
            enhanced,
            run_display: RunDisplay::default(),
            precision: Precision::default(),
            bell: false,
            font: font::index_of(font::DEFAULT),
            scramble: String::new(),
            scrambler: Scrambler::new(Puzzle::Cube3),
            quit: false,
            warned: 0,
        };
        // Don't wait for the solver's tables: draw now, show the scramble
        // when it arrives (`tick` picks it up).
        app.scramble = app.scrambler.try_next().unwrap_or_default();
        app
    }

    /// Use `path` for saving and start from what was loaded.
    pub fn with_storage(mut self, path: PathBuf, loaded: Loaded) -> Self {
        self.path = Some(path);
        self.data = loaded.data;
        self.status = loaded.warning;
        let settings = &self.data.settings;
        self.font = font::index_of(&settings.font);
        self.inspection_on = settings.inspection;
        self.bell = settings.bell;
        self.reset_scrambler();
        self
    }

    /// The puzzle being timed.
    pub fn puzzle(&self) -> Puzzle {
        self.data.puzzle()
    }

    /// Start making scrambles for the current puzzle (after switching puzzle
    /// or loading a file whose current session isn't a 3x3).
    fn reset_scrambler(&mut self) {
        self.scrambler = Scrambler::new(self.puzzle());
        self.scramble = self.scrambler.try_next().unwrap_or_default();
    }

    pub fn solves(&self) -> &[Solve] {
        self.data.solves()
    }

    fn persist(&mut self) {
        self.data.settings = Settings {
            font: font::name_at(self.font).into(),
            inspection: self.inspection_on,
            bell: self.bell,
        };
        if let Some(path) = &self.path
            && let Err(e) = store::save(path, &self.data)
        {
            self.status = Some(format!("save failed: {e}"));
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> io::Result<()> {
        while !self.quit {
            terminal.draw(|f| ui::draw(f, self))?;
            if let Some(t) = self.tick_interval()
                && !event::poll(t)?
            {
                self.tick(Instant::now());
                continue;
            }
            // Handle everything that is queued before redrawing, so a stop
            // keypress is never delayed behind a frame.
            loop {
                self.on_event(event::read()?, Instant::now());
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
            self.tick(Instant::now());
        }
        Ok(())
    }

    /// How long the loop may wait for input; `None` blocks until a key.
    fn tick_interval(&self) -> Option<Duration> {
        match self.state {
            State::Idle if self.pb_flash.is_some() || self.scramble.is_empty() => {
                Some(Duration::from_millis(50))
            }
            State::Idle => None,
            State::Inspection { .. } => Some(Duration::from_millis(50)),
            State::Holding { .. } | State::Running { .. } => Some(Duration::from_millis(16)),
        }
    }

    pub fn on_event(&mut self, ev: Event, now: Instant) {
        match ev {
            Event::Key(key) => self.on_key(key, now),
            // A key let go while another window had focus sends no release.
            Event::FocusLost => self.down = None,
            _ => {}
        }
    }

    pub fn on_key(&mut self, key: KeyEvent, now: Instant) {
        let code = typed(key);
        if key.kind == KeyEventKind::Press
            && code == KeyCode::Char('c')
            && key.modifiers.contains(KeyModifiers::CONTROL)
        {
            self.quit = true;
            return;
        }
        // Holding a key auto-repeats it. kitty, foot and Alacritty mark the
        // repeats as such; Ghostty and WezTerm send them as fresh presses, so
        // when release events exist, a press of the key that is already down
        // is a repeat too.
        let repeat = match key.kind {
            KeyEventKind::Repeat => true,
            KeyEventKind::Press => self.enhanced && self.down.replace(key.code) == Some(key.code),
            KeyEventKind::Release => {
                if self.down == Some(key.code) {
                    self.down = None;
                }
                if code == KeyCode::Char(' ') {
                    self.space_release(now);
                }
                return;
            }
        };
        if repeat {
            // Auto-repeat only scrolls the history list.
            let scrolls = matches!(
                code,
                KeyCode::Up
                    | KeyCode::Down
                    | KeyCode::PageUp
                    | KeyCode::PageDown
                    | KeyCode::Char('j' | 'k')
            );
            if let Some(selected) = self.history
                && self.confirm_delete.is_none()
                && scrolls
            {
                self.history_key(code, selected);
            }
            return;
        }
        if matches!(self.state, State::Running { .. }) {
            self.stop(now);
            return;
        }
        self.status = None;
        self.pb_flash = None;
        if std::mem::take(&mut self.confirm_session_delete) {
            if code == KeyCode::Char('y') {
                self.delete_session();
            }
            return;
        }
        if let Some(i) = self.confirm_delete.take() {
            if code == KeyCode::Char('y') {
                self.delete(i);
            }
            return;
        }
        if std::mem::take(&mut self.show_help) || std::mem::take(&mut self.show_preview) {
            return;
        }
        if let Some(selected) = self.picker {
            self.picker_key(code, selected);
            return;
        }
        if let Some(view) = self.stats.as_mut() {
            match code {
                KeyCode::Char('r') => view.range = view.range.next(),
                KeyCode::Char(c @ '1'..='4') => {
                    view.range = Range::ALL[c.to_digit(10).unwrap_or(1) as usize - 1];
                }
                KeyCode::Esc | KeyCode::Char('s' | 'q') => self.stats = None,
                _ => {}
            }
            return;
        }
        if let Some(selected) = self.history {
            self.history_key(code, selected);
            return;
        }
        let idle = self.state == State::Idle;
        let last = self.solves().len().checked_sub(1);
        match code {
            KeyCode::Char(' ') => self.space_press(now),
            KeyCode::Esc => self.cancel(),
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('i') if idle => {
                self.inspection_on = !self.inspection_on;
                self.persist();
            }
            KeyCode::Char('+' | '=') if idle => self.mark(last, Penalty::Plus2),
            KeyCode::Char('d') if idle => self.mark(last, Penalty::Dnf),
            KeyCode::Char('o') if idle => self.mark(last, Penalty::Ok),
            KeyCode::Backspace if idle => self.confirm_delete = last,
            KeyCode::Char('h') if idle => self.history = last,
            // `/` too: WezTerm sends Shift+/ without the SHIFT flag
            KeyCode::Char('?' | '/') | KeyCode::F(1) if idle => self.show_help = true,
            KeyCode::Char('e') if idle => {
                self.picker = Some(
                    puzzle::ALL
                        .iter()
                        .position(|&p| p == self.puzzle())
                        .unwrap_or(1),
                );
            }
            KeyCode::Char('v') if idle => self.show_preview = true,
            KeyCode::Char('s') if idle => self.stats = Some(StatsView::new(self)),
            KeyCode::Char('n') if idle => self.new_session(),
            KeyCode::Char('X') if idle => self.confirm_session_delete = true,
            KeyCode::Char('[') if idle => self.switch_session(-1),
            KeyCode::Char(']') if idle => self.switch_session(1),
            KeyCode::Char('p') => self.run_display = self.run_display.next(),
            KeyCode::Char('b') => {
                self.bell = !self.bell;
                self.persist();
            }
            KeyCode::Char('f') => self.cycle_font(1),
            KeyCode::Char('F') => self.cycle_font(-1),
            _ => {}
        }
    }

    fn space_press(&mut self, now: Instant) {
        self.state = match self.state {
            State::Idle if self.inspection_on => {
                self.warned = 0;
                State::Inspection { start: now }
            }
            State::Idle if self.enhanced => State::Holding {
                since: now,
                inspection: None,
            },
            State::Idle => State::Running {
                start: now,
                penalty: Penalty::Ok,
            },
            State::Inspection { start } if self.enhanced => State::Holding {
                since: now,
                inspection: Some(start),
            },
            State::Inspection { start } => Self::running(now, Some(start)),
            other => other,
        };
    }

    fn space_release(&mut self, now: Instant) {
        if let State::Holding { since, inspection } = self.state {
            self.state = if now.duration_since(since) >= HOLD_READY {
                Self::running(now, inspection)
            } else {
                match inspection {
                    Some(start) => State::Inspection { start },
                    None => State::Idle,
                }
            };
        }
    }

    /// Esc: abandon an inspection or a hold.
    fn cancel(&mut self) {
        if matches!(self.state, State::Inspection { .. } | State::Holding { .. }) {
            self.state = State::Idle;
        }
    }

    /// Start the clock; WCA inspection overrun decides the penalty.
    fn running(now: Instant, inspection: Option<Instant>) -> State {
        let penalty = match inspection.map(|s| now.duration_since(s).as_millis() as u64) {
            Some(ms) if ms >= DNF_AFTER => Penalty::Dnf,
            Some(ms) if ms >= PLUS2_AFTER => Penalty::Plus2,
            _ => Penalty::Ok,
        };
        State::Running {
            start: now,
            penalty,
        }
    }

    fn stop(&mut self, now: Instant) {
        if let State::Running { start, penalty } = self.state {
            let ms = now.duration_since(start).as_millis() as u64;
            let solve = Solve::new(ms, penalty, self.scramble.clone());
            // Only a beaten best counts, so the first solve doesn't flash.
            let new_pb = matches!(
                (stats::best(&self.results()), solve.result()),
                (Some(TimeResult::Ms(old)), TimeResult::Ms(new)) if new < old
            );
            self.pb_flash = new_pb.then_some(now);
            self.data.solves_mut().push(solve);
            self.scramble = self.scrambler.try_next().unwrap_or_default();
            self.state = State::Idle;
            self.persist();
        }
    }

    /// Switch to the next (or previous) digit font and remember it.
    fn cycle_font(&mut self, step: isize) {
        let n = font::choices() as isize;
        self.font = (self.font as isize + step).rem_euclid(n) as usize;
        self.persist();
    }

    fn mark(&mut self, index: Option<usize>, penalty: Penalty) {
        if let Some(solve) = index.and_then(|i| self.data.solves_mut().get_mut(i)) {
            solve.penalty = penalty;
            self.persist();
        }
    }

    fn delete(&mut self, index: usize) {
        if index < self.solves().len() {
            self.data.solves_mut().remove(index);
            self.persist();
        }
        // keep the browser on a solve that still exists
        let len = self.solves().len();
        self.history = self.history.filter(|_| len > 0).map(|i| i.min(len - 1));
    }

    /// Keys while the history browser is open. The list runs newest first, so
    /// "down" moves to older solves (lower indices).
    fn history_key(&mut self, code: KeyCode, selected: usize) {
        let newest = self.solves().len().saturating_sub(1);
        let to = |i: usize| Some(i.min(newest));
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.history = to(selected + 1),
            KeyCode::Down | KeyCode::Char('j') => self.history = to(selected.saturating_sub(1)),
            KeyCode::PageUp => self.history = to(selected + 10),
            KeyCode::PageDown => self.history = to(selected.saturating_sub(10)),
            KeyCode::Home | KeyCode::Char('g') => self.history = to(newest),
            KeyCode::End | KeyCode::Char('G') => self.history = to(0),
            KeyCode::Char('+' | '=') => self.mark(Some(selected), Penalty::Plus2),
            KeyCode::Char('d') => self.mark(Some(selected), Penalty::Dnf),
            KeyCode::Char('o') => self.mark(Some(selected), Penalty::Ok),
            KeyCode::Backspace | KeyCode::Delete => self.confirm_delete = Some(selected),
            KeyCode::Esc | KeyCode::Char('h' | 'q') => self.history = None,
            _ => {}
        }
    }

    /// Start a new session, unless the current one is still empty.
    fn new_session(&mut self) {
        if self.solves().is_empty() {
            return;
        }
        self.data.sessions.push(Session::new(self.puzzle()));
        self.data.current = self.data.sessions.len() - 1;
        self.persist();
    }

    /// Remove the current session and all its solves. If it is the last
    /// session of its puzzle it is emptied instead, so every puzzle you have
    /// used keeps a session.
    fn delete_session(&mut self) {
        let puzzle = self.puzzle();
        let of_puzzle = self.data.sessions_of(puzzle);
        if of_puzzle.len() > 1 {
            let at = of_puzzle
                .iter()
                .position(|&i| i == self.data.current)
                .unwrap_or(0);
            self.data.sessions.remove(self.data.current);
            // stay on the same puzzle: the session that took its place, or
            // the one before if it was the last
            let remaining = self.data.sessions_of(puzzle);
            self.data.current = remaining[at.min(remaining.len() - 1)];
        } else {
            self.data.sessions[self.data.current] = Session::new(puzzle);
        }
        self.persist();
    }

    /// Move to the previous (-1) or next (+1) session of the same puzzle.
    fn switch_session(&mut self, step: isize) {
        let of_puzzle = self.data.sessions_of(self.puzzle());
        let at = of_puzzle
            .iter()
            .position(|&i| i == self.data.current)
            .unwrap_or(0);
        if let Some(&i) = at.checked_add_signed(step).and_then(|k| of_puzzle.get(k)) {
            self.data.current = i;
            self.persist();
        }
    }

    /// Switch to `puzzle`: its most recent session, or a new one if you have
    /// never timed it.
    fn select_puzzle(&mut self, puzzle: Puzzle) {
        if puzzle == self.puzzle() {
            return;
        }
        let existing = self.data.sessions_of(puzzle).last().copied();
        self.data.current = existing.unwrap_or_else(|| {
            self.data.sessions.push(Session::new(puzzle));
            self.data.sessions.len() - 1
        });
        self.history = None;
        self.reset_scrambler();
        self.persist();
    }

    /// Keys while the puzzle picker is open.
    fn picker_key(&mut self, code: KeyCode, selected: usize) {
        let last = puzzle::ALL.len() - 1;
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.picker = Some(selected.saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => self.picker = Some((selected + 1).min(last)),
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.picker = None;
                self.select_puzzle(puzzle::ALL[selected]);
            }
            // 2..7 pick a cube by its size
            KeyCode::Char(c @ '2'..='7') => {
                self.picker = None;
                let size = c.to_digit(10).unwrap_or(3) as usize;
                if let Some(&p) = puzzle::ALL.iter().find(|p| p.size() == size) {
                    self.select_puzzle(p);
                }
            }
            KeyCode::Esc | KeyCode::Char('e' | 'q') => self.picker = None,
            _ => {}
        }
    }

    /// Periodic work: the inspection warning bell.
    pub fn tick(&mut self, now: Instant) {
        if self.scramble.is_empty()
            && let Some(s) = self.scrambler.try_next()
        {
            self.scramble = s;
        }
        if self
            .pb_flash
            .is_some_and(|t| now.duration_since(t) >= PB_FLASH)
        {
            self.pb_flash = None;
        }
        let Some(start) = self.inspection_start() else {
            return;
        };
        let ms = now.duration_since(start).as_millis() as u64;
        let level = if ms >= WARN_12S {
            2
        } else if ms >= WARN_8S {
            1
        } else {
            0
        };
        if level > self.warned {
            self.warned = level;
            if self.bell {
                let mut out = io::stdout();
                let _ = out.write_all(b"\x07");
                let _ = out.flush();
            }
        }
    }

    fn inspection_start(&self) -> Option<Instant> {
        match self.state {
            State::Inspection { start } => Some(start),
            State::Holding { inspection, .. } => inspection,
            _ => None,
        }
    }

    pub fn view(&self, now: Instant) -> View {
        let prec = self.precision;
        match self.state {
            State::Idle if self.confirm_session_delete => View {
                text: self.last_text(),
                tone: Tone::Warn12,
                hint: format!(
                    "delete session {} and its {} solves? y / n",
                    self.data.current + 1,
                    self.solves().len()
                ),
            },
            State::Idle if self.confirm_delete.is_some() => View {
                text: self.last_text(),
                tone: Tone::Warn12,
                hint: self.delete_prompt(),
            },
            State::Idle
                if self
                    .pb_flash
                    .is_some_and(|t| now.duration_since(t) < PB_FLASH) =>
            {
                let elapsed = now.duration_since(self.pb_flash.unwrap_or(now)).as_millis();
                let tone = if (elapsed / PB_BLINK_MS).is_multiple_of(2) {
                    Tone::Pb
                } else {
                    Tone::PbAlt
                };
                View {
                    text: self.last_text(),
                    tone,
                    hint: "new personal best!".into(),
                }
            }
            State::Idle => {
                let text = self.last_text();
                let hint = match (self.enhanced, self.inspection_on) {
                    (_, true) => "space: start inspection",
                    (true, false) => "hold space, release to start",
                    (false, false) => "space: start",
                };
                View {
                    text,
                    tone: Tone::Idle,
                    hint: hint.into(),
                }
            }
            State::Inspection { start } => {
                let ms = now.duration_since(start).as_millis() as u64;
                let (tone, hint) = match ms {
                    WARN_12S.. => (Tone::Warn12, "12s!"),
                    WARN_8S.. => (Tone::Warn8, "8s!"),
                    _ if self.enhanced => (Tone::Inspect, "hold space when ready"),
                    _ => (Tone::Inspect, "space: start"),
                };
                View {
                    text: fmt::fmt_inspection(ms),
                    tone,
                    hint: hint.into(),
                }
            }
            State::Holding { since, inspection } => {
                let ready = now.duration_since(since) >= HOLD_READY;
                let text = match inspection {
                    Some(start) => {
                        fmt::fmt_inspection(now.duration_since(start).as_millis() as u64)
                    }
                    None => fmt::fmt_time(0, prec),
                };
                let (tone, hint) = if ready {
                    (Tone::HoldReady, "ready - release to start")
                } else {
                    (Tone::HoldWait, "hold...")
                };
                View {
                    text,
                    tone,
                    hint: hint.into(),
                }
            }
            State::Running { start, .. } => {
                let ms = now.duration_since(start).as_millis() as u64;
                View {
                    text: fmt::fmt_running(ms, self.run_display, prec),
                    tone: Tone::Running,
                    hint: String::new(),
                }
            }
        }
    }

    /// The y/n question shown while a delete waits for confirmation.
    pub fn delete_prompt(&self) -> String {
        match self.confirm_delete {
            Some(i) if i + 1 == self.solves().len() => "delete last solve? y / n".into(),
            Some(i) => format!("delete solve {}? y / n", i + 1),
            None => String::new(),
        }
    }

    fn last_text(&self) -> String {
        match self.solves().last() {
            Some(s) => fmt::fmt_solve(s.ms, s.penalty, self.precision),
            None => fmt::fmt_time(0, self.precision),
        }
    }

    pub fn results(&self) -> Vec<TimeResult> {
        self.solves().iter().map(Solve::result).collect()
    }

    pub fn summary(&self) -> Summary {
        stats::summarize(&self.results())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, ms: u64) -> Instant {
        base + Duration::from_millis(ms)
    }

    fn press(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Press)
    }

    fn release(code: KeyCode) -> KeyEvent {
        KeyEvent::new_with_kind(code, KeyModifiers::NONE, KeyEventKind::Release)
    }

    const SPACE: KeyCode = KeyCode::Char(' ');

    #[test]
    fn full_solve_with_inspection() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.on_key(press(SPACE), at(t, 0));
        assert!(matches!(app.state, State::Inspection { .. }));
        app.on_key(release(SPACE), at(t, 80));
        assert!(matches!(app.state, State::Inspection { .. }));
        app.on_key(press(SPACE), at(t, 5_000));
        assert!(matches!(app.state, State::Holding { .. }));
        app.on_key(release(SPACE), at(t, 5_400));
        assert!(matches!(
            app.state,
            State::Running {
                penalty: Penalty::Ok,
                ..
            }
        ));
        app.on_key(press(KeyCode::Char('x')), at(t, 17_890));
        assert_eq!(app.state, State::Idle);
        assert_eq!(app.solves().len(), 1);
        assert_eq!(app.solves()[0].ms, 12_490);
        assert_eq!(app.solves()[0].penalty, Penalty::Ok);
    }

    #[test]
    fn early_release_returns_to_inspection() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.on_key(press(SPACE), at(t, 0));
        app.on_key(press(SPACE), at(t, 3_000));
        app.on_key(release(SPACE), at(t, 3_200));
        assert_eq!(app.state, State::Inspection { start: t });
    }

    #[test]
    fn early_release_without_inspection_returns_to_idle() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.inspection_on = false;
        app.on_key(press(SPACE), at(t, 0));
        assert!(matches!(
            app.state,
            State::Holding {
                inspection: None,
                ..
            }
        ));
        app.on_key(release(SPACE), at(t, 299));
        assert_eq!(app.state, State::Idle);
        app.on_key(press(SPACE), at(t, 1_000));
        app.on_key(release(SPACE), at(t, 1_300));
        assert!(matches!(
            app.state,
            State::Running {
                penalty: Penalty::Ok,
                ..
            }
        ));
    }

    #[test]
    fn inspection_overrun_penalties() {
        for (start_ms, penalty) in [
            (14_999, Penalty::Ok),
            (15_000, Penalty::Plus2),
            (16_999, Penalty::Plus2),
            (17_000, Penalty::Dnf),
        ] {
            let t = Instant::now();
            let mut app = App::new(true);
            app.on_key(press(SPACE), at(t, 0));
            app.on_key(release(SPACE), at(t, 80));
            app.on_key(press(SPACE), at(t, start_ms - 400));
            app.on_key(release(SPACE), at(t, start_ms));
            assert!(
                matches!(app.state, State::Running { penalty: p, .. } if p == penalty),
                "{start_ms}"
            );
        }
    }

    #[test]
    fn penalty_is_saved_with_the_solve() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.on_key(press(SPACE), at(t, 0));
        app.on_key(release(SPACE), at(t, 80));
        app.on_key(press(SPACE), at(t, 15_000));
        app.on_key(release(SPACE), at(t, 15_400));
        app.on_key(press(SPACE), at(t, 25_400));
        assert_eq!(app.solves()[0].penalty, Penalty::Plus2);
        assert_eq!(app.solves()[0].result(), TimeResult::Ms(12_000));
    }

    #[test]
    fn stopping_with_space_ignores_its_release() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.inspection_on = false;
        app.on_key(press(SPACE), at(t, 0));
        app.on_key(release(SPACE), at(t, 400));
        app.on_key(press(SPACE), at(t, 9_000));
        app.on_key(release(SPACE), at(t, 9_100));
        assert_eq!(app.state, State::Idle);
        assert_eq!(app.solves().len(), 1);
    }

    #[test]
    fn repeats_are_ignored() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.inspection_on = false;
        app.on_key(press(SPACE), at(t, 0));
        let rep = KeyEvent::new_with_kind(SPACE, KeyModifiers::NONE, KeyEventKind::Repeat);
        app.on_key(rep, at(t, 500));
        assert!(matches!(app.state, State::Holding { since, .. } if since == t));
    }

    #[test]
    fn basic_mode_space_starts_and_stops() {
        let t = Instant::now();
        let mut app = App::new(false);
        app.inspection_on = false;
        app.on_key(press(SPACE), at(t, 0));
        assert!(matches!(app.state, State::Running { .. }));
        app.on_key(press(SPACE), at(t, 7_770));
        assert_eq!(app.solves()[0].ms, 7_770);
    }

    #[test]
    fn basic_mode_with_inspection() {
        let t = Instant::now();
        let mut app = App::new(false);
        app.on_key(press(SPACE), at(t, 0));
        assert!(matches!(app.state, State::Inspection { .. }));
        app.on_key(press(SPACE), at(t, 16_000));
        assert!(matches!(
            app.state,
            State::Running {
                penalty: Penalty::Plus2,
                ..
            }
        ));
    }

    #[test]
    fn escape_cancels_inspection_and_hold() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.on_key(press(SPACE), at(t, 0));
        app.on_key(press(KeyCode::Esc), at(t, 100));
        assert_eq!(app.state, State::Idle);
        app.on_key(press(SPACE), at(t, 200));
        app.on_key(press(SPACE), at(t, 300));
        app.on_key(press(KeyCode::Esc), at(t, 400));
        assert_eq!(app.state, State::Idle);
        assert!(app.solves().is_empty());
    }

    #[test]
    fn q_quits_when_idle_but_stops_when_running() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.inspection_on = false;
        app.on_key(press(SPACE), at(t, 0));
        app.on_key(release(SPACE), at(t, 400));
        app.on_key(press(KeyCode::Char('q')), at(t, 5_000));
        app.on_key(release(KeyCode::Char('q')), at(t, 5_050));
        assert!(!app.quit);
        assert_eq!(app.solves().len(), 1);
        app.on_key(press(KeyCode::Char('q')), at(t, 5_100));
        assert!(app.quit);
    }

    #[test]
    fn ctrl_c_quits_even_while_running() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.state = State::Running {
            start: t,
            penalty: Penalty::Ok,
        };
        let ev = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        app.on_key(ev, at(t, 100));
        assert!(app.quit);
    }

    #[test]
    fn inspection_toggle_only_when_idle() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('i'));
        assert!(!app.inspection_on);
        key(&mut app, KeyCode::Char('i'));
        key(&mut app, SPACE);
        key(&mut app, KeyCode::Char('i'));
        assert!(app.inspection_on);
    }

    #[test]
    fn views_follow_the_state() {
        let t = Instant::now();
        let mut app = App::new(true);
        assert_eq!(app.view(t).text, "0.00");
        app.on_key(press(SPACE), at(t, 0));
        app.on_key(release(SPACE), at(t, 80));
        assert_eq!(app.view(at(t, 500)).text, "15");
        assert_eq!(app.view(at(t, 7_100)).text, "8");
        assert_eq!(app.view(at(t, 7_100)).tone, Tone::Inspect);
        let v = app.view(at(t, 8_000));
        assert_eq!((v.tone, v.hint.as_str()), (Tone::Warn8, "8s!"));
        let v = app.view(at(t, 12_500));
        assert_eq!((v.tone, v.hint.as_str()), (Tone::Warn12, "12s!"));
        assert_eq!(app.view(at(t, 15_200)).text, "+2");
        assert_eq!(app.view(at(t, 17_200)).text, "DNF");
        app.on_key(press(SPACE), at(t, 1_000));
        assert_eq!(app.view(at(t, 1_100)).tone, Tone::HoldWait);
        assert_eq!(app.view(at(t, 1_300)).tone, Tone::HoldReady);
        app.on_key(release(SPACE), at(t, 1_400));
        let v = app.view(at(t, 3_456));
        assert_eq!((v.text.as_str(), v.tone), ("2.05", Tone::Running));
    }

    #[test]
    fn idle_view_shows_the_last_solve() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.data
            .solves_mut()
            .push(Solve::new(12_340, Penalty::Plus2, String::new()));
        assert_eq!(app.view(t).text, "14.34+");
    }

    /// A full keystroke, press then release, like a real keyboard.
    fn key(app: &mut App, code: KeyCode) {
        app.on_key(press(code), Instant::now());
        app.on_key(release(code), Instant::now());
    }

    fn with_solves(times: &[u64]) -> App {
        let mut app = App::new(true);
        for &ms in times {
            app.data
                .solves_mut()
                .push(Solve::new(ms, Penalty::Ok, String::new()));
        }
        app
    }

    #[test]
    fn penalty_keys_mark_the_last_solve() {
        let mut app = with_solves(&[10_000, 12_340]);
        key(&mut app, KeyCode::Char('+'));
        assert_eq!(app.solves()[1].penalty, Penalty::Plus2);
        assert_eq!(app.view(Instant::now()).text, "14.34+");
        key(&mut app, KeyCode::Char('d'));
        assert_eq!(app.solves()[1].penalty, Penalty::Dnf);
        assert_eq!(app.view(Instant::now()).text, "DNF");
        key(&mut app, KeyCode::Char('o'));
        assert_eq!(app.solves()[1].penalty, Penalty::Ok);
        assert_eq!(app.solves()[0].penalty, Penalty::Ok);
    }

    #[test]
    fn shift_equals_also_marks_plus2() {
        let mut app = with_solves(&[10_000]);
        key(&mut app, KeyCode::Char('='));
        assert_eq!(app.solves()[0].penalty, Penalty::Plus2);
    }

    #[test]
    fn penalty_keys_without_solves_do_nothing() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('d'));
        assert!(app.solves().is_empty());
    }

    #[test]
    fn penalty_keys_ignored_while_inspecting() {
        let mut app = with_solves(&[10_000]);
        key(&mut app, SPACE);
        key(&mut app, KeyCode::Char('d'));
        assert_eq!(app.solves()[0].penalty, Penalty::Ok);
    }

    #[test]
    fn backspace_asks_before_deleting() {
        let mut app = with_solves(&[1_000, 2_000]);
        key(&mut app, KeyCode::Backspace);
        assert!(app.confirm_delete.is_some());
        assert_eq!(app.solves().len(), 2);
        assert_eq!(app.view(Instant::now()).hint, "delete last solve? y / n");
        key(&mut app, KeyCode::Char('y'));
        assert_eq!(app.solves().len(), 1);
        assert_eq!(app.solves()[0].ms, 1_000);
        assert!(app.confirm_delete.is_none());
    }

    #[test]
    fn any_other_key_cancels_the_delete() {
        for cancel in [KeyCode::Char('n'), KeyCode::Esc, SPACE, KeyCode::Char('q')] {
            let mut app = with_solves(&[1_000]);
            key(&mut app, KeyCode::Backspace);
            key(&mut app, cancel);
            assert_eq!(app.solves().len(), 1);
            assert!(app.confirm_delete.is_none());
            assert_eq!(app.state, State::Idle);
            assert!(!app.quit);
        }
    }

    #[test]
    fn backspace_with_no_solves_does_not_ask() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Backspace);
        assert!(app.confirm_delete.is_none());
    }

    #[test]
    fn new_session_and_navigation() {
        let mut app = with_solves(&[1_000]);
        key(&mut app, KeyCode::Char('n'));
        assert_eq!((app.data.current, app.data.sessions.len()), (1, 2));
        assert!(app.solves().is_empty());
        // an empty session isn't duplicated
        key(&mut app, KeyCode::Char('n'));
        assert_eq!(app.data.sessions.len(), 2);
        key(&mut app, KeyCode::Char('['));
        assert_eq!(app.solves().len(), 1);
        key(&mut app, KeyCode::Char('['));
        assert_eq!(app.data.current, 0);
        key(&mut app, KeyCode::Char(']'));
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(app.data.current, 1);
    }

    /// Tick until the background scramble arrives.
    fn settle(app: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while app.scramble.is_empty() {
            assert!(Instant::now() < deadline, "no scramble in 10 s");
            app.tick(Instant::now());
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn stopping_a_solve_makes_a_new_scramble() {
        let t = Instant::now();
        let mut app = App::new(true);
        settle(&mut app);
        app.inspection_on = false;
        let before = app.scramble.clone();
        app.on_key(press(SPACE), t);
        app.on_key(release(SPACE), at(t, 400));
        app.on_key(press(SPACE), at(t, 5_000));
        settle(&mut app);
        assert_ne!(app.scramble, before);
        assert_eq!(app.solves()[0].scramble, before);
    }

    #[test]
    fn startup_does_not_wait_for_a_scramble() {
        // App::new returns at once; while the scramble is missing the loop
        // keeps ticking (instead of blocking on input) so it shows up.
        let app = App::new(true);
        if app.scramble.is_empty() {
            assert!(app.tick_interval().is_some());
        }
        let mut app = app;
        settle(&mut app);
        assert!(app.tick_interval().is_none());
    }

    #[test]
    fn changes_are_saved_and_reloaded() {
        let dir = std::env::temp_dir().join(format!("cubetimer-app-test-{}", std::process::id()));
        let path = dir.join("sessions.json");
        let _ = std::fs::remove_dir_all(&dir);

        let mut app = App::new(true).with_storage(path.clone(), store::load(&path));
        let t = Instant::now();
        app.inspection_on = false;
        app.on_key(press(SPACE), t);
        app.on_key(release(SPACE), at(t, 400));
        app.on_key(press(SPACE), at(t, 9_400));
        key(&mut app, KeyCode::Char('+'));
        key(&mut app, KeyCode::Char('n'));

        let reloaded = store::load(&path).data;
        assert_eq!(reloaded, app.data);
        assert_eq!(reloaded.sessions[0].solves[0].ms, 9_000);
        assert_eq!(reloaded.sessions[0].solves[0].penalty, Penalty::Plus2);
        assert_eq!(reloaded.current, 1);

        key(&mut app, KeyCode::Char('['));
        key(&mut app, KeyCode::Backspace);
        key(&mut app, KeyCode::Char('y'));
        assert!(store::load(&path).data.sessions[0].solves.is_empty());
    }

    #[test]
    fn save_failure_shows_in_the_status_line() {
        let mut app = with_solves(&[1_000]);
        // a path whose parent is a file can't be created
        let file = std::env::temp_dir().join(format!("cubetimer-app-file-{}", std::process::id()));
        std::fs::write(&file, "x").unwrap();
        app.path = Some(file.join("sessions.json"));
        key(&mut app, KeyCode::Char('d'));
        assert!(
            app.status
                .as_deref()
                .is_some_and(|s| s.starts_with("save failed"))
        );
        key(&mut app, KeyCode::Char('o'));
        std::fs::remove_file(&file).unwrap();
    }

    fn solve_at(app: &mut App, t: Instant, ms: u64) {
        app.inspection_on = false;
        app.on_key(press(SPACE), t);
        app.on_key(release(SPACE), at(t, 400));
        app.on_key(press(SPACE), at(t, 400 + ms));
        app.on_key(release(SPACE), at(t, 450 + ms));
    }

    #[test]
    fn beating_the_best_flashes_then_fades() {
        let t = Instant::now();
        let mut app = with_solves(&[10_000, 12_000]);
        solve_at(&mut app, t, 9_000);
        let done = at(t, 9_400);
        let v = app.view(done);
        assert_eq!((v.tone, v.hint.as_str()), (Tone::Pb, "new personal best!"));
        assert_eq!(v.text, "9.00");
        assert_eq!(app.view(at(done, 250)).tone, Tone::PbAlt);
        assert_eq!(app.view(at(done, 500)).tone, Tone::Pb);
        assert_eq!(app.view(at(done, 1_499)).tone, Tone::PbAlt);
        assert_eq!(app.view(at(done, 1_500)).tone, Tone::Idle);
        assert!(app.tick_interval().is_some());
        settle(&mut app); // a pending scramble also keeps the loop ticking
        app.tick(at(done, 1_500));
        assert!(app.tick_interval().is_none());
    }

    #[test]
    fn no_flash_for_first_solve_slower_or_equal_times() {
        let t = Instant::now();
        let mut app = App::new(true);
        solve_at(&mut app, t, 9_000);
        assert_eq!(app.view(at(t, 9_400)).tone, Tone::Idle);
        for ms in [9_000, 9_500] {
            solve_at(&mut app, t, ms);
            assert_eq!(app.view(at(t, 400 + ms)).tone, Tone::Idle, "{ms}");
        }
    }

    #[test]
    fn dnf_and_plus2_never_flash() {
        let t = Instant::now();
        let mut app = with_solves(&[10_000]);
        app.state = State::Running {
            start: t,
            penalty: Penalty::Dnf,
        };
        app.on_key(press(KeyCode::Char('x')), at(t, 5_000));
        app.on_key(release(KeyCode::Char('x')), at(t, 5_050));
        assert_eq!(app.view(at(t, 5_000)).tone, Tone::Idle);
        app.state = State::Running {
            start: t,
            penalty: Penalty::Plus2,
        };
        app.on_key(press(KeyCode::Char('x')), at(t, 9_000)); // 9 + 2 = 11 s, slower
        assert_eq!(app.view(at(t, 9_000)).tone, Tone::Idle);
    }

    #[test]
    fn a_keypress_ends_the_flash() {
        let t = Instant::now();
        let mut app = with_solves(&[10_000]);
        solve_at(&mut app, t, 5_000);
        app.on_key(press(KeyCode::Char('p')), at(t, 5_500));
        assert_eq!(app.view(at(t, 5_500)).tone, Tone::Idle);
    }

    #[test]
    fn f_cycles_through_every_font_and_wraps() {
        let mut app = App::new(true);
        let start = app.font;
        let mut seen = vec![start];
        for _ in 0..font::choices() {
            key(&mut app, KeyCode::Char('f'));
            seen.push(app.font);
        }
        assert_eq!(*seen.last().unwrap(), start);
        seen.pop();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), font::choices());
        // shift-F goes back
        key(&mut app, KeyCode::Char('F'));
        assert_eq!(app.font, (start + font::choices() - 1) % font::choices());
    }

    #[test]
    fn font_works_in_any_state_but_not_while_running() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.on_key(press(SPACE), t); // inspection
        app.on_key(release(SPACE), t);
        let before = app.font;
        app.on_key(press(KeyCode::Char('f')), t);
        app.on_key(release(KeyCode::Char('f')), t);
        assert_ne!(app.font, before);
        app.state = State::Running {
            start: t,
            penalty: Penalty::Ok,
        };
        let before = app.font;
        app.on_key(press(KeyCode::Char('f')), at(t, 100)); // stops the solve instead
        assert_eq!(app.font, before);
        assert_eq!(app.solves().len(), 1);
    }

    #[test]
    fn settings_survive_a_restart() {
        let dir =
            std::env::temp_dir().join(format!("cubetimer-settings-test-{}", std::process::id()));
        let path = dir.join("sessions.json");
        let _ = std::fs::remove_dir_all(&dir);

        let mut app = App::new(true).with_storage(path.clone(), store::load(&path));
        assert_eq!(font::name_at(app.font), font::DEFAULT);
        let target = font::index_of("pixel");
        while app.font != target {
            key(&mut app, KeyCode::Char('f'));
        }
        key(&mut app, KeyCode::Char('i')); // inspection off
        key(&mut app, KeyCode::Char('b')); // bell on

        let app = App::new(true).with_storage(path.clone(), store::load(&path));
        assert_eq!(font::name_at(app.font), "pixel");
        assert!(!app.inspection_on);
        assert!(app.bell);
    }

    #[test]
    fn an_unknown_saved_font_falls_back_to_the_default() {
        let mut loaded = store::Loaded {
            data: Data::default(),
            warning: None,
        };
        loaded.data.settings.font = "from-a-newer-version".into();
        let app = App::new(true).with_storage(PathBuf::from("/nonexistent/sessions.json"), loaded);
        assert_eq!(font::name_at(app.font), font::DEFAULT);
    }

    #[test]
    fn history_browses_marks_and_deletes_any_solve() {
        let mut app = with_solves(&[1_000, 2_000, 3_000, 4_000]);
        key(&mut app, KeyCode::Char('h'));
        assert_eq!(app.history, Some(3)); // newest first
        key(&mut app, KeyCode::Down);
        key(&mut app, KeyCode::Char('j'));
        assert_eq!(app.history, Some(1));
        key(&mut app, KeyCode::Char('d'));
        assert_eq!(app.solves()[1].penalty, Penalty::Dnf);
        assert_eq!(app.solves()[3].penalty, Penalty::Ok);
        key(&mut app, KeyCode::Char('+'));
        assert_eq!(app.solves()[1].penalty, Penalty::Plus2);
        // delete asks first, then removes exactly that solve
        key(&mut app, KeyCode::Backspace);
        assert_eq!(app.delete_prompt(), "delete solve 2? y / n");
        key(&mut app, KeyCode::Char('y'));
        let left: Vec<u64> = app.solves().iter().map(|s| s.ms).collect();
        assert_eq!(left, vec![1_000, 3_000, 4_000]);
        assert_eq!(app.history, Some(1));
        key(&mut app, KeyCode::Esc);
        assert_eq!(app.history, None);
    }

    #[test]
    fn history_selection_stays_in_range() {
        let mut app = with_solves(&[1_000, 2_000, 3_000]);
        key(&mut app, KeyCode::Char('h'));
        key(&mut app, KeyCode::Up);
        key(&mut app, KeyCode::PageUp);
        assert_eq!(app.history, Some(2));
        key(&mut app, KeyCode::PageDown);
        key(&mut app, KeyCode::Down);
        assert_eq!(app.history, Some(0));
        key(&mut app, KeyCode::Home);
        assert_eq!(app.history, Some(2));
        key(&mut app, KeyCode::End);
        assert_eq!(app.history, Some(0));
    }

    #[test]
    fn deleting_the_only_solve_closes_history() {
        let mut app = with_solves(&[1_000]);
        key(&mut app, KeyCode::Char('h'));
        key(&mut app, KeyCode::Delete);
        key(&mut app, KeyCode::Char('y'));
        assert!(app.solves().is_empty());
        assert_eq!(app.history, None);
    }

    #[test]
    fn history_keys_do_not_leak_to_the_timer() {
        let mut app = with_solves(&[1_000]);
        key(&mut app, KeyCode::Char('h'));
        key(&mut app, SPACE);
        key(&mut app, KeyCode::Char('q'));
        assert_eq!(app.state, State::Idle);
        assert!(!app.quit);
        assert_eq!(app.history, None); // q closed the browser, not the app
    }

    #[test]
    fn history_needs_solves_and_an_idle_timer() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('h'));
        assert_eq!(app.history, None);
        let mut app = with_solves(&[1_000]);
        key(&mut app, SPACE); // inspecting
        key(&mut app, KeyCode::Char('h'));
        assert_eq!(app.history, None);
    }

    #[test]
    fn help_opens_with_question_mark_and_any_key_closes_it() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('?'));
        assert!(app.show_help);
        key(&mut app, SPACE);
        assert!(!app.show_help);
        assert_eq!(app.state, State::Idle); // the closing key does nothing else
    }

    #[test]
    fn ctrl_c_quits_from_history_and_help() {
        let ctrl_c = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        let mut app = with_solves(&[1_000]);
        key(&mut app, KeyCode::Char('h'));
        app.on_key(ctrl_c, Instant::now());
        assert!(app.quit);
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('?'));
        app.on_key(ctrl_c, Instant::now());
        assert!(app.quit);
    }

    fn press_with(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, KeyEventKind::Press)
    }

    // What real keyboards send under the kitty protocol: the unshifted key
    // plus SHIFT (checked in kitty, foot, Ghostty, Alacritty and WezTerm).
    #[test]
    fn shifted_keys_arrive_as_base_key_plus_shift() {
        let shift = KeyModifiers::SHIFT;
        let mut app = with_solves(&[10_000]);
        app.on_key(press_with(KeyCode::Char('='), shift), Instant::now());
        assert_eq!(app.solves()[0].penalty, Penalty::Plus2);

        let mut app = App::new(true);
        app.on_key(press_with(KeyCode::Char('/'), shift), Instant::now());
        assert!(app.show_help, "Shift+/ is ?");

        let mut app = App::new(true);
        let start = app.font;
        app.on_key(press_with(KeyCode::Char('f'), shift), Instant::now());
        assert_eq!(
            app.font,
            (start + font::choices() - 1) % font::choices(),
            "Shift+f is F"
        );

        let mut app = with_solves(&[1_000, 2_000, 3_000]);
        key(&mut app, KeyCode::Char('h'));
        app.on_key(press_with(KeyCode::Char('g'), shift), Instant::now());
        assert_eq!(app.history, Some(0), "Shift+g is G (oldest)");
    }

    #[test]
    fn wezterm_style_shift_slash_still_opens_help() {
        // WezTerm reports the press of Shift+/ as a bare '/'
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('/'));
        assert!(app.show_help);
    }

    #[test]
    fn f1_opens_help_on_any_layout() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::F(1));
        assert!(app.show_help);
    }

    // Ghostty and WezTerm send a held key's auto-repeat as more presses.
    #[test]
    fn repeated_presses_of_a_held_key_are_repeats() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.inspection_on = false;
        app.state = State::Running {
            start: t,
            penalty: Penalty::Ok,
        };
        // stop with space and keep holding it: the repeats must not start
        // a new attempt
        app.on_key(press(SPACE), at(t, 9_000));
        for ms in [9_500, 9_533, 9_566, 9_600] {
            app.on_key(press(SPACE), at(t, ms));
        }
        app.on_key(release(SPACE), at(t, 9_700));
        assert_eq!(app.state, State::Idle);
        assert_eq!(app.solves().len(), 1);
        // the next real tap works
        app.on_key(press(SPACE), at(t, 10_000));
        assert!(matches!(app.state, State::Holding { .. }));
    }

    #[test]
    fn holding_space_from_idle_only_starts_inspection() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.on_key(press(SPACE), at(t, 0));
        for ms in [500, 533, 566] {
            app.on_key(press(SPACE), at(t, ms)); // auto-repeat as presses
        }
        app.on_key(release(SPACE), at(t, 900));
        assert!(
            matches!(app.state, State::Inspection { .. }),
            "{:?}",
            app.state
        );
    }

    #[test]
    fn holding_the_stop_key_does_not_quit_or_mark() {
        let t = Instant::now();
        for stop in ['q', 'd'] {
            let mut app = App::new(true);
            app.state = State::Running {
                start: t,
                penalty: Penalty::Ok,
            };
            app.on_key(press(KeyCode::Char(stop)), at(t, 5_000));
            app.on_key(press(KeyCode::Char(stop)), at(t, 5_500)); // repeat
            assert!(!app.quit, "{stop}");
            assert_eq!(app.solves()[0].penalty, Penalty::Ok, "{stop}");
        }
    }

    #[test]
    fn without_release_events_every_press_counts() {
        // basic mode can't tell repeats from presses, so it doesn't try
        let t = Instant::now();
        let mut app = App::new(false);
        app.inspection_on = false;
        app.on_key(press(SPACE), t);
        assert!(matches!(app.state, State::Running { .. }));
        app.on_key(press(SPACE), at(t, 3_000));
        assert_eq!(app.solves().len(), 1);
    }

    #[test]
    fn holding_up_or_down_scrolls_history() {
        let mut app = with_solves(&[1_000, 2_000, 3_000, 4_000, 5_000]);
        key(&mut app, KeyCode::Char('h'));
        let t = Instant::now();
        // kitty-style repeats
        app.on_key(press(KeyCode::Down), t);
        let rep = KeyEvent::new_with_kind(KeyCode::Down, KeyModifiers::NONE, KeyEventKind::Repeat);
        app.on_key(rep, t);
        app.on_key(release(KeyCode::Down), t);
        assert_eq!(app.history, Some(2));
        // Ghostty-style repeats (more presses)
        app.on_key(press(KeyCode::Down), t);
        app.on_key(press(KeyCode::Down), t);
        app.on_key(release(KeyCode::Down), t);
        assert_eq!(app.history, Some(0));
        // but repeats of other keys do nothing there
        app.on_key(press(KeyCode::Char('d')), t);
        app.on_key(press(KeyCode::Char('d')), t);
        assert_eq!(app.solves()[0].penalty, Penalty::Dnf);
    }

    #[test]
    fn losing_focus_forgets_a_held_key() {
        let t = Instant::now();
        let mut app = with_solves(&[1_000]);
        app.on_key(press(KeyCode::Char('d')), t);
        app.on_event(Event::FocusLost, t); // released in another window
        app.on_key(press(KeyCode::Char('o')), t);
        app.on_key(release(KeyCode::Char('o')), t);
        app.on_key(press(KeyCode::Char('d')), t);
        assert_eq!(app.solves()[0].penalty, Penalty::Dnf);
        let mut app = with_solves(&[1_000]);
        app.on_key(press(KeyCode::Char('i')), t);
        app.on_event(Event::FocusLost, t);
        app.on_key(press(KeyCode::Char('i')), t); // a new press, not a repeat
        assert!(app.inspection_on);
    }

    #[test]
    fn deleting_a_session_asks_first_and_keeps_the_others() {
        let mut app = with_solves(&[1_000, 2_000]);
        key(&mut app, KeyCode::Char('n'));
        app.data
            .solves_mut()
            .push(Solve::new(5_000, Penalty::Ok, String::new()));
        key(&mut app, KeyCode::Char('n'));
        app.data
            .solves_mut()
            .push(Solve::new(7_000, Penalty::Ok, String::new()));
        key(&mut app, KeyCode::Char('[')); // session 2 of 3
        key(&mut app, KeyCode::Char('X'));
        assert_eq!(
            app.view(Instant::now()).hint,
            "delete session 2 and its 1 solves? y / n"
        );
        key(&mut app, KeyCode::Char('n')); // anything but y cancels
        assert_eq!(app.data.sessions.len(), 3);
        key(&mut app, KeyCode::Char('X'));
        key(&mut app, KeyCode::Char('y'));
        assert_eq!(app.data.sessions.len(), 2);
        assert_eq!(app.solves()[0].ms, 7_000); // now on what was session 3
        assert_eq!(app.data.sessions[0].solves.len(), 2);
    }

    #[test]
    fn deleting_the_last_session_empties_it() {
        let mut app = with_solves(&[1_000]);
        key(&mut app, KeyCode::Char('X'));
        key(&mut app, KeyCode::Char('y'));
        assert_eq!(app.data.sessions.len(), 1);
        assert!(app.solves().is_empty());
    }

    #[test]
    fn deleting_the_final_session_index_moves_back() {
        let mut app = with_solves(&[1_000]);
        key(&mut app, KeyCode::Char('n'));
        app.data
            .solves_mut()
            .push(Solve::new(2_000, Penalty::Ok, String::new()));
        assert_eq!(app.data.current, 1);
        key(&mut app, KeyCode::Char('X'));
        key(&mut app, KeyCode::Char('y'));
        assert_eq!((app.data.current, app.data.sessions.len()), (0, 1));
        assert_eq!(app.solves()[0].ms, 1_000);
    }

    #[test]
    fn the_picker_switches_puzzle_and_keeps_each_puzzles_solves_apart() {
        let mut app = with_solves(&[1_000, 2_000]);
        assert_eq!(app.puzzle(), Puzzle::Cube3);
        key(&mut app, KeyCode::Char('e'));
        assert_eq!(app.picker, Some(1)); // opens on the current puzzle
        key(&mut app, KeyCode::Char('4'));
        assert_eq!(app.picker, None);
        assert_eq!(app.puzzle(), Puzzle::Cube4);
        assert!(app.solves().is_empty(), "a new 4x4 session");
        app.data
            .solves_mut()
            .push(Solve::new(50_000, Penalty::Ok, String::new()));
        // back to 3x3: its own session, untouched
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('3'));
        assert_eq!(app.puzzle(), Puzzle::Cube3);
        assert_eq!(app.solves().len(), 2);
        // and 4x4 again finds its session instead of making another
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('4'));
        assert_eq!(app.solves().len(), 1);
        assert_eq!(app.data.sessions.len(), 2);
    }

    #[test]
    fn the_picker_can_be_driven_by_arrows_and_cancelled() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Up); // 3x3 -> 2x2
        key(&mut app, KeyCode::Up); // stays at the top
        assert_eq!(app.picker, Some(0));
        key(&mut app, KeyCode::Enter);
        assert_eq!(app.puzzle(), Puzzle::Cube2);
        key(&mut app, KeyCode::Char('e'));
        for _ in 0..10 {
            key(&mut app, KeyCode::Down);
        }
        assert_eq!(app.picker, Some(puzzle::ALL.len() - 1)); // stops at 7x7
        key(&mut app, KeyCode::Esc);
        assert_eq!((app.picker, app.puzzle()), (None, Puzzle::Cube2));
    }

    #[test]
    fn picker_keys_do_not_leak_to_the_timer() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, SPACE); // picks the highlighted 3x3, doesn't start inspection
        assert_eq!(app.state, State::Idle);
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('q'));
        assert!(!app.quit);
    }

    #[test]
    fn picking_the_current_puzzle_changes_nothing() {
        let mut app = with_solves(&[1_000]);
        settle(&mut app);
        let before = app.scramble.clone();
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('3'));
        assert_eq!(app.scramble, before);
        assert_eq!(app.data.sessions.len(), 1);
    }

    #[test]
    fn a_new_puzzle_gets_its_own_kind_of_scramble() {
        use crate::nxn;
        let mut app = App::new(true);
        for (digit, size) in [('2', 2), ('5', 5), ('7', 7), ('3', 3)] {
            key(&mut app, KeyCode::Char('e'));
            key(&mut app, KeyCode::Char(digit));
            settle(&mut app);
            let moves = nxn::parse(&app.scramble).unwrap();
            match size {
                2 => assert!(moves.len() <= 11),
                3 => assert!(moves.len() <= 21),
                n => assert_eq!(moves.len(), nxn::scramble_length(n)),
            }
        }
    }

    #[test]
    fn switching_puzzle_clears_the_old_scramble_until_the_new_one_arrives() {
        let mut app = App::new(true);
        settle(&mut app);
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('6'));
        // never the old 3x3 scramble on a 6x6
        assert!(
            app.scramble.is_empty() || app.scramble.contains('w'),
            "{}",
            app.scramble
        );
    }

    #[test]
    fn solves_keep_the_scramble_of_their_own_puzzle() {
        let t = Instant::now();
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('5'));
        settle(&mut app);
        let scramble = app.scramble.clone();
        solve_at(&mut app, t, 5_000);
        assert_eq!(app.solves()[0].scramble, scramble);
        assert_eq!(app.data.sessions[app.data.current].puzzle, "555");
    }

    #[test]
    fn sessions_navigate_and_start_within_the_current_puzzle() {
        let mut app = with_solves(&[1_000]); // 3x3 session 0
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('2')); // 2x2 session 1
        app.data
            .solves_mut()
            .push(Solve::new(3_000, Penalty::Ok, String::new()));
        key(&mut app, KeyCode::Char('n')); // 2x2 session 2
        assert_eq!(app.data.sessions[app.data.current].puzzle, "222");
        app.data
            .solves_mut()
            .push(Solve::new(4_000, Penalty::Ok, String::new()));
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('3'));
        key(&mut app, KeyCode::Char('n')); // 3x3 session, appended after the 2x2 ones
        app.data
            .solves_mut()
            .push(Solve::new(2_000, Penalty::Ok, String::new()));
        assert_eq!(app.data.sessions.len(), 4);
        // [ goes to the other 3x3 session, skipping both 2x2 ones
        key(&mut app, KeyCode::Char('['));
        assert_eq!(app.solves()[0].ms, 1_000);
        key(&mut app, KeyCode::Char('['));
        assert_eq!(app.solves()[0].ms, 1_000, "no earlier 3x3 session");
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(app.solves()[0].ms, 2_000);
        key(&mut app, KeyCode::Char(']'));
        assert_eq!(app.solves()[0].ms, 2_000, "no later 3x3 session");
    }

    #[test]
    fn deleting_a_session_never_leaves_its_puzzle() {
        let mut app = with_solves(&[1_000]);
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('4'));
        app.data
            .solves_mut()
            .push(Solve::new(9_000, Penalty::Ok, String::new()));
        key(&mut app, KeyCode::Char('X'));
        key(&mut app, KeyCode::Char('y'));
        // the only 4x4 session is emptied, not removed, and we stay on 4x4
        assert_eq!(app.puzzle(), Puzzle::Cube4);
        assert!(app.solves().is_empty());
        assert_eq!(app.data.sessions.len(), 2);
        // with two 4x4 sessions, deleting one lands on the other 4x4
        app.data
            .solves_mut()
            .push(Solve::new(9_000, Penalty::Ok, String::new()));
        key(&mut app, KeyCode::Char('n'));
        app.data
            .solves_mut()
            .push(Solve::new(8_000, Penalty::Ok, String::new()));
        key(&mut app, KeyCode::Char('X'));
        key(&mut app, KeyCode::Char('y'));
        assert_eq!(app.puzzle(), Puzzle::Cube4);
        assert_eq!(app.solves()[0].ms, 9_000);
        assert_eq!(app.data.sessions.len(), 2);
    }

    #[test]
    fn the_puzzle_survives_a_restart() {
        let dir =
            std::env::temp_dir().join(format!("cubetimer-puzzle-test-{}", std::process::id()));
        let path = dir.join("sessions.json");
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = App::new(true).with_storage(path.clone(), store::load(&path));
        key(&mut app, KeyCode::Char('e'));
        key(&mut app, KeyCode::Char('5'));
        let mut app = App::new(true).with_storage(path.clone(), store::load(&path));
        assert_eq!(app.puzzle(), Puzzle::Cube5);
        settle(&mut app);
        assert_eq!(crate::nxn::parse(&app.scramble).unwrap().len(), 60);
    }

    #[test]
    fn full_preview_opens_with_v_and_any_key_closes_it() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('v'));
        assert!(app.show_preview);
        key(&mut app, SPACE);
        assert!(!app.show_preview);
        assert_eq!(app.state, State::Idle);
        // not while inspecting
        key(&mut app, SPACE);
        key(&mut app, KeyCode::Char('v'));
        assert!(!app.show_preview);
    }

    #[test]
    fn s_opens_the_statistics_screen_and_keys_change_the_range() {
        let mut app = with_solves(&[10_000, 11_000, 12_000]);
        key(&mut app, KeyCode::Char('s'));
        assert_eq!(app.stats.as_ref().unwrap().range, Range::Last100);
        key(&mut app, KeyCode::Char('r'));
        assert_eq!(app.stats.as_ref().unwrap().range, Range::Last500);
        key(&mut app, KeyCode::Char('r'));
        key(&mut app, KeyCode::Char('r'));
        assert_eq!(
            app.stats.as_ref().unwrap().range,
            Range::Last50,
            "wraps around"
        );
        for (digit, range) in [
            ('1', Range::Last50),
            ('2', Range::Last100),
            ('3', Range::Last500),
            ('4', Range::All),
        ] {
            key(&mut app, KeyCode::Char(digit));
            assert_eq!(app.stats.as_ref().unwrap().range, range);
        }
        key(&mut app, KeyCode::Esc);
        assert!(app.stats.is_none());
        key(&mut app, KeyCode::Char('s'));
        key(&mut app, KeyCode::Char('s'));
        assert!(app.stats.is_none(), "s closes it too");
        key(&mut app, KeyCode::Char('s'));
        key(&mut app, KeyCode::Char('q'));
        assert!(
            app.stats.is_none() && !app.quit,
            "q closes the screen, not the app"
        );
    }

    #[test]
    fn the_statistics_screen_is_a_snapshot_with_the_right_numbers() {
        let mut app = with_solves(&[2_000, 4_000, 6_000, 8_000, 10_000]);
        app.data.solves_mut()[1].penalty = Penalty::Plus2;
        app.data.solves_mut()[2].penalty = Penalty::Dnf;
        key(&mut app, KeyCode::Char('s'));
        let view = app.stats.as_ref().unwrap();
        assert_eq!((view.plus2, view.dnf), (1, 1));
        assert_eq!(view.report.count, 5);
        assert_eq!(view.report.finished, 4);
    }

    #[test]
    fn statistics_keys_do_not_leak_to_the_timer() {
        let mut app = with_solves(&[1_000]);
        key(&mut app, KeyCode::Char('s'));
        key(&mut app, SPACE);
        key(&mut app, KeyCode::Char('d'));
        key(&mut app, KeyCode::Char('h'));
        assert_eq!(app.state, State::Idle);
        assert_eq!(app.solves()[0].penalty, Penalty::Ok);
        assert!(app.history.is_none());
        assert!(app.stats.is_some());
    }

    #[test]
    fn statistics_open_only_when_idle_and_ctrl_c_still_quits() {
        let mut app = with_solves(&[1_000]);
        key(&mut app, SPACE); // inspecting
        key(&mut app, KeyCode::Char('s'));
        assert!(app.stats.is_none());
        let ctrl_c = KeyEvent::new_with_kind(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL,
            KeyEventKind::Press,
        );
        let mut app = with_solves(&[1_000]);
        key(&mut app, KeyCode::Char('s'));
        app.on_key(ctrl_c, Instant::now());
        assert!(app.quit);
    }

    #[test]
    fn statistics_work_for_an_empty_session() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Char('s'));
        let view = app.stats.as_ref().unwrap();
        assert_eq!(view.report.count, 0);
    }
}

/// Helpers for tests in other modules.
#[cfg(test)]
pub mod tests_support {
    use super::*;

    /// Switch to the cube of the given size digit, as the picker would.
    pub fn set_puzzle(app: &mut App, digit: char) {
        let size = digit.to_digit(10).unwrap() as usize;
        let p = puzzle::ALL
            .iter()
            .copied()
            .find(|p| p.size() == size)
            .unwrap();
        app.select_puzzle(p);
    }
}
