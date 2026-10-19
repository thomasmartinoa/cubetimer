//! App state and the timer state machine.
//!
//! Handlers take `now: Instant` so the state machine can be tested without
//! sleeping.

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::DefaultTerminal;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::fmt::{self, Precision, RunDisplay};
use crate::stats::{self, Penalty, Summary, TimeResult};
use crate::scramble;
use crate::store::{self, Data, Loaded, Session, Solve};
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
    Inspection { start: Instant },
    /// Space is held down; `inspection` is the start of the running inspection.
    Holding { since: Instant, inspection: Option<Instant> },
    Running { start: Instant, penalty: Penalty },
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

pub struct App {
    pub state: State,
    pub data: Data,
    /// Where sessions are saved; `None` keeps everything in memory (tests).
    path: Option<PathBuf>,
    /// A one-line message for the footer (save errors and the like).
    pub status: Option<String>,
    /// Waiting for y/n after Backspace.
    pub confirm_delete: bool,
    /// When the last solve set a new best single; drives the flash.
    pb_flash: Option<Instant>,
    pub inspection_on: bool,
    /// True when the terminal reports key release events (kitty protocol).
    pub enhanced: bool,
    pub run_display: RunDisplay,
    pub precision: Precision,
    pub bell: bool,
    pub scramble: String,
    pub quit: bool,
    /// Inspection warnings already announced: 0 none, 1 at 8 s, 2 at 12 s.
    warned: u8,
}

impl App {
    pub fn new(enhanced: bool) -> Self {
        Self {
            state: State::Idle,
            data: Data::default(),
            path: None,
            status: None,
            confirm_delete: false,
            pb_flash: None,
            inspection_on: true,
            enhanced,
            run_display: RunDisplay::default(),
            precision: Precision::default(),
            bell: false,
            scramble: scramble::generate(),
            quit: false,
            warned: 0,
        }
    }

    /// Use `path` for saving and start from what was loaded.
    pub fn with_storage(mut self, path: PathBuf, loaded: Loaded) -> Self {
        self.path = Some(path);
        self.data = loaded.data;
        self.status = loaded.warning;
        self
    }

    pub fn solves(&self) -> &[Solve] {
        self.data.solves()
    }

