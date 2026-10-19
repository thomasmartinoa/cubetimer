//! Kociemba's two-phase algorithm: finds a short (not optimal) solution for any
//! cube state, which is how WCA random-state scrambles are made.
//!
//! Phase 1 brings the cube into the subgroup G1 = <U, D, R2, L2, F2, B2>, where
//! every piece is oriented and the middle-layer edges are in the middle layer.
//! Phase 2 solves it using only G1 moves. Both phases are IDA* searches over
//! small coordinates, with move tables and exact-distance pruning tables built
//! once on first use (about 4 MB, a fraction of a second in a release build).

use std::collections::VecDeque;
use std::sync::OnceLock;

use crate::cube::{Cube, Move, SOLVED};

const N_TWIST: usize = 2187;
const N_FLIP: usize = 2048;
const N_SLICE: usize = 495;
const N_PERM8: usize = 40320;
const N_SLICE_PERM: usize = 24;

const PHASE1_MOVES: [usize; 18] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17];
/// The G1 moves, as indices into `Move::all()`: U U2 U' D D2 D' R2 L2 F2 B2.
const PHASE2_MOVES: [usize; 10] = [0, 1, 2, 9, 10, 11, 4, 13, 7, 16];

struct Tables {
    twist: Vec<[u16; 18]>,
    flip: Vec<[u16; 18]>,
    slice: Vec<[u16; 18]>,
    corner_perm: Vec<[u16; 10]>,
    ud_edge_perm: Vec<[u16; 10]>,
    slice_perm: Vec<[u16; 10]>,
    /// Exact distance to phase-1 goal for (twist, slice) and (flip, slice).
    twist_slice: Vec<u8>,
    flip_slice: Vec<u8>,
    /// Exact distance to solved within G1 for (corners, slice edges) and
    /// (U/D edges, slice edges).
    corner_slice: Vec<u8>,
    edge_slice: Vec<u8>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        let twist = move_table(N_TWIST, Cube::twist, &PHASE1_MOVES);
        let flip = move_table(N_FLIP, Cube::flip, &PHASE1_MOVES);
        let slice = move_table(N_SLICE, Cube::slice, &PHASE1_MOVES);
        let corner_perm = move_table(N_PERM8, Cube::corner_perm, &PHASE2_MOVES);
        let ud_edge_perm = move_table(N_PERM8, Cube::ud_edge_perm, &PHASE2_MOVES);
        let slice_perm = move_table(N_SLICE_PERM, Cube::slice_perm, &PHASE2_MOVES);
        Tables {
            twist_slice: prune_table(&twist, &slice),
            flip_slice: prune_table(&flip, &slice),
            corner_slice: prune_table(&corner_perm, &slice_perm),
            edge_slice: prune_table(&ud_edge_perm, &slice_perm),
            twist,
            flip,
            slice,
            corner_perm,
            ud_edge_perm,
            slice_perm,
        }
    })
}

/// Build the tables now (they are otherwise built on the first solve).
#[allow(dead_code)] // used by examples/scramblebench.rs
pub fn warm_up() {
    tables();
}

/// How each move changes a coordinate. A move's effect on a coordinate depends
/// only on the coordinate, so any cube with that coordinate value will do:
/// a breadth-first walk from the solved cube finds one for every value.
fn move_table<const M: usize>(
    n: usize,
    coord: fn(&Cube) -> usize,
    moves: &[usize; M],
) -> Vec<[u16; M]> {
    let mut reps: Vec<Option<Cube>> = vec![None; n];
    reps[coord(&SOLVED)] = Some(SOLVED);
    let mut queue = VecDeque::from([SOLVED]);
    while let Some(c) = queue.pop_front() {
        for &m in moves {
            let next = c.apply(Move::from_index(m));
            let k = coord(&next);
            if reps[k].is_none() {
                reps[k] = Some(next);
                queue.push_back(next);
            }
        }
    }
    reps.into_iter()
        .map(|rep| {
            let rep = rep.expect("every coordinate value is reachable");
            let mut row = [0u16; M];
            for (slot, &m) in row.iter_mut().zip(moves) {
                *slot = coord(&rep.apply(Move::from_index(m))) as u16;
            }
            row
        })
        .collect()
}

