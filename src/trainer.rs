//! The algorithm trainer: scrambles that set up a chosen case, so you can
//! drill the algorithms you are learning.
//!
//! Each scramble picks one of the chosen cases at random (every case equally
//! often), turns the last layer a random amount before and after it, and
//! randomizes whatever the case doesn't depend on (for OLL and F2L, the rest
//! of the last layer), so a case can't be recognized from its scramble. The
//! two-phase solver then finds a normal-looking scramble for that state.
//!
//! Scrambles are for a cube held the WCA way (white on top, green in front),
//! and set the case up on the yellow face: you scramble, turn the cube over
//! (white cross down, yellow up), and the case is in front of you exactly as
//! the algorithm browser draws it.

use std::sync::mpsc::{Receiver, sync_channel};
use std::thread;

use rand::seq::SliceRandom;
use rand::{Rng, RngExt};

use crate::algs::{self, Case, Set};
use crate::algview::DISPLAY;
use crate::cube::{self, Cube, SOLVED};
use crate::nxn::{self, Nxn};
use crate::scramble;
use crate::stats::{Penalty, TimeResult};
use crate::store::Solve;

/// A random last layer that keeps every piece's orientation: the four top
/// corners and edges shuffled, with matching parity.
fn random_ll_permutation(rng: &mut impl Rng) -> Cube {
    let mut c = SOLVED;
    let mut cp = [0u8, 1, 2, 3];
    let mut ep = [0u8, 1, 2, 3];
    cp.shuffle(rng);
    ep.shuffle(rng);
    if parity(&cp) != parity(&ep) {
        ep.swap(0, 1);
    }
    c.cp[..4].copy_from_slice(&cp);
    c.ep[..4].copy_from_slice(&ep);
    c
}

/// A random last layer: shuffled and twisted, the first two layers solved.
fn random_ll(rng: &mut impl Rng) -> Cube {
    let mut c = random_ll_permutation(rng);
    for i in 0..3 {
        c.co[i] = rng.random_range(0..3);
        c.eo[i] = rng.random_range(0..2);
    }
    c.co[3] = (6 - c.co[..3].iter().sum::<u8>()) % 3;
    c.eo[3] = c.eo[..3].iter().sum::<u8>() % 2;
    c
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

/// A state that is `case`, ready to solve with its algorithm after an
/// adjustment of the top layer.
pub fn state_for(case: &Case, rng: &mut impl Rng) -> Cube {
    let s = case.state();
    let before = algs::u_turns(rng.random_range(0..4));
    let after = algs::u_turns(rng.random_range(0..4));
    match case.set {
        // the case is the permutation: only turn the top layer around it
        Set::Pll | Set::TwoLookPll => before.multiply(&s).multiply(&after),
        // orientation cases: shuffle the top layer first (orientation stays),
        // then the case
        Set::Oll | Set::TwoLookOll => random_ll_permutation(rng).multiply(&s).multiply(&after),
        // F2L: a random last layer first; the pair isn't in it yet, so this
        // doesn't change the case
        Set::F2l => random_ll(rng).multiply(&s).multiply(&after),
    }
}

/// The cube with yellow on top, as cases are learned and drawn, turned over
/// into the WCA scrambling orientation (white on top, green in front). The
/// case in the model's top layer ends up on the yellow face.
pub fn to_wca(state: &Cube) -> Cube {
    let shown: Vec<u8> = state
        .facelets()
        .iter()
        .map(|&c| DISPLAY[c as usize])
        .collect();
    let mut cube = Nxn::from_facelets(3, &shown);
    // turning it over the way a cuber does (z2) puts white back on top
    cube.apply(nxn::Move {
        face: 2,
        layers: 3,
        turns: 2,
    });
    Cube::from_facelets(&cube.facelets()).expect("a turned-over cube is still a cube")
}

/// Makes training scrambles on a background thread, one ahead.
pub struct Drill {
    pub set: Set,
    pub cases: Vec<&'static Case>,
    next: Receiver<(&'static Case, String)>,
}

impl Drill {
    /// A drill on `cases` (all of one set). Panics if `cases` is empty.
    pub fn new(set: Set, cases: Vec<&'static Case>) -> Drill {
        assert!(!cases.is_empty());
        let (tx, rx) = sync_channel(1);
        let pool = cases.clone();
        thread::spawn(move || {
            let mut rng = rand::rng();
            loop {
                let case = pool[rng.random_range(0..pool.len())];
                let state = to_wca(&state_for(case, &mut rng));
                let scramble = cube::format(&scramble::scramble_for(&state));
                if tx.send((case, scramble)).is_err() {
                    break; // the drill is over
                }
            }
        });
        Drill {
            set,
            cases,
            next: rx,
        }
    }

    /// The next scramble and its case, if it is ready.
    pub fn try_next(&self) -> Option<(&'static Case, String)> {
        // a closed channel only happens if the thread panicked; then there is
        // nothing more to drill
        self.next.try_recv().ok()
    }

    /// The cases to drill in `set`: the ones marked as being learned, or
    /// all of them if none are.
    pub fn cases_for(set: Set, learning: impl Fn(&Case) -> bool) -> Vec<&'static Case> {
        let marked: Vec<&'static Case> = set.cases().iter().filter(|c| learning(c)).collect();
        if marked.is_empty() {
            set.cases().iter().collect()
        } else {
            marked
        }
    }
}

/// Your times on one case: solves, mean (of finished ones), best.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CaseTimes {
    pub count: usize,
    pub mean: Option<u64>,
    pub best: Option<u64>,
}

