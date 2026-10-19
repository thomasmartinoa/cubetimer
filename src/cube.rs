//! The 3x3x3 cube at the piece level, in Kociemba's conventions.
//!
//! A `Cube` records where each corner and edge is and how it is twisted or
//! flipped. Moves are cubes too: applying a move is multiplying by it.
//! Corners: URF UFL ULB UBR DFR DLF DBL DRB. Edges: UR UF UL UB DR DF DL DB
//! FR FL BL BR. Faces: U R F D L B.

use rand::seq::SliceRandom;
use rand::{Rng, RngExt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Cube {
    /// `cp[i]` is the corner sitting in position `i`.
    pub cp: [u8; 8],
    /// Twist of the corner in position `i`: 0, 1 or 2 (clockwise turns).
    pub co: [u8; 8],
    pub ep: [u8; 12],
    /// Flip of the edge in position `i`: 0 or 1.
    pub eo: [u8; 12],
}

pub const SOLVED: Cube = Cube {
    cp: [0, 1, 2, 3, 4, 5, 6, 7],
    co: [0; 8],
    ep: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    eo: [0; 12],
};

pub const FACE_NAMES: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

/// Clockwise quarter turn of each face, in `FACE_NAMES` order.
const FACE_TURNS: [Cube; 6] = [
    // U
    Cube {
        cp: [3, 0, 1, 2, 4, 5, 6, 7],
        co: [0; 8],
        ep: [3, 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11],
        eo: [0; 12],
    },
    // R
    Cube {
        cp: [4, 1, 2, 0, 7, 5, 6, 3],
        co: [2, 0, 0, 1, 1, 0, 0, 2],
        ep: [8, 1, 2, 3, 11, 5, 6, 7, 4, 9, 10, 0],
        eo: [0; 12],
    },
    // F
    Cube {
        cp: [1, 5, 2, 3, 0, 4, 6, 7],
        co: [1, 2, 0, 0, 2, 1, 0, 0],
        ep: [0, 9, 2, 3, 4, 8, 6, 7, 1, 5, 10, 11],
        eo: [0, 1, 0, 0, 0, 1, 0, 0, 1, 1, 0, 0],
    },
    // D
    Cube {
        cp: [0, 1, 2, 3, 5, 6, 7, 4],
        co: [0; 8],
        ep: [0, 1, 2, 3, 5, 6, 7, 4, 8, 9, 10, 11],
        eo: [0; 12],
    },
    // L
    Cube {
        cp: [0, 2, 6, 3, 4, 1, 5, 7],
        co: [0, 1, 2, 0, 0, 2, 1, 0],
        ep: [0, 1, 10, 3, 4, 5, 9, 7, 8, 2, 6, 11],
        eo: [0; 12],
    },
    // B
    Cube {
        cp: [0, 1, 3, 7, 4, 5, 2, 6],
        co: [0, 0, 1, 2, 0, 0, 2, 1],
        ep: [0, 1, 2, 11, 4, 5, 6, 10, 8, 9, 3, 7],
        eo: [0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 1, 1],
    },
];

/// A face turn: `face` in `FACE_NAMES` order, `turns` clockwise quarter turns
/// (1, 2 or 3, written `R`, `R2`, `R'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub face: u8,
    pub turns: u8,
}

impl Move {
    /// All 18 moves; move `i` is face `i / 3` turned `i % 3 + 1` times.
    pub fn all() -> impl Iterator<Item = Move> {
        (0..18).map(Move::from_index)
    }

    pub fn from_index(i: usize) -> Move {
        Move {
            face: (i / 3) as u8,
            turns: (i % 3 + 1) as u8,
        }
    }

    pub fn index(self) -> usize {
        self.face as usize * 3 + self.turns as usize - 1
    }

    pub fn inverse(self) -> Move {
        Move {
            face: self.face,
            turns: 4 - self.turns,
        }
    }

    /// The move as a cube, for multiplying.
    pub fn cube(self) -> Cube {
        move_cubes()[self.index()]
    }
}

impl std::fmt::Display for Move {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let suffix = match self.turns {
            1 => "",
            2 => "2",
            _ => "'",
        };
        write!(f, "{}{suffix}", FACE_NAMES[self.face as usize])
    }
}

