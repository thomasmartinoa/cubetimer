//! The numbers behind the charts: round axis ticks, outlier handling, rolling
//! averages, personal bests and histograms. Everything here is plain maths so
//! it can be tested without drawing anything. Times are in seconds as `f64`
//! because that is what the plotting wants.

use crate::stats::{self, TimeResult};

/// A y axis: `ticks` are evenly spaced round values, `lo` and `hi` the first
/// and last of them.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    pub lo: f64,
    pub hi: f64,
    pub ticks: Vec<f64>,
}

/// A step of 1, 2 or 5 times a power of ten such that `range` needs at most
/// `max_ticks` intervals.
pub fn nice_step(range: f64, max_ticks: usize) -> f64 {
    let max_ticks = max_ticks.max(1) as f64;
    let raw = (range / max_ticks).max(1e-9);
    let magnitude = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0]
        .iter()
        .map(|m| m * magnitude)
        .find(|&step| step >= raw)
        .unwrap_or(10.0 * magnitude)
}

/// Round ticks covering `lo..=hi` with at most `max_ticks` intervals. The
/// axis extends to the first tick at or below `lo` and the last at or above
/// `hi`. A flat range (`lo == hi`) still gets a usable axis.
pub fn nice_axis(lo: f64, hi: f64, max_ticks: usize) -> Axis {
    let (lo, hi) = if hi - lo < 1e-6 {
        (lo - 0.5, hi + 0.5)
    } else {
        (lo, hi)
    };
    let step = nice_step(hi - lo, max_ticks);
    let first = (lo / step).floor() as i64;
    let last = (hi / step).ceil() as i64;
    let ticks: Vec<f64> = (first..=last)
        .map(|k| (k as f64 * step * 1e9).round() / 1e9)
        .collect();
    Axis {
        lo: ticks[0],
        hi: *ticks.last().unwrap_or(&hi),
        ticks,
    }
}

/// The `p`th percentile (0..=100) of values sorted ascending, by linear
/// interpolation. `None` when there are no values.
pub fn percentile(sorted: &[f64], p: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = p.clamp(0.0, 100.0) / 100.0 * (sorted.len() - 1) as f64;
    let (below, above) = (rank.floor() as usize, rank.ceil() as usize);
    Some(sorted[below] + (sorted[above] - sorted[below]) * (rank - below as f64))
}

/// How many solves the chart covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Range {
    Last50,
    #[default]
    Last100,
    Last500,
    All,
}

impl Range {
    pub const ALL: [Range; 4] = [Range::Last50, Range::Last100, Range::Last500, Range::All];

    pub fn label(self) -> &'static str {
        match self {
            Range::Last50 => "last 50",
            Range::Last100 => "last 100",
            Range::Last500 => "last 500",
            Range::All => "all",
        }
    }

    pub fn next(self) -> Range {
        let i = Range::ALL.iter().position(|&r| r == self).unwrap_or(0);
        Range::ALL[(i + 1) % Range::ALL.len()]
    }

    /// Index of the first solve shown out of `len`.
    pub fn start(self, len: usize) -> usize {
        let keep = match self {
            Range::Last50 => 50,
            Range::Last100 => 100,
            Range::Last500 => 500,
            Range::All => usize::MAX,
        };
        len.saturating_sub(keep)
    }
}

/// Everything the main chart draws, in solve numbers (1-based) and seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Series {
    /// Finished solves that fit on the axis.
    pub times: Vec<(f64, f64)>,
    /// Solves slower than the axis, drawn at its top edge.
    pub slow: Vec<(f64, f64)>,
    /// DNFs, drawn along the bottom edge.
    pub dnfs: Vec<(f64, f64)>,
    /// Solves that set a new best single.
    pub personal_bests: Vec<(f64, f64)>,
    pub ao5: Vec<(f64, f64)>,
    pub ao12: Vec<(f64, f64)>,
    pub axis: Axis,
    /// First and last solve number on the x axis.
    pub x: (f64, f64),
}

