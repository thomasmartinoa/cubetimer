//! The puzzles cubetimer can time.

/// An N×N×N cube event, from 2x2 to 7x7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Puzzle {
    Cube2,
    Cube3,
    Cube4,
    Cube5,
    Cube6,
    Cube7,
}

pub const ALL: [Puzzle; 6] = [
    Puzzle::Cube2,
    Puzzle::Cube3,
    Puzzle::Cube4,
    Puzzle::Cube5,
    Puzzle::Cube6,
    Puzzle::Cube7,
];

/// What sessions saved before puzzles existed are.
pub const DEFAULT_ID: &str = "333";

impl Puzzle {
    /// The WCA event id, which is also what is saved to disk.
    pub fn id(self) -> &'static str {
        match self {
            Puzzle::Cube2 => "222",
            Puzzle::Cube3 => "333",
            Puzzle::Cube4 => "444",
            Puzzle::Cube5 => "555",
            Puzzle::Cube6 => "666",
            Puzzle::Cube7 => "777",
        }
    }

    pub fn from_id(id: &str) -> Option<Puzzle> {
        ALL.into_iter().find(|p| p.id() == id)
    }

    /// Cubes per side.
    pub fn size(self) -> usize {
        match self {
            Puzzle::Cube2 => 2,
            Puzzle::Cube3 => 3,
            Puzzle::Cube4 => 4,
            Puzzle::Cube5 => 5,
            Puzzle::Cube6 => 6,
            Puzzle::Cube7 => 7,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Puzzle::Cube2 => "2x2",
            Puzzle::Cube3 => "3x3",
            Puzzle::Cube4 => "4x4",
            Puzzle::Cube5 => "5x5",
            Puzzle::Cube6 => "6x6",
            Puzzle::Cube7 => "7x7",
        }
    }

    /// The puzzle called `id` on disk; an unknown id is treated as 3x3 so a
    /// file from a newer version still opens.
    pub fn from_id_or_default(id: &str) -> Puzzle {
        Puzzle::from_id(id).unwrap_or(Puzzle::Cube3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_are_unique() {
        for p in ALL {
            assert_eq!(Puzzle::from_id(p.id()), Some(p));
            assert_eq!(
                p.id().chars().next().unwrap().to_digit(10).unwrap() as usize,
                p.size()
            );
        }
        assert!(
            ALL.iter()
                .enumerate()
                .all(|(i, a)| ALL[..i].iter().all(|b| a.id() != b.id()))
        );
        assert_eq!(Puzzle::from_id("999"), None);
        assert_eq!(Puzzle::from_id(""), None);
    }

    #[test]
    fn default_id_is_the_3x3() {
        assert_eq!(Puzzle::from_id(DEFAULT_ID), Some(Puzzle::Cube3));
        assert_eq!(Puzzle::from_id_or_default("from-the-future"), Puzzle::Cube3);
    }

    #[test]
    fn names_match_sizes() {
        for p in ALL {
            assert_eq!(p.name(), format!("{0}x{0}", p.size()));
        }
    }
}
