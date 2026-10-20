//! Algorithms in the notation speedcubers write them in.
//!
//! Besides face turns (`R U' F2`) algorithms use wide turns (`r`, `Rw`),
//! slice turns (`M` `E` `S`) and whole-cube rotations (`x` `y` `z`).
//! `parse` turns all of those into layer turns of a 3x3 sticker model, and
//! `effect` gives what an algorithm does to the pieces, with the cube turned
//! back so the centers are where they started.

use crate::anim::Step;
use crate::cube::Cube;
#[cfg(test)]
use crate::cube::SOLVED;
use crate::nxn::{Move, Nxn};

/// Face numbers in `nxn` order.
const U: u8 = 0;
const R: u8 = 1;
const F: u8 = 2;
const D: u8 = 3;
const L: u8 = 4;

/// Parse an algorithm into layer turns of a 3x3. Brackets, parentheses and
/// extra spaces are ignored; `2'` is the same as `2`. `None` if any token
/// isn't a move.
pub fn parse(alg: &str) -> Option<Vec<Move>> {
    let cleaned: String = alg
        .chars()
        .map(|c| if "()[]".contains(c) { ' ' } else { c })
        .collect();
    let mut out = Vec::new();
    for token in cleaned.split_whitespace() {
        out.extend(parse_token(token)?);
    }
    Some(out)
}

fn parse_token(token: &str) -> Option<Vec<Move>> {
    // the amount: nothing, 2, 2' (same as 2) or '
    let (body, turns) = if let Some(b) = token.strip_suffix("2'") {
        (b, 2)
    } else if let Some(b) = token.strip_suffix('2') {
        (b, 2)
    } else if let Some(b) = token.strip_suffix('\'') {
        (b, 3)
    } else {
        (token, 1)
    };
    let face = |c: char| "URFDLB".find(c).map(|i| i as u8);
    let turn = |face, layers, turns| Move {
        face,
        layers,
        turns,
    };
    let moves = match body {
        // face turns
        b if b.len() == 1 && face(b.chars().next()?).is_some() => {
            vec![turn(face(b.chars().next()?)?, 1, turns)]
        }
        // wide turns: Rw or r
        b if b.len() == 2 && b.ends_with('w') => vec![turn(face(b.chars().next()?)?, 2, turns)],
        b if b.len() == 1 && "urfdlb".contains(b) => {
            vec![turn(
                face(b.to_ascii_uppercase().chars().next()?)?,
                2,
                turns,
            )]
        }
        // rotations turn every layer: x like R, y like U, z like F
        "x" => vec![turn(R, 3, turns)],
        "y" => vec![turn(U, 3, turns)],
        "z" => vec![turn(F, 3, turns)],
        // slices: the middle layer only, turning like L (M), D (E) or F (S)
        "M" => vec![turn(L, 2, turns), turn(L, 1, 4 - turns)],
        "E" => vec![turn(D, 2, turns), turn(D, 1, 4 - turns)],
        "S" => vec![turn(F, 2, turns), turn(F, 1, 4 - turns)],
        _ => return None,
    };
    Some(moves)
}

/// An algorithm as named steps for the 3D player: each token keeps its label
/// (`M`, `x`, `Rw'`), with the layer turns it stands for.
pub fn steps(alg: &str) -> Option<Vec<Step>> {
    let cleaned: String = alg
        .chars()
        .map(|c| if "()[]".contains(c) { ' ' } else { c })
        .collect();
    cleaned
        .split_whitespace()
        .map(|t| {
            Some(Step {
                label: t.to_string(),
                moves: parse_token(t)?,
            })
        })
        .collect()
}

/// The moves that undo `moves`.
#[cfg(test)]
fn invert(moves: &[Move]) -> Vec<Move> {
    moves.iter().rev().map(|m| m.inverse()).collect()
}

