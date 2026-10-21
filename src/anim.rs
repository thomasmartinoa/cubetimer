//! Playing a move sequence on a cube, one animated turn at a time.
//!
//! The player only keeps time; it never sleeps. Ask it what to show at an
//! instant (`frame`) and it answers with the cube so far and the turn in
//! progress. Handlers take `now` so it can be tested without waiting.

use std::time::{Duration, Instant};

use crate::nxn::{Move, Nxn};
use crate::render3d::Turning;

/// Quarter-turn speeds to choose from, slowest first.
pub const SPEEDS_MS: [u64; 6] = [800, 500, 320, 200, 120, 70];
const DEFAULT_SPEED: usize = 3;

/// One step of a sequence as a cuber names it: a face turn, a wide turn, a
/// rotation, or a slice (which is two layer turns that animate as one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub label: String,
    /// The layer turns the step is made of: one, or two for a slice (the
    /// wide turn, then the outer layer turned back).
    pub moves: Vec<Move>,
}

impl Step {
    /// One plain layer turn, labelled the way `nxn` writes it.
    pub fn turn(m: Move) -> Step {
        Step {
            label: m.to_string(),
            moves: vec![m],
        }
    }

    /// How the step looks while it turns: the turning layers, and for a
    /// slice the outer layer that stays still.
    fn turning(&self, fraction: f64) -> Turning {
        let except = match self.moves.as_slice() {
            [wide, outer] if outer.face == wide.face && outer.layers < wide.layers => Some(*outer),
            _ => None,
        };
        Turning {
            m: self.moves[0],
            fraction,
            except,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Player {
    /// The cube before the first step.
    start: Nxn,
    steps: Vec<Step>,
    /// Steps fully done so far.
    pos: usize,
    /// When the step in progress started, while playing.
    turn_started: Option<Instant>,
    speed: usize,
}

impl Player {
    /// A player for `moves` on a solved cube of `size`, showing the end
    /// state (how the timer shows a scramble).
    pub fn new(size: usize, moves: Vec<Move>) -> Player {
        let steps: Vec<Step> = moves.into_iter().map(Step::turn).collect();
        Player::from_steps(Nxn::solved(size), steps, true)
    }

    /// A player for `steps` from `start`, showing the end state if `at_end`,
    /// otherwise the start (how an algorithm is shown on its case).
    pub fn from_steps(start: Nxn, steps: Vec<Step>, at_end: bool) -> Player {
        let pos = if at_end { steps.len() } else { 0 };
        Player {
            start,
            steps,
            pos,
            turn_started: None,
            speed: DEFAULT_SPEED,
        }
    }

    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// Steps done so far (the next one is `steps()[pos()]`).
    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn is_playing(&self) -> bool {
        self.turn_started.is_some()
    }

    pub fn speed_ms(&self) -> u64 {
        SPEEDS_MS[self.speed]
    }

    fn turn_time(&self, step: &Step) -> Duration {
        // a half turn takes longer than a quarter, but not twice as long
        let base = self.speed_ms() as f64;
        let k = if step.moves[0].turns == 2 { 1.5 } else { 1.0 };
        Duration::from_millis((base * k) as u64)
    }

    /// Play from where it is; from the start if it is at the end.
    pub fn play(&mut self, now: Instant) {
        if self.steps.is_empty() {
            return;
        }
        if self.pos == self.steps.len() {
            self.pos = 0;
        }
        self.turn_started = Some(now);
    }

    pub fn pause(&mut self) {
        self.turn_started = None;
    }

    pub fn toggle(&mut self, now: Instant) {
        if self.is_playing() {
            self.pause();
        } else {
            self.play(now);
        }
    }

    /// Step one move forward or back (pausing).
    pub fn step(&mut self, forward: bool) {
        self.pause();
        if forward {
            self.pos = (self.pos + 1).min(self.steps.len());
        } else {
            self.pos = self.pos.saturating_sub(1);
        }
    }

    pub fn rewind(&mut self) {
        self.pause();
        self.pos = 0;
    }

    pub fn fast_forward(&mut self) {
        self.pause();
        self.pos = self.steps.len();
    }

    pub fn faster(&mut self) {
        self.speed = (self.speed + 1).min(SPEEDS_MS.len() - 1);
    }

    pub fn slower(&mut self) {
        self.speed = self.speed.saturating_sub(1);
    }

    /// Advance the clock to `now`: finish every step whose time is up.
    pub fn advance(&mut self, now: Instant) {
        while let Some(started) = self.turn_started {
            let Some(step) = self.steps.get(self.pos) else {
                self.turn_started = None;
                break;
            };
            let done = started + self.turn_time(step);
            if now < done {
                break;
            }
            self.pos += 1;
            self.turn_started = (self.pos < self.steps.len()).then_some(done);
        }
    }

    /// The cube with the first `pos` steps done, and the step in progress.
    pub fn frame(&self, now: Instant) -> (Nxn, Option<Turning>) {
        let mut cube = self.start.clone();
        for step in &self.steps[..self.pos] {
            cube.apply_all(&step.moves);
        }
        let turning = self.turn_started.and_then(|started| {
            let step = self.steps.get(self.pos)?;
            let elapsed = now.saturating_duration_since(started).as_secs_f64();
            let fraction = (elapsed / self.turn_time(step).as_secs_f64()).clamp(0.0, 1.0);
            // ease in and out, like a hand turning a layer
            let eased = fraction * fraction * (3.0 - 2.0 * fraction);
            Some(step.turning(eased))
        });
        (cube, turning)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nxn;

    fn player(seq: &str) -> Player {
        Player::new(3, nxn::parse(seq).unwrap())
    }

    fn ms(t: Instant, m: u64) -> Instant {
        t + Duration::from_millis(m)
    }

    fn state_after(seq: &str) -> Nxn {
        let mut c = Nxn::solved(3);
        c.apply_all(&nxn::parse(seq).unwrap());
        c
    }

    #[test]
    fn starts_showing_the_finished_scramble() {
        let p = player("R U F");
        assert_eq!(p.pos(), 3);
        assert!(!p.is_playing());
        let (cube, turning) = p.frame(Instant::now());
        assert_eq!(cube, state_after("R U F"));
        assert!(turning.is_none());
    }

    #[test]
    fn play_from_the_end_restarts_from_solved() {
        let t = Instant::now();
        let mut p = player("R U F");
        p.play(t);
        assert_eq!(p.pos(), 0);
        let (cube, turning) = p.frame(t);
        assert!(cube.is_solved());
        assert_eq!(turning.unwrap().fraction, 0.0);
    }

    #[test]
    fn turns_finish_one_after_another_in_time() {
        let t = Instant::now();
        let mut p = player("R U F");
        let step = p.speed_ms();
        p.play(t);
        p.advance(ms(t, step / 2));
        assert_eq!(p.pos(), 0);
        let (_, turning) = p.frame(ms(t, step / 2));
        assert!(
            (turning.unwrap().fraction - 0.5).abs() < 1e-9,
            "eased midpoint is 0.5"
        );
        p.advance(ms(t, step));
        assert_eq!(p.pos(), 1);
        p.advance(ms(t, 3 * step - 1));
        assert_eq!(p.pos(), 2);
        p.advance(ms(t, 3 * step));
        assert_eq!(p.pos(), 3);
        assert!(!p.is_playing(), "stops at the end");
        assert_eq!(p.frame(ms(t, 99_999)).0, state_after("R U F"));
    }

    #[test]
    fn a_long_gap_catches_up_without_skipping_moves() {
        let t = Instant::now();
        let mut p = player("R U F R' U'");
        p.play(t);
        p.advance(ms(t, 60_000));
        assert_eq!(p.pos(), 5);
        assert_eq!(p.frame(ms(t, 60_000)).0, state_after("R U F R' U'"));
    }

    #[test]
    fn half_turns_take_longer() {
        let t = Instant::now();
        let mut p = player("R2 U");
        let step = p.speed_ms();
        p.play(t);
        p.advance(ms(t, step));
        assert_eq!(p.pos(), 0, "R2 is still turning");
        p.advance(ms(t, step * 3 / 2));
        assert_eq!(p.pos(), 1);
    }

    #[test]
    fn pause_step_and_jump() {
        let t = Instant::now();
        let mut p = player("R U F");
        p.rewind();
        assert!(p.frame(t).0.is_solved());
        p.step(true);
        p.step(true);
        assert_eq!(p.frame(t).0, state_after("R U"));
        p.step(false);
        assert_eq!(p.pos(), 1);
        p.step(false);
        p.step(false);
        assert_eq!(p.pos(), 0, "can't go before the start");
        p.fast_forward();
        p.step(true);
        assert_eq!(p.pos(), 3, "or past the end");
        p.play(t);
        p.toggle(t);
        assert!(!p.is_playing());
        p.toggle(t);
        assert!(p.is_playing());
        p.step(true);
        assert!(!p.is_playing(), "stepping pauses");
    }

    #[test]
    fn playing_resumes_mid_sequence() {
        let t = Instant::now();
        let mut p = player("R U F");
        p.rewind();
        p.step(true);
        p.play(t);
        assert_eq!(p.pos(), 1, "resumes rather than restarting");
    }

    #[test]
    fn speeds_are_bounded() {
        let mut p = player("R");
        for _ in 0..10 {
            p.faster();
        }
        assert_eq!(p.speed_ms(), *SPEEDS_MS.last().unwrap());
        for _ in 0..10 {
            p.slower();
        }
        assert_eq!(p.speed_ms(), SPEEDS_MS[0]);
        assert!(SPEEDS_MS.windows(2).all(|w| w[0] > w[1]));
    }

    #[test]
    fn an_empty_sequence_never_plays() {
        let t = Instant::now();
        let mut p = Player::new(3, vec![]);
        p.play(t);
        assert!(!p.is_playing());
        p.advance(ms(t, 1000));
        assert!(p.frame(t).0.is_solved());
    }

    #[test]
    fn the_turn_in_progress_is_the_next_move() {
        let t = Instant::now();
        let mut p = player("R U");
        p.play(t);
        let step = p.speed_ms();
        p.advance(ms(t, step + 10));
        let (_, turning) = p.frame(ms(t, step + 10));
        assert_eq!(turning.unwrap().m, nxn::parse("U").unwrap()[0]);
    }

    #[test]
    fn a_player_can_start_from_any_cube_at_the_start() {
        let mut start = Nxn::solved(3);
        start.apply_all(&nxn::parse("R U").unwrap());
        let steps = vec![
            Step::turn(nxn::parse("U'").unwrap()[0]),
            Step::turn(nxn::parse("R'").unwrap()[0]),
        ];
        let mut p = Player::from_steps(start.clone(), steps, false);
        let t = Instant::now();
        assert_eq!(p.pos(), 0);
        assert_eq!(p.frame(t).0, start, "shows the case first");
        p.fast_forward();
        assert!(p.frame(t).0.is_solved(), "and the steps solve it");
    }

    #[test]
    fn a_slice_step_turns_only_the_middle_layer() {
        let wide = nxn::Move {
            face: 4,
            layers: 2,
            turns: 1,
        };
        let outer = nxn::Move {
            face: 4,
            layers: 1,
            turns: 3,
        };
        let step = Step {
            label: "M".into(),
            moves: vec![wide, outer],
        };
        let t = step.turning(0.5);
        assert_eq!((t.m, t.except), (wide, Some(outer)));
        assert_eq!(Step::turn(wide).turning(0.5).except, None);
        // and doing the step applies both turns
        let mut p = Player::from_steps(Nxn::solved(3), vec![step], false);
        p.fast_forward();
        let mut want = Nxn::solved(3);
        want.apply_all(&[wide, outer]);
        assert_eq!(p.frame(Instant::now()).0, want);
    }
}
