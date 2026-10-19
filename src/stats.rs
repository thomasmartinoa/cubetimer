//! Session statistics: single results, mean-of-N and average-of-N.
//!
//! Everything is in integer milliseconds. Averages are floored to the
//! millisecond here; `fmt` rounds them to the nearest hundredth, which is
//! exact because every rounding threshold is a whole millisecond.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Penalty {
    #[default]
    Ok,
    #[serde(rename = "plus2")]
    Plus2,
    Dnf,
}

/// A solve outcome. The derived `Ord` makes every `Ms` sort before `Dnf`, so
/// a DNF is always the worst result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TimeResult {
    Ms(u64),
    Dnf,
}

impl TimeResult {
    /// Result of a raw time with a penalty applied (+2 adds 2000 ms).
    pub fn new(raw_ms: u64, penalty: Penalty) -> Self {
        match penalty {
            Penalty::Ok => Self::Ms(raw_ms),
            Penalty::Plus2 => Self::Ms(raw_ms + 2000),
            Penalty::Dnf => Self::Dnf,
        }
    }
}

/// Number of solves dropped from each end of an aoN: 5%, at least one.
fn trim_count(n: usize) -> usize {
    n.div_ceil(20).max(1)
}

/// Mean of a set of results, `Dnf` if any is a DNF. `None` when empty.
fn mean(results: &[TimeResult]) -> Option<TimeResult> {
    if results.is_empty() {
        return None;
    }
    let mut sum = 0;
    for r in results {
        match r {
            TimeResult::Ms(ms) => sum += ms,
            TimeResult::Dnf => return Some(TimeResult::Dnf),
        }
    }
    Some(TimeResult::Ms(sum / results.len() as u64))
}

/// Plain mean of the last `n` results (mo3 is `mo(results, 3)`).
pub fn mo(results: &[TimeResult], n: usize) -> Option<TimeResult> {
    if n == 0 || results.len() < n {
        return None;
    }
    mean(&results[results.len() - n..])
}

/// Average of the last `n` results: drop the best and worst 5% (at least one
/// each) and average the rest. More DNFs than dropped makes the average DNF.
pub fn ao(results: &[TimeResult], n: usize) -> Option<TimeResult> {
    if n < 3 || results.len() < n {
        return None;
    }
    let mut window = results[results.len() - n..].to_vec();
    window.sort();
    let trim = trim_count(n);
    mean(&window[trim..n - trim])
}

/// Best single result (`Dnf` only if every result is a DNF).
pub fn best(results: &[TimeResult]) -> Option<TimeResult> {
    results.iter().copied().min()
}

/// Best aoN over every window of the session.
pub fn best_ao(results: &[TimeResult], n: usize) -> Option<TimeResult> {
    if n < 3 || results.len() < n {
        return None;
    }
    (n..=results.len())
        .filter_map(|end| ao(&results[..end], n))
        .min()
}

