//! WCA-style scrambles, made the way the official TNoodle program makes them.
//!
//! 3x3x3: pick a cube state uniformly at random from all states that need at
//! least 2 moves to solve (Regulation 4b3), find a solution of at most 21 moves
//! with the two-phase algorithm, and give its inverse as the scramble.
//! 2x2x2: a uniformly random state needing at least 4 moves (see `twobytwo`).
//! 4x4x4 to 7x7x7: 40, 60, 80 and 100 random moves (see `nxn`).

use std::sync::mpsc::{Receiver, TryRecvError, sync_channel};
use std::thread;

use rand::Rng;

use crate::cube::{self, Cube, Move, SOLVED};
use crate::nxn;
use crate::puzzle::Puzzle;
use crate::twobytwo;
use crate::twophase;

/// TNoodle's limit for 3x3x3 scrambles.
pub const MAX_LENGTH: usize = 21;

/// Regulation 4b3: the state must need at least 2 moves to solve, so it can't
/// be solved or one move away.
fn far_enough(state: &Cube) -> bool {
    *state != SOLVED && Move::all().all(|m| SOLVED.apply(m) != *state)
}

/// The moves that take a solved cube to `state`: the inverse of a solution.
fn scramble_for(state: &Cube) -> Vec<Move> {
    // 21 moves always exist (any state has a 20-move solution), so this can't fail.
    let solution = twophase::solve(state, MAX_LENGTH).expect("every state has a 21-move solution");
    solution.iter().rev().map(|m| m.inverse()).collect()
}

/// A scramble for `puzzle`.
pub fn generate_for(puzzle: Puzzle, rng: &mut impl Rng) -> String {
    match puzzle {
        Puzzle::Cube2 => twobytwo::generate_with(rng),
        Puzzle::Cube3 => generate_with(rng),
        big => {
            let size = big.size();
            nxn::format(&nxn::random_moves(size, nxn::scramble_length(size), rng))
        }
    }
}

pub fn generate_with(rng: &mut impl Rng) -> String {
    loop {
        let state = Cube::random(rng);
        if far_enough(&state) {
            return cube::format(&scramble_for(&state));
        }
    }
}

/// Makes scrambles on a background thread, one ahead, so a new scramble is
/// ready the moment a solve ends even when the search takes a while.
pub struct Scrambler {
    puzzle: Puzzle,
    next: Receiver<String>,
}

impl Scrambler {
    pub fn new(puzzle: Puzzle) -> Self {
        let (tx, rx) = sync_channel(1);
        thread::spawn(move || {
            let mut rng = rand::rng();
            // Ends when the Scrambler is dropped and the send fails.
            while tx.send(generate_for(puzzle, &mut rng)).is_ok() {}
        });
        Self { puzzle, next: rx }
    }

    #[cfg(test)]
    pub fn puzzle(&self) -> Puzzle {
        self.puzzle
    }

    /// The next scramble; waits only if the first one isn't ready yet.
    #[cfg(test)]
    pub fn next(&self) -> String {
        self.next
            .recv()
            .unwrap_or_else(|_| generate_for(self.puzzle, &mut rand::rng()))
    }

