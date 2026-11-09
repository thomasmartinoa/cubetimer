//! Cube patterns: famous ones, letters and hearts, and your own.
//!
//! A pattern is a cube state made from a solved cube, held white on top and
//! green in front. Presets come with the algorithm that makes them; the tests
//! check every one against the cube model, and the letter ones against the
//! shape they are meant to show.
//!
//! For patterns you paint yourself, `complete` turns the painted stickers
//! into a real cube (stickers left open are filled in, keeping as many
//! pieces at home as it can) and `moves_to_make` finds a short way to get
//! there from solved.

#[cfg(test)]
use std::time::{Duration, Instant};

use crate::alg;
use crate::cube::{
    self, CORNER_COLORS, CORNER_FACELETS, Cube, EDGE_COLORS, EDGE_FACELETS, Move, SOLVED,
};
use crate::twophase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Classic,
    Letters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pattern {
    pub group: Group,
    pub name: &'static str,
    /// From a solved cube, white on top and green in front.
    pub alg: &'static str,
    /// What to look for, and where.
    pub about: &'static str,
}

impl Pattern {
    /// The cube the algorithm makes (the tests check every preset has one).
    pub fn state(&self) -> Cube {
        alg::effect(self.alg).unwrap_or(SOLVED)
    }

    #[cfg(test)]
    pub fn moves(&self) -> usize {
        alg::steps(self.alg).map_or(0, |s| s.len())
    }
}

const fn classic(name: &'static str, alg: &'static str, about: &'static str) -> Pattern {
    Pattern {
        group: Group::Classic,
        name,
        alg,
        about,
    }
}

const fn letter(name: &'static str, alg: &'static str, about: &'static str) -> Pattern {
    Pattern {
        group: Group::Letters,
        name,
        alg,
        about,
    }
}