/// The 24 whole-cube orientations, each as the rotations that reach it.
fn orientations() -> Vec<Vec<Move>> {
    let x = Move {
        face: R,
        layers: 3,
        turns: 1,
    };
    let y = Move {
        face: U,
        layers: 3,
        turns: 1,
    };
    let z = Move {
        face: F,
        layers: 3,
        turns: 1,
    };
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut out: Vec<Vec<Move>> = Vec::new();
    let mut queue: std::collections::VecDeque<Vec<Move>> = [vec![]].into();
    while let Some(path) = queue.pop_front() {
        let mut c = Nxn::solved(3);
        c.apply_all(&path);
        let centers: Vec<u8> = (0..6).map(|f| c.facelets()[f * 9 + 4]).collect();
        if seen.contains(&centers) {
            continue;
        }
        seen.push(centers);
        for r in [x, y, z] {
            let mut next = path.clone();
            next.push(r);
            queue.push_back(next);
        }
        out.push(path);
    }
    out
}

/// A 3x3 after `moves`, turned as a whole so its centers are back where they
/// started, as pieces. `None` only if `moves` isn't valid for a 3x3.
pub fn state_after(moves: &[Move]) -> Option<Cube> {
    let mut c = Nxn::solved(3);
    c.apply_all(moves);
    for rotation in orientations() {
        let mut turned = c.clone();
        turned.apply_all(&rotation);
        let f = turned.facelets();
        if (0..6).all(|face| f[face * 9 + 4] == face as u8) {
            return Cube::from_facelets(&f);
        }
    }
    None
}

/// What `alg` does to a solved cube, as pieces. `None` if it doesn't parse.
pub fn effect(alg: &str) -> Option<Cube> {
    state_after(&parse(alg)?)
}

/// The state an algorithm solves: the inverse of what it does.
///
/// Not the reversed moves applied to a solved cube: an algorithm with a
/// rotation ends with the cube held differently, and its reversed moves
/// only undo it when done in that frame. The inverse of the measured effect
/// doesn't depend on the frame.
pub fn case_of(alg: &str) -> Option<Cube> {
    Some(effect(alg)?.inverse())
}

/// Whether two algorithms do the same thing to the pieces.
#[cfg(test)]
pub fn same_effect(a: &str, b: &str) -> bool {
    matches!((effect(a), effect(b)), (Some(x), Some(y)) if x == y)
}

/// Whether `alg` leaves a solved cube solved (up to how it is held).
#[cfg(test)]
fn is_identity(alg: &str) -> bool {
    effect(alg) == Some(SOLVED)
}

