//! WCA-style random-move 3x3 scrambles.

use rand::{Rng, RngExt};

const FACES: [char; 6] = ['U', 'R', 'F', 'D', 'L', 'B'];
const MODIFIERS: [&str; 3] = ["", "'", "2"];
pub const LENGTH: usize = 20;

/// U/D, R/L and F/B share an axis; faces on one axis commute.
fn axis(face: usize) -> usize {
    face % 3
}

pub fn generate() -> String {
    generate_with(&mut rand::rng())
}

/// 20 moves, never the same face twice in a row, and never three moves on one
/// axis in a row (`R L R`).
pub fn generate_with(rng: &mut impl Rng) -> String {
    let mut moves: Vec<String> = Vec::with_capacity(LENGTH);
    let mut history: Vec<usize> = Vec::with_capacity(LENGTH);
    while history.len() < LENGTH {
        let face = rng.random_range(0..FACES.len());
        if let Some(&last) = history.last() {
            let same_axis = axis(face) == axis(last);
            let prev_same_axis = history.len() >= 2 && axis(history[history.len() - 2]) == axis(last);
            if face == last || (same_axis && prev_same_axis) {
                continue;
            }
        }
        history.push(face);
        let modifier = MODIFIERS[rng.random_range(0..MODIFIERS.len())];
        moves.push(format!("{}{modifier}", FACES[face]));
    }
    moves.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn faces(s: &str) -> Vec<usize> {
        s.split(' ')
            .map(|m| FACES.iter().position(|&f| f == m.chars().next().unwrap()).unwrap())
            .collect()
    }

    #[test]
    fn correct_length_and_notation() {
        let mut rng = StdRng::seed_from_u64(1);
        for _ in 0..200 {
            let s = generate_with(&mut rng);
            let moves: Vec<&str> = s.split(' ').collect();
            assert_eq!(moves.len(), LENGTH);
            for m in moves {
                assert!(matches!(m.len(), 1 | 2), "{m}");
                assert!(FACES.contains(&m.chars().next().unwrap()), "{m}");
                assert!(m.len() == 1 || m.ends_with('\'') || m.ends_with('2'), "{m}");
            }
        }
    }

    #[test]
    fn no_same_face_twice_in_a_row() {
        let mut rng = StdRng::seed_from_u64(2);
        for _ in 0..500 {
            let f = faces(&generate_with(&mut rng));
            assert!(f.windows(2).all(|w| w[0] != w[1]));
        }
    }

    #[test]
    fn no_three_moves_on_one_axis() {
        let mut rng = StdRng::seed_from_u64(3);
        for _ in 0..500 {
            let f = faces(&generate_with(&mut rng));
            assert!(
                f.windows(3).all(|w| !(axis(w[0]) == axis(w[1]) && axis(w[1]) == axis(w[2]))),
                "{f:?}"
            );
        }
    }

    #[test]
    fn scrambles_differ() {
        assert_ne!(generate(), generate());
    }

    #[test]
    fn every_face_and_modifier_appears() {
        let mut rng = StdRng::seed_from_u64(4);
        let all: String = (0..50).map(|_| generate_with(&mut rng)).collect::<Vec<_>>().join(" ");
        for f in FACES {
            assert!(all.contains(f));
        }
        assert!(all.contains('\'') && all.contains('2'));
    }
}