/// Every preset, easiest first within a group.
pub const PATTERNS: &[Pattern] = &[
    // the classics, by length
    classic(
        "Stripes",
        "R2 L2 F2 B2",
        "Two-color stripes down every side, checkerboards on top and bottom.",
    ),
    classic("Bars", "U2 D2 F2 B2", "Three bars across every face."),
    classic(
        "Checkerboard",
        "R2 L2 U2 D2 F2 B2",
        "The best-known pattern: a checkerboard on every face. Cubers also write it M2 E2 S2.",
    ),
    classic(
        "Perpendicular lines",
        "U2 L2 B2 U2 L2 U2",
        "Lines that cross over from face to face.",
    ),
    classic(
        "Plus minus",
        "U2 R2 L2 U2 R2 L2",
        "A plus on the front and back, a minus on the sides.",
    ),
    classic(
        "Six spots",
        "U D' R L' F B' U D'",
        "Every center a different color from the rest of its face.",
    ),
    classic(
        "Four spots",
        "F2 B2 U D' R2 L2 U D'",
        "Spots on the four sides; the top and bottom stay solved.",
    ),
    classic(
        "Tetris",
        "L R F B U' D' L' R'",
        "Interlocking blocks like falling Tetris pieces.",
    ),
    classic(
        "Cross",
        "U F B' L2 U2 L2 F' B U2 L2 U",
        "A cross on every face, in the opposite face's color.",
    ),
    classic(
        "Opposite corners",
        "R L U2 F2 D2 F2 R L F2 D2 B2 D2",
        "Opposite colors in two corners of each side, spots top and bottom.",
    ),
    classic(
        "Green mamba",
        "R D R F R' F' B D R' U' B' U D2",
        "A snake winding round the cube.",
    ),
    classic(
        "Anaconda",
        "L U B' U' R L' B R' F B' D R D' F'",
        "A longer snake.",
    ),
    classic(
        "Python",
        "F2 R' B' U R' L F' L F' B D' R B L2",
        "A snake with stripes on top and bottom.",
    ),
    classic(
        "Black mamba",
        "R D L F' R L' D R' U D' B U' R' D'",
        "A snake that crosses whole faces.",
    ),
    classic(
        "Twister",
        "F R' U L F' L' F U' R U L' U' L F'",
        "A band twisting round the middle.",
    ),
    classic(
        "Cube in a cube",
        "F L F U' R U F2 L2 U' L' B D' B' L2 U",
        "A small cube sitting in the top front corner.",
    ),
    classic(
        "Plummer's cross",
        "R2 L' D F2 R' D' R' L U' D R D B2 R' U D2",
        "A cross on every face, each in a different color.",
    ),
    classic(
        "Spiral",
        "L' B' D U R U' R' D2 R2 D L D' L' R' F U",
        "A spiral running over three faces.",
    ),
    classic(
        "Exchanged cube in a cube",
        "F2 R2 U2 R2 F2 L2 D' B2 L2 F U L' B2 L U' F'",
        "The cube in a cube, with the colors swapped round.",
    ),
    classic(
        "Tricolor stripes",
        "F U F R L2 B D' R D2 L D' B R2 L F U F",
        "Three stripes of different colors down every side.",
    ),
    classic(
        "Exchanged peaks",
        "F U2 L F L' B L U B' R' L' U R' D' F' B R2",
        "Mountain peaks of one color rising into the next face.",
    ),
    classic(
        "Twisted peaks",
        "F B' U F U F U L B L2 B' U F' L U L' B",
        "Peaks that twist round the cube.",
    ),
    classic(
        "Cube in a cube in a cube",
        "U' L' U' F' R2 B' R F U B2 U B' L U' F U R F'",
        "Three cubes nested in the top front corner.",
    ),
    classic(
        "Wire",
        "R L F B R L F B R L F B R2 B2 L2 R2 B2 L2",
        "Wires running round the sides; top and bottom stay solved.",
    ),
    classic(
        "Displaced motif",
        "L2 B2 D' B2 D L2 U R2 D R2 B U R' F2 R U' B' U'",
        "Blocks slid out of place on every face.",
    ),
    classic(
        "Tablecloth",
        "R L U2 F' U2 D2 R2 L2 F' D2 F2 D R2 L2 F2 B2 D B2 L2",
        "A woven tablecloth on every face.",
    ),
    classic(
        "Diagonal minority cross",
        "R2 F U2 F2 D R2 B2 U2 F2 R2 B' U2 R2 B2 D L2 U2 F2 D2",
        "Crosses with a different color in every corner.",
    ),
    classic(
        "Superflip",
        "U R2 F B R B2 R U2 L B2 R U' D' R2 F R' L B2 U2 F2",
        "Every edge flipped in place. It is one of the hardest positions: no shorter way to solve it exists.",
    ),
    classic(
        "Kilt",
        "U' R2 L2 F2 B2 U' R L F B' U F2 D2 R2 L2 F2 U2 F2 U' F2",
        "A tartan of crosses and checks.",
    ),
    classic(
        "Cyclone",
        "L2 D2 B2 R2 D2 B2 L2 D U' L' D U F2 L R B F D' L' R'",
        "A storm swirling round the cube.",
    ),
    classic(
        "Twisted cube in a cube",
        "U' L D2 B2 D F' L2 B L B' R2 D B2 U' L2 U' F2 L2 F2 U2",
        "The cube in a cube, twisted.",
    ),
    classic(
        "Cata's serpent",
        "R2 F2 U' D' B2 L2 F2 L2 U D R2 F2 U2 F2 U2 F2 B2 U2 F2 D2",
        "A snake and checkerboards together.",
    ),
    classic(
        "Gift box",
        "U B2 R2 B2 L2 F2 R2 D' F2 L2 B F' L F2 D U' R2 F' L' R'",
        "Ribbons crossing on top and bottom like a wrapped present.",
    ),
    // letters, hearts and words: look at the front (green center)
    letter(
        "I ♥ U",
        "R2 U D2 B2 R' U D' B L2 B U D' L D' R L",
        "Read it as you turn the cube to the left: a green I on the front, a red heart on the right, a white U on the back.",
    ),
    letter(
        "HI",
        "L2 U2 R B' D' B R2 L2 B' R B R' D2 R2",
        "A green H on the front and a red I on the right.",
    ),
    letter(
        "Heart",
        "R2 L2 U2 R2 L2",
        "A green heart on the front and a blue one on the back.",
    ),
    letter(
        "I",
        "U2 R2 L2 U2",
        "A green I on the front, a blue one on the back.",
    ),
    letter("U", "R2 D2 L2", "A U on the front and on the back."),
    letter(
        "T",
        "R2 U2 L2 U2",
        "A green T on the front, a blue one on the back.",
    ),
    letter(
        "H",
        "R2 U2 D2 R2",
        "A green H on the front, a blue one on the back.",
    ),
    letter("L", "B D L'", "An orange L on the front."),
    letter("C", "U2 R2 D2 F2", "A C on the front and on the back."),
    letter(
        "E",
        "U2 R2 U2 B2",
        "A green E on the front, a blue one on the back.",
    ),
    letter("O", "R2 U2 D2 L2", "An O on the front and on the back."),
    letter(
        "X",
        "U2 D2 R2 L2",
        "A green X on the front, a blue one on the back.",
    ),
    letter("V", "R2 L2 F2 D2", "A V on the front and on the back."),
    letter(
        "Y",
        "R2 L2 U2",
        "A green Y on the front, a blue one on the back.",
    ),
    letter("P", "R2 F D2", "A green P on the front."),
    letter("J", "U2 F D2", "A blue J on the front."),
    letter("A", "R2 D2 L2 U2", "An A on the front and on the back."),
    letter("F", "U2 R2 F U2", "A green F on the front."),
    letter("S", "U2 F L2 B R2", "A green S on the front."),
    letter("Z", "R2 U F' U' R'", "A green Z on the front."),
    letter(
        "Plus",
        "U D' F2 B2 U' D",
        "A plus on the front and back, an I on each side.",
    ),
];

