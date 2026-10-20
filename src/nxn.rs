//! An N×N×N cube as a set of colored stickers in space.
//!
//! Every sticker has a position and a face normal; a move rotates the stickers
//! in some layers about an axis. That works for any size, so the same code
//! serves 2x2 to 7x7. Coordinates are doubled so the center of an even cube is
//! not a half integer: for size N they run -(N-1), -(N-3), ..., N-1.
//!
//! Faces are U R F D L B (the same order as `cube.rs`); +x is R, +y is U and
//! +z is F.

use std::fmt;

use rand::{Rng, RngExt};

pub const FACE_NAMES: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];

/// Axis (0 = x, 1 = y, 2 = z) and sign of each face's outward normal.
const NORMALS: [(usize, i8); 6] = [(1, 1), (0, 1), (2, 1), (1, -1), (0, -1), (2, -1)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sticker {
    pos: [i16; 3],
    normal: [i8; 3],
    color: u8,
}

/// A turn of the outer `layers` layers of `face` (1 is plain `R`, 2 is `Rw`,
/// 3 is `3Rw`), `turns` clockwise quarter turns (1 = `R`, 2 = `R2`, 3 = `R'`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub face: u8,
    pub layers: u8,
    pub turns: u8,
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let face = FACE_NAMES[self.face as usize];
        match self.layers {
            1 => write!(f, "{face}")?,
            2 => write!(f, "{face}w")?,
            n => write!(f, "{n}{face}w")?,
        }
        match self.turns {
            1 => Ok(()),
            2 => write!(f, "2"),
            _ => write!(f, "'"),
        }
    }
}

impl Move {
    #[cfg(test)]
    pub fn inverse(self) -> Move {
        Move {
            turns: 4 - self.turns,
            ..self
        }
    }

    /// The axis the move turns about: 0 is x (R, L), 1 is y (U, D), 2 is z.
    pub fn axis(self) -> usize {
        NORMALS[self.face as usize].0
    }

    /// +1 if the move's face is on the positive side of its axis (U R F),
    /// -1 otherwise.
    pub fn sign(self) -> i8 {
        NORMALS[self.face as usize].1
    }
}

