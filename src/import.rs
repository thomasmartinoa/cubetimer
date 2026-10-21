//! Importing a csTimer export (Export → "To file" in csTimer).
//!
//! The file is JSON: `session1`, `session2`, ... each a list of solves
//! `[[penalty, time_ms], scramble, comment, timestamp]`, where the penalty is
//! 0 (none), 2000 (+2) or -1 (DNF) and the time is the raw time without the
//! penalty. Which puzzle a session is comes from `properties.sessionData`
//! (a JSON string) under `opt.scrType`. Anything missing is tolerated: a
//! solve needs only its time.

use serde_json::Value;

use crate::puzzle::Puzzle;
use crate::stats::Penalty;
use crate::store::{Data, Session, Solve};

/// What an import did (or would do).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub sessions: usize,
    pub solves: usize,
    /// Sessions of puzzles cubetimer doesn't have (pyraminx, ...), by csTimer
    /// scramble type, with their solve counts.
    pub skipped: Vec<(String, usize)>,
    /// Solves that couldn't be read.
    pub bad_solves: usize,
}

/// The puzzle for a csTimer scramble type (`333`, `333oh`, `444wca`, ...).
fn puzzle_for(scr_type: &str) -> Option<Puzzle> {
    // csTimer names start with the puzzle size: 222so, 333ni, 444wca, ...
    match scr_type.get(..3)? {
        "222" => Some(Puzzle::Cube2),
        "333" => Some(Puzzle::Cube3),
        "444" => Some(Puzzle::Cube4),
        "555" => Some(Puzzle::Cube5),
        "666" => Some(Puzzle::Cube6),
        "777" => Some(Puzzle::Cube7),
        _ => None,
    }
}

fn session_info(root: &Value) -> Value {
    root.pointer("/properties/sessionData")
        .and_then(Value::as_str)
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null)
}

fn parse_solve(v: &Value) -> Option<Solve> {
    let head = v.get(0)?;
    let penalty_raw = head.get(0).and_then(Value::as_i64).unwrap_or(0);
    let ms = head.get(1).and_then(Value::as_u64)?;
    let penalty = match penalty_raw {
        -1 => Penalty::Dnf,
        p if p > 0 => Penalty::Plus2,
        _ => Penalty::Ok,
    };
    let text = |i: usize| {
        v.get(i)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    };
    let mut solve = Solve::new(ms, penalty, text(1).unwrap_or_default().to_string());
    solve.ts = v.get(3).and_then(Value::as_u64).unwrap_or(0);
    solve.comment = text(2).map(String::from);
    Some(solve)
}

/// Read a csTimer export. `Err` if it isn't one.
pub fn parse(text: &str) -> Result<(Vec<Session>, Summary), String> {
    let root: Value =
        serde_json::from_str(text).map_err(|e| format!("not a csTimer export: {e}"))?;
    let obj = root
        .as_object()
        .ok_or("not a csTimer export: expected a JSON object")?;
    let info = session_info(&root);
    // session1, session2, ... in numeric order
    let mut keys: Vec<(u32, &String)> = obj
        .keys()
        .filter_map(|k| k.strip_prefix("session")?.parse().ok().map(|n| (n, k)))
        .collect();
    keys.sort();
    if keys.is_empty() {
        return Err("not a csTimer export: no sessions found".into());
    }
    let (mut sessions, mut summary) = (Vec::new(), Summary::default());
    for (n, key) in keys {
        let Some(list) = obj[key].as_array() else {
            continue;
        };
        let meta = &info[n.to_string()];
        let scr_type = meta
            .pointer("/opt/scrType")
            .and_then(Value::as_str)
            .unwrap_or("333");
        let Some(puzzle) = puzzle_for(scr_type) else {
            if !list.is_empty() {
                summary.skipped.push((scr_type.to_string(), list.len()));
            }
            continue;
        };
        let mut session = Session::new(puzzle);
        // csTimer's default name is the session number; keep only real names
        session.name = meta
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.parse::<u32>().is_err())
            .map(String::from);
        for v in list {
            match parse_solve(v) {
                Some(s) => session.solves.push(s),
                None => summary.bad_solves += 1,
            }
        }
        if session.solves.is_empty() {
            continue;
        }
        summary.sessions += 1;
        summary.solves += session.solves.len();
        sessions.push(session);
    }
    Ok((sessions, summary))
}

/// Add imported sessions to `data` (after the existing ones).
pub fn merge(data: &mut Data, sessions: Vec<Session>) {
    data.sessions.extend(sessions);
}