/// The 18 moves as cubes, built once.
fn move_cubes() -> &'static [Cube; 18] {
    static CUBES: std::sync::OnceLock<[Cube; 18]> = std::sync::OnceLock::new();
    CUBES.get_or_init(|| {
        let mut all = [SOLVED; 18];
        for (face, turn) in FACE_TURNS.iter().enumerate() {
            let mut c = SOLVED;
            for turns in 0..3 {
                c = c.multiply(turn);
                all[face * 3 + turns] = c;
            }
        }
        all
    })
}

impl Cube {
    /// `self` followed by `other`.
    pub fn multiply(&self, other: &Cube) -> Cube {
        let mut out = SOLVED;
        for i in 0..8 {
            let from = other.cp[i] as usize;
            out.cp[i] = self.cp[from];
            out.co[i] = (self.co[from] + other.co[i]) % 3;
        }
        for i in 0..12 {
            let from = other.ep[i] as usize;
            out.ep[i] = self.ep[from];
            out.eo[i] = (self.eo[from] + other.eo[i]) % 2;
        }
        out
    }

    pub fn apply(&self, m: Move) -> Cube {
        self.multiply(&m.cube())
    }

    pub fn apply_all(&self, moves: &[Move]) -> Cube {
        moves.iter().fold(*self, |c, &m| c.apply(m))
    }

    /// A state drawn uniformly from all solvable states.
    pub fn random(rng: &mut impl Rng) -> Cube {
        let mut c = SOLVED;
        c.cp.shuffle(rng);
        c.ep.shuffle(rng);
        // Corner and edge permutation parities must agree; swapping two edges
        // maps odd to even one-to-one, so the result stays uniform.
        if parity(&c.cp) != parity(&c.ep) {
            c.ep.swap(0, 1);
        }
        for i in 0..7 {
            c.co[i] = rng.random_range(0..3);
        }
        c.co[7] = (3 - c.co[..7].iter().map(|&x| x as u32).sum::<u32>() % 3) as u8 % 3;
        for i in 0..11 {
            c.eo[i] = rng.random_range(0..2);
        }
        c.eo[11] = (c.eo[..11].iter().map(|&x| x as u32).sum::<u32>() % 2) as u8;
        c
    }

    /// Whether this is a legal state (reachable by turning faces).
    pub fn is_solvable(&self) -> bool {
        let is_perm = |p: &[u8]| {
            let mut seen = vec![false; p.len()];
            p.iter()
                .all(|&x| (x as usize) < p.len() && !std::mem::replace(&mut seen[x as usize], true))
        };
        is_perm(&self.cp)
            && is_perm(&self.ep)
            && self.co.iter().all(|&x| x < 3)
            && self.eo.iter().all(|&x| x < 2)
            && self.co.iter().map(|&x| x as u32).sum::<u32>() % 3 == 0
            && self.eo.iter().map(|&x| x as u32).sum::<u32>() % 2 == 0
            && parity(&self.cp) == parity(&self.ep)
    }

    // Coordinates used by the two-phase solver.

    /// Corner twist, 0..2187.
    pub fn twist(&self) -> usize {
        self.co[..7].iter().fold(0, |a, &x| a * 3 + x as usize)
    }

    /// Edge flip, 0..2048.
    pub fn flip(&self) -> usize {
        self.eo[..11].iter().fold(0, |a, &x| a * 2 + x as usize)
    }

    /// Which four positions hold the middle-layer edges (FR FL BL BR), in any
    /// order: 0..495, and 0 when they are all in the middle layer.
    pub fn slice(&self) -> usize {
        let mut a = 0;
        let mut seen = 0;
        for j in (0..12).rev() {
            if self.ep[j] >= 8 {
                a += binomial(11 - j, seen + 1);
                seen += 1;
            }
        }
        a
    }

    /// Corner permutation, 0..40320.
    pub fn corner_perm(&self) -> usize {
        rank(&self.cp)
    }

    /// Permutation of the eight U and D layer edges, 0..40320. Only meaningful
    /// once those edges are all in the U and D layers (phase 2).
    pub fn ud_edge_perm(&self) -> usize {
        rank(&self.ep[..8])
    }

    /// Permutation of the four middle-layer edges, 0..24. Only meaningful once
    /// they are all in the middle layer (phase 2).
    pub fn slice_perm(&self) -> usize {
        let mut p = [0u8; 4];
        for (dst, &e) in p.iter_mut().zip(&self.ep[8..]) {
            *dst = e.wrapping_sub(8);
        }
        rank(&p)
    }
}