pub fn group(g: Group) -> Vec<&'static Pattern> {
    PATTERNS.iter().filter(|p| p.group == g).collect()
}

/// A sticker the editor leaves open: any color will do. (The gray of the
/// sticker palette.)
pub const ANY: u8 = 6;

pub const COLOR_NAMES: [&str; 6] = ["white", "red", "green", "yellow", "orange", "blue"];
const FACE_WORDS: [&str; 6] = ["top", "right", "front", "bottom", "left", "back"];
const CORNER_FACES: [[usize; 3]; 8] = [
    [0, 1, 2],
    [0, 2, 4],
    [0, 4, 5],
    [0, 5, 1],
    [3, 2, 1],
    [3, 4, 2],
    [3, 5, 4],
    [3, 1, 5],
];
const EDGE_FACES: [[usize; 2]; 12] = [
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

fn corner_place(i: usize) -> String {
    let [a, b, c] = CORNER_FACES[i].map(|f| FACE_WORDS[f]);
    format!("the {a}-{b}-{c} corner")
}

fn edge_place(i: usize) -> String {
    let [a, b] = EDGE_FACES[i].map(|f| FACE_WORDS[f]);
    format!("the {a}-{b} edge")
}

fn colors(cs: &[u8]) -> String {
    cs.iter()
        .map(|&c| COLOR_NAMES[c as usize])
        .collect::<Vec<_>>()
        .join("-")
}

fn fits_corner(f: &[u8; 54], pos: usize, piece: usize, twist: usize) -> bool {
    (0..3).all(|k| {
        let s = f[CORNER_FACELETS[pos][(k + twist) % 3]];
        s == ANY || s == CORNER_COLORS[piece][k]
    })
}

fn fits_edge(f: &[u8; 54], pos: usize, piece: usize, flip: usize) -> bool {
    (0..2).all(|k| {
        let s = f[EDGE_FACELETS[pos][(k + flip) % 2]];
        s == ANY || s == EDGE_COLORS[piece][k]
    })
}

/// Problems you can see without solving anything: too many stickers of a
/// color, or stickers that no single piece has. `None` if there are none.
pub fn quick_check(f: &[u8; 54]) -> Option<String> {
    for c in 0..6u8 {
        let n = f.iter().filter(|&&s| s == c).count();
        if n > 9 {
            return Some(format!(
                "{n} {} stickers: a cube has 9 of each color",
                COLOR_NAMES[c as usize]
            ));
        }
    }
    let known = |stickers: &[usize]| -> Vec<u8> {
        stickers
            .iter()
            .map(|&k| f[k])
            .filter(|&s| s != ANY)
            .collect()
    };
    for (pos, stickers) in CORNER_FACELETS.iter().enumerate() {
        if !(0..8).any(|p| (0..3).any(|t| fits_corner(f, pos, p, t))) {
            let known = known(stickers);
            let mirror = known.len() == 3 && !has_clash(&known);
            return Some(format!(
                "{}: no corner is {}{}",
                corner_place(pos),
                colors(&known),
                if mirror {
                    " in that order (it's a mirror image)"
                } else {
                    ""
                }
            ));
        }
    }
    for (pos, stickers) in EDGE_FACELETS.iter().enumerate() {
        if !(0..12).any(|p| (0..2).any(|fl| fits_edge(f, pos, p, fl))) {
            return Some(format!(
                "{}: no edge is {}",
                edge_place(pos),
                colors(&known(stickers))
            ));
        }
    }
    // a piece painted in full in two places
    let mut seen = [None; 8];
    for (pos, stickers) in CORNER_FACELETS.iter().enumerate() {
        if stickers.iter().any(|&k| f[k] == ANY) {
            continue;
        }
        if let Some(p) = (0..8).find(|&p| (0..3).any(|t| fits_corner(f, pos, p, t)))
            && let Some(other) = seen[p].replace(pos)
        {
            return Some(format!(
                "the {} corner is in two places ({} and {})",
                colors(&CORNER_COLORS[p]),
                corner_place(other).trim_start_matches("the "),
                corner_place(pos).trim_start_matches("the ")
            ));
        }
    }
    let mut seen = [None; 12];
    for (pos, stickers) in EDGE_FACELETS.iter().enumerate() {
        if stickers.iter().any(|&k| f[k] == ANY) {
            continue;
        }
        if let Some(p) = (0..12).find(|&p| (0..2).any(|fl| fits_edge(f, pos, p, fl)))
            && let Some(other) = seen[p].replace(pos)
        {
            return Some(format!(
                "the {} edge is in two places ({} and {})",
                colors(&EDGE_COLORS[p]),
                edge_place(other).trim_start_matches("the "),
                edge_place(pos).trim_start_matches("the ")
            ));
        }
    }
    None
}

/// Two colors on one piece that can't be: the same twice, or opposites.
fn has_clash(cs: &[u8]) -> bool {
    cs.iter()
        .enumerate()
        .any(|(i, &a)| cs[i + 1..].iter().any(|&b| a == b || a % 3 == b % 3))
}

/// Gives up after this many tries, so a hopeless painting can't hang.
const BUDGET: u32 = 3_000_000;

struct Fill<'a> {
    f: &'a [u8; 54],
    cube: Cube,
    used_corners: [bool; 8],
    used_edges: [bool; 12],
    corner_order: Vec<usize>,
    edge_order: Vec<usize>,
    budget: u32,
}