/// Times per case key from a drill session's solves.
pub fn case_times(solves: &[Solve], key: &str) -> CaseTimes {
    let times: Vec<u64> = solves
        .iter()
        .filter(|s| s.case.as_deref() == Some(key) && s.penalty != Penalty::Dnf)
        .filter_map(|s| match s.result() {
            TimeResult::Ms(ms) => Some(ms),
            TimeResult::Dnf => None,
        })
        .collect();
    let count = solves
        .iter()
        .filter(|s| s.case.as_deref() == Some(key))
        .count();
    CaseTimes {
        count,
        mean: (!times.is_empty()).then(|| times.iter().sum::<u64>() / times.len() as u64),
        best: times.iter().copied().min(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::algs::{F2L, OLL, PLL};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    /// Which case of its set a state is, by trying every case's algorithm
    /// with every top-layer adjustment around it.
    fn identify(set: Set, state: &Cube) -> Option<&'static str> {
        for case in set.cases() {
            let eff = crate::alg::effect(case.alg).unwrap();
            for a in 0..4 {
                for b in 0..4 {
                    let after = state
                        .multiply(&algs::u_turns(a))
                        .multiply(&eff)
                        .multiply(&algs::u_turns(b));
                    let solved = match set {
                        Set::Pll => after == SOLVED,
                        Set::Oll => algs::f2l_solved(&after) && algs::ll_oriented(&after),
                        Set::F2l => {
                            (0..12)
                                .filter(|&i| i >= 4)
                                .all(|i| after.ep[i] == i as u8 && after.eo[i] == 0)
                                && (4..8).all(|i| after.cp[i] == i as u8 && after.co[i] == 0)
                        }
                        _ => unreachable!(),
                    };
                    if solved {
                        return Some(case.name);
                    }
                }
            }
        }
        None
    }

    #[test]
    fn every_generated_state_is_the_case_it_claims() {
        let mut rng = StdRng::seed_from_u64(51);
        for (set, cases) in [(Set::Pll, PLL), (Set::Oll, OLL), (Set::F2l, F2L)] {
            for case in cases {
                for _ in 0..3 {
                    let state = state_for(case, &mut rng);
                    assert!(state.is_solvable());
                    assert_eq!(identify(set, &state), Some(case.name), "{}", case.title());
                }
            }
        }
    }

    #[test]
    fn training_states_keep_the_first_two_layers_where_they_should() {
        let mut rng = StdRng::seed_from_u64(52);
        for case in PLL.iter().chain(OLL) {
            assert!(
                algs::f2l_solved(&state_for(case, &mut rng)),
                "{}",
                case.title()
            );
        }
        for case in F2L {
            let s = state_for(case, &mut rng);
            for i in [4, 5, 6, 7, 9, 10, 11] {
                assert!(
                    s.ep[i] == i as u8 && s.eo[i] == 0,
                    "{}: cross or other slot moved",
                    case.title()
                );
            }
        }
    }

    #[test]
    fn the_rest_of_the_last_layer_is_randomized() {
        // two OLL states of the same case almost never have the same last
        // layer permutation
        let mut rng = StdRng::seed_from_u64(53);
        let case = &OLL[44];
        let perms: std::collections::HashSet<[u8; 4]> = (0..40)
            .map(|_| {
                let s = state_for(case, &mut rng);
                [s.cp[0], s.cp[1], s.cp[2], s.cp[3]]
            })
            .collect();
        assert!(perms.len() > 10, "{}", perms.len());
    }

    #[test]
    fn a_drill_scramble_sets_up_its_case() {
        let cases: Vec<&'static Case> = PLL
            .iter()
            .filter(|c| ["T", "Y", "H"].contains(&c.name))
            .collect();
        let drill = Drill::new(Set::Pll, cases.clone());
        let mut seen = std::collections::HashSet::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while seen.len() < 3 {
            assert!(
                std::time::Instant::now() < deadline,
                "only {seen:?} in 20 s"
            );
            if let Some((case, scramble)) = drill.try_next() {
                assert!(cases.contains(&case));
                let state = from_wca(&SOLVED.apply_all(&cube::parse(&scramble).unwrap()));
                assert_eq!(identify(Set::Pll, &state), Some(case.name), "{scramble}");
                seen.insert(case.name);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }

    #[test]
    fn the_drill_uses_marked_cases_or_all() {
        let all = Drill::cases_for(Set::Pll, |_| false);
        assert_eq!(all.len(), 21);
        let some = Drill::cases_for(Set::Pll, |c| c.name.starts_with('G'));
        assert_eq!(
            some.iter().map(|c| c.name).collect::<Vec<_>>(),
            ["Ga", "Gb", "Gc", "Gd"]
        );
    }

    #[test]
    fn times_per_case() {
        let mut solves = vec![];
        for (ms, case, pen) in [
            (2_000, "PLL:T", Penalty::Ok),
            (3_000, "PLL:T", Penalty::Ok),
            (9_000, "PLL:T", Penalty::Dnf),
            (1_500, "PLL:Y", Penalty::Plus2),
        ] {
            let mut s = Solve::new(ms, pen, String::new());
            s.case = Some(case.into());
            solves.push(s);
        }
        let t = case_times(&solves, "PLL:T");
        assert_eq!(
            t,
            CaseTimes {
                count: 3,
                mean: Some(2_500),
                best: Some(2_000)
            }
        );
        let y = case_times(&solves, "PLL:Y");
        assert_eq!((y.mean, y.best), (Some(3_500), Some(3_500)), "+2 counts");
        assert_eq!(case_times(&solves, "PLL:H"), CaseTimes::default());
    }

    /// Undo `to_wca`: the scrambled cube turned over, read with yellow on top.
    fn from_wca(state: &Cube) -> Cube {
        let mut cube = Nxn::from_facelets(3, &state.facelets());
        cube.apply(nxn::Move {
            face: 2,
            layers: 3,
            turns: 2,
        });
        let back: Vec<u8> = cube
            .facelets()
            .iter()
            .map(|&c| DISPLAY.iter().position(|&d| d == c).unwrap() as u8)
            .collect();
        Cube::from_facelets(&back).unwrap()
    }

    #[test]
    fn training_scrambles_put_the_case_on_the_yellow_face() {
        let mut rng = StdRng::seed_from_u64(54);
        for case in PLL.iter().chain(OLL) {
            let state = state_for(case, &mut rng);
            let wca = to_wca(&state);
            assert_eq!(from_wca(&wca), state, "{}", case.title());
            // held the WCA way, the white layer and the middle layer are
            // solved; everything the case changes is in the yellow (D) layer
            for i in 0..4 {
                assert!(
                    wca.cp[i] == i as u8 && wca.co[i] == 0,
                    "{}: white corner {i}",
                    case.title()
                );
                assert!(
                    wca.ep[i] == i as u8 && wca.eo[i] == 0,
                    "{}: white edge {i}",
                    case.title()
                );
            }
            for i in 8..12 {
                assert!(
                    wca.ep[i] == i as u8 && wca.eo[i] == 0,
                    "{}: middle edge {i}",
                    case.title()
                );
            }
        }
        // F2L: the white cross is solved (it is on top while scrambling)
        for case in F2L {
            let wca = to_wca(&state_for(case, &mut rng));
            for i in 0..4 {
                assert!(
                    wca.ep[i] == i as u8 && wca.eo[i] == 0,
                    "{}: cross edge {i}",
                    case.title()
                );
            }
        }
    }
}
