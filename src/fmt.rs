//! Time formatting that matches the WCA, csTimer and Stackmat conventions.
//! Times are integer milliseconds; they are only turned into text here.

use crate::stats::{Penalty, TimeResult};

/// Display precision for times.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Precision {
    /// Hundredths (`12.34`), the WCA default.
    #[default]
    Centi,
    /// Thousandths (`12.345`). Not user-selectable yet.
    #[allow(dead_code)]
    Milli,
}

impl Precision {
    fn digits(self) -> u32 {
        match self {
            Self::Centi => 2,
            Self::Milli => 3,
        }
    }
}

/// How the time is shown while the timer is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunDisplay {
    #[default]
    Full,
    Seconds,
    Hidden,
}

impl RunDisplay {
    pub fn next(self) -> Self {
        match self {
            Self::Full => Self::Seconds,
            Self::Seconds => Self::Hidden,
            Self::Hidden => Self::Full,
        }
    }
}

/// Format `total` units (cs or ms depending on `digits`) as `s.cc`, or
/// `m:ss.cc` from 60 s up.
fn fmt_units(total: u64, digits: u32) -> String {
    let div = 10u64.pow(digits);
    let (secs, frac) = (total / div, total % div);
    let w = digits as usize;
    if secs >= 60 {
        format!("{}:{:02}.{frac:0w$}", secs / 60, secs % 60)
    } else {
        format!("{secs}.{frac:0w$}")
    }
}

/// A single time, truncated (never rounded) to the precision.
pub fn fmt_time(ms: u64, prec: Precision) -> String {
    let total = match prec {
        Precision::Centi => ms / 10,
        Precision::Milli => ms,
    };
    fmt_units(total, prec.digits())
}

/// An average or mean, rounded to the nearest hundredth. `None` (not enough
/// solves yet) shows as `-`.
pub fn fmt_avg(avg: Option<TimeResult>, prec: Precision) -> String {
    match avg {
        None => "-".into(),
        Some(TimeResult::Dnf) => "DNF".into(),
        Some(TimeResult::Ms(ms)) => fmt_avg_ms(ms, prec),
    }
}

/// A single result (best time): truncated like any single, `-` when absent.
pub fn fmt_single(result: Option<TimeResult>, prec: Precision) -> String {
    match result {
        None => "-".into(),
        Some(TimeResult::Dnf) => "DNF".into(),
        Some(TimeResult::Ms(ms)) => fmt_time(ms, prec),
    }
}

/// A mean in ms (such as the session mean), rounded like an average.
pub fn fmt_avg_ms(ms: u64, prec: Precision) -> String {
    let total = match prec {
        Precision::Centi => (ms + 5) / 10,
        Precision::Milli => ms,
    };
    fmt_units(total, prec.digits())
}

/// A solve on the big display: `14.34+` for +2, plain `DNF` for a DNF.
pub fn fmt_solve(raw_ms: u64, penalty: Penalty, prec: Precision) -> String {
    match penalty {
        Penalty::Ok => fmt_time(raw_ms, prec),
        Penalty::Plus2 => format!("{}+", fmt_time(raw_ms + 2000, prec)),
        Penalty::Dnf => "DNF".into(),
    }
}

/// A solve in the recent-times list: a DNF keeps its raw time, `DNF(12.34)`.
pub fn fmt_solve_list(raw_ms: u64, penalty: Penalty, prec: Precision) -> String {
    match penalty {
        Penalty::Dnf => format!("DNF({})", fmt_time(raw_ms, prec)),
        _ => fmt_solve(raw_ms, penalty, prec),
    }
}

/// The time shown while running, per the `p` display mode.
pub fn fmt_running(ms: u64, mode: RunDisplay, prec: Precision) -> String {
    match mode {
        RunDisplay::Full => fmt_time(ms, prec),
        RunDisplay::Seconds => {
            let s = ms / 1000;
            if s >= 60 {
                format!("{}:{:02}", s / 60, s % 60)
            } else {
                s.to_string()
            }
        }
        RunDisplay::Hidden => "solving...".into(),
    }
}