impl Summary {
    pub fn describe(&self, dry_run: bool) -> String {
        let verb = if dry_run { "would import" } else { "imported" };
        let mut out = format!(
            "{verb} {} solves in {} sessions",
            self.solves, self.sessions
        );
        for (kind, n) in &self.skipped {
            out += &format!("\nskipped {n} solves of {kind} (cubetimer has cubes only)");
        }
        if self.bad_solves > 0 {
            out += &format!(
                "\nskipped {} solves that could not be read",
                self.bad_solves
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXPORT: &str = r#"{
      "session1": [
        [[0, 12340], "R U R' U'", "", 1700000000],
        [[2000, 15000], "F2 D", "lockup", 1700000100],
        [[-1, 20000], "B", "", 1700000200]
      ],
      "session2": [[[0, 50000], "Rw U", "", 1700001000]],
      "session3": [[[0, 9000], "R U", "", 1700002000]],
      "session4": [],
      "properties": {"sessionData": "{\"1\":{\"name\":1,\"opt\":{}},\"2\":{\"name\":\"big cubes\",\"opt\":{\"scrType\":\"444wca\"}},\"3\":{\"name\":3,\"opt\":{\"scrType\":\"pyrso\"}}}"}
    }"#;

    #[test]
    fn reads_sessions_solves_penalties_and_puzzles() {
        let (sessions, summary) = parse(EXPORT).unwrap();
        assert_eq!(sessions.len(), 2);
        let s = &sessions[0];
        assert_eq!(s.puzzle(), Puzzle::Cube3);
        assert_eq!(s.name, None, "the default number name isn't kept");
        assert_eq!(s.solves.len(), 3);
        let times: Vec<(u64, Penalty)> = s.solves.iter().map(|x| (x.ms, x.penalty)).collect();
        assert_eq!(
            times,
            [
                (12_340, Penalty::Ok),
                (15_000, Penalty::Plus2),
                (20_000, Penalty::Dnf)
            ]
        );
        assert_eq!(s.solves[1].comment.as_deref(), Some("lockup"));
        assert_eq!(
            (s.solves[0].scramble.as_str(), s.solves[0].ts),
            ("R U R' U'", 1_700_000_000)
        );
        assert_eq!(sessions[1].puzzle(), Puzzle::Cube4);
        assert_eq!(sessions[1].name.as_deref(), Some("big cubes"));
        assert_eq!((summary.sessions, summary.solves), (2, 4));
        assert_eq!(summary.skipped, [("pyrso".to_string(), 1)]);
        assert_eq!(summary.bad_solves, 0);
    }

    #[test]
    fn plus_two_keeps_the_raw_time_so_the_result_is_penalized_once() {
        let (sessions, _) = parse(EXPORT).unwrap();
        let s = &sessions[0].solves[1];
        assert_eq!(s.result(), crate::stats::TimeResult::Ms(17_000));
    }

    #[test]
    fn tolerates_missing_fields_and_odd_solves() {
        let text = r#"{"session1": [[[0, 1000]], [[0, 2000], "R"], "junk", [[0]], [[0, 3000], "U", null, null]]}"#;
        let (sessions, summary) = parse(text).unwrap();
        assert_eq!(sessions[0].solves.len(), 3);
        assert_eq!(summary.bad_solves, 2);
        assert_eq!(sessions[0].solves[0].scramble, "");
        assert_eq!(sessions[0].solves[2].ts, 0);
        // no sessionData at all: everything is 3x3
        assert_eq!(sessions[0].puzzle(), Puzzle::Cube3);
    }

    #[test]
    fn rejects_files_that_are_not_exports() {
        for bad in ["", "not json", "[1,2]", "{}", r#"{"a": 1}"#] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn sessions_come_in_numeric_order() {
        let text = r#"{"session10": [[[0, 10000]]], "session2": [[[0, 2000]]], "session1": [[[0, 1000]]]}"#;
        let (sessions, _) = parse(text).unwrap();
        let first: Vec<u64> = sessions.iter().map(|s| s.solves[0].ms).collect();
        assert_eq!(first, [1_000, 2_000, 10_000]);
    }

    #[test]
    fn scramble_types_map_to_cubes() {
        for (t, p) in [
            ("333", Some(Puzzle::Cube3)),
            ("333oh", Some(Puzzle::Cube3)),
            ("222so", Some(Puzzle::Cube2)),
            ("444wca", Some(Puzzle::Cube4)),
            ("555wca", Some(Puzzle::Cube5)),
            ("666wca", Some(Puzzle::Cube6)),
            ("777wca", Some(Puzzle::Cube7)),
            ("pyrso", None),
            ("skbso", None),
            ("sqrs", None),
            ("", None),
        ] {
            assert_eq!(puzzle_for(t), p, "{t}");
        }
    }

    #[test]
    fn merging_adds_sessions_after_the_existing_ones_and_exports_back() {
        let mut data = Data::default();
        data.solves_mut()
            .push(Solve::new(9_000, Penalty::Ok, "R".into()));
        let (sessions, summary) = parse(EXPORT).unwrap();
        merge(&mut data, sessions);
        assert_eq!(data.sessions.len(), 3);
        assert_eq!(
            data.sessions[0].solves[0].ms, 9_000,
            "your own solves are untouched"
        );
        assert!(
            summary
                .describe(true)
                .starts_with("would import 4 solves in 2 sessions")
        );
        assert!(
            summary
                .describe(false)
                .contains("skipped 1 solves of pyrso")
        );
        // and the CSV export sees them
        assert!(
            crate::store::to_csv(&data).contains("1.800,")
                || crate::store::to_csv(&data).contains("17.000")
        );
    }
}