/// Moves written out as `R U2 Rw'`.
pub fn format(moves: &[Move]) -> String {
    moves
        .iter()
        .map(Move::to_string)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Parse a sequence such as `R U2 Rw' 3Bw`. `None` if any move is malformed.
pub fn parse(seq: &str) -> Option<Vec<Move>> {
    seq.split_whitespace().map(parse_move).collect()
}

fn parse_move(tok: &str) -> Option<Move> {
    let (turns, body) = if let Some(b) = tok.strip_suffix('\'') {
        (3, b)
    } else if let Some(b) = tok.strip_suffix('2') {
        (2, b)
    } else {
        (1, tok)
    };
    let digits = body.chars().take_while(char::is_ascii_digit).count();
    let (prefix, rest) = body.split_at(digits);
    let (face, wide) = match rest.strip_suffix('w') {
        Some(f) => (f, true),
        None => (rest, false),
    };
    let mut chars = face.chars();
    let letter = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let face = FACE_NAMES.iter().position(|&f| f == letter)? as u8;
    let layers = match (prefix, wide) {
        ("", false) => 1,
        ("", true) => 2,
        (n, true) => n.parse::<u8>().ok().filter(|&n| n >= 3)?,
        _ => return None,
    };
    Some(Move {
        face,
        layers,
        turns,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nxn {
    size: usize,
    stickers: Vec<Sticker>,
}

impl Nxn {
    pub fn solved(size: usize) -> Nxn {
        assert!((2..=9).contains(&size));
        let n = size as i16;
        let coords: Vec<i16> = (0..n).map(|i| 2 * i - (n - 1)).collect();
        let mut stickers = Vec::with_capacity(6 * size * size);
        for (face, &(axis, sign)) in NORMALS.iter().enumerate() {
            for &a in &coords {
                for &b in &coords {
                    let mut pos = [0i16; 3];
                    pos[axis] = sign as i16 * n; // the sticker sits on the outer surface
                    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                    pos[u] = a;
                    pos[v] = b;
                    let mut normal = [0i8; 3];
                    normal[axis] = sign;
                    stickers.push(Sticker {
                        pos,
                        normal,
                        color: face as u8,
                    });
                }
            }
        }
        Nxn { size, stickers }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    /// Turn the layers of `m` clockwise as seen from its face, `m.turns` times.
    pub fn apply(&mut self, m: Move) {
        assert!((1..=self.size).contains(&(m.layers as usize)));
        let (axis, sign) = NORMALS[m.face as usize];
        let n = self.size as i16;
        // Layer k from the face (0 is outermost) has coordinate sign*(n-1-2k).
        let lowest = (n - 1) - 2 * (m.layers as i16 - 1);
        for s in &mut self.stickers {
            let c = sign as i16 * s.pos[axis];
            // a face's own stickers sit on the surface, at coordinate n
            if c >= lowest && c <= n {
                for _ in 0..m.turns {
                    rotate(s, axis, sign);
                }
            }
        }
    }

    pub fn apply_all(&mut self, moves: &[Move]) {
        for &m in moves {
            self.apply(m);
        }
    }

    /// Every sticker as (center, outward normal, color) in cube coordinates:
    /// the cube spans -1..1 on each axis, so a sticker is `2 / size` wide.
    /// What the 3D view draws.
    pub fn stickers(&self) -> impl Iterator<Item = ([f64; 3], [i8; 3], u8)> + '_ {
        let n = self.size as f64;
        self.stickers.iter().map(move |s| {
            let pos = s.pos.map(|c| c as f64 / n);
            (pos, s.normal, s.color)
        })
    }

    /// Whether a sticker at `center` (as from `stickers`) is in the layers that
    /// `m` turns.
    pub fn in_layers(&self, m: Move, center: [f64; 3]) -> bool {
        let (axis, sign) = NORMALS[m.face as usize];
        let n = self.size as f64;
        // back in doubled coordinates: layer k from the face is at n-1-2k
        let c = sign as f64 * center[axis] * n;
        let lowest = (n - 1.0) - 2.0 * (m.layers as f64 - 1.0);
        c >= lowest - 0.5
    }

    /// The colors (0..6, U R F D L B) of all 6 N² stickers, face by face; each
    /// face is `size` rows of `size` columns as seen from outside with U above
    /// F: sticker `face * N² + row * N + column`.
    pub fn facelets(&self) -> Vec<u8> {
        let n = self.size;
        let mut out = vec![0u8; 6 * n * n];
        for s in &self.stickers {
            let face = NORMALS
                .iter()
                .position(|&(a, sg)| s.normal[a] == sg)
                .unwrap();
            let (row, col) = net_position(face, s.pos, n);
            out[face * n * n + row * n + col] = s.color;
        }
        out
    }

    #[cfg(test)]
    pub fn is_solved(&self) -> bool {
        self.facelets()
            .chunks(self.size * self.size)
            .enumerate()
            .all(|(f, c)| c.iter().all(|&x| x == f as u8))
    }
}

/// Rotate a sticker a quarter turn clockwise about the outward `axis` of a
/// face with the given `sign`.
fn rotate(s: &mut Sticker, axis: usize, sign: i8) {
    // Clockwise seen from +axis is a rotation by -90 degrees about +axis, so
    // (u, v) -> (v, -u) with (u, v) the next two axes in cyclic order. From the
    // -axis side the same turn is the opposite rotation.
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let turn = |p: &mut [i16; 3]| {
        let (pu, pv) = (p[u], p[v]);
        if sign > 0 {
            p[u] = pv;
            p[v] = -pu;
        } else {
            p[u] = -pv;
            p[v] = pu;
        }
    };
    turn(&mut s.pos);
    let mut normal = s.normal.map(i16::from);
    turn(&mut normal);
    s.normal = normal.map(|c| c as i8);
}

/// Row and column of a sticker on its face's net, from its position.
fn net_position(face: usize, pos: [i16; 3], n: usize) -> (usize, usize) {
    let idx = |c: i16| ((c + n as i16 - 1) / 2) as usize; // 0..n from coordinate
    let rev = |c: i16| n - 1 - idx(c);
    let (x, y, z) = (pos[0], pos[1], pos[2]);
    match face {
        0 => (idx(z), idx(x)), // U: back at the top
        2 => (rev(y), idx(x)), // F
        1 => (rev(y), rev(z)), // R: front on the left
        5 => (rev(y), rev(x)), // B: right side on the left
        4 => (rev(y), idx(z)), // L: back on the left
        _ => (rev(z), idx(x)), // D: front at the top
    }
}

/// A random-move scramble for a big cube, in the style the WCA uses for them
/// (TNoodle): `length` turns of the outer layers and, for the wide turns,
/// layers up to half the cube. On even cubes the innermost half is only turned
/// from R, U and F, since turning it from the other side is the same thing up
/// to whole-cube rotation.
pub fn random_moves(size: usize, length: usize, rng: &mut impl Rng) -> Vec<Move> {
    let max_layers = size / 2;
    let mut moves: Vec<Move> = Vec::with_capacity(length);
    // faces (and layer counts) already turned in the current run on one axis
    let mut run: Vec<(u8, u8)> = Vec::new();
    let mut run_axis = usize::MAX;
    while moves.len() < length {
        let face = rng.random_range(0..6) as u8;
        let layers = rng.random_range(1..=max_layers) as u8;
        let half_of_even =
            size.is_multiple_of(2) && layers as usize == max_layers && max_layers > 1;
        if half_of_even && face >= 3 {
            continue; // L, D and B equivalents are skipped
        }
        let axis = NORMALS[face as usize].0;
        if axis == run_axis && run.contains(&(face, layers)) {
            continue; // the same layers twice in a row cancel or merge
        }
        if axis != run_axis {
            run.clear();
            run_axis = axis;
        }
        run.push((face, layers));
        moves.push(Move {
            face,
            layers,
            turns: rng.random_range(1..=3),
        });
    }
    moves
}

/// The number of random moves the WCA scrambler uses for a cube of this size.
pub fn scramble_length(size: usize) -> usize {
    match size {
        4 => 40,
        5 => 60,
        6 => 80,
        7 => 100,
        _ => 25,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn m(s: &str) -> Move {
        parse_move(s).unwrap()
    }

    #[test]
    fn parse_and_display_round_trip() {
        for s in [
            "R", "U2", "F'", "Rw", "Rw2", "Rw'", "3Rw", "3Bw'", "4Fw2", "Lw",
        ] {
            assert_eq!(m(s).to_string(), s);
        }
        let seq = "R U2 Rw' 3Fw D L2";
        let moves = parse(seq).unwrap();
        assert_eq!(
            moves
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<_>>()
                .join(" "),
            seq
        );
        assert_eq!(parse("").unwrap(), vec![]);
    }

    #[test]
    fn parse_rejects_garbage() {
        for bad in [
            "X", "R3", "Rww", "1Rw", "2Rw", "3R", "Rw3", "w", "R2'", "U ' ",
        ] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn a_solved_cube_has_one_color_per_face() {
        for n in 2..=7 {
            let c = Nxn::solved(n);
            assert!(c.is_solved());
            assert_eq!(c.facelets().len(), 6 * n * n);
            for color in 0..6u8 {
                assert_eq!(c.facelets().iter().filter(|&&x| x == color).count(), n * n);
            }
        }
    }

    #[test]
    fn every_turn_has_order_four_and_inverts() {
        for n in 2..=7 {
            for face in 0..6u8 {
                for layers in 1..=((n / 2).max(1)) as u8 {
                    let mv = Move {
                        face,
                        layers,
                        turns: 1,
                    };
                    let mut c = Nxn::solved(n);
                    for k in 1..=4 {
                        c.apply(mv);
                        assert_eq!(c.is_solved(), k == 4, "{n} {mv}");
                    }
                    c.apply(mv);
                    c.apply(mv.inverse());
                    assert!(c.is_solved());
                }
            }
        }
    }

    // The 3x3 model in cube.rs is verified against known facts about the
    // cube (orders of sequences, the superflip, ...); agreeing with it for
    // every sticker on random sequences verifies this model and the net.
    #[test]
    fn agrees_with_the_verified_3x3_model() {
        let mut rng = StdRng::seed_from_u64(21);
        for _ in 0..300 {
            let len = rng.random_range(1..30);
            let idx: Vec<usize> = (0..len).map(|_| rng.random_range(0..18)).collect();
            let a = cube::SOLVED.apply_all(
                &idx.iter()
                    .map(|&i| cube::Move::from_index(i))
                    .collect::<Vec<_>>(),
            );
            let mut b = Nxn::solved(3);
            b.apply_all(
                &idx.iter()
                    .map(|&i| Move {
                        face: (i / 3) as u8,
                        layers: 1,
                        turns: (i % 3 + 1) as u8,
                    })
                    .collect::<Vec<_>>(),
            );
            assert_eq!(a.facelets().to_vec(), b.facelets(), "{idx:?}");
        }
    }

    #[test]
    fn the_known_3x3_sequence_orders_hold() {
        let order = |s: &str| {
            let moves = parse(s).unwrap();
            let mut c = Nxn::solved(3);
            let mut k = 0;
            loop {
                c.apply_all(&moves);
                k += 1;
                if c.is_solved() {
                    return k;
                }
            }
        };
        assert_eq!(order("R U"), 105);
        assert_eq!(order("R U R' U'"), 6);
        assert_eq!(order("R U2 D' B D'"), 1260);
    }

    // What the 2x2 net shows after R and U on a 4x4, checked by hand: the
    // turn is clockwise seen from the face, taking the front column up.
    #[test]
    fn wide_turns_move_exactly_the_outer_layers() {
        let mut c = Nxn::solved(4);
        c.apply(m("Rw"));
        let f = c.facelets();
        let n = 4;
        // F face: the two right columns now show D, the left two still F
        for row in 0..n {
            assert_eq!(f[2 * 16 + row * n], 2);
            assert_eq!(f[2 * 16 + row * n + 1], 2);
            assert_eq!(f[2 * 16 + row * n + 2], 3);
            assert_eq!(f[2 * 16 + row * n + 3], 3);
        }
        // L face untouched
        assert!(f[4 * 16..5 * 16].iter().all(|&x| x == 4));
    }

    #[test]
    fn deeper_turns_move_more_layers() {
        let mut c = Nxn::solved(6);
        c.apply(m("3Rw"));
        let f = c.facelets();
        let n = 6;
        // F face: three right columns changed, three left unchanged
        for col in 0..n {
            let want = if col >= 3 { 3 } else { 2 };
            assert_eq!(f[2 * 36 + col], want, "col {col}");
        }
    }

    #[test]
    fn opposite_faces_commute_on_every_size() {
        for n in 2..=7 {
            let a = [m("R"), m("L")];
            let mut x = Nxn::solved(n);
            x.apply_all(&a);
            let mut y = Nxn::solved(n);
            y.apply_all(&[a[1], a[0]]);
            assert_eq!(x, y);
        }
    }

    #[test]
    fn scramble_lengths_follow_the_wca_table() {
        assert_eq!(
            (4..=7).map(scramble_length).collect::<Vec<_>>(),
            vec![40, 60, 80, 100]
        );
    }

    #[test]
    fn random_moves_have_the_right_length_and_move_set() {
        let mut rng = StdRng::seed_from_u64(22);
        for n in 4..=7 {
            for _ in 0..50 {
                let moves = random_moves(n, scramble_length(n), &mut rng);
                assert_eq!(moves.len(), scramble_length(n));
                for mv in &moves {
                    assert!(mv.layers as usize <= n / 2, "{n} {mv}");
                    assert!((1..=3).contains(&mv.turns));
                    if n % 2 == 0 && mv.layers as usize == n / 2 && n > 2 {
                        assert!(mv.face < 3, "{n} {mv}: innermost half only from U R F");
                    }
                }
            }
        }
    }

    #[test]
    fn random_moves_never_repeat_layers_on_one_axis_in_a_row() {
        let mut rng = StdRng::seed_from_u64(23);
        for n in 4..=7 {
            for _ in 0..50 {
                let moves = random_moves(n, scramble_length(n), &mut rng);
                let mut run: Vec<(u8, u8)> = vec![];
                let mut axis = usize::MAX;
                for mv in moves {
                    if mv.axis() != axis {
                        run.clear();
                        axis = mv.axis();
                    }
                    assert!(!run.contains(&(mv.face, mv.layers)), "{n}");
                    run.push((mv.face, mv.layers));
                }
            }
        }
    }

    #[test]
    fn random_moves_use_every_kind_of_move() {
        let mut rng = StdRng::seed_from_u64(24);
        let all: Vec<Move> = (0..40)
            .flat_map(|_| random_moves(7, 100, &mut rng))
            .collect();
        for face in 0..6u8 {
            for layers in 1..=3u8 {
                assert!(
                    all.iter().any(|x| x.face == face && x.layers == layers),
                    "{face} {layers}"
                );
            }
        }
        for turns in 1..=3 {
            assert!(all.iter().any(|x| x.turns == turns));
        }
    }

    #[test]
    fn scrambles_actually_scramble() {
        let mut rng = StdRng::seed_from_u64(25);
        for n in 4..=7 {
            let mut c = Nxn::solved(n);
            c.apply_all(&random_moves(n, scramble_length(n), &mut rng));
            assert!(!c.is_solved());
            // and the sticker counts are preserved
            for color in 0..6u8 {
                assert_eq!(c.facelets().iter().filter(|&&x| x == color).count(), n * n);
            }
        }
    }
}
