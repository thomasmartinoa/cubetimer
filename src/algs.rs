//! The CFOP algorithm library: every PLL, OLL and F2L case with a standard
//! algorithm, and the beginner two-look subsets.
//!
//! Nothing here is trusted on sight. The tests check every algorithm against
//! the cube model: it must leave the first two layers alone (or, for F2L, the
//! cross and the other three slots), and the algorithms of a set must solve
//! every possible case of that set exactly once. The counts are fixed by the
//! cube: 288 last-layer permutations fall into 21 PLL cases plus solved, 216
//! orientation patterns into 57 OLL cases plus solved, and the 150 places the
//! front-right pair can be in, into 41 F2L cases plus solved.

use crate::alg;
use crate::cube::{Cube, Move, SOLVED};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Set {
    /// Permute the last layer.
    Pll,
    /// Orient the last layer.
    Oll,
    /// Solve a corner-edge pair into the front-right slot.
    F2l,
    /// Beginner OLL in two steps: orient the edges, then the corners.
    TwoLookOll,
    /// Beginner PLL in two steps: permute the corners, then the edges.
    TwoLookPll,
}

impl Set {
    pub const ALL: [Set; 5] = [
        Set::Pll,
        Set::Oll,
        Set::F2l,
        Set::TwoLookOll,
        Set::TwoLookPll,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Set::Pll => "PLL",
            Set::Oll => "OLL",
            Set::F2l => "F2L",
            Set::TwoLookOll => "2-look OLL",
            Set::TwoLookPll => "2-look PLL",
        }
    }

    pub fn cases(self) -> &'static [Case] {
        match self {
            Set::Pll => PLL,
            Set::Oll => OLL,
            Set::F2l => F2L,
            Set::TwoLookOll => TWO_LOOK_OLL,
            Set::TwoLookPll => TWO_LOOK_PLL,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Case {
    pub set: Set,
    /// The case's usual name: "T", "21", ...
    pub name: &'static str,
    pub group: &'static str,
    pub alg: &'static str,
}

impl Case {
    /// The full name: "T-perm", "OLL 21", "F2L 5", ...
    pub fn title(&self) -> String {
        match self.set {
            Set::Pll => format!("{}-perm", self.name),
            Set::Oll => format!("OLL {}", self.name),
            Set::F2l => format!("F2L {}", self.name),
            Set::TwoLookOll | Set::TwoLookPll => self.name.to_string(),
        }
    }

    /// The case's group. For F2L it comes from where the pair is, so it is
    /// always right: both pieces on top, one of them in the slot, or both.
    pub fn group_name(&self) -> &'static str {
        if self.set != Set::F2l {
            return self.group;
        }
        let s = self.state();
        let corner_in_slot = s.cp[4] == 4;
        let edge_in_slot = s.ep[8] == 8;
        match (corner_in_slot, edge_in_slot) {
            (false, false) => "Both pieces on top",
            (true, false) => "Corner in the slot",
            (false, true) => "Edge in the slot",
            (true, true) => "Both pieces in the slot",
        }
    }

    /// A key that is unique across all sets, for saving progress.
    pub fn key(&self) -> String {
        format!("{}:{}", self.set.name(), self.name)
    }

    /// The state this case's algorithm solves.
    pub fn state(&self) -> Cube {
        alg::case_of(self.alg).expect("every algorithm parses (tested)")
    }
}

/// Every case of every set, for looking up by key.
#[cfg(test)]
pub fn all_cases() -> impl Iterator<Item = &'static Case> {
    Set::ALL.iter().flat_map(|s| s.cases().iter())
}

#[cfg(test)]
pub fn find(key: &str) -> Option<&'static Case> {
    all_cases().find(|c| c.key() == key)
}

/// The U turn, to adjust the last layer.
pub fn u_turns(k: usize) -> Cube {
    (0..k % 4).fold(SOLVED, |c, _| c.apply(Move::from_index(0)))
}

/// What the first two layers look like: every D-layer and middle-layer piece
/// in place and oriented.
#[cfg(test)]
pub fn f2l_solved(c: &Cube) -> bool {
    (4..8).all(|i| c.cp[i] == i as u8 && c.co[i] == 0)
        && (4..12).all(|i| c.ep[i] == i as u8 && c.eo[i] == 0)
}

/// Every last-layer piece oriented (whatever its place).
#[cfg(test)]
pub fn ll_oriented(c: &Cube) -> bool {
    (0..4).all(|i| c.co[i] == 0 && c.eo[i] == 0)
}