/// Stickers of each corner position, clockwise starting from the U or D
/// sticker. Faces are numbered U R F D L B and each face's stickers 0..9 row
/// by row, in the usual net: U above F, L F R B in a row, D below F.
#[cfg(test)]
const CORNER_FACELETS: [[usize; 3]; 8] = [
    [8, 9, 20],
    [6, 18, 38],
    [0, 36, 47],
    [2, 45, 11],
    [29, 26, 15],
    [27, 44, 24],
    [33, 53, 42],
    [35, 17, 51],
];
#[cfg(test)]
const EDGE_FACELETS: [[usize; 2]; 12] = [
    [5, 10],
    [7, 19],
    [3, 37],
    [1, 46],
    [32, 16],
    [28, 25],
    [30, 43],
    [34, 52],
    [23, 12],
    [21, 41],
    [50, 39],
    [48, 14],
];
/// The face colors of each corner and edge piece, in the same sticker order.
#[cfg(test)]
const CORNER_COLORS: [[u8; 3]; 8] = [
    [0, 1, 2],
    [0, 2, 4],
    [0, 4, 5],
    [0, 5, 1],
    [3, 2, 1],
    [3, 4, 2],
    [3, 5, 4],
    [3, 1, 5],
];
#[cfg(test)]
const EDGE_COLORS: [[u8; 2]; 12] = [
    [0, 1],
    [0, 2],
    [0, 4],
    [0, 5],
    [3, 1],
    [3, 2],
    [3, 4],
    [3, 5],
    [2, 1],
    [2, 4],
    [5, 4],
    [5, 1],
];

/// Sticker view of the cube, kept as the reference the general N×N model in
/// `nxn.rs` is tested against.
#[cfg(test)]
impl Cube {
    /// The color (as a face number, U R F D L B) of all 54 stickers, face by
    /// face: sticker `9 * face + row * 3 + column`.
    pub fn facelets(&self) -> [u8; 54] {
        let mut f = [0u8; 54];
        for face in 0..6 {
            f[face * 9 + 4] = face as u8;
        }
        for i in 0..8 {
            let (piece, twist) = (self.cp[i] as usize, self.co[i] as usize);
            for k in 0..3 {
                f[CORNER_FACELETS[i][(k + twist) % 3]] = CORNER_COLORS[piece][k];
            }
        }
        for i in 0..12 {
            let (piece, flip) = (self.ep[i] as usize, self.eo[i] as usize);
            for k in 0..2 {
                f[EDGE_FACELETS[i][(k + flip) % 2]] = EDGE_COLORS[piece][k];
            }
        }
        f
    }
}

fn parity(p: &[u8]) -> bool {
    let mut odd = false;
    for i in 0..p.len() {
        for j in i + 1..p.len() {
            odd ^= p[i] > p[j];
        }
    }
    odd
}

fn binomial(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    (0..k).fold(1, |acc, i| acc * (n - i) / (i + 1))
}

/// Lexicographic rank of a permutation of `0..p.len()` (its Lehmer code).
fn rank(p: &[u8]) -> usize {
    let mut r = 0;
    for i in 0..p.len() {
        let smaller_after = p[i + 1..].iter().filter(|&&x| x < p[i]).count();
        r = r * (p.len() - i) + smaller_after;
    }
    r
}