    fn persist(&mut self) {
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
            State::Idle if self.pb_flash.is_some() => Some(Duration::from_millis(50)),
            State::Idle => None,
            State::Inspection { .. } => Some(Duration::from_millis(50)),
            State::Holding { .. } | State::Running { .. } => Some(Duration::from_millis(16)),
        }
    }

    pub fn on_event(&mut self, ev: Event, now: Instant) {
        if let Event::Key(key) = ev {
            self.on_key(key, now);
        }
    }

    pub fn on_key(&mut self, key: KeyEvent, now: Instant) {
        if key.kind == KeyEventKind::Repeat {
            return;
        }
        let ctrl_c = key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL);
        if key.kind == KeyEventKind::Press && ctrl_c {
            self.quit = true;
            return;
        }
        match key.kind {
            KeyEventKind::Release => {
                if key.code == KeyCode::Char(' ') {
                    self.space_release(now);
                }
            }
            KeyEventKind::Press => {
                if matches!(self.state, State::Running { .. }) {
                    self.stop(now);
                    return;
                }
                self.status = None;
                self.pb_flash = None;
                if std::mem::take(&mut self.confirm_delete) {
                    if key.code == KeyCode::Char('y') {
                        self.delete_last();
                    }
                    return;
                }
                let idle = self.state == State::Idle;
                match key.code {
                    KeyCode::Char(' ') => self.space_press(now),
                    KeyCode::Esc => self.cancel(),
                    KeyCode::Char('q') => self.quit = true,
                    KeyCode::Char('i') if idle => self.inspection_on = !self.inspection_on,
                    KeyCode::Char('+' | '=') if idle => self.mark_last(Penalty::Plus2),
                    KeyCode::Char('d') if idle => self.mark_last(Penalty::Dnf),
                    KeyCode::Char('o') if idle => self.mark_last(Penalty::Ok),
                    KeyCode::Backspace if idle && !self.solves().is_empty() => {
                        self.confirm_delete = true;
                    }
                    KeyCode::Char('n') if idle => self.new_session(),
                    KeyCode::Char('[') if idle => self.switch_session(-1),
                    KeyCode::Char(']') if idle => self.switch_session(1),
                    KeyCode::Char('p') => self.run_display = self.run_display.next(),
                    KeyCode::Char('b') => self.bell = !self.bell,
                    _ => {}
                }
            }
            KeyEventKind::Repeat => {}
        }
    }

    fn space_press(&mut self, now: Instant) {
        self.state = match self.state {
            State::Idle if self.inspection_on => {
                self.warned = 0;
                State::Inspection { start: now }
            }
            State::Idle if self.enhanced => State::Holding { since: now, inspection: None },
            State::Idle => State::Running { start: now, penalty: Penalty::Ok },
            State::Inspection { start } if self.enhanced => {
                State::Holding { since: now, inspection: Some(start) }
            }
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
        State::Running { start: now, penalty }
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
            self.scramble = scramble::generate();
            self.state = State::Idle;
            self.persist();
        }
    }

    fn mark_last(&mut self, penalty: Penalty) {
        if let Some(last) = self.data.solves_mut().last_mut() {
            last.penalty = penalty;
            self.persist();
        }
    }

    fn delete_last(&mut self) {
        if self.data.solves_mut().pop().is_some() {
            self.persist();
        }
    }

    /// Start a new session, unless the current one is still empty.
    fn new_session(&mut self) {
        if self.solves().is_empty() {
            return;
        }
        self.data.sessions.push(Session::default());
        self.data.current = self.data.sessions.len() - 1;
        self.persist();
    }

    fn switch_session(&mut self, step: isize) {
        let next = self.data.current.checked_add_signed(step);
        if let Some(i) = next.filter(|&i| i < self.data.sessions.len()) {
            self.data.current = i;
            self.persist();
        }
    }

    /// Periodic work: the inspection warning bell.
    pub fn tick(&mut self, now: Instant) {
        if self.pb_flash.is_some_and(|t| now.duration_since(t) >= PB_FLASH) {
            self.pb_flash = None;
        }
        let Some(start) = self.inspection_start() else { return };
        let ms = now.duration_since(start).as_millis() as u64;
        let level = if ms >= WARN_12S { 2 } else if ms >= WARN_8S { 1 } else { 0 };
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
            State::Idle if self.confirm_delete => View {
                text: self.last_text(),
                tone: Tone::Warn12,
                hint: "delete last solve? y / n".into(),
            },
            State::Idle if self.pb_flash.is_some_and(|t| now.duration_since(t) < PB_FLASH) => {
                let elapsed = now.duration_since(self.pb_flash.unwrap_or(now)).as_millis();
                let tone = if (elapsed / PB_BLINK_MS).is_multiple_of(2) { Tone::Pb } else { Tone::PbAlt };
                View { text: self.last_text(), tone, hint: "new personal best!".into() }
            }
            State::Idle => {
                let text = self.last_text();
                let hint = match (self.enhanced, self.inspection_on) {
                    (_, true) => "space: start inspection",
                    (true, false) => "hold space, release to start",
                    (false, false) => "space: start",
                };
                View { text, tone: Tone::Idle, hint: hint.into() }
            }
            State::Inspection { start } => {
                let ms = now.duration_since(start).as_millis() as u64;
                let (tone, hint) = match ms {
                    WARN_12S.. => (Tone::Warn12, "12s!"),
                    WARN_8S.. => (Tone::Warn8, "8s!"),
                    _ if self.enhanced => (Tone::Inspect, "hold space when ready"),
                    _ => (Tone::Inspect, "space: start"),
                };
                View { text: fmt::fmt_inspection(ms), tone, hint: hint.into() }
            }
            State::Holding { since, inspection } => {
                let ready = now.duration_since(since) >= HOLD_READY;
                let text = match inspection {
                    Some(start) => fmt::fmt_inspection(now.duration_since(start).as_millis() as u64),
                    None => fmt::fmt_time(0, prec),
                };
                let (tone, hint) = if ready {
                    (Tone::HoldReady, "ready - release to start")
                } else {
                    (Tone::HoldWait, "hold...")
                };
                View { text, tone, hint: hint.into() }
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
        assert!(matches!(app.state, State::Running { penalty: Penalty::Ok, .. }));
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
        assert!(matches!(app.state, State::Holding { inspection: None, .. }));
        app.on_key(release(SPACE), at(t, 299));
        assert_eq!(app.state, State::Idle);
        app.on_key(press(SPACE), at(t, 1_000));
        app.on_key(release(SPACE), at(t, 1_300));
        assert!(matches!(app.state, State::Running { penalty: Penalty::Ok, .. }));
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
        assert!(matches!(app.state, State::Running { penalty: Penalty::Plus2, .. }));
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
        assert!(!app.quit);
        assert_eq!(app.solves().len(), 1);
        app.on_key(press(KeyCode::Char('q')), at(t, 5_100));
        assert!(app.quit);
    }

    #[test]
    fn ctrl_c_quits_even_while_running() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.state = State::Running { start: t, penalty: Penalty::Ok };
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
        let t = Instant::now();
        let mut app = App::new(true);
        app.on_key(press(KeyCode::Char('i')), t);
        assert!(!app.inspection_on);
        app.on_key(press(KeyCode::Char('i')), t);
        app.on_key(press(SPACE), t);
        app.on_key(press(KeyCode::Char('i')), t);
        assert!(app.inspection_on);
    }

    #[test]
    fn views_follow_the_state() {
        let t = Instant::now();
        let mut app = App::new(true);
        assert_eq!(app.view(t).text, "0.00");
        app.on_key(press(SPACE), at(t, 0));
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
        app.data.solves_mut().push(Solve::new(12_340, Penalty::Plus2, String::new()));
        assert_eq!(app.view(t).text, "14.34+");
    }

    fn key(app: &mut App, code: KeyCode) {
        app.on_key(press(code), Instant::now());
    }

    fn with_solves(times: &[u64]) -> App {
        let mut app = App::new(true);
        for &ms in times {
            app.data.solves_mut().push(Solve::new(ms, Penalty::Ok, String::new()));
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
        assert!(app.confirm_delete);
        assert_eq!(app.solves().len(), 2);
        assert_eq!(app.view(Instant::now()).hint, "delete last solve? y / n");
        key(&mut app, KeyCode::Char('y'));
        assert_eq!(app.solves().len(), 1);
        assert_eq!(app.solves()[0].ms, 1_000);
        assert!(!app.confirm_delete);
    }

    #[test]
    fn any_other_key_cancels_the_delete() {
        for cancel in [KeyCode::Char('n'), KeyCode::Esc, SPACE, KeyCode::Char('q')] {
            let mut app = with_solves(&[1_000]);
            key(&mut app, KeyCode::Backspace);
            key(&mut app, cancel);
            assert_eq!(app.solves().len(), 1);
            assert!(!app.confirm_delete);
            assert_eq!(app.state, State::Idle);
            assert!(!app.quit);
        }
    }

    #[test]
    fn backspace_with_no_solves_does_not_ask() {
        let mut app = App::new(true);
        key(&mut app, KeyCode::Backspace);
        assert!(!app.confirm_delete);
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

    #[test]
    fn stopping_a_solve_makes_a_new_scramble() {
        let t = Instant::now();
        let mut app = App::new(true);
        app.inspection_on = false;
        let before = app.scramble.clone();
        app.on_key(press(SPACE), t);
        app.on_key(release(SPACE), at(t, 400));
        app.on_key(press(SPACE), at(t, 5_000));
        assert_ne!(app.scramble, before);
        assert_eq!(app.solves()[0].scramble, before);
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
        assert!(app.status.as_deref().is_some_and(|s| s.starts_with("save failed")));
        key(&mut app, KeyCode::Char('o'));
        std::fs::remove_file(&file).unwrap();
    }

    fn solve_at(app: &mut App, t: Instant, ms: u64) {
        app.inspection_on = false;
        app.on_key(press(SPACE), t);
        app.on_key(release(SPACE), at(t, 400));
        app.on_key(press(SPACE), at(t, 400 + ms));
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
        app.state = State::Running { start: t, penalty: Penalty::Dnf };
        app.on_key(press(KeyCode::Char('x')), at(t, 5_000));
        assert_eq!(app.view(at(t, 5_000)).tone, Tone::Idle);
        app.state = State::Running { start: t, penalty: Penalty::Plus2 };
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
}