static TWO_LOOK_OLL: &[Case] = &[
    Case {
        set: Set::TwoLookOll,
        name: "Edges: line",
        group: "Orient the edges",
        alg: "F R U R' U' F'",
    },
    Case {
        set: Set::TwoLookOll,
        name: "Edges: L",
        group: "Orient the edges",
        alg: "f R U R' U' f'",
    },
    Case {
        set: Set::TwoLookOll,
        name: "Edges: dot",
        group: "Orient the edges",
        alg: "F R U R' U' F' f R U R' U' f'",
    },
    Case {
        set: Set::TwoLookOll,
        name: "Corners: Sune",
        group: "Orient the corners",
        alg: "R U R' U R U2 R'",
    },
    Case {
        set: Set::TwoLookOll,
        name: "Corners: Antisune",
        group: "Orient the corners",
        alg: "y R U2 R' U' R U' R'",
    },
    Case {
        set: Set::TwoLookOll,
        name: "Corners: H",
        group: "Orient the corners",
        alg: "R U R' U R U' R' U R U2 R'",
    },
    Case {
        set: Set::TwoLookOll,
        name: "Corners: Pi",
        group: "Orient the corners",
        alg: "R U2 R2 U' R2 U' R2 U2 R",
    },
    Case {
        set: Set::TwoLookOll,
        name: "Corners: Headlights",
        group: "Orient the corners",
        alg: "R2 D R' U2 R D' R' U2 R'",
    },
    Case {
        set: Set::TwoLookOll,
        name: "Corners: T",
        group: "Orient the corners",
        alg: "r U R' U' r' F R F'",
    },
    Case {
        set: Set::TwoLookOll,
        name: "Corners: Bowtie",
        group: "Orient the corners",
        alg: "R U2 R D R' U2 R D' R2",
    },
];

static TWO_LOOK_PLL: &[Case] = &[
    Case {
        set: Set::TwoLookPll,
        name: "Corners: headlights",
        group: "Permute the corners",
        alg: "R U R' U' R' F R2 U' R' U' R U R' F'",
    },
    Case {
        set: Set::TwoLookPll,
        name: "Corners: diagonal",
        group: "Permute the corners",
        alg: "F R U' R' U' R U R' F' R U R' U' R' F R F'",
    },
    Case {
        set: Set::TwoLookPll,
        name: "Edges: Ua",
        group: "Permute the edges",
        alg: "y2 M2 U M U2 M' U M2",
    },
    Case {
        set: Set::TwoLookPll,
        name: "Edges: Ub",
        group: "Permute the edges",
        alg: "y2 M2 U' M U2 M' U' M2",
    },
    Case {
        set: Set::TwoLookPll,
        name: "Edges: H",
        group: "Permute the edges",
        alg: "M2 U M2 U2 M2 U M2",
    },
    Case {
        set: Set::TwoLookPll,
        name: "Edges: Z",
        group: "Permute the edges",
        alg: "M' U' M2 U' M2 U' M' U2 M2",
    },
];

// Algorithms from SpeedCubeDB (speedcubedb.com/a/3x3), the first listed
// for each case. Every one is checked by the tests below.
pub static PLL: &[Case] = &[
    Case {
        set: Set::Pll,
        name: "Aa",
        group: "Corners only",
        alg: "x R' U R' D2 R U' R' D2 R2 x'",
    },
    Case {
        set: Set::Pll,
        name: "Ab",
        group: "Corners only",
        alg: "x R2 D2 R U R' D2 R U' R x'",
    },
    Case {
        set: Set::Pll,
        name: "E",
        group: "Corners only",
        alg: "y x' R U' R' D R U R' D' R U R' D R U' R' D' x",
    },
    Case {
        set: Set::Pll,
        name: "F",
        group: "Adjacent corner swap",
        alg: "y R' U' F' R U R' U' R' F R2 U' R' U' R U R' U R",
    },
    Case {
        set: Set::Pll,
        name: "Ga",
        group: "G perms",
        alg: "R2 U R' U R' U' R U' R2 D U' R' U R D'",
    },
    Case {
        set: Set::Pll,
        name: "Gb",
        group: "G perms",
        alg: "D R' U' R U D' R2 U R' U R U' R U' R2",
    },
    Case {
        set: Set::Pll,
        name: "Gc",
        group: "G perms",
        alg: "R2 U' R U' R U R' U R2 D' U R U' R' D",
    },
    Case {
        set: Set::Pll,
        name: "Gd",
        group: "G perms",
        alg: "R U R' U' D R2 U' R U' R' U R' U R2 D'",
    },
    Case {
        set: Set::Pll,
        name: "H",
        group: "Edges only",
        alg: "M2 U M2 U2 M2 U M2",
    },
    Case {
        set: Set::Pll,
        name: "Ja",
        group: "Adjacent corner swap",
        alg: "y2 x R2 F R F' R U2 r' U r U2 x'",
    },
    Case {
        set: Set::Pll,
        name: "Jb",
        group: "Adjacent corner swap",
        alg: "R U R' F' R U R' U' R' F R2 U' R'",
    },
    Case {
        set: Set::Pll,
        name: "Na",
        group: "Diagonal corner swap",
        alg: "R U R' U R U R' F' R U R' U' R' F R2 U' R' U2 R U' R'",
    },
    Case {
        set: Set::Pll,
        name: "Nb",
        group: "Diagonal corner swap",
        alg: "R' U R U' R' F' U' F R U R' F R' F' R U' R",
    },
    Case {
        set: Set::Pll,
        name: "Ra",
        group: "Adjacent corner swap",
        alg: "y R U' R' U' R U R D R' U' R D' R' U2 R'",
    },
    Case {
        set: Set::Pll,
        name: "Rb",
        group: "Adjacent corner swap",
        alg: "R' U2 R U2 R' F R U R' U' R' F' R2",
    },
    Case {
        set: Set::Pll,
        name: "T",
        group: "Adjacent corner swap",
        alg: "R U R' U' R' F R2 U' R' U' R U R' F'",
    },
    Case {
        set: Set::Pll,
        name: "Ua",
        group: "Edges only",
        alg: "y2 M2 U M U2 M' U M2",
    },
    Case {
        set: Set::Pll,
        name: "Ub",
        group: "Edges only",
        alg: "y2 M2 U' M U2 M' U' M2",
    },
    Case {
        set: Set::Pll,
        name: "V",
        group: "Diagonal corner swap",
        alg: "R' U R' U' R D' R' D R' U D' R2 U' R2 D R2",
    },
    Case {
        set: Set::Pll,
        name: "Y",
        group: "Diagonal corner swap",
        alg: "F R U' R' U' R U R' F' R U R' U' R' F R F'",
    },
    Case {
        set: Set::Pll,
        name: "Z",
        group: "Edges only",
        alg: "M' U' M2 U' M2 U' M' U2 M2",
    },
];