/// Moves written out as `R U2 F'`.
pub fn format(moves: &[Move]) -> String {
    moves
        .iter()
        .map(Move::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a move sequence like `R U2 F'`.
#[cfg(test)]
pub fn parse(seq: &str) -> Option<Vec<Move>> {
    seq.split_whitespace()
        .map(|tok| {
            let mut chars = tok.chars();
            let first = chars.next()?;
            let face = FACE_NAMES.iter().position(|&f| f == first)? as u8;
            let turns = match chars.as_str() {
                "" => 1,
                "2" => 2,
                "'" => 3,
                _ => return None,
            };
            Some(Move { face, turns })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn seq(s: &str) -> Vec<Move> {
        parse(s).unwrap()
    }

    fn order(moves: &[Move]) -> usize {
        let mut c = SOLVED.apply_all(moves);
        let mut n = 1;
        while c != SOLVED {
            c = c.apply_all(moves);
            n += 1;
        }
        n
    }

    #[test]
    fn every_face_turn_has_order_four() {
        for face in FACE_NAMES {
            assert_eq!(order(&seq(&face.to_string())), 4, "{face}");
        }
    }

    #[test]
    fn opposite_faces_commute_and_others_do_not() {
        for a in 0..6u8 {
            for b in 0..6u8 {
                let ab =
                    SOLVED.apply_all(&[Move { face: a, turns: 1 }, Move { face: b, turns: 1 }]);
                let ba =
                    SOLVED.apply_all(&[Move { face: b, turns: 1 }, Move { face: a, turns: 1 }]);
                assert_eq!(ab == ba, a % 3 == b % 3, "{a} {b}");
            }
        }
    }

    // Orders of well-known sequences pin down every entry of the move
    // definitions: a single wrong number changes at least one of them.
    #[test]
    fn known_sequence_orders() {
        assert_eq!(order(&seq("R U")), 105);
        assert_eq!(order(&seq("R U R' U'")), 6);
        assert_eq!(order(&seq("R U2 D' B D'")), 1260);
        assert_eq!(order(&seq("F R")), 105);
        assert_eq!(order(&seq("L B")), 105);
        assert_eq!(order(&seq("U F")), 105);
        assert_eq!(order(&seq("R2 U2")), 6);
        assert_eq!(order(&seq("R L")), 4);
    }

    #[test]
    fn superflip_flips_every_edge_and_nothing_else() {
        let sf = SOLVED.apply_all(&seq("U R2 F B R B2 R U2 L B2 R U' D' R2 F R' L B2 U2 F2"));
        assert_eq!(sf.cp, SOLVED.cp);
        assert_eq!(sf.co, SOLVED.co);
        assert_eq!(sf.ep, SOLVED.ep);
        assert_eq!(sf.eo, [1; 12]);
    }

    #[test]
    fn a_sequence_followed_by_its_inverse_is_solved() {
        let mut rng = StdRng::seed_from_u64(9);
        for _ in 0..200 {
            let moves: Vec<Move> = (0..25)
                .map(|_| Move::from_index(rng.random_range(0..18)))
                .collect();
            let inv: Vec<Move> = moves.iter().rev().map(|m| m.inverse()).collect();
            assert_eq!(SOLVED.apply_all(&moves).apply_all(&inv), SOLVED);
        }
    }

    #[test]
    fn moves_keep_states_solvable() {
        let mut rng = StdRng::seed_from_u64(1);
        let mut c = SOLVED;
        for _ in 0..1000 {
            c = c.apply(Move::from_index(rng.random_range(0..18)));
            assert!(c.is_solvable());
        }
    }

    #[test]
    fn random_states_are_solvable_and_varied() {
        let mut rng = StdRng::seed_from_u64(2);
        let states: Vec<Cube> = (0..500).map(|_| Cube::random(&mut rng)).collect();
        assert!(states.iter().all(Cube::is_solvable));
        // both permutation parities and every corner twist value turn up
        assert!(states.iter().any(|c| parity(&c.cp)) && states.iter().any(|c| !parity(&c.cp)));
        for v in 0..3 {
            assert!(states.iter().any(|c| c.co[7] == v));
        }
    }

    #[test]
    fn random_state_pieces_are_uniform() {
        // Each corner should land in position 0 about 1/8 of the time.
        let mut rng = StdRng::seed_from_u64(3);
        let n = 80_000;
        let mut counts = [0usize; 8];
        let mut edge0 = [0usize; 12];
        let mut twist = [0usize; 3];
        for _ in 0..n {
            let c = Cube::random(&mut rng);
            counts[c.cp[0] as usize] += 1;
            edge0[c.ep[0] as usize] += 1;
            twist[c.co[7] as usize] += 1;
        }
        let close = |count: usize, expected: f64| (count as f64 - expected).abs() < expected * 0.05;
        assert!(
            counts.iter().all(|&k| close(k, n as f64 / 8.0)),
            "{counts:?}"
        );
        assert!(
            edge0.iter().all(|&k| close(k, n as f64 / 12.0)),
            "{edge0:?}"
        );
        assert!(twist.iter().all(|&k| close(k, n as f64 / 3.0)), "{twist:?}");
    }

    #[test]
    fn unsolvable_states_are_detected() {
        let mut c = SOLVED;
        c.co[0] = 1;
        assert!(!c.is_solvable());
        let mut c = SOLVED;
        c.eo[0] = 1;
        assert!(!c.is_solvable());
        let mut c = SOLVED;
        c.ep.swap(0, 1);
        assert!(!c.is_solvable());
    }

    #[test]
    fn coordinates_are_zero_when_solved_and_in_range() {
        let s = SOLVED;
        assert_eq!(
            (
                s.twist(),
                s.flip(),
                s.slice(),
                s.corner_perm(),
                s.ud_edge_perm(),
                s.slice_perm()
            ),
            (0, 0, 0, 0, 0, 0)
        );
        let mut rng = StdRng::seed_from_u64(4);
        for _ in 0..1000 {
            let c = Cube::random(&mut rng);
            assert!(
                c.twist() < 2187 && c.flip() < 2048 && c.slice() < 495 && c.corner_perm() < 40320
            );
        }
    }

    #[test]
    fn rank_is_a_bijection() {
        let mut seen = vec![false; 24];
        let mut p = [0u8, 1, 2, 3];
        // all 24 permutations via Heap's algorithm
        fn heap(k: usize, p: &mut [u8; 4], seen: &mut Vec<bool>) {
            if k == 1 {
                let r = rank(p);
                assert!(!seen[r]);
                seen[r] = true;
                return;
            }
            for i in 0..k {
                heap(k - 1, p, seen);
                if k.is_multiple_of(2) {
                    p.swap(i, k - 1)
                } else {
                    p.swap(0, k - 1)
                }
            }
        }
        heap(4, &mut p, &mut seen);
        assert!(seen.iter().all(|&s| s));
    }

    #[test]
    fn parse_and_display_round_trip() {
        let s = "R U2 F' D L2 B'";
        let moves = seq(s);
        assert_eq!(
            moves
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            s
        );
        assert!(parse("R X").is_none());
        assert!(parse("R3").is_none());
        assert_eq!(parse("").unwrap(), vec![]);
    }

    fn stickers(moves: &str) -> [u8; 54] {
        SOLVED.apply_all(&seq(moves)).facelets()
    }

    const U: u8 = 0;
    const R: u8 = 1;
    const F: u8 = 2;
    const D: u8 = 3;
    const L: u8 = 4;
    const B: u8 = 5;

    #[test]
    fn solved_faces_are_one_color() {
        let f = SOLVED.facelets();
        for face in 0..6 {
            assert!(f[face * 9..face * 9 + 9].iter().all(|&c| c == face as u8));
        }
    }

    // What each clockwise turn visibly does to a cube held white (U) up,
    // green (F) front.
    #[test]
    fn r_lifts_the_front_right_column_onto_the_top() {
        let f = stickers("R");
        for (top, front, down, back) in [(2, 20, 29, 51), (5, 23, 32, 48), (8, 26, 35, 45)] {
            assert_eq!(f[top], F); // U right column <- F
            assert_eq!(f[front], D); // F right column <- D
            assert_eq!(f[down], B); // D right column <- B (upside down)
            assert_eq!(f[back], U); // B left column <- U
        }
        assert!(f[36..45].iter().all(|&c| c == L));
    }

    #[test]
    fn u_moves_the_front_top_row_to_the_left() {
        let f = stickers("U");
        for k in 0..3 {
            assert_eq!(f[18 + k], R); // F top row <- R
            assert_eq!(f[36 + k], F); // L top row <- F
            assert_eq!(f[45 + k], L); // B top row <- L
            assert_eq!(f[9 + k], B); // R top row <- B
        }
        assert!(f[27..36].iter().all(|&c| c == D));
    }

    #[test]
    fn f_turns_the_top_bottom_row_onto_the_right() {
        let f = stickers("F");
        for k in 0..3 {
            assert_eq!(f[6 + k], L); // U bottom row <- L
            assert_eq!(f[9 + 3 * k], U); // R left column <- U
            assert_eq!(f[27 + k], R); // D top row <- R
            assert_eq!(f[38 + 3 * k], D); // L right column <- D
        }
        assert!(f[45..54].iter().all(|&c| c == B));
    }

    #[test]
    fn every_state_has_nine_stickers_of_each_color() {
        let mut rng = StdRng::seed_from_u64(7);
        for _ in 0..500 {
            let f = Cube::random(&mut rng).facelets();
            for color in 0..6 {
                assert_eq!(f.iter().filter(|&&c| c == color).count(), 9);
            }
        }
    }

    #[test]
    fn checkerboard_pattern() {
        // M2 E2 S2 as outer turns: every face becomes a checkerboard of itself
        // and its opposite.
        let f = stickers("R2 L2 U2 D2 F2 B2");
        for face in 0..6u8 {
            let opposite = (face + 3) % 6;
            for k in 0..9 {
                let want = if k % 2 == 0 { face } else { opposite };
                assert_eq!(f[face as usize * 9 + k], want, "face {face} sticker {k}");
            }
        }
    }
}