    /// The next scramble if it is ready, without waiting. If the worker thread
    /// is gone (it can only panic), make one here instead.
    pub fn try_next(&self) -> Option<String> {
        match self.next.try_recv() {
            Ok(s) => Some(s),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(generate_for(self.puzzle, &mut rand::rng())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::parse;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn a_scramble_produces_exactly_its_state() {
        let mut rng = StdRng::seed_from_u64(11);
        for _ in 0..40 {
            let state = Cube::random(&mut rng);
            let moves = scramble_for(&state);
            assert_eq!(SOLVED.apply_all(&moves), state);
            // and survives the round trip through text
            assert_eq!(
                SOLVED.apply_all(&parse(&cube::format(&moves)).unwrap()),
                state
            );
        }
    }

    #[test]
    fn scrambles_are_short_and_in_wca_notation() {
        let mut rng = StdRng::seed_from_u64(12);
        for _ in 0..40 {
            let s = generate_with(&mut rng);
            let moves: Vec<&str> = s.split(' ').collect();
            assert!(moves.len() <= MAX_LENGTH, "{s}");
            assert!(moves.len() >= 2, "{s}");
            for m in &moves {
                assert!(matches!(m.len(), 1 | 2), "{m}");
                assert!("URFDLB".contains(&m[..1]), "{m}");
                assert!(m.len() == 1 || m.ends_with('\'') || m.ends_with('2'), "{m}");
            }
        }
    }

    #[test]
    fn no_redundant_moves() {
        // never the same face twice in a row, never three on one axis (R L R)
        let mut rng = StdRng::seed_from_u64(13);
        for _ in 0..40 {
            let moves = parse(&generate_with(&mut rng)).unwrap();
            for w in moves.windows(2) {
                assert_ne!(w[0].face, w[1].face);
            }
            for w in moves.windows(3) {
                assert!(!(w[0].face % 3 == w[1].face % 3 && w[1].face % 3 == w[2].face % 3));
            }
        }
    }

    #[test]
    fn states_closer_than_two_moves_are_rejected() {
        assert!(!far_enough(&SOLVED));
        for m in Move::all() {
            assert!(!far_enough(&SOLVED.apply(m)), "{m}");
        }
        assert!(far_enough(&SOLVED.apply_all(&parse("R U").unwrap())));
        assert!(far_enough(&SOLVED.apply_all(&parse("R L").unwrap())));
    }

    #[test]
    fn scrambles_differ() {
        let mut rng = StdRng::seed_from_u64(43);
        assert_ne!(generate_with(&mut rng), generate_with(&mut rng));
    }

    #[test]
    fn try_next_never_blocks_and_eventually_delivers() {
        let s = Scrambler::new(Puzzle::Cube3);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let got = loop {
            if let Some(x) = s.try_next() {
                break x;
            }
            assert!(std::time::Instant::now() < deadline, "no scramble in 10 s");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(parse(&got).is_some());
    }

    #[test]
    fn scrambler_hands_out_fresh_valid_scrambles() {
        let s = Scrambler::new(Puzzle::Cube3);
        let a = s.next();
        let b = s.next();
        assert_ne!(a, b);
        for x in [a, b] {
            let moves = parse(&x).unwrap();
            assert!(!moves.is_empty() && moves.len() <= MAX_LENGTH);
        }
    }

    #[test]
    fn every_puzzle_gets_a_scramble_of_the_right_kind() {
        let mut rng = StdRng::seed_from_u64(41);
        for p in crate::puzzle::ALL {
            for _ in 0..20 {
                let text = generate_for(p, &mut rng);
                let moves = nxn::parse(&text).unwrap_or_else(|| panic!("{p:?}: {text}"));
                match p {
                    Puzzle::Cube2 => assert!((4..=11).contains(&moves.len()), "{text}"),
                    Puzzle::Cube3 => assert!((2..=MAX_LENGTH).contains(&moves.len()), "{text}"),
                    big => assert_eq!(moves.len(), nxn::scramble_length(big.size()), "{text}"),
                }
                // wide turns appear only where the cube has them
                let deepest = moves.iter().map(|m| m.layers).max().unwrap() as usize;
                assert!(deepest <= (p.size() / 2).max(1), "{p:?} {text}");
            }
        }
    }

    #[test]
    fn a_scrambler_makes_scrambles_for_its_puzzle() {
        for p in [Puzzle::Cube2, Puzzle::Cube5] {
            let s = Scrambler::new(p);
            assert_eq!(s.puzzle(), p);
            let text = s.next();
            let moves = nxn::parse(&text).unwrap();
            if p == Puzzle::Cube5 {
                assert_eq!(moves.len(), 60);
            } else {
                assert!(moves.len() <= 11);
            }
        }
    }
}