pub static OLL: &[Case] = &[
    Case {
        set: Set::Oll,
        name: "1",
        group: "Dot",
        alg: "R U2 R2 F R F' U2 R' F R F'",
    },
    Case {
        set: Set::Oll,
        name: "2",
        group: "Dot",
        alg: "y' R U' R2 D' r U r' D R2 U R'",
    },
    Case {
        set: Set::Oll,
        name: "3",
        group: "Dot",
        alg: "y' f R U R' U' f' U' F R U R' U' F'",
    },
    Case {
        set: Set::Oll,
        name: "4",
        group: "Dot",
        alg: "y' R' F2 R2 U2 R' F' R U2 R2 F2 R",
    },
    Case {
        set: Set::Oll,
        name: "5",
        group: "Square",
        alg: "r' U2 R U R' U r",
    },
    Case {
        set: Set::Oll,
        name: "6",
        group: "Square",
        alg: "r U2 R' U' R U' r'",
    },
    Case {
        set: Set::Oll,
        name: "7",
        group: "Lightning",
        alg: "r U R' U R U2 r'",
    },
    Case {
        set: Set::Oll,
        name: "8",
        group: "Lightning",
        alg: "y2 r' U' R U' R' U2 r",
    },
    Case {
        set: Set::Oll,
        name: "9",
        group: "Fish",
        alg: "y R U R' U' R' F R2 U R' U' F'",
    },
    Case {
        set: Set::Oll,
        name: "10",
        group: "Fish",
        alg: "R U R' U R' F R F' R U2 R'",
    },
    Case {
        set: Set::Oll,
        name: "11",
        group: "Lightning",
        alg: "r' R2 U R' U R U2 R' U M'",
    },
    Case {
        set: Set::Oll,
        name: "12",
        group: "Lightning",
        alg: "y' M' R' U' R U' R' U2 R U' M",
    },
    Case {
        set: Set::Oll,
        name: "13",
        group: "Knight move",
        alg: "F U R U2 R' U' R U R' F'",
    },
    Case {
        set: Set::Oll,
        name: "14",
        group: "Knight move",
        alg: "R' F R U R' F' R F U' F'",
    },
    Case {
        set: Set::Oll,
        name: "15",
        group: "Knight move",
        alg: "r' U' r R' U' R U r' U r",
    },
    Case {
        set: Set::Oll,
        name: "16",
        group: "Knight move",
        alg: "r U r' R U R' U' r U' r'",
    },
    Case {
        set: Set::Oll,
        name: "17",
        group: "Dot",
        alg: "R U R' U R' F R F' U2 R' F R F'",
    },
    Case {
        set: Set::Oll,
        name: "18",
        group: "Dot",
        alg: "y R U2 R2 F R F' U2 M' U R U' r'",
    },
    Case {
        set: Set::Oll,
        name: "19",
        group: "Dot",
        alg: "y S' R U R' S U' R' F R F'",
    },
    Case {
        set: Set::Oll,
        name: "20",
        group: "Dot",
        alg: "r U R' U' M2 U R U' R' U' M'",
    },
    Case {
        set: Set::Oll,
        name: "21",
        group: "Cross (OCLL)",
        alg: "R U R' U R U' R' U R U2 R'",
    },
    Case {
        set: Set::Oll,
        name: "22",
        group: "Cross (OCLL)",
        alg: "R U2 R2 U' R2 U' R2 U2 R",
    },
    Case {
        set: Set::Oll,
        name: "23",
        group: "Cross (OCLL)",
        alg: "R2 D R' U2 R D' R' U2 R'",
    },
    Case {
        set: Set::Oll,
        name: "24",
        group: "Cross (OCLL)",
        alg: "r U R' U' r' F R F'",
    },
    Case {
        set: Set::Oll,
        name: "25",
        group: "Cross (OCLL)",
        alg: "R U2 R D R' U2 R D' R2",
    },
    Case {
        set: Set::Oll,
        name: "26",
        group: "Cross (OCLL)",
        alg: "y R U2 R' U' R U' R'",
    },
    Case {
        set: Set::Oll,
        name: "27",
        group: "Cross (OCLL)",
        alg: "R U R' U R U2 R'",
    },
    Case {
        set: Set::Oll,
        name: "28",
        group: "Corners oriented",
        alg: "r U R' U' M U R U' R'",
    },
    Case {
        set: Set::Oll,
        name: "29",
        group: "Awkward",
        alg: "r2 D' r U r' D r2 U' r' U' r",
    },
    Case {
        set: Set::Oll,
        name: "30",
        group: "Awkward",
        alg: "y2 F U R U2 R' U' R U2 R' U' F'",
    },
    Case {
        set: Set::Oll,
        name: "31",
        group: "P",
        alg: "R' U' F U R U' R' F' R",
    },
    Case {
        set: Set::Oll,
        name: "32",
        group: "P",
        alg: "S R U R' U' R' F R f'",
    },
    Case {
        set: Set::Oll,
        name: "33",
        group: "T",
        alg: "R U R' U' R' F R F'",
    },
    Case {
        set: Set::Oll,
        name: "34",
        group: "C",
        alg: "y f R f' U' r' U' R U M'",
    },
    Case {
        set: Set::Oll,
        name: "35",
        group: "Fish",
        alg: "R U2 R2 F R F' R U2 R'",
    },
    Case {
        set: Set::Oll,
        name: "36",
        group: "W",
        alg: "y R U R2 F' U' F U R2 U2 R'",
    },
    Case {
        set: Set::Oll,
        name: "37",
        group: "Fish",
        alg: "F R' F' R U R U' R'",
    },
    Case {
        set: Set::Oll,
        name: "38",
        group: "W",
        alg: "R U R' U R U' R' U' R' F R F'",
    },
    Case {
        set: Set::Oll,
        name: "39",
        group: "Lightning",
        alg: "y' f' r U r' U' r' F r S",
    },
    Case {
        set: Set::Oll,
        name: "40",
        group: "Lightning",
        alg: "y R' F R U R' U' F' U R",
    },
    Case {
        set: Set::Oll,
        name: "41",
        group: "Awkward",
        alg: "y2 R U R' U R U2 R' F R U R' U' F'",
    },
    Case {
        set: Set::Oll,
        name: "42",
        group: "Awkward",
        alg: "R' U' R U' R' U2 R F R U R' U' F'",
    },
    Case {
        set: Set::Oll,
        name: "43",
        group: "P",
        alg: "y R' U' F' U F R",
    },
    Case {
        set: Set::Oll,
        name: "44",
        group: "P",
        alg: "f R U R' U' f'",
    },
    Case {
        set: Set::Oll,
        name: "45",
        group: "T",
        alg: "F R U R' U' F'",
    },
    Case {
        set: Set::Oll,
        name: "46",
        group: "C",
        alg: "R' U' R' F R F' U R",
    },
    Case {
        set: Set::Oll,
        name: "47",
        group: "L",
        alg: "y' F R' F' R U2 R U' R' U R U2 R'",
    },
    Case {
        set: Set::Oll,
        name: "48",
        group: "L",
        alg: "F R U R' U' R U R' U' F'",
    },
    Case {
        set: Set::Oll,
        name: "49",
        group: "L",
        alg: "y2 r U' r2 U r2 U r2 U' r",
    },
    Case {
        set: Set::Oll,
        name: "50",
        group: "L",
        alg: "r' U r2 U' r2 U' r2 U r'",
    },
    Case {
        set: Set::Oll,
        name: "51",
        group: "Line",
        alg: "y2 F U R U' R' U R U' R' F'",
    },
    Case {
        set: Set::Oll,
        name: "52",
        group: "Line",
        alg: "y2 R' F' U' F U' R U R' U R",
    },
    Case {
        set: Set::Oll,
        name: "53",
        group: "L",
        alg: "r' U' R U' R' U R U' R' U2 r",
    },
    Case {
        set: Set::Oll,
        name: "54",
        group: "L",
        alg: "r U R' U R U' R' U R U2 r'",
    },
    Case {
        set: Set::Oll,
        name: "55",
        group: "Line",
        alg: "y R' F U R U' R2 F' R2 U R' U' R",
    },
    Case {
        set: Set::Oll,
        name: "56",
        group: "Line",
        alg: "r U r' U R U' R' U R U' R' r U' r'",
    },
    Case {
        set: Set::Oll,
        name: "57",
        group: "Corners oriented",
        alg: "R U R' U' M' U R U' r'",
    },
];