impl Fill<'_> {
    /// Place the corners from `k` on. `None` when out of tries.
    fn corners(&mut self, k: usize) -> Option<bool> {
        if k == 8 {
            let twist: u32 = self.cube.co.iter().map(|&x| x as u32).sum();
            return if twist.is_multiple_of(3) {
                self.edges(0)
            } else {
                Some(false)
            };
        }
        let pos = self.corner_order[k];
        // the piece that belongs here first, so open stickers stay solved
        for piece in std::iter::once(pos).chain((0..8).filter(|&p| p != pos)) {
            if self.used_corners[piece] {
                continue;
            }
            for twist in 0..3 {
                self.budget = self.budget.checked_sub(1)?;
                if !fits_corner(self.f, pos, piece, twist) {
                    continue;
                }
                self.used_corners[piece] = true;
                self.cube.cp[pos] = piece as u8;
                self.cube.co[pos] = twist as u8;
                if self.corners(k + 1)? {
                    return Some(true);
                }
                self.used_corners[piece] = false;
            }
        }
        Some(false)
    }

    fn edges(&mut self, k: usize) -> Option<bool> {
        if k == 12 {
            return Some(self.cube.is_solvable());
        }
        let pos = self.edge_order[k];
        for piece in std::iter::once(pos).chain((0..12).filter(|&p| p != pos)) {
            if self.used_edges[piece] {
                continue;
            }
            for flip in 0..2 {
                self.budget = self.budget.checked_sub(1)?;
                if !fits_edge(self.f, pos, piece, flip) {
                    continue;
                }
                self.used_edges[piece] = true;
                self.cube.ep[pos] = piece as u8;
                self.cube.eo[pos] = flip as u8;
                if self.edges(k + 1)? {
                    return Some(true);
                }
                self.used_edges[piece] = false;
            }
        }
        Some(false)
    }
}

/// Why a fully painted cube that passes `quick_check` still can't be made.
fn full_problem(f: &[u8; 54]) -> String {
    let mut c = SOLVED;
    for pos in 0..8 {
        if let Some((p, t)) = (0..8)
            .flat_map(|p| (0..3).map(move |t| (p, t)))
            .find(|&(p, t)| fits_corner(f, pos, p, t))
        {
            c.cp[pos] = p as u8;
            c.co[pos] = t as u8;
        }
    }
    for pos in 0..12 {
        if let Some((p, fl)) = (0..12)
            .flat_map(|p| (0..2).map(move |fl| (p, fl)))
            .find(|&(p, fl)| fits_edge(f, pos, p, fl))
        {
            c.ep[pos] = p as u8;
            c.eo[pos] = fl as u8;
        }
    }
    if c.co.iter().map(|&x| x as u32).sum::<u32>() % 3 != 0 {
        "a corner is twisted: turning faces can't make this, only taking a corner out could".into()
    } else if c.eo.iter().map(|&x| x as u32).sum::<u32>() % 2 != 0 {
        "an edge is flipped: turning faces can't make this, only taking an edge out could".into()
    } else {
        "two pieces are swapped: turning faces can't make this, only taking pieces out could".into()
    }
}