/// Breadth-first distances from (0, 0) in the product of two coordinates.
fn prune_table<const M: usize>(a: &[[u16; M]], b: &[[u16; M]]) -> Vec<u8> {
    let nb = b.len();
    let mut dist = vec![u8::MAX; a.len() * nb];
    dist[0] = 0;
    let mut frontier = vec![0usize];
    let mut depth = 0;
    while !frontier.is_empty() {
        depth += 1;
        let mut next = Vec::new();
        for &i in &frontier {
            let (x, y) = (i / nb, i % nb);
            for m in 0..M {
                let j = a[x][m] as usize * nb + b[y][m] as usize;
                if dist[j] == u8::MAX {
                    dist[j] = depth;
                    next.push(j);
                }
            }
        }
        frontier = next;
    }
    dist
}

/// Whether `face` may follow `last`: never the same face twice, and for
/// opposite faces (which commute) only one of the two orders.
fn allowed_after(last: Option<Move>, face: u8) -> bool {
    match last {
        None => true,
        Some(l) => face != l.face && !(face % 3 == l.face % 3 && face > l.face),
    }
}

struct Search<'a> {
    t: &'a Tables,
    start: Cube,
    max_len: usize,
    path: Vec<Move>,
}

impl Search<'_> {
    fn phase1(&mut self, twist: usize, flip: usize, slice: usize, depth: usize) -> bool {
        let t = self.t;
        let h = t.twist_slice[twist * N_SLICE + slice].max(t.flip_slice[flip * N_SLICE + slice])
            as usize;
        if h > depth {
            return false;
        }
        if depth == 0 {
            // In G1. A phase-1 path ending in a G1 move would just be a
            // shorter phase 1 found earlier, so skip it.
            let ends_in_g1 = self
                .path
                .last()
                .is_some_and(|m| PHASE2_MOVES.contains(&m.index()));
            return !ends_in_g1 && self.start_phase2();
        }
        for m in 0..18 {
            let mv = Move::from_index(m);
            if !allowed_after(self.path.last().copied(), mv.face) {
                continue;
            }
            self.path.push(mv);
            if self.phase1(
                t.twist[twist][m] as usize,
                t.flip[flip][m] as usize,
                t.slice[slice][m] as usize,
                depth - 1,
            ) {
                return true;
            }
            self.path.pop();
        }
        false
    }

    fn start_phase2(&mut self) -> bool {
        let c = self.start.apply_all(&self.path);
        let (cp, ep, sp) = (c.corner_perm(), c.ud_edge_perm(), c.slice_perm());
        let budget = self.max_len - self.path.len();
        (0..=budget).any(|depth| self.phase2(cp, ep, sp, depth))
    }

    fn phase2(&mut self, cp: usize, ep: usize, sp: usize, depth: usize) -> bool {
        let t = self.t;
        let h = t.corner_slice[cp * N_SLICE_PERM + sp].max(t.edge_slice[ep * N_SLICE_PERM + sp])
            as usize;
        if h > depth {
            return false;
        }
        if depth == 0 {
            // both distances are 0 only when solved
            return true;
        }
        for (j, &m) in PHASE2_MOVES.iter().enumerate() {
            let mv = Move::from_index(m);
            if !allowed_after(self.path.last().copied(), mv.face) {
                continue;
            }
            self.path.push(mv);
            if self.phase2(
                t.corner_perm[cp][j] as usize,
                t.ud_edge_perm[ep][j] as usize,
                t.slice_perm[sp][j] as usize,
                depth - 1,
            ) {
                return true;
            }
            self.path.pop();
        }
        false
    }
}

