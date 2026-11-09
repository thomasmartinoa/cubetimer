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
            // saturating: a hand-edited or imported file can hold any number
            Penalty::Plus2 => Self::Ms(raw_ms.saturating_add(2000)),
            Penalty::Dnf => Self::Dnf,
        }
    }
}

/// Number of solves dropped from each end of an aoN: 5%, at least one.
fn trim_count(n: usize) -> usize {
    n.div_ceil(20).max(1)
}

/// Floored mean of `count` times adding up to `sum`. Sums are `u128` so no
/// number of solves (or absurd time in a hand-edited file) can overflow.
pub fn floor_mean(sum: u128, count: usize) -> u64 {
    u64::try_from(sum / count as u128).unwrap_or(u64::MAX)
}

/// Mean of a set of results, `Dnf` if any is a DNF. `None` when empty.
fn mean(results: &[TimeResult]) -> Option<TimeResult> {
    if results.is_empty() {
        return None;
    }
    let mut sum = 0u128;
    for r in results {
        match r {
            TimeResult::Ms(ms) => sum += u128::from(*ms),
            TimeResult::Dnf => return Some(TimeResult::Dnf),
        }
    }
    Some(TimeResult::Ms(floor_mean(sum, results.len())))
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

/// The aoN ending at every solve from the `n`th on, as `(index of the last
/// solve, average)`; the same numbers as calling `ao` on each prefix, but in
/// one pass. The window is kept sorted (a DNF sorts last), so each step is a
/// binary-search insert and remove, and the kept times are the window's
/// finished total minus the few trimmed at each end.
pub fn rolling_ao(results: &[TimeResult], n: usize) -> Vec<(usize, TimeResult)> {
    if n < 3 || results.len() < n {
        return Vec::new();
    }
    // a DNF is u64::MAX, so it sorts after every time
    const DNF: u64 = u64::MAX;
    let key = |r: TimeResult| match r {
        TimeResult::Ms(ms) => ms.min(DNF - 1),
        TimeResult::Dnf => DNF,
    };
    let finished = |k: u64| if k == DNF { 0 } else { u128::from(k) };
    let trim = trim_count(n);
    let mut sorted: Vec<u64> = Vec::with_capacity(n + 1);
    // sum of the finished times in the window
    let mut total = 0u128;
    let mut out = Vec::with_capacity(results.len() + 1 - n);
    for (i, &r) in results.iter().enumerate() {
        let k = key(r);
        sorted.insert(sorted.partition_point(|&x| x < k), k);
        total += finished(k);
        if i >= n {
            let old = key(results[i - n]);
            sorted.remove(sorted.partition_point(|&x| x < old));
            total -= finished(old);
        }
        if i + 1 < n {
            continue;
        }
        // more DNFs than are trimmed: one is among the kept times
        let avg = if sorted[n - trim - 1] == DNF {
            TimeResult::Dnf
        } else {
            let trimmed: u128 = sorted[..trim]
                .iter()
                .chain(&sorted[n - trim..])
                .map(|&k| finished(k))
                .sum();
            TimeResult::Ms(floor_mean(total - trimmed, n - 2 * trim))
        };
        out.push((i, avg));
    }
    out
}

/// Best aoN over every window of the session.
pub fn best_ao(results: &[TimeResult], n: usize) -> Option<TimeResult> {
    rolling_ao(results, n).into_iter().map(|(_, a)| a).min()
}

/// Mean of the non-DNF solves, in ms (csTimer's session mean).
pub fn session_mean(results: &[TimeResult]) -> Option<u64> {
    let (sum, count) = results
        .iter()
        .filter_map(|r| match r {
            TimeResult::Ms(ms) => Some(u128::from(*ms)),
            TimeResult::Dnf => None,
        })
        .fold((0u128, 0usize), |(s, c), ms| (s + ms, c + 1));
    (count > 0).then(|| floor_mean(sum, count))
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

/// An average written out the way it is posted: the dropped times in
/// brackets, then the mean. `(12.10) 13.20 14.00 13.80 (16.43) = 13.67`.
/// `None` when there aren't `n` solves. Times are in the order they were
/// solved; DNFs show as `DNF`.
pub fn ao_details(results: &[TimeResult], n: usize, prec: crate::fmt::Precision) -> Option<String> {
    use crate::fmt;
    let avg = ao(results, n)?;
    let window = &results[results.len() - n..];
    // drop the best and worst by position, so equal times drop one each
    let trim = trim_count(n);
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| (window[i], i));
    let dropped: Vec<usize> = order[..trim]
        .iter()
        .chain(&order[n - trim..])
        .copied()
        .collect();
    let show = |r: &TimeResult| match r {
        TimeResult::Ms(ms) => fmt::fmt_time(*ms, prec),
        TimeResult::Dnf => "DNF".to_string(),
    };
    let parts: Vec<String> = window
        .iter()
        .enumerate()
        .map(|(i, r)| {
            if dropped.contains(&i) {
                format!("({})", show(r))
            } else {
                show(r)
            }
        })
        .collect();
    Some(format!(
        "{} = {}",
        parts.join(" "),
        fmt::fmt_avg(Some(avg), prec)
    ))
}

/// The averages the statistics screen lists.
pub const REPORT_AVERAGES: [usize; 5] = [5, 12, 50, 100, 1000];

/// Everything on the statistics screen's summary, computed once.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub count: usize,
    pub finished: usize,
    pub best: Option<TimeResult>,
    /// The slowest finished solve.
    pub worst: Option<u64>,
    pub mean: Option<u64>,
    pub std_dev_ms: Option<f64>,
    /// `(n, current aoN, best aoN)` for each of `REPORT_AVERAGES`.
    pub averages: Vec<(usize, Option<TimeResult>, Option<TimeResult>)>,
}