/// A real cube with these stickers (`ANY` stickers filled in, keeping as
/// many pieces at home as possible), or why there is none.
pub fn complete(f: &[u8; 54]) -> Result<Cube, String> {
    if (0..6).any(|face| f[face * 9 + 4] != face as u8) {
        return Err("the centers must stay white, red, green, yellow, orange, blue".into());
    }
    if let Some(problem) = quick_check(f) {
        return Err(problem);
    }
    let open = |stickers: &[usize]| stickers.iter().filter(|&&k| f[k] == ANY).count();
    // most painted pieces first: they have the fewest choices
    let mut corner_order: Vec<usize> = (0..8).collect();
    corner_order.sort_by_key(|&i| open(&CORNER_FACELETS[i]));
    let mut edge_order: Vec<usize> = (0..12).collect();
    edge_order.sort_by_key(|&i| open(&EDGE_FACELETS[i]));
    let mut fill = Fill {
        f,
        cube: SOLVED,
        used_corners: [false; 8],
        used_edges: [false; 12],
        corner_order,
        edge_order,
        budget: BUDGET,
    };
    match fill.corners(0) {
        Some(true) => Ok(fill.cube),
        _ if !f.contains(&ANY) => Err(full_problem(f)),
        Some(false) => Err(
            "no real cube has these stickers: a corner would end up twisted or an edge flipped. \
             Change a sticker or leave more open"
                .into(),
        ),
        None => Err("too many ways to fill the open stickers: paint a few more".into()),
    }
}

/// Whether a cube's stickers match a painting (`ANY` matches anything).
#[cfg(test)]
fn matches(stickers: &[u8; 54], painted: &[u8; 54]) -> bool {
    stickers
        .iter()
        .zip(painted)
        .all(|(&s, &p)| p == ANY || s == p)
}

/// A painted sticker to check: where it is on its piece, and its color.
#[derive(Clone, Copy)]
enum Check {
    /// Sticker `slot` of the corner in position `pos`.
    Corner {
        pos: usize,
        slot: usize,
        color: u8,
    },
    Edge {
        pos: usize,
        slot: usize,
        color: u8,
    },
}

impl Check {
    fn holds(self, c: &Cube) -> bool {
        match self {
            Check::Corner { pos, slot, color } => {
                let (piece, twist) = (c.cp[pos] as usize, c.co[pos] as usize);
                CORNER_COLORS[piece][(slot + 3 - twist) % 3] == color
            }
            Check::Edge { pos, slot, color } => {
                let (piece, flip) = (c.ep[pos] as usize, c.eo[pos] as usize);
                EDGE_COLORS[piece][(slot + flip) % 2] == color
            }
        }
    }
}

/// The painted stickers (centers left out: they never move).
fn checks(painted: &[u8; 54]) -> Vec<Check> {
    let mut out = Vec::new();
    for (pos, stickers) in CORNER_FACELETS.iter().enumerate() {
        for (slot, &k) in stickers.iter().enumerate() {
            if painted[k] != ANY {
                out.push(Check::Corner {
                    pos,
                    slot,
                    color: painted[k],
                });
            }
        }
    }
    for (pos, stickers) in EDGE_FACELETS.iter().enumerate() {
        for (slot, &k) in stickers.iter().enumerate() {
            if painted[k] != ANY {
                out.push(Check::Edge {
                    pos,
                    slot,
                    color: painted[k],
                });
            }
        }
    }
    out
}