/// `nxn` moves written out, for showing parsed algorithms in tests.
#[cfg(test)]
fn show(moves: &[Move]) -> String {
    crate::nxn::format(moves)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cube;

    #[test]
    fn plain_face_turns_match_the_piece_model() {
        for s in ["R", "U'", "F2", "R U R' U'", "D L2 B' R U2"] {
            let piece = SOLVED.apply_all(&cube::parse(s).unwrap());
            assert_eq!(effect(s), Some(piece), "{s}");
        }
    }

    #[test]
    fn notation_variants_parse_the_same() {
        assert_eq!(parse("Rw U2"), parse("r U2"));
        assert_eq!(parse("(R U R' U')"), parse("R U R' U'"));
        assert_eq!(parse("[R U] R'"), parse("R U R'"));
        assert_eq!(parse("R2'"), parse("R2"));
        assert_eq!(parse("  R   U  "), parse("R U"));
        assert_eq!(parse("").unwrap(), vec![]);
        for bad in ["X", "R3", "Rww", "rw", "Q", "M3", "2R", "R''"] {
            assert!(parse(bad).is_none(), "{bad:?}");
        }
    }

    // Identities every cuber knows: if any of these fail, the notation is
    // wrong somewhere.
    #[test]
    fn rotations_and_slices_satisfy_the_standard_identities() {
        // a rotation alone changes nothing about the pieces
        for r in ["x", "y", "z", "x'", "y2", "z'", "x y z"] {
            assert!(is_identity(r), "{r}");
        }
        // r is R plus the middle slice turning with it: r = R M'
        assert!(same_effect("r", "R M'"));
        assert!(same_effect("l", "L M"));
        assert!(same_effect("u", "U E'"));
        assert!(same_effect("d", "D E"));
        assert!(same_effect("f", "F S"));
        assert!(same_effect("b", "B S'"));
        // a rotation is the same as turning all three layers
        assert!(same_effect("x", "R M' L'"));
        assert!(same_effect("y", "U E' D'"));
        assert!(same_effect("z", "F S B'"));
        // after a rotation a face letter means another face: y turns the
        // cube like U, so the old back is now on the right; x turns it like
        // R, so the old front is on top; z like F, so the old left is on top
        assert!(same_effect("y R y'", "B"));
        assert!(same_effect("x U x'", "F"));
        assert!(same_effect("z U z'", "L"));
        // the M slice turns like L: M U M' moves the middle-layer edges up
        assert!(same_effect("M", "L' R x"));
    }

    #[test]
    fn well_known_sequences_do_what_they_should() {
        // H-perm: swaps opposite edges on top, nothing else
        let h = effect("M2 U M2 U2 M2 U M2").unwrap();
        assert_eq!(h.cp, SOLVED.cp, "corners untouched");
        assert_eq!(h.co, SOLVED.co);
        assert_eq!(&h.ep[..4], &[2, 3, 0, 1], "UR<->UL, UF<->UB");
        assert_eq!(&h.ep[4..], &SOLVED.ep[4..]);
        assert_eq!(h.eo, SOLVED.eo);
        // the sexy move six times is the identity
        assert!(is_identity(
            "R U R' U' R U R' U' R U R' U' R U R' U' R U R' U' R U R' U'"
        ));
        // an algorithm and its inverse cancel
        let alg = "x R' U R' D2 R U' R' D2 R2 x'";
        let moves = parse(alg).unwrap();
        let mut both = moves.clone();
        both.extend(invert(&moves));
        assert_eq!(state_after(&both), Some(SOLVED));
    }

    #[test]
    fn steps_keep_the_written_names_and_add_up_to_the_algorithm() {
        let alg = "(R U2) M' x' y2 Rw'";
        let steps = steps(alg).unwrap();
        let labels: Vec<&str> = steps.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(labels, ["R", "U2", "M'", "x'", "y2", "Rw'"]);
        let flat: Vec<Move> = steps.iter().flat_map(|s| s.moves.clone()).collect();
        assert_eq!(Some(flat), parse(alg));
        assert_eq!(steps[2].moves.len(), 2, "a slice is two layer turns");
        assert!(super::steps("R Q").is_none());
    }

    #[test]
    fn there_are_24_orientations() {
        assert_eq!(orientations().len(), 24);
    }

    // The bug the first version had: a case computed by undoing the moves
    // from a cube held normally was wrong for algorithms with a rotation.
    #[test]
    fn cases_of_algorithms_with_rotations_are_solved_by_them() {
        for a in [
            "y' R' U R",
            "y' U R' U R U' R' U' R",
            "y R U R' y'",
            "x R' U R' D2 R U' R' D2 R2 x'",
            "y2 M2 U M U2 M' U M2",
            "z U R' z' U2",
        ] {
            let (case, eff) = (case_of(a).unwrap(), effect(a).unwrap());
            assert_eq!(case.multiply(&eff), SOLVED, "{a}");
        }
        // y' R' U R takes the front-right pair out of its slot
        let case = case_of("y' R' U R").unwrap();
        assert_ne!(case.cp[4], 4);
        assert_eq!(
            (case.cp[7], case.ep[11]),
            (7, 11),
            "the back-right slot is untouched"
        );
    }

    #[test]
    fn the_case_is_the_algorithm_undone() {
        let alg = "R U R' U' R' F R2 U' R' U' R U R' F'"; // T-perm
        let case = case_of(alg).unwrap();
        assert_eq!(case.multiply(&effect(alg).unwrap()), SOLVED);
        assert!(show(&parse("M").unwrap()).contains("Lw"));
    }
}