/// A solution of at most `max_len` moves, or `None` if there is none that
/// short. Any state has one of at most 20 moves; asking for 20 or more always
/// succeeds, and the two-phase search finds 21 quickly.
pub fn solve(cube: &Cube, max_len: usize) -> Option<Vec<Move>> {
    debug_assert!(cube.is_solvable());
    let t = tables();
    let mut search = Search {
        t,
        start: *cube,
        max_len,
        path: Vec::new(),
    };
    let (twist, flip, slice) = (cube.twist(), cube.flip(), cube.slice());
    for depth in 0..=max_len {
        if search.phase1(twist, flip, slice, depth) {
            return Some(search.path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube::parse;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn tables_cover_every_coordinate() {
        let t = tables();
        assert_eq!(
            (t.twist.len(), t.flip.len(), t.slice.len()),
            (N_TWIST, N_FLIP, N_SLICE)
        );
        assert_eq!(
            (
                t.corner_perm.len(),
                t.ud_edge_perm.len(),
                t.slice_perm.len()
            ),
            (N_PERM8, N_PERM8, N_SLICE_PERM)
        );
        // every pruning entry was reached
        for p in [
            &t.twist_slice,
            &t.flip_slice,
            &t.corner_slice,
            &t.edge_slice,
        ] {
            assert!(p.iter().all(|&d| d != u8::MAX));
        }
    }

    #[test]
    fn known_pruning_depths() {
        // Published maxima: phase 1 needs at most 12 moves, phase 2 at most 18;
        // these projections are lower bounds, so they can't exceed that.
        let t = tables();
        assert!(*t.twist_slice.iter().max().unwrap() <= 12);
        assert!(*t.flip_slice.iter().max().unwrap() <= 12);
        assert!(*t.corner_slice.iter().max().unwrap() <= 18);
        assert!(*t.edge_slice.iter().max().unwrap() <= 18);
    }

    #[test]
    fn move_tables_agree_with_the_cube_model() {
        let t = tables();
        let mut rng = StdRng::seed_from_u64(5);
        for _ in 0..300 {
            let c = Cube::random(&mut rng);
            for m in 0..18 {
                let d = c.apply(Move::from_index(m));
                assert_eq!(t.twist[c.twist()][m] as usize, d.twist());
                assert_eq!(t.flip[c.flip()][m] as usize, d.flip());
                assert_eq!(t.slice[c.slice()][m] as usize, d.slice());
            }
        }
    }

    #[test]
    fn solved_cube_needs_no_moves() {
        assert_eq!(solve(&SOLVED, 21), Some(vec![]));
    }

    #[test]
    fn short_scrambles_are_solved() {
        // The search returns the first solution within the limit, not the
        // shortest (like min2phase); a limit equal to the scramble length
        // forces a short one.
        for s in ["R", "U2", "F'", "R U", "R U R' U'", "L2 D B'"] {
            let moves = parse(s).unwrap();
            let c = SOLVED.apply_all(&moves);
            let sol = solve(&c, 21).unwrap();
            assert_eq!(c.apply_all(&sol), SOLVED, "{s}");
            let tight = solve(&c, moves.len()).unwrap();
            assert!(tight.len() <= moves.len(), "{s}");
            assert_eq!(c.apply_all(&tight), SOLVED, "{s}");
        }
    }

    #[test]
    fn superflip_is_solved_within_21() {
        let c =
            SOLVED.apply_all(&parse("U R2 F B R B2 R U2 L B2 R U' D' R2 F R' L B2 U2 F2").unwrap());
        let sol = solve(&c, 21).unwrap();
        assert!(sol.len() <= 21);
        assert_eq!(c.apply_all(&sol), SOLVED);
    }

    #[test]
    fn random_states_are_solved_within_21_moves() {
        let mut rng = StdRng::seed_from_u64(6);
        for _ in 0..30 {
            let c = Cube::random(&mut rng);
            let sol = solve(&c, 21).expect("a 21-move solution");
            assert!(sol.len() <= 21);
            assert_eq!(c.apply_all(&sol), SOLVED);
            for w in sol.windows(2) {
                assert!(allowed_after(Some(w[0]), w[1].face), "{w:?}");
            }
        }
    }

    #[test]
    fn a_too_small_limit_finds_nothing() {
        let c = SOLVED.apply_all(&parse("R U F").unwrap());
        assert_eq!(solve(&c, 2), None);
        assert!(solve(&c, 3).is_some());
    }
}