/// The fewest moves (at most `max_len`) that turn a solved cube into one
/// that matches `painted`, or `None` if there are none that short or
/// `keep_going` said stop. Tries every sequence, shortest first, so it is
/// only quick for short ones: 7 moves take about a second.
pub fn find_short(
    painted: &[u8; 54],
    max_len: usize,
    keep_going: &dyn Fn() -> bool,
) -> Option<Vec<Move>> {
    struct Walk<'a> {
        checks: Vec<Check>,
        path: Vec<Move>,
        keep_going: &'a dyn Fn() -> bool,
        nodes: u32,
    }
    impl Walk<'_> {
        fn search(&mut self, c: Cube, depth: usize) -> Option<bool> {
            if depth == 0 {
                return Some(self.checks.iter().all(|k| k.holds(&c)));
            }
            self.nodes = self.nodes.wrapping_add(1);
            if self.nodes.is_multiple_of(4096) && !(self.keep_going)() {
                return None;
            }
            for m in Move::all() {
                // never the same face twice, and opposite faces in one order
                if let Some(last) = self.path.last()
                    && (m.face == last.face || (m.face % 3 == last.face % 3 && m.face < last.face))
                {
                    continue;
                }
                self.path.push(m);
                if self.search(c.apply(m), depth - 1)? {
                    return Some(true);
                }
                self.path.pop();
            }
            Some(false)
        }
    }
    let mut walk = Walk {
        checks: checks(painted),
        path: Vec::new(),
        keep_going,
        nodes: 0,
    };
    for depth in 0..=max_len {
        if walk.search(SOLVED, depth)? {
            return Some(walk.path);
        }
    }
    None
}

/// A way to make `state` from a solved cube, found at once (at most 21
/// moves).
pub fn first_way(state: &Cube) -> Vec<Move> {
    let solution = twophase::solve(state, 21).expect("every state has a 21-move solution");
    solution.iter().rev().map(|m| m.inverse()).collect()
}

/// Ways to make `state` from a solved cube, each shorter than the last:
/// the two-phase solver's first answer at once, then shorter ones for as
/// long as `keep_going` says so. `better` gets each one.
pub fn shorten(state: &Cube, keep_going: &dyn Fn() -> bool, better: &mut dyn FnMut(&[Move])) {
    let invert = |sol: Vec<Move>| -> Vec<Move> { sol.iter().rev().map(|m| m.inverse()).collect() };
    let mut best = invert(twophase::solve(state, 21).expect("every state has a 21-move solution"));
    better(&best);
    while let Some(len) = best.len().checked_sub(1) {
        match twophase::solve_while(state, len, keep_going) {
            Some(sol) => {
                best = invert(sol);
                better(&best);
            }
            None => break,
        }
    }
}

/// The shortest way to make `state` that `shorten` finds in `time`.
#[cfg(test)]
pub fn moves_to_make(state: &Cube, time: Duration) -> Vec<Move> {
    let deadline = Instant::now() + time;
    let mut best = Vec::new();
    shorten(state, &|| Instant::now() < deadline, &mut |m| {
        best = m.to_vec()
    });
    best
}