/// Inspection countdown in whole seconds: `15 … 1`, then `+2` from 15 s to
/// 17 s elapsed, then `DNF`.
pub fn fmt_inspection(elapsed_ms: u64) -> String {
    match elapsed_ms {
        0..15_000 => (15 - elapsed_ms / 1000).to_string(),
        15_000..17_000 => "+2".into(),
        _ => "DNF".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Precision::{Centi, Milli};

    #[test]
    fn truncates_not_rounds() {
        assert_eq!(fmt_time(12_349, Centi), "12.34");
        assert_eq!(fmt_time(12_340, Centi), "12.34");
        assert_eq!(fmt_time(12_999, Centi), "12.99");
    }

    #[test]
    fn boundaries() {
        assert_eq!(fmt_time(59_999, Centi), "59.99");
        assert_eq!(fmt_time(60_000, Centi), "1:00.00");
        assert_eq!(fmt_time(599_999, Centi), "9:59.99");
        assert_eq!(fmt_time(600_000, Centi), "10:00.00");
        assert_eq!(fmt_time(603_000, Centi), "10:03.00");
        assert_eq!(fmt_time(62_340, Centi), "1:02.34");
        assert_eq!(fmt_time(9, Centi), "0.00");
        assert_eq!(fmt_time(0, Centi), "0.00");
    }

    #[test]
    fn no_leading_zeros_under_a_minute() {
        assert_eq!(fmt_time(7_050, Centi), "7.05");
        assert_eq!(fmt_time(14_210, Centi), "14.21");
    }

    #[test]
    fn milli_precision() {
        assert_eq!(fmt_time(12_345, Milli), "12.345");
        assert_eq!(fmt_time(7_005, Milli), "7.005");
        assert_eq!(fmt_time(59_999, Milli), "59.999");
        assert_eq!(fmt_time(60_000, Milli), "1:00.000");
        assert_eq!(fmt_time(0, Milli), "0.000");
    }

    #[test]
    fn averages_round_to_nearest_hundredth() {
        let a = |ms| fmt_avg(Some(TimeResult::Ms(ms)), Centi);
        assert_eq!(a(12_344), "12.34");
        assert_eq!(a(12_345), "12.35");
        assert_eq!(a(12_349), "12.35");
        assert_eq!(a(59_995), "1:00.00");
        assert_eq!(a(4), "0.00");
        assert_eq!(a(5), "0.01");
    }

    #[test]
    fn rounding_vs_truncation() {
        assert_eq!(fmt_time(12_345, Centi), "12.34");
        assert_eq!(fmt_avg_ms(12_345, Centi), "12.35");
    }

    #[test]
    fn missing_and_dnf_averages() {
        assert_eq!(fmt_avg(None, Centi), "-");
        assert_eq!(fmt_avg(Some(TimeResult::Dnf), Centi), "DNF");
    }

    #[test]
    fn single_results() {
        assert_eq!(fmt_single(Some(TimeResult::Ms(12_349)), Centi), "12.34");
        assert_eq!(fmt_single(Some(TimeResult::Dnf), Centi), "DNF");
        assert_eq!(fmt_single(None, Centi), "-");
    }

    #[test]
    fn solve_penalties() {
        assert_eq!(fmt_solve(12_340, Penalty::Ok, Centi), "12.34");
        assert_eq!(fmt_solve(12_340, Penalty::Plus2, Centi), "14.34+");
        assert_eq!(fmt_solve(12_340, Penalty::Dnf, Centi), "DNF");
        assert_eq!(fmt_solve(58_500, Penalty::Plus2, Centi), "1:00.50+");
        assert_eq!(fmt_solve(12_345, Penalty::Plus2, Milli), "14.345+");
    }

    #[test]
    fn list_keeps_raw_time_for_dnf() {
        assert_eq!(fmt_solve_list(12_340, Penalty::Dnf, Centi), "DNF(12.34)");
        assert_eq!(fmt_solve_list(12_340, Penalty::Plus2, Centi), "14.34+");
        assert_eq!(fmt_solve_list(12_349, Penalty::Ok, Centi), "12.34");
    }

    #[test]
    fn running_modes() {
        assert_eq!(fmt_running(12_349, RunDisplay::Full, Centi), "12.34");
        assert_eq!(fmt_running(12_999, RunDisplay::Seconds, Centi), "12");
        assert_eq!(fmt_running(75_000, RunDisplay::Seconds, Centi), "1:15");
        assert_eq!(fmt_running(5_000, RunDisplay::Hidden, Centi), "solving...");
    }

    #[test]
    fn run_display_cycles() {
        let m = RunDisplay::Full;
        assert_eq!(m.next().next().next(), m);
    }

    #[test]
    fn inspection_countdown() {
        assert_eq!(fmt_inspection(0), "15");
        assert_eq!(fmt_inspection(999), "15");
        assert_eq!(fmt_inspection(1_000), "14");
        assert_eq!(fmt_inspection(14_000), "1");
        assert_eq!(fmt_inspection(14_999), "1");
        assert_eq!(fmt_inspection(15_000), "+2");
        assert_eq!(fmt_inspection(16_999), "+2");
        assert_eq!(fmt_inspection(17_000), "DNF");
    }
}
