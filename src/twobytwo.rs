//! WCA-style random-state scrambles for the 2x2x2 cube.
//!
//! The 2x2 has 3,674,160 states once the DBL corner is held fixed (so only the
//! faces U, R and F ever turn). A breadth-first search over all of them gives
//! the exact number of moves each needs. Regulation 4b3b asks for a uniformly
//! random state that needs at least 4 moves; the scramble is the inverse of a
//! random optimal solution, so it is as short as it can be (at most 11 moves).

use std::sync::OnceLock;

use rand::{Rng, RngExt};

use crate::cube::{self, Move, SOLVED};

/// Corner positions that can move: all but DBL (6).
const POSITIONS: [usize; 7] = [0, 1, 2, 3, 4, 5, 7];
const N_PERM: usize = 5040;
const N_TWIST: usize = 729;
pub const N_STATES: usize = N_PERM * N_TWIST;
/// Regulation 4b3b: the state must need at least this many moves.
pub const MIN_DISTANCE: u8 = 4;

/// The nine moves U U2 U' R R2 R' F F2 F' as indices into `cube::Move::all()`.
const MOVES: [usize; 9] = [0, 1, 2, 3, 4, 5, 6, 7, 8];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    /// The corner in each of the 8 positions (DBL, index 6, never moves).
    pub cp: [u8; 8],
    pub co: [u8; 8],
}

pub const START: State = State {
    cp: [0, 1, 2, 3, 4, 5, 6, 7],
    co: [0; 8],
};

/// How each move changes corner positions and twists.
fn move_tables() -> &'static [State; 9] {
    static T: OnceLock<[State; 9]> = OnceLock::new();
    T.get_or_init(|| {
        let mut out = [START; 9];
        for (slot, &m) in out.iter_mut().zip(&MOVES) {
            let c = SOLVED.apply(Move::from_index(m));
            *slot = State { cp: c.cp, co: c.co };
        }
        out
    })
}

impl State {
    pub fn apply(&self, move_index: usize) -> State {
        let m = &move_tables()[move_index];
        let mut out = *self;
        for i in 0..8 {
            let from = m.cp[i] as usize;
            out.cp[i] = self.cp[from];
            out.co[i] = (self.co[from] + m.co[i]) % 3;
        }
        out
    }

    /// 0..N_STATES, one per state.
    pub fn index(&self) -> usize {
        let pieces: Vec<u8> = POSITIONS.iter().map(|&p| self.cp[p]).collect();
        let mut perm = 0;
        for i in 0..7 {
            let smaller = pieces[i + 1..].iter().filter(|&&x| x < pieces[i]).count();
            perm = perm * (7 - i) + smaller;
        }
        // the last of the seven twists is determined by the other six
        let twist = POSITIONS[..6]
            .iter()
            .fold(0, |a, &p| a * 3 + self.co[p] as usize);
        perm * N_TWIST + twist
    }

    pub fn from_index(index: usize) -> State {
        let (mut perm, mut twist) = (index / N_TWIST, index % N_TWIST);
        // unrank the permutation of the seven movable corners
        let mut lehmer = [0usize; 7];
        for i in (0..7).rev() {
            let base = 7 - i;
            lehmer[i] = perm % base;
            perm /= base;
        }
        let mut available: Vec<u8> = POSITIONS.iter().map(|&p| p as u8).collect();
        let mut s = START;
        for (i, &pos) in POSITIONS.iter().enumerate() {
            s.cp[pos] = available.remove(lehmer[i]);
        }
        let mut sum = 0;
        for &pos in POSITIONS[..6].iter().rev() {
            s.co[pos] = (twist % 3) as u8;
            sum += s.co[pos] as usize;
            twist /= 3;
        }
        s.co[POSITIONS[6]] = ((3 - sum % 3) % 3) as u8;
        s
    }

    pub fn is_solved(&self) -> bool {
        *self == START
    }
}

/// Moves-to-solve for every state, from a breadth-first search.
fn distances() -> &'static [u8] {
    static D: OnceLock<Vec<u8>> = OnceLock::new();
    D.get_or_init(|| {
        let mut dist = vec![u8::MAX; N_STATES];
        dist[START.index()] = 0;
        let mut frontier = vec![START.index()];
        let mut depth = 0;
        while !frontier.is_empty() {
            depth += 1;
            let mut next = Vec::new();
            for &i in &frontier {
                let s = State::from_index(i);
                for m in 0..9 {
                    let j = s.apply(m).index();
                    if dist[j] == u8::MAX {
                        dist[j] = depth;
                        next.push(j);
                    }
                }
            }
            frontier = next;
        }
        dist
    })
}

/// Build the distance table now rather than on the first scramble.
#[allow(dead_code)] // used by the benchmark example
pub fn warm_up() {
    distances();
}

pub fn distance(state: &State) -> u8 {
    distances()[state.index()]
}

/// A state drawn uniformly from all 3,674,160.
pub fn random_state(rng: &mut impl Rng) -> State {
    State::from_index(rng.random_range(0..N_STATES))
}

/// A uniformly random state that needs at least `MIN_DISTANCE` moves.
pub fn random_scramble_state(rng: &mut impl Rng) -> State {
    loop {
        let s = random_state(rng);
        if distance(&s) >= MIN_DISTANCE {
            return s;
        }
    }
}