/// The moves written out.
pub fn format(moves: &[Move]) -> String {
    cube::format(moves)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Which stickers of a face show the letter (1) and which the
    /// background (0), row by row as you look at the face.
    fn shows(state: &Cube, face: usize, mask: [u8; 9]) -> bool {
        let f = state.facelets();
        let s = &f[face * 9..face * 9 + 9];
        let color = |bit| {
            let mut it = (0..9).filter(|&k| mask[k] == bit).map(|k| s[k]);
            let first = it.next();
            first.filter(|&c| it.all(|x| x == c))
        };
        matches!((color(1), color(0)), (Some(a), Some(b)) if a != b)
    }

    const R: usize = 1;
    const F: usize = 2;
    const B: usize = 5;

    #[test]
    fn every_preset_parses_and_makes_a_pattern() {
        for p in PATTERNS {
            assert!(alg::effect(p.alg).is_some(), "{} doesn't parse", p.name);
            assert_ne!(p.state(), SOLVED, "{} does nothing", p.name);
            assert_eq!(p.moves(), p.alg.split_whitespace().count(), "{}", p.name);
        }
    }

    #[test]
    fn preset_names_are_unique() {
        for (i, p) in PATTERNS.iter().enumerate() {
            assert!(
                PATTERNS[i + 1..]
                    .iter()
                    .all(|q| q.name != p.name && q.alg != p.alg),
                "{}",
                p.name
            );
        }
    }

    #[test]
    fn the_famous_ones_look_right() {
        let find = |name| PATTERNS.iter().find(|p| p.name == name).unwrap().state();
        // checkerboard: every face alternates its color and the opposite one
        let f = find("Checkerboard").facelets();
        for face in 0..6 {
            for k in 0..9 {
                let want = if k % 2 == 0 { face } else { (face + 3) % 6 };
                assert_eq!(f[face * 9 + k] as usize, want);
            }
        }
        // superflip: everything in place, every edge flipped
        let s = find("Superflip");
        assert_eq!((s.cp, s.co, s.ep), (SOLVED.cp, SOLVED.co, SOLVED.ep));
        assert!(s.eo.iter().all(|&x| x == 1));
        // six spots: each center differs from all the stickers around it
        let f = find("Six spots").facelets();
        for face in 0..6 {
            let around = f[face * 9];
            assert!((0..9).all(|k| k == 4 || f[face * 9 + k] == around));
            assert_ne!(around, f[face * 9 + 4]);
        }
        // four spots and wire leave top and bottom solved
        for name in ["Four spots", "Wire"] {
            let f = find(name).facelets();
            assert!(f[..9].iter().all(|&c| c == 0), "{name}");
            assert!(f[27..36].iter().all(|&c| c == 3), "{name}");
        }
    }

    #[test]
    fn letters_show_their_shape() {
        const HEART: [u8; 9] = [1, 0, 1, 1, 1, 1, 0, 1, 0];
        let cases: &[(&str, &[usize], [u8; 9])] = &[
            ("Heart", &[F, B], HEART),
            ("I", &[F, B], [1, 1, 1, 0, 1, 0, 1, 1, 1]),
            ("U", &[F, B], [1, 0, 1, 1, 0, 1, 1, 1, 1]),
            ("T", &[F, B], [1, 1, 1, 0, 1, 0, 0, 1, 0]),
            ("H", &[F, B], [1, 0, 1, 1, 1, 1, 1, 0, 1]),
            ("L", &[F], [1, 0, 0, 1, 0, 0, 1, 1, 1]),
            ("C", &[F, B], [1, 1, 1, 1, 0, 0, 1, 1, 1]),
            ("E", &[F, B], [1, 1, 1, 1, 1, 0, 1, 1, 1]),
            ("O", &[F, B], [1, 1, 1, 1, 0, 1, 1, 1, 1]),
            ("X", &[F, B], [1, 0, 1, 0, 1, 0, 1, 0, 1]),
            ("V", &[F, B], [1, 0, 1, 1, 0, 1, 0, 1, 0]),
            ("Y", &[F, B], [1, 0, 1, 0, 1, 0, 0, 1, 0]),
            ("P", &[F], [1, 1, 1, 1, 1, 1, 1, 0, 0]),
            ("J", &[F], [0, 0, 1, 0, 0, 1, 1, 1, 1]),
            ("A", &[F, B], [0, 1, 0, 1, 0, 1, 1, 1, 1]),
            ("F", &[F], [1, 1, 1, 1, 1, 0, 1, 0, 0]),
            ("S", &[F], [0, 1, 1, 0, 1, 0, 1, 1, 0]),
            ("Z", &[F], [1, 1, 0, 0, 1, 0, 0, 1, 1]),
            ("Plus", &[F, B], [0, 1, 0, 1, 1, 1, 0, 1, 0]),
            ("HI", &[F], [1, 0, 1, 1, 1, 1, 1, 0, 1]),
            ("HI", &[R], [1, 1, 1, 0, 1, 0, 1, 1, 1]),
            ("I ♥ U", &[F], [0, 1, 0, 0, 1, 0, 0, 1, 0]),
            ("I ♥ U", &[R], HEART),
            ("I ♥ U", &[B], [1, 0, 1, 1, 0, 1, 1, 1, 1]),
        ];
        let letters = group(Group::Letters);
        for (name, faces, mask) in cases {
            let p = letters.iter().find(|p| p.name == *name).expect(name);
            for &face in *faces {
                assert!(shows(&p.state(), face, *mask), "{name} on face {face}");
            }
        }
    }

    fn painted(state: &Cube) -> [u8; 54] {
        state.facelets()
    }

    #[test]
    fn a_full_painting_of_a_real_cube_completes_to_itself() {
        for p in PATTERNS {
            assert_eq!(complete(&painted(&p.state())), Ok(p.state()), "{}", p.name);
        }
    }

    #[test]
    fn open_stickers_stay_solved_where_they_can() {
        let mut f = [ANY; 54];
        for face in 0..6 {
            f[face * 9 + 4] = face as u8;
        }
        assert_eq!(complete(&f), Ok(SOLVED));
    }

    #[test]
    fn a_painted_heart_on_the_front_alone_completes() {
        let mut f = [ANY; 54];
        for face in 0..6 {
            f[face * 9 + 4] = face as u8;
        }
        // a green heart on red
        let heart = [1, 0, 1, 1, 1, 1, 0, 1, 0];
        for k in 0..9 {
            f[18 + k] = if heart[k] == 1 { 2 } else { 1 };
        }
        let c = complete(&f).unwrap();
        assert!(c.is_solvable());
        assert!(shows(&c, F, heart));
        let moves = moves_to_make(&c, Duration::from_millis(300));
        assert_eq!(SOLVED.apply_all(&moves), c);
    }

    #[test]
    fn impossible_paintings_say_why() {
        let solved = SOLVED.facelets();
        // a twisted corner
        let mut f = solved;
        let [a, b, c] = CORNER_FACELETS[0];
        (f[a], f[b], f[c]) = (solved[b], solved[c], solved[a]);
        assert!(complete(&f).unwrap_err().contains("twisted"));
        // a flipped edge
        let mut f = solved;
        let [a, b] = EDGE_FACELETS[0];
        f.swap(a, b);
        assert!(complete(&f).unwrap_err().contains("flipped"));
        // two edges swapped
        let mut f = solved;
        for (&a, &b) in EDGE_FACELETS[0].iter().zip(&EDGE_FACELETS[1]) {
            f.swap(a, b);
        }
        let e = complete(&f).unwrap_err();
        assert!(e.contains("swapped"), "{e}");
        // ten whites
        let mut f = solved;
        f[18] = 0;
        assert!(complete(&f).unwrap_err().contains("10 white"));
        // white and yellow on one edge
        let mut f = [ANY; 54];
        for face in 0..6 {
            f[face * 9 + 4] = face as u8;
        }
        f[EDGE_FACELETS[1][0]] = 0;
        f[EDGE_FACELETS[1][1]] = 3;
        let e = complete(&f).unwrap_err();
        assert!(e.contains("top-front edge: no edge is white-yellow"), "{e}");
        // a moved center
        let mut f = solved;
        f[4] = 1;
        assert!(complete(&f).is_err());
    }

    #[test]
    fn sticker_checks_agree_with_the_stickers() {
        use rand::SeedableRng;
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        for _ in 0..200 {
            let a = Cube::random(&mut rng);
            let b = Cube::random(&mut rng);
            let painted = a.facelets();
            assert!(checks(&painted).iter().all(|k| k.holds(&a)));
            let agree = checks(&painted).iter().all(|k| k.holds(&b));
            assert_eq!(agree, matches(&b.facelets(), &painted));
            for (k, check) in checks(&painted).into_iter().enumerate() {
                let sticker = match check {
                    Check::Corner { pos, slot, .. } => CORNER_FACELETS[pos][slot],
                    Check::Edge { pos, slot, .. } => EDGE_FACELETS[pos][slot],
                };
                assert_eq!(
                    check.holds(&b),
                    b.facelets()[sticker] == painted[sticker],
                    "check {k}"
                );
            }
        }
    }

    #[test]
    fn short_paintings_get_the_fewest_moves() {
        // a green X on blue, painted on the front alone: U2 D2 R2 L2
        let mut f = [ANY; 54];
        for face in 0..6 {
            f[face * 9 + 4] = face as u8;
        }
        let x = [1, 0, 1, 0, 1, 0, 1, 0, 1];
        for k in 0..9 {
            f[18 + k] = if x[k] == 1 { 2 } else { 5 };
        }
        let moves = find_short(&f, 6, &|| true).expect("4 moves");
        assert_eq!(moves.len(), 4);
        assert!(matches(&SOLVED.apply_all(&moves).facelets(), &f));
        // nothing that short for a scramble
        let far = SOLVED
            .apply_all(&cube::parse("R U F D L B R").unwrap())
            .facelets();
        assert_eq!(find_short(&far, 4, &|| true), None);
        // and it stops when told to
        assert_eq!(find_short(&far, 7, &|| false), None);
    }

    #[test]
    fn moves_to_make_makes_the_state() {
        for p in PATTERNS.iter().take(6) {
            let moves = moves_to_make(&p.state(), Duration::from_millis(200));
            assert_eq!(SOLVED.apply_all(&moves), p.state(), "{}", p.name);
            assert!(moves.len() <= 21);
        }
        // a short pattern gets a short answer
        let moves = moves_to_make(
            &SOLVED.apply_all(&cube::parse("R U").unwrap()),
            Duration::from_millis(500),
        );
        assert_eq!(moves.len(), 2);
        assert!(moves_to_make(&SOLVED, Duration::ZERO).is_empty());
    }
}