pub fn report(results: &[TimeResult]) -> Report {
    let times: Vec<u64> = results
        .iter()
        .filter_map(|r| match r {
            TimeResult::Ms(ms) => Some(*ms),
            TimeResult::Dnf => None,
        })
        .collect();
    Report {
        count: results.len(),
        finished: times.len(),
        best: best(results),
        worst: times.iter().copied().max(),
        mean: session_mean(results),
        std_dev_ms: crate::chart::std_dev(&times),
        averages: REPORT_AVERAGES
            .iter()
            .map(|&n| (n, ao(results, n), best_ao(results, n)))
            .collect(),
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
    fn rolling_ao_matches_the_average_of_every_prefix() {
        // a fixed pseudo-random mix of times, ties and DNFs
        let mut x: u64 = 12_345;
        let results: Vec<TimeResult> = (0..400)
            .map(|_| {
                x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                match (x >> 33) % 13 {
                    0 => Dnf,
                    1 => Ms(10_000), // ties
                    k => Ms(8_000 + k * 517 + (x >> 40) % 3_000),
                }
            })
            .collect();
        for n in [3, 5, 12, 21, 50, 100] {
            let rolling = rolling_ao(&results, n);
            assert_eq!(rolling.len(), results.len() + 1 - n, "ao{n}");
            for &(i, avg) in &rolling {
                assert_eq!(Some(avg), ao(&results[..=i], n), "ao{n} at {i}");
            }
            let naive = (n..=results.len())
                .filter_map(|e| ao(&results[..e], n))
                .min();
            assert_eq!(best_ao(&results, n), naive, "best ao{n}");
        }
        assert!(rolling_ao(&results[..4], 5).is_empty());
        assert!(rolling_ao(&results, 2).is_empty());
    }

    #[test]
    fn absurd_times_saturate_instead_of_overflowing() {
        // a hand-edited or imported file can hold any u64
        let huge = u64::MAX - 1;
        assert_eq!(TimeResult::new(u64::MAX, Penalty::Plus2), Ms(u64::MAX));
        let r = [Ms(huge), Ms(huge), Ms(huge), Ms(huge), Ms(huge)];
        assert_eq!(ao(&r, 5), Some(Ms(huge)));
        assert_eq!(best_ao(&r, 5), Some(Ms(huge)));
        assert_eq!(mo(&r, 3), Some(Ms(huge)));
        assert_eq!(session_mean(&r), Some(huge));
        report(&r);
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

    #[test]
    fn report_summarizes_a_session() {
        let r = report(&[Ms(2000), Ms(4000), Dnf, Ms(6000), Ms(8000)]);
        assert_eq!((r.count, r.finished), (5, 4));
        assert_eq!(
            (r.best, r.worst, r.mean),
            (Some(Ms(2000)), Some(8000), Some(5000))
        );
        assert!((r.std_dev_ms.unwrap() - 2236.07).abs() < 0.01);
        // 5 solves: ao5 exists (the DNF is the dropped worst), longer ones don't
        let ao5 = r.averages.iter().find(|a| a.0 == 5).unwrap();
        assert_eq!(ao5.1, Some(Ms(6000)));
        assert!(
            r.averages
                .iter()
                .filter(|a| a.0 > 5)
                .all(|a| a.1.is_none() && a.2.is_none())
        );
        assert_eq!(r.averages.len(), REPORT_AVERAGES.len());
    }

    #[test]
    fn report_of_nothing_and_of_all_dnfs() {
        let none = report(&[]);
        assert_eq!(
            (none.count, none.finished, none.best, none.worst),
            (0, 0, None, None)
        );
        assert!(none.mean.is_none() && none.std_dev_ms.is_none());
        let dnfs = report(&[Dnf, Dnf]);
        assert_eq!((dnfs.count, dnfs.finished, dnfs.best), (2, 0, Some(Dnf)));
        assert_eq!(dnfs.worst, None);
    }

    #[test]
    fn report_handles_a_big_session_quickly() {
        // a csTimer import can be this big; re-sorting every window took
        // seconds here, one sliding pass takes milliseconds
        let results: Vec<TimeResult> = (0..200_000)
            .map(|i| {
                if i % 97 == 0 {
                    Dnf
                } else {
                    Ms(10_000 + (i * 7919) % 4000)
                }
            })
            .collect();
        let start = std::time::Instant::now();
        let r = report(&results);
        assert!(
            start.elapsed() < std::time::Duration::from_secs(1),
            "{:?}",
            start.elapsed()
        );
        assert!(r.averages.iter().all(|a| a.1.is_some() && a.2.is_some()));
    }

    #[test]
    fn average_details_bracket_the_dropped_times() {
        use crate::fmt::Precision::Centi;
        let r = [Ms(12_100), Ms(13_200), Ms(14_000), Ms(13_800), Ms(16_430)];
        assert_eq!(
            ao_details(&r, 5, Centi).unwrap(),
            "(12.10) 13.20 14.00 13.80 (16.43) = 13.67"
        );
        // only the last n solves, in order; a DNF is the dropped worst
        let r = [Ms(1), Ms(10_000), Ms(11_000), Dnf, Ms(9_000), Ms(12_000)];
        assert_eq!(
            ao_details(&r, 5, Centi).unwrap(),
            "10.00 11.00 (DNF) (9.00) 12.00 = 11.00"
        );
        // equal times drop one of each
        let r = [Ms(5_000); 5];
        assert_eq!(
            ao_details(&r, 5, Centi).unwrap(),
            "(5.00) 5.00 5.00 5.00 (5.00) = 5.00"
        );
        assert!(ao_details(&r[..4], 5, Centi).is_none());
        // too many DNFs
        let r = [Ms(1000), Ms(2000), Ms(3000), Dnf, Dnf];
        assert!(ao_details(&r, 5, Centi).unwrap().ends_with("= DNF"));
    }
}