/// Mean of the non-DNF solves, in ms (csTimer's session mean).
pub fn session_mean(results: &[TimeResult]) -> Option<u64> {
    let times: Vec<u64> = results
        .iter()
        .filter_map(|r| match r {
            TimeResult::Ms(ms) => Some(*ms),
            TimeResult::Dnf => None,
        })
        .collect();
    if times.is_empty() {
        None
    } else {
        Some(times.iter().sum::<u64>() / times.len() as u64)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Summary {
    pub count: usize,
    pub best: Option<TimeResult>,
    pub mean: Option<u64>,
    pub mo3: Option<TimeResult>,
    pub ao5: Option<TimeResult>,
    pub ao12: Option<TimeResult>,
    pub ao100: Option<TimeResult>,
    pub best_ao5: Option<TimeResult>,
    pub best_ao12: Option<TimeResult>,
}

pub fn summarize(results: &[TimeResult]) -> Summary {
    Summary {
        count: results.len(),
        best: best(results),
        mean: session_mean(results),
        mo3: mo(results, 3),
        ao5: ao(results, 5),
        ao12: ao(results, 12),
        ao100: ao(results, 100),
        best_ao5: best_ao(results, 5),
        best_ao12: best_ao(results, 12),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TimeResult::{Dnf, Ms};

    fn ms(v: &[u64]) -> Vec<TimeResult> {
        v.iter().map(|&x| Ms(x)).collect()
    }

    #[test]
    fn ordering_puts_dnf_last() {
        let mut v = vec![Dnf, Ms(5000), Ms(100)];
        v.sort();
        assert_eq!(v, vec![Ms(100), Ms(5000), Dnf]);
    }

    #[test]
    fn penalties() {
        assert_eq!(TimeResult::new(12_340, Penalty::Ok), Ms(12_340));
        assert_eq!(TimeResult::new(12_340, Penalty::Plus2), Ms(14_340));
        assert_eq!(TimeResult::new(12_340, Penalty::Dnf), Dnf);
    }

    #[test]
    fn trim_counts() {
        assert_eq!(trim_count(5), 1);
        assert_eq!(trim_count(12), 1);
        assert_eq!(trim_count(20), 1);
        assert_eq!(trim_count(21), 2);
        assert_eq!(trim_count(100), 5);
    }

    #[test]
    fn ao5_drops_best_and_worst() {
        // drops 1000 and 9000, mean of 2000/3000/4000
        assert_eq!(ao(&ms(&[9000, 2000, 1000, 4000, 3000]), 5), Some(Ms(3000)));
    }

    #[test]
    fn ao_uses_last_n_only() {
        assert_eq!(
            ao(&ms(&[99_999, 1000, 2000, 3000, 4000, 5000]), 5),
            Some(Ms(3000))
        );
    }

    #[test]
    fn ao_needs_enough_solves() {
        assert_eq!(ao(&ms(&[1, 2, 3, 4]), 5), None);
        assert_eq!(ao(&[], 5), None);
        assert_eq!(ao(&ms(&[1, 2]), 2), None);
    }

    #[test]
    fn ao_one_dnf_is_dropped_as_worst() {
        let r = [Ms(1000), Ms(2000), Ms(3000), Ms(4000), Dnf];
        assert_eq!(ao(&r, 5), Some(Ms(3000)));
    }

    #[test]
    fn ao_two_dnfs_in_ao5_is_dnf() {
        let r = [Ms(1000), Ms(2000), Ms(3000), Dnf, Dnf];
        assert_eq!(ao(&r, 5), Some(Dnf));
    }

    #[test]
    fn ao12_drops_only_one_each_end() {
        let mut r = ms(&[1000; 12]);
        r[3] = Dnf;
        assert_eq!(ao(&r, 12), Some(Ms(1000)));
        r[7] = Dnf;
        assert_eq!(ao(&r, 12), Some(Dnf));
    }

    #[test]
    fn ao100_drops_five_each_end() {
        let mut r = ms(&[1000; 100]);
        for i in 0..5 {
            r[i * 10] = Dnf;
        }
        assert_eq!(ao(&r, 100), Some(Ms(1000)));
        r[99] = Dnf;
        assert_eq!(ao(&r, 100), Some(Dnf));
    }

    #[test]
    fn ao_mean_is_floored() {
        // kept: 1000, 1000, 1001 -> 3001 / 3 = 1000.33
        assert_eq!(ao(&ms(&[0, 1000, 1000, 1001, 9999]), 5), Some(Ms(1000)));
    }

    #[test]
    fn mo3_is_plain_mean() {
        assert_eq!(mo(&ms(&[1000, 2000, 6000]), 3), Some(Ms(3000)));
        assert_eq!(mo(&[Ms(1000), Ms(2000), Dnf], 3), Some(Dnf));
        assert_eq!(mo(&ms(&[1000, 2000]), 3), None);
    }

    #[test]
    fn best_single() {
        assert_eq!(best(&[Dnf, Ms(5000), Ms(4000)]), Some(Ms(4000)));
        assert_eq!(best(&[Dnf, Dnf]), Some(Dnf));
        assert_eq!(best(&[]), None);
    }

    #[test]
    fn best_ao_scans_all_windows() {
        let r = ms(&[
            9000, 9000, 9000, 9000, 9000, 1000, 1000, 1000, 1000, 1000, 9000,
        ]);
        assert_eq!(best_ao(&r, 5), Some(Ms(1000)));
        assert_eq!(best_ao(&r[..4], 5), None);
    }

    #[test]
    fn best_ao_prefers_real_time_over_dnf() {
        let r = [Dnf, Dnf, Ms(1), Ms(2), Ms(3), Ms(4), Ms(5), Ms(6)];
        assert_eq!(best_ao(&r, 5), Some(Ms(3)));
    }

    #[test]
    fn session_mean_skips_dnf() {
        assert_eq!(session_mean(&[Ms(1000), Dnf, Ms(3000)]), Some(2000));
        assert_eq!(session_mean(&[Dnf]), None);
    }

    #[test]
    fn summary_of_empty_session() {
        let s = summarize(&[]);
        assert_eq!(s.count, 0);
        assert!(s.best.is_none() && s.ao5.is_none() && s.mean.is_none());
    }

    #[test]
    fn penalty_serde_names() {
        assert_eq!(serde_json::to_string(&Penalty::Ok).unwrap(), "\"ok\"");
        assert_eq!(serde_json::to_string(&Penalty::Plus2).unwrap(), "\"plus2\"");
        assert_eq!(serde_json::to_string(&Penalty::Dnf).unwrap(), "\"dnf\"");
    }
}