/// A scramble that produces `state` from a solved cube: the inverse of one of
/// its optimal solutions, chosen at random.
pub fn scramble_for(state: &State, rng: &mut impl Rng) -> Vec<Move> {
    let dist = distances();
    let mut solution: Vec<usize> = Vec::new();
    let mut cur = *state;
    let mut last_face = usize::MAX;
    while !cur.is_solved() {
        let d = dist[cur.index()];
        let options: Vec<usize> = (0..9)
            .filter(|&m| m / 3 != last_face && dist[cur.apply(m).index()] + 1 == d)
            .collect();
        // an optimal solution never turns the same face twice in a row
        let m = options[rng.random_range(0..options.len())];
        solution.push(m);
        last_face = m / 3;
        cur = cur.apply(m);
    }
    solution
        .iter()
        .rev()
        .map(|&m| Move::from_index(m).inverse())
        .collect()
}

pub fn generate_with(rng: &mut impl Rng) -> String {
    let state = random_scramble_state(rng);
    cube::format(&scramble_for(&state, rng))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nxn;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn index_and_state_are_inverse_bijections() {
        for i in (0..N_STATES).step_by(7) {
            let s = State::from_index(i);
            assert_eq!(s.index(), i);
            assert_eq!(s.cp[6], 6, "the fixed corner stays put");
            assert_eq!(s.co[6], 0);
            assert_eq!(s.co.iter().map(|&x| x as usize).sum::<usize>() % 3, 0);
        }
        assert_eq!(START.index(), 0);
        assert_eq!(State::from_index(0), START);
    }

    #[test]
    fn moves_never_disturb_the_fixed_corner() {
        let mut s = START;
        for m in [0, 4, 8, 1, 3, 7, 2, 5, 6] {
            s = s.apply(m);
            assert_eq!((s.cp[6], s.co[6]), (6, 0));
        }
    }

    // The published number of 2x2 states at each distance (half-turn metric,
    // one corner fixed): the table must reproduce it exactly.
    #[test]
    fn distance_distribution_matches_the_known_counts() {
        let mut counts = vec![0usize; 12];
        for &d in distances() {
            counts[d as usize] += 1;
        }
        assert_eq!(
            counts,
            vec![
                1, 9, 54, 321, 1847, 9992, 50136, 227536, 870072, 1887748, 623800, 2644
            ]
        );
        assert_eq!(counts.iter().sum::<usize>(), N_STATES);
    }

    #[test]
    fn states_closer_than_four_moves_are_rejected() {
        let mut rng = StdRng::seed_from_u64(31);
        for _ in 0..2_000 {
            assert!(distance(&random_scramble_state(&mut rng)) >= MIN_DISTANCE);
        }
        // 1 + 9 + 54 + 321 states are under 4 moves: about 0.01 %
        let easy: usize = [1usize, 9, 54, 321].iter().sum();
        assert!(easy * 1000 < N_STATES);
    }

    #[test]
    fn random_states_are_spread_over_all_distances() {
        let mut rng = StdRng::seed_from_u64(32);
        let mut seen = [0usize; 12];
        for _ in 0..20_000 {
            seen[distance(&random_state(&mut rng)) as usize] += 1;
        }
        // by far the most states need 9 moves; every distance 5..=10 appears
        assert!(seen[9] > seen[8] && seen[9] > seen[10]);
        assert!(seen[5..=10].iter().all(|&n| n > 0));
    }

    #[test]
    fn a_scramble_reaches_exactly_its_state() {
        let mut rng = StdRng::seed_from_u64(33);
        for _ in 0..300 {
            let state = random_scramble_state(&mut rng);
            let moves = scramble_for(&state, &mut rng);
            assert_eq!(moves.len(), distance(&state) as usize, "optimal length");
            let mut s = START;
            for m in &moves {
                s = s.apply(m.index());
            }
            assert_eq!(s, state);
        }
    }

    #[test]
    fn scrambles_are_short_and_use_only_u_r_f() {
        let mut rng = StdRng::seed_from_u64(34);
        for _ in 0..300 {
            let text = generate_with(&mut rng);
            let moves = cube::parse(&text).unwrap();
            assert!(
                (MIN_DISTANCE as usize..=11).contains(&moves.len()),
                "{text}"
            );
            // faces are numbered U R F D L B: only the first three turn
            assert!(moves.iter().all(|m| m.face < 3), "{text}");
            for w in moves.windows(2) {
                assert_ne!(w[0].face, w[1].face, "{text}");
            }
        }
    }

    // Applying the scramble to a real 2x2 (the sticker model) leaves the
    // DBL corner in place: the 2x2 is scrambled relative to a fixed corner.
    #[test]
    fn the_sticker_model_agrees_the_corner_stays_fixed() {
        let mut rng = StdRng::seed_from_u64(35);
        for _ in 0..50 {
            let text = generate_with(&mut rng);
            let mut c = nxn::Nxn::solved(2);
            c.apply_all(&nxn::parse(&text).unwrap());
            let f = c.facelets();
            // 2x2 net: the D face's back-left sticker (row 1, col 0) and the L
            // and B faces' bottom stickers next to it belong to DBL
            assert_eq!(f[3 * 4 + 2], 3);
            assert_eq!(f[4 * 4 + 2], 4);
            assert_eq!(f[5 * 4 + 3], 5);
        }
    }

    #[test]
    fn different_calls_give_different_scrambles() {
        let mut rng = StdRng::seed_from_u64(36);
        assert_ne!(generate_with(&mut rng), generate_with(&mut rng));
    }
}