pub static F2L: &[Case] = &[
    Case {
        set: Set::F2l,
        name: "1",
        group: "",
        alg: "U R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "2",
        group: "",
        alg: "F R' F' R",
    },
    Case {
        set: Set::F2l,
        name: "3",
        group: "",
        alg: "F' U' F",
    },
    Case {
        set: Set::F2l,
        name: "4",
        group: "",
        alg: "R U R'",
    },
    Case {
        set: Set::F2l,
        name: "5",
        group: "",
        alg: "U' R U R' U2 R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "6",
        group: "",
        alg: "U' r U' R' U R U r'",
    },
    Case {
        set: Set::F2l,
        name: "7",
        group: "",
        alg: "U' R U2 R' U' R U2 R'",
    },
    Case {
        set: Set::F2l,
        name: "8",
        group: "",
        alg: "r' U2 R2 U R2 U r",
    },
    Case {
        set: Set::F2l,
        name: "9",
        group: "",
        alg: "U' R U' R' U F' U' F",
    },
    Case {
        set: Set::F2l,
        name: "10",
        group: "",
        alg: "U' R U R' U R U R'",
    },
    Case {
        set: Set::F2l,
        name: "11",
        group: "",
        alg: "U' R U2 R' U F' U' F",
    },
    Case {
        set: Set::F2l,
        name: "12",
        group: "",
        alg: "R U' R' U R U' R' U2 R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "13",
        group: "",
        alg: "y' U R' U R U' R' U' R",
    },
    Case {
        set: Set::F2l,
        name: "14",
        group: "",
        alg: "U' R U' R' U R U R'",
    },
    Case {
        set: Set::F2l,
        name: "15",
        group: "",
        alg: "M U r U' r' U' M'",
    },
    Case {
        set: Set::F2l,
        name: "16",
        group: "",
        alg: "R U' R' U2 F' U' F",
    },
    Case {
        set: Set::F2l,
        name: "17",
        group: "",
        alg: "R U2 R' U' R U R'",
    },
    Case {
        set: Set::F2l,
        name: "18",
        group: "",
        alg: "y' R' U2 R U R' U' R",
    },
    Case {
        set: Set::F2l,
        name: "19",
        group: "",
        alg: "U R U2 R' U R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "20",
        group: "",
        alg: "y' U' R' U2 R U' R' U R",
    },
    Case {
        set: Set::F2l,
        name: "21",
        group: "",
        alg: "U2 R U R' U R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "22",
        group: "",
        alg: "r U' r' U2 r U r'",
    },
    Case {
        set: Set::F2l,
        name: "23",
        group: "",
        alg: "U R U' R' U' R U' R' U R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "24",
        group: "",
        alg: "F U R U' R' F' R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "25",
        group: "",
        alg: "U' R' F R F' R U R'",
    },
    Case {
        set: Set::F2l,
        name: "26",
        group: "",
        alg: "U R U' R' F R' F' R",
    },
    Case {
        set: Set::F2l,
        name: "27",
        group: "",
        alg: "R U' R' U R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "28",
        group: "",
        alg: "R U R' U' F R' F' R",
    },
    Case {
        set: Set::F2l,
        name: "29",
        group: "",
        alg: "R' F R F' U R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "30",
        group: "",
        alg: "R U R' U' R U R'",
    },
    Case {
        set: Set::F2l,
        name: "31",
        group: "",
        alg: "U' R' F R F' R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "32",
        group: "",
        alg: "U R U' R' U R U' R' U R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "33",
        group: "",
        alg: "U' R U' R' U2 R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "34",
        group: "",
        alg: "U R U R' U2 R U R'",
    },
    Case {
        set: Set::F2l,
        name: "35",
        group: "",
        alg: "U' R U R' U F' U' F",
    },
    Case {
        set: Set::F2l,
        name: "36",
        group: "",
        alg: "U F' U' F U' R U R'",
    },
    Case {
        set: Set::F2l,
        name: "37",
        group: "",
        alg: "R2 U2 F R2 F' U2 R' U R'",
    },
    Case {
        set: Set::F2l,
        name: "38",
        group: "",
        alg: "R U' R' U' R U R' U2 R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "39",
        group: "",
        alg: "R U' R' U R U2 R' U R U' R'",
    },
    Case {
        set: Set::F2l,
        name: "40",
        group: "",
        alg: "r U' r' U2 r U r' R U R'",
    },
    Case {
        set: Set::F2l,
        name: "41",
        group: "",
        alg: "R U' R' r U' r' U2 r U r'",
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashSet};

    /// Every permutation of 0..n.
    fn permutations(n: usize) -> Vec<Vec<u8>> {
        if n == 0 {
            return vec![vec![]];
        }
        let mut out = Vec::new();
        for p in permutations(n - 1) {
            for i in 0..n {
                let mut q = p.clone();
                q.insert(i, (n - 1) as u8);
                out.push(q);
            }
        }
        out
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

    #[test]
    fn every_algorithm_parses() {
        for c in all_cases() {
            assert!(alg::parse(c.alg).is_some(), "{} {:?}", c.title(), c.alg);
        }
        assert_eq!((PLL.len(), OLL.len(), F2L.len()), (21, 57, 41));
    }

    #[test]
    fn keys_are_unique_and_findable() {
        let keys: HashSet<String> = all_cases().map(Case::key).collect();
        assert_eq!(keys.len(), all_cases().count());
        for c in all_cases() {
            assert_eq!(find(&c.key()), Some(c));
        }
        assert_eq!(find("PLL:T").unwrap().title(), "T-perm");
        assert_eq!(find("OLL:21").unwrap().title(), "OLL 21");
        assert!(find("PLL:nonsense").is_none());
    }

    // ---- PLL ----

    /// The last-layer permutation key of a cube: corner and edge places.
    fn pll_key(c: &Cube) -> ([u8; 4], [u8; 4]) {
        (
            [c.cp[0], c.cp[1], c.cp[2], c.cp[3]],
            [c.ep[0], c.ep[1], c.ep[2], c.ep[3]],
        )
    }

    /// Every state a PLL algorithm solves: its case with any U turn before
    /// and after (adjusting the face before the algorithm and after it).
    fn pll_class(case: &Cube) -> BTreeSet<([u8; 4], [u8; 4])> {
        let mut out = BTreeSet::new();
        for a in 0..4 {
            for b in 0..4 {
                out.insert(pll_key(&u_turns(a).multiply(case).multiply(&u_turns(b))));
            }
        }
        out
    }

    #[test]
    fn pll_algorithms_only_permute_the_last_layer() {
        for c in PLL {
            let e = alg::effect(c.alg).unwrap();
            assert!(f2l_solved(&e), "{} breaks the first two layers", c.title());
            assert!(ll_oriented(&e), "{} twists or flips a piece", c.title());
            let solved_class = pll_class(&SOLVED);
            assert!(
                !solved_class.contains(&pll_key(&c.state())),
                "{} does nothing useful",
                c.title()
            );
        }
    }

    #[test]
    fn the_21_pll_algorithms_solve_every_case_exactly_once() {
        let mut covered = BTreeSet::new();
        let mut classes: Vec<BTreeSet<_>> = Vec::new();
        for c in PLL {
            let class = pll_class(&c.state());
            for (other, earlier) in PLL.iter().zip(&classes) {
                assert!(
                    earlier.is_disjoint(&class),
                    "{} and {} are the same case",
                    c.title(),
                    other.title()
                );
            }
            covered.extend(class.iter().copied());
            classes.push(class);
        }
        covered.extend(pll_class(&SOLVED));
        // every last-layer permutation the cube allows: corner and edge
        // permutations of the same parity
        let all: BTreeSet<_> = permutations(4)
            .iter()
            .flat_map(|cp| {
                permutations(4)
                    .into_iter()
                    .filter(move |ep| parity(cp) == parity(ep))
                    .map(move |ep| (cp.clone(), ep))
            })
            .map(|(cp, ep)| ([cp[0], cp[1], cp[2], cp[3]], [ep[0], ep[1], ep[2], ep[3]]))
            .collect();
        assert_eq!(all.len(), 288);
        assert_eq!(covered, all);
    }

    #[test]
    fn pll_groups_match_what_each_algorithm_does() {
        for c in PLL {
            let s = c.state();
            // check the groups on the best-aligned version of the case
            let fits = (0..4).any(|a| {
                let x = u_turns(a).multiply(&s);
                let corners_home = (0..4).filter(|&i| x.cp[i] == i as u8).count();
                let edges_home = (0..4).filter(|&i| x.ep[i] == i as u8).count();
                match c.group {
                    "Edges only" => corners_home == 4,
                    "Corners only" => edges_home == 4,
                    "Adjacent corner swap" | "Diagonal corner swap" | "G perms" => {
                        corners_home < 4 && edges_home < 4
                    }
                    other => panic!("unknown group {other}"),
                }
            });
            assert!(fits, "{} doesn't fit {:?}", c.title(), c.group);
        }
    }

    #[test]
    fn pll_names_follow_their_algorithms() {
        // T swaps the two corners on the right and the left and right edges
        let t = find("PLL:T").unwrap().state();
        assert_eq!(pll_key(&t), ([3, 1, 2, 0], [2, 1, 0, 3]));
        // H swaps opposite edges, Z neighbouring ones; corners stay
        let h = find("PLL:H").unwrap().state();
        assert_eq!(pll_key(&h), ([0, 1, 2, 3], [2, 3, 0, 1]));
        let z = pll_class(&find("PLL:Z").unwrap().state());
        assert!(z.iter().all(
            |(cp, ep)| ep.iter().zip(0..).filter(|(e, i)| **e != *i).count() == 4
                || cp != &[0, 1, 2, 3]
        ));
    }

    // ---- OLL ----

    fn oll_key(c: &Cube) -> ([u8; 4], [u8; 4]) {
        (
            [c.co[0], c.co[1], c.co[2], c.co[3]],
            [c.eo[0], c.eo[1], c.eo[2], c.eo[3]],
        )
    }

    /// Orientation patterns an OLL algorithm handles: its case turned to
    /// any side (a U turn before the algorithm).
    fn oll_class(case: &Cube) -> BTreeSet<([u8; 4], [u8; 4])> {
        (0..4)
            .map(|b| oll_key(&case.multiply(&u_turns(b))))
            .collect()
    }

    #[test]
    fn oll_algorithms_keep_the_first_two_layers() {
        for c in OLL {
            let e = alg::effect(c.alg).unwrap();
            assert!(f2l_solved(&e), "{} breaks the first two layers", c.title());
            assert!(!ll_oriented(&c.state()), "{} starts oriented", c.title());
        }
    }

    #[test]
    fn the_57_oll_algorithms_solve_every_case_exactly_once() {
        let mut covered = BTreeSet::new();
        let mut classes: Vec<BTreeSet<_>> = Vec::new();
        for c in OLL {
            let class = oll_class(&c.state());
            for (other, earlier) in OLL.iter().zip(&classes) {
                assert!(
                    earlier.is_disjoint(&class),
                    "{} and {} are the same case",
                    c.title(),
                    other.title()
                );
            }
            covered.extend(class.iter().copied());
            classes.push(class);
        }
        covered.extend(oll_class(&SOLVED));
        let mut all = BTreeSet::new();
        for co in 0..81u32 {
            let c = [co % 3, co / 3 % 3, co / 9 % 3, co / 27 % 3].map(|x| x as u8);
            if c.iter().map(|&x| x as u32).sum::<u32>() % 3 != 0 {
                continue;
            }
            for eo in 0..16u32 {
                let e = [eo & 1, eo >> 1 & 1, eo >> 2 & 1, eo >> 3 & 1].map(|x| x as u8);
                if e.iter().map(|&x| x as u32).sum::<u32>() % 2 == 0 {
                    all.insert((c, e));
                }
            }
        }
        assert_eq!(all.len(), 216);
        assert_eq!(covered, all);
    }

    #[test]
    fn oll_cross_cases_have_oriented_edges() {
        for c in OLL.iter().filter(|c| c.group == "Cross (OCLL)") {
            assert!(oll_key(&c.state()).1 == [0; 4], "{}", c.title());
        }
        for c in OLL.iter().filter(|c| c.group == "Corners oriented") {
            assert!(oll_key(&c.state()).0 == [0; 4], "{}", c.title());
        }
        for c in OLL.iter().filter(|c| c.group == "Dot") {
            assert!(
                oll_key(&c.state()).1 == [1; 4],
                "{}: no edge oriented",
                c.title()
            );
        }
    }

    // ---- F2L ----

    /// Where the front-right pair is: corner place and twist, edge place and
    /// flip.
    fn pair(c: &Cube) -> (u8, u8, u8, u8) {
        let ci = (0..8).find(|&i| c.cp[i] == 4).unwrap();
        let ei = (0..12).find(|&i| c.ep[i] == 8).unwrap();
        (ci as u8, c.co[ci], ei as u8, c.eo[ei])
    }

    /// The pair after a U turn: pieces on top move round, the slot stays.
    fn pair_after_u(p: (u8, u8, u8, u8)) -> (u8, u8, u8, u8) {
        let u = Move::from_index(0).cube();
        let moved = |pos: u8, perm: &[u8]| -> u8 {
            if pos >= 4 {
                pos
            } else {
                (0..4).find(|&i| perm[i] == pos).unwrap() as u8
            }
        };
        (moved(p.0, &u.cp), p.1, moved(p.2, &u.ep), p.3)
    }

    fn f2l_class(case: &Cube) -> BTreeSet<(u8, u8, u8, u8)> {
        let mut p = pair(case);
        let mut out = BTreeSet::new();
        for _ in 0..4 {
            out.insert(p);
            p = pair_after_u(p);
        }
        out
    }

    #[test]
    fn f2l_algorithms_keep_the_cross_and_the_other_slots() {
        for c in F2L {
            let e = alg::effect(c.alg).unwrap();
            // cross edges and the three other slots untouched
            for i in [4, 5, 6, 7, 9, 10, 11] {
                assert!(
                    e.ep[i] == i as u8 && e.eo[i] == 0,
                    "{} moves edge {i}",
                    c.title()
                );
            }
            for i in [5, 6, 7] {
                assert!(
                    e.cp[i] == i as u8 && e.co[i] == 0,
                    "{} moves corner {i}",
                    c.title()
                );
            }
            // and the pair it solves comes from the top layer or the slot
            let (cpos, _, epos, _) = pair(&c.state());
            assert!(cpos < 5 && (epos < 4 || epos == 8), "{}", c.title());
        }
    }

    #[test]
    fn the_41_f2l_algorithms_solve_every_case_exactly_once() {
        let mut covered = BTreeSet::new();
        let mut classes: Vec<BTreeSet<_>> = Vec::new();
        for c in F2L {
            let class = f2l_class(&c.state());
            for (other, earlier) in F2L.iter().zip(&classes) {
                assert!(
                    earlier.is_disjoint(&class),
                    "{} and {} are the same case",
                    c.title(),
                    other.title()
                );
            }
            covered.extend(class.iter().copied());
            classes.push(class);
        }
        covered.extend(f2l_class(&SOLVED));
        let mut all = BTreeSet::new();
        for cpos in [0, 1, 2, 3, 4] {
            for co in 0..3 {
                for epos in [0, 1, 2, 3, 8] {
                    for eo in 0..2 {
                        all.insert((cpos, co, epos, eo));
                    }
                }
            }
        }
        assert_eq!(all.len(), 150);
        assert_eq!(covered, all);
    }

    // ---- 2-look ----

    #[test]
    fn two_look_oll_edges_then_corners() {
        let edge_class =
            |c: &Cube| -> BTreeSet<[u8; 4]> { oll_class(c).into_iter().map(|k| k.1).collect() };
        let mut covered = BTreeSet::new();
        for c in TWO_LOOK_OLL
            .iter()
            .filter(|c| c.group == "Orient the edges")
        {
            let e = alg::effect(c.alg).unwrap();
            assert!(f2l_solved(&e), "{}", c.title());
            let class = edge_class(&c.state());
            assert!(covered.is_disjoint(&class), "{}", c.title());
            covered.extend(class);
        }
        covered.extend(edge_class(&SOLVED));
        assert_eq!(covered.len(), 8, "every edge orientation pattern");
        // the corner step is the seven cross OLLs, with the same algorithms
        let corners: Vec<_> = TWO_LOOK_OLL
            .iter()
            .filter(|c| c.group == "Orient the corners")
            .collect();
        assert_eq!(corners.len(), 7);
        for c in corners {
            assert!(
                OLL.iter()
                    .any(|o| o.group == "Cross (OCLL)" && alg::same_effect(o.alg, c.alg)),
                "{}",
                c.title()
            );
        }
    }

    #[test]
    fn two_look_pll_corners_then_edges() {
        // the corner step: the two algorithms cover every corner permutation,
        // whatever the edges do
        let corner_class =
            |c: &Cube| -> BTreeSet<[u8; 4]> { pll_class(c).into_iter().map(|k| k.0).collect() };
        let mut covered = BTreeSet::new();
        for c in TWO_LOOK_PLL
            .iter()
            .filter(|c| c.group == "Permute the corners")
        {
            let e = alg::effect(c.alg).unwrap();
            assert!(f2l_solved(&e) && ll_oriented(&e), "{}", c.title());
            let class = corner_class(&c.state());
            assert!(covered.is_disjoint(&class), "{}", c.title());
            covered.extend(class);
        }
        covered.extend(corner_class(&SOLVED));
        assert_eq!(covered.len(), 24, "every corner permutation");
        // the edge step: exactly the edges-only PLLs
        for c in TWO_LOOK_PLL
            .iter()
            .filter(|c| c.group == "Permute the edges")
        {
            assert!(
                PLL.iter()
                    .any(|p| p.group == "Edges only" && alg::same_effect(p.alg, c.alg)),
                "{}",
                c.title()
            );
        }
    }

    #[test]
    fn f2l_groups_follow_where_the_pair_is() {
        let count = |g: &str| F2L.iter().filter(|c| c.group_name() == g).count();
        // the standard split of the 41 cases
        assert_eq!(count("Both pieces on top"), 24);
        assert_eq!(count("Corner in the slot"), 6);
        assert_eq!(count("Edge in the slot"), 6);
        assert_eq!(count("Both pieces in the slot"), 5);
        assert_eq!(find("PLL:T").unwrap().group_name(), "Adjacent corner swap");
    }
}