fn secs(ms: u64) -> f64 {
    ms as f64 / 1000.0
}

/// Rolling average of the last `n` solves ending at each solve from `from`
/// on (a DNF average is left out), as `(solve number, seconds)`.
pub fn rolling_ao(results: &[TimeResult], n: usize, from: usize) -> Vec<(f64, f64)> {
    (from.max(n.saturating_sub(1))..results.len())
        .filter_map(|i| match stats::ao(&results[..=i], n) {
            Some(TimeResult::Ms(ms)) => Some(((i + 1) as f64, secs(ms))),
            _ => None,
        })
        .collect()
}

/// Indices of the solves that beat every earlier finished solve. The first
/// solve counts, so a session's line starts with a marker.
pub fn personal_best_indices(results: &[TimeResult]) -> Vec<usize> {
    let mut best = u64::MAX;
    let mut out = Vec::new();
    for (i, r) in results.iter().enumerate() {
        if let TimeResult::Ms(ms) = r
            && *ms < best
        {
            best = *ms;
            out.push(i);
        }
    }
    out
}

/// The chart data for `results[from..]`. The y axis runs from the fastest
/// time to the 98th percentile (so a few terrible solves can't flatten the
/// rest), padded a little and snapped to round values.
pub fn series(results: &[TimeResult], from: usize, max_ticks: usize) -> Option<Series> {
    let from = from.min(results.len());
    let shown = &results[from..];
    let mut finished: Vec<f64> = shown
        .iter()
        .filter_map(|r| match r {
            TimeResult::Ms(ms) => Some(secs(*ms)),
            TimeResult::Dnf => None,
        })
        .collect();
    if finished.is_empty() {
        return None;
    }
    finished.sort_by(|a, b| a.total_cmp(b));
    let fastest = finished[0];
    let cap = percentile(&finished, 98.0).unwrap_or(fastest);
    let pad = ((cap - fastest) * 0.08).max(0.05);
    let axis = nice_axis((fastest - pad).max(0.0), cap + pad, max_ticks);

    let (mut times, mut slow, mut dnfs) = (Vec::new(), Vec::new(), Vec::new());
    for (k, r) in shown.iter().enumerate() {
        let x = (from + k + 1) as f64;
        match r {
            TimeResult::Ms(ms) if secs(*ms) > axis.hi => slow.push((x, axis.hi)),
            TimeResult::Ms(ms) => times.push((x, secs(*ms))),
            TimeResult::Dnf => dnfs.push((x, axis.lo)),
        }
    }
    let personal_bests = personal_best_indices(results)
        .into_iter()
        .filter(|&i| i >= from)
        .filter_map(|i| match results[i] {
            TimeResult::Ms(ms) => Some(((i + 1) as f64, secs(ms))),
            TimeResult::Dnf => None,
        })
        .collect();
    let (first, last) = ((from + 1) as f64, results.len() as f64);
    Some(Series {
        times,
        slow,
        dnfs,
        personal_bests,
        ao5: rolling_ao(results, 5, from),
        ao12: rolling_ao(results, 12, from),
        axis,
        x: (first, last.max(first + 1.0)),
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    pub lo: f64,
    pub hi: f64,
    pub count: usize,
}

/// Histogram of finished times: buckets of a round width, no more than
/// `max_buckets`, covering the 2nd to 98th percentile (slower and faster ones
/// go into the end buckets so every solve is counted).
pub fn histogram(times: &[f64], max_buckets: usize) -> Vec<Bucket> {
    if times.is_empty() || max_buckets == 0 {
        return Vec::new();
    }
    let mut sorted = times.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let lo = percentile(&sorted, 2.0).unwrap_or(sorted[0]);
    let hi = percentile(&sorted, 98.0).unwrap_or(sorted[sorted.len() - 1]);
    let axis = nice_axis(lo, hi, max_buckets);
    let n = axis.ticks.len() - 1;
    let mut buckets: Vec<Bucket> = (0..n)
        .map(|i| Bucket {
            lo: axis.ticks[i],
            hi: axis.ticks[i + 1],
            count: 0,
        })
        .collect();
    for &t in times {
        // find the bucket by position; outside the range goes to the ends
        let i = axis.ticks[1..n]
            .iter()
            .take_while(|&&edge| t >= edge)
            .count();
        buckets[i].count += 1;
    }
    buckets
}

/// Standard deviation of the finished times in ms (population formula, like
/// csTimer), `None` with fewer than two.
pub fn std_dev(times_ms: &[u64]) -> Option<f64> {
    if times_ms.len() < 2 {
        return None;
    }
    let n = times_ms.len() as f64;
    let mean = times_ms.iter().map(|&t| t as f64).sum::<f64>() / n;
    let var = times_ms
        .iter()
        .map(|&t| (t as f64 - mean).powi(2))
        .sum::<f64>()
        / n;
    Some(var.sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;
    use TimeResult::{Dnf, Ms};

    fn r(ms: &[u64]) -> Vec<TimeResult> {
        ms.iter().map(|&m| Ms(m)).collect()
    }

    #[test]
    fn steps_are_one_two_or_five_times_a_power_of_ten() {
        for (range, ticks, step) in [
            (10.0, 10, 1.0),
            (10.0, 5, 2.0),
            (10.0, 4, 5.0),
            (3.0, 6, 0.5),
            (0.9, 9, 0.1),
            (1.0, 10, 0.1),
            (100.0, 8, 20.0),
            (7.3, 3, 5.0),
        ] {
            assert!(
                (nice_step(range, ticks) - step).abs() < 1e-9,
                "{range} {ticks}"
            );
        }
        for k in 1..200 {
            let range = k as f64 * 0.37;
            for ticks in 2..12 {
                let step = nice_step(range, ticks);
                assert!(
                    range / step <= ticks as f64 + 1e-9,
                    "{range} {ticks} {step}"
                );
                let m = 10f64.powf(step.log10().floor());
                let lead = (step / m).round();
                assert!([1.0, 2.0, 5.0, 10.0].contains(&lead), "{step}");
            }
        }
    }

    #[test]
    fn axis_covers_the_range_with_round_values() {
        let a = nice_axis(12.37, 16.81, 6);
        assert_eq!(a.ticks, vec![12.0, 13.0, 14.0, 15.0, 16.0, 17.0]);
        assert_eq!((a.lo, a.hi), (12.0, 17.0));
        let a = nice_axis(0.42, 0.97, 6);
        assert!(a.lo <= 0.42 && a.hi >= 0.97);
        assert!(a.ticks.len() <= 7);
        // evenly spaced: what the plotting widget assumes for its labels
        for w in a.ticks.windows(3) {
            assert!(((w[1] - w[0]) - (w[2] - w[1])).abs() < 1e-9);
        }
    }

    #[test]
    fn axis_handles_flat_and_tiny_ranges() {
        let flat = nice_axis(10.0, 10.0, 6);
        assert!(flat.lo < 10.0 && flat.hi > 10.0 && flat.ticks.len() >= 2);
        let one = nice_axis(0.0, 0.0, 6);
        assert!(one.ticks.len() >= 2);
        assert!(nice_axis(1.0, 1.0 + 1e-12, 6).hi > 1.0);
    }

    #[test]
    fn percentiles_interpolate() {
        let v = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile(&v, 0.0), Some(1.0));
        assert_eq!(percentile(&v, 100.0), Some(5.0));
        assert_eq!(percentile(&v, 50.0), Some(3.0));
        assert_eq!(percentile(&v, 25.0), Some(2.0));
        assert!((percentile(&v, 10.0).unwrap() - 1.4).abs() < 1e-9);
        assert_eq!(percentile(&[7.0], 98.0), Some(7.0));
        assert_eq!(percentile(&[], 50.0), None);
        assert_eq!(percentile(&v, 500.0), Some(5.0));
    }

    #[test]
    fn range_windows() {
        assert_eq!(Range::Last50.start(30), 0);
        assert_eq!(Range::Last50.start(50), 0);
        assert_eq!(Range::Last50.start(51), 1);
        assert_eq!(Range::Last100.start(1000), 900);
        assert_eq!(Range::Last500.start(1000), 500);
        assert_eq!(Range::All.start(1000), 0);
        assert_eq!(Range::default(), Range::Last100);
        let mut r = Range::Last50;
        for _ in 0..4 {
            r = r.next();
        }
        assert_eq!(r, Range::Last50);
        assert!(Range::ALL.iter().all(|r| !r.label().is_empty()));
    }

    #[test]
    fn personal_bests_follow_each_new_record() {
        let results = [
            Ms(5000),
            Ms(6000),
            Ms(4000),
            Dnf,
            Ms(4000),
            Ms(3900),
            Ms(5000),
        ];
        assert_eq!(personal_best_indices(&results), vec![0, 2, 5]);
        assert!(personal_best_indices(&[]).is_empty());
        assert!(personal_best_indices(&[Dnf, Dnf]).is_empty());
    }

    #[test]
    fn rolling_ao_matches_the_stats_functions() {
        let results = r(&[9000, 2000, 1000, 4000, 3000, 5000, 7000]);
        let ao5 = rolling_ao(&results, 5, 0);
        // windows ending at solves 5, 6 and 7
        assert_eq!(
            ao5.iter().map(|p| p.0).collect::<Vec<_>>(),
            vec![5.0, 6.0, 7.0]
        );
        // each drops its best and worst: (2+4+3)/3, (2+4+3)/3 and (4+3+5)/3
        assert_eq!(ao5[0].1, 3.0);
        assert_eq!(ao5[1].1, 3.0);
        assert_eq!(ao5[2].1, 4.0);
        // starting later keeps the same values for the points that remain
        assert_eq!(rolling_ao(&results, 5, 5), ao5[1..].to_vec());
        assert!(rolling_ao(&results, 12, 0).is_empty());
    }

    #[test]
    fn dnf_averages_are_left_out_of_the_line() {
        let results = [Ms(1000), Ms(2000), Ms(3000), Dnf, Dnf, Ms(4000)];
        let ao5 = rolling_ao(&results, 5, 0);
        assert!(
            ao5.iter().all(|&(x, _)| x != 5.0),
            "two DNFs make that average a DNF"
        );
    }

    #[test]
    fn one_terrible_solve_does_not_flatten_the_chart() {
        let mut times: Vec<u64> = (0..100).map(|i| 12_000 + (i % 7) * 100).collect();
        times.push(95_000); // a 95 s outlier
        let s = series(&r(&times), 0, 6).unwrap();
        // the axis fits the normal times (about 12 to 13 s), not 95 s
        assert!(s.axis.hi < 16.0, "{:?}", s.axis);
        assert!(s.axis.lo >= 11.0);
        // the outlier is still shown, clamped to the top edge
        assert_eq!(s.slow, vec![(101.0, s.axis.hi)]);
        assert_eq!(s.times.len(), 100);
    }

    #[test]
    fn dnfs_sit_on_the_bottom_edge() {
        let results = [Ms(10_000), Dnf, Ms(11_000), Ms(12_000)];
        let s = series(&results, 0, 6).unwrap();
        assert_eq!(s.dnfs, vec![(2.0, s.axis.lo)]);
        assert_eq!(s.times.len(), 3);
    }

    #[test]
    fn every_solve_is_in_exactly_one_group() {
        let mut v: Vec<TimeResult> = (0..60).map(|i| Ms(10_000 + (i * 137) % 3000)).collect();
        v[10] = Dnf;
        v[20] = Ms(120_000);
        let s = series(&v, 0, 6).unwrap();
        assert_eq!(s.times.len() + s.slow.len() + s.dnfs.len(), 60);
    }

    #[test]
    fn a_range_limits_the_points_but_keeps_solve_numbers() {
        let results: Vec<TimeResult> = (0..200).map(|i| Ms(10_000 + i)).collect();
        let from = Range::Last50.start(results.len());
        let s = series(&results, from, 6).unwrap();
        assert_eq!(s.times.len(), 50);
        assert_eq!(s.times[0].0, 151.0);
        assert_eq!(s.x, (151.0, 200.0));
        // averages for the first visible point use solves before the window
        assert_eq!(s.ao5[0].0, 151.0);
    }

    #[test]
    fn personal_bests_before_the_window_are_not_marked() {
        let results = [Ms(5000), Ms(9000), Ms(9500), Ms(9100), Ms(4000)];
        let s = series(&results, 2, 6).unwrap();
        assert_eq!(s.personal_bests, vec![(5.0, 4.0)]);
    }

    #[test]
    fn series_needs_a_finished_solve() {
        assert!(series(&[], 0, 6).is_none());
        assert!(series(&[Dnf, Dnf], 0, 6).is_none());
        let one = series(&r(&[10_000]), 0, 6).unwrap();
        assert_eq!(one.times.len(), 1);
        assert!(one.axis.lo < 10.0 && one.axis.hi > 10.0);
        assert!(one.x.1 > one.x.0, "x range is never empty");
    }

    #[test]
    fn identical_times_still_make_a_chart() {
        let s = series(&r(&[10_000; 30]), 0, 6).unwrap();
        assert_eq!(s.times.len(), 30);
        assert!(s.axis.ticks.len() >= 2);
    }

    #[test]
    fn histogram_counts_every_solve_once() {
        let times: Vec<f64> = (0..500).map(|i| 12.0 + (i % 50) as f64 * 0.1).collect();
        let h = histogram(&times, 10);
        assert!(h.len() <= 10 && h.len() >= 2);
        assert_eq!(h.iter().map(|b| b.count).sum::<usize>(), 500);
        // buckets are contiguous, and the same width
        for w in h.windows(2) {
            assert!((w[0].hi - w[1].lo).abs() < 1e-9);
            assert!(((w[0].hi - w[0].lo) - (w[1].hi - w[1].lo)).abs() < 1e-9);
        }
    }

    #[test]
    fn histogram_puts_outliers_in_the_end_buckets() {
        let mut times: Vec<f64> = vec![12.0; 98];
        times.push(1.0);
        times.push(300.0);
        let h = histogram(&times, 8);
        assert_eq!(h.iter().map(|b| b.count).sum::<usize>(), 100);
        assert!(h[0].count >= 1 && h.last().unwrap().count >= 1);
    }

    #[test]
    fn histogram_edge_cases() {
        assert!(histogram(&[], 10).is_empty());
        assert!(histogram(&[1.0], 0).is_empty());
        let one = histogram(&[12.0], 10);
        assert_eq!(one.iter().map(|b| b.count).sum::<usize>(), 1);
        let same = histogram(&[12.0; 40], 10);
        assert_eq!(same.iter().map(|b| b.count).sum::<usize>(), 40);
    }

    #[test]
    fn standard_deviation() {
        assert_eq!(std_dev(&[]), None);
        assert_eq!(std_dev(&[5000]), None);
        assert_eq!(std_dev(&[1000, 1000, 1000]), Some(0.0));
        // 2, 4, 4, 4, 5, 5, 7, 9 has a population deviation of exactly 2
        let v: Vec<u64> = [2, 4, 4, 4, 5, 5, 7, 9].iter().map(|x| x * 1000).collect();
        assert!((std_dev(&v).unwrap() - 2000.0).abs() < 1e-9);
    }
}
