//! Solve records and session persistence.
//!
//! Sessions live in `<data dir>/cubetimer/sessions.json` and are written
//! atomically (temp file, then rename).

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::fmt;
use crate::puzzle::{self, Puzzle};
use crate::stats::{Penalty, TimeResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Solve {
    /// Raw time in milliseconds, before any penalty.
    pub ms: u64,
    pub penalty: Penalty,
    pub scramble: String,
    /// Unix timestamp in seconds.
    pub ts: u64,
}

impl Solve {
    pub fn new(ms: u64, penalty: Penalty, scramble: String) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        Self {
            ms,
            penalty,
            scramble,
            ts,
        }
    }

    pub fn result(&self) -> TimeResult {
        TimeResult::new(self.ms, self.penalty)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub solves: Vec<Solve>,
    /// WCA event id (`333`, `222`, ...). Files from before puzzles existed
    /// have none, and those sessions are 3x3.
    #[serde(default = "default_puzzle_id")]
    pub puzzle: String,
}

fn default_puzzle_id() -> String {
    puzzle::DEFAULT_ID.into()
}

impl Default for Session {
    fn default() -> Self {
        Session::new(Puzzle::Cube3)
    }
}

impl Session {
    pub fn new(puzzle: Puzzle) -> Session {
        Session {
            solves: Vec::new(),
            puzzle: puzzle.id().into(),
        }
    }

    pub fn puzzle(&self) -> Puzzle {
        Puzzle::from_id_or_default(&self.puzzle)
    }
}

/// Preferences that survive a restart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Name of the digit font, see `font::FONTS`.
    pub font: String,
    pub inspection: bool,
    pub bell: bool,
    /// How the 3D cube is drawn, see `view3d::Look`.
    pub look: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font: crate::font::DEFAULT.into(),
            inspection: true,
            bell: false,
            look: crate::view3d::Look::default().name().into(),
        }
    }
}

/// Everything saved to disk. Always holds at least one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Data {
    pub sessions: Vec<Session>,
    /// Index of the session being timed.
    #[serde(default)]
    pub current: usize,
    #[serde(default)]
    pub settings: Settings,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            sessions: vec![Session::default()],
            current: 0,
            settings: Settings::default(),
        }
    }
}

impl Data {
    /// Repair what a hand-edited file might break: no sessions, bad index.
    fn normalize(mut self) -> Self {
        if self.sessions.is_empty() {
            self.sessions.push(Session::default());
        }
        self.current = self.current.min(self.sessions.len() - 1);
        self
    }

    pub fn solves(&self) -> &[Solve] {
        &self.sessions[self.current].solves
    }

    /// The puzzle of the session being timed.
    pub fn puzzle(&self) -> Puzzle {
        self.sessions[self.current].puzzle()
    }

    /// Indices of every session of `puzzle`, in order.
    pub fn sessions_of(&self, puzzle: Puzzle) -> Vec<usize> {
        (0..self.sessions.len())
            .filter(|&i| self.sessions[i].puzzle() == puzzle)
            .collect()
    }

    pub fn solves_mut(&mut self) -> &mut Vec<Solve> {
        &mut self.sessions[self.current].solves
    }
}

pub struct Loaded {
    pub data: Data,
    /// Set when the file was unreadable and set aside.
    pub warning: Option<String>,
}

/// Every solve as CSV, one row per solve, for spreadsheets and other tools.
/// Times are in seconds with millisecond precision; `result` is the time with
/// its penalty applied, or `DNF`.
pub fn to_csv(data: &Data) -> String {
    let field = |f: &str| {
        if f.contains([',', '"', '\n']) {
            format!("\"{}\"", f.replace('"', "\"\""))
        } else {
            f.to_string()
        }
    };
    let secs = |ms: u64| format!("{}.{:03}", ms / 1000, ms % 1000);
    let mut out = String::from("session,puzzle,solve,time,penalty,result,scramble,date\n");
    for (si, session) in data.sessions.iter().enumerate() {
        for (i, s) in session.solves.iter().enumerate() {
            let penalty = match s.penalty {
                Penalty::Ok => "ok",
                Penalty::Plus2 => "+2",
                Penalty::Dnf => "dnf",
            };
            let result = match s.result() {
                TimeResult::Ms(ms) => secs(ms),
                TimeResult::Dnf => "DNF".into(),
            };
            out += &format!(
                "{},{},{},{},{penalty},{result},{},{}\n",
                si + 1,
                session.puzzle().name(),
                i + 1,
                secs(s.ms),
                field(&s.scramble),
                fmt::fmt_iso(s.ts)
            );
        }
    }
    out
}

pub fn default_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("cubetimer").join("sessions.json"))
}

/// Load sessions. A missing file is an empty store. A file that can't be
/// parsed is renamed to `*.corrupt` (so the next save can't destroy it) and an
/// empty store is returned with a warning.
/// Read the sessions without changing anything on disk. A missing file is an
/// empty store; an unreadable one is an error saying why.
pub fn read(path: &Path) -> Result<Data, String> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<Data>(&text)
            .map(Data::normalize)
            .map_err(|e| e.to_string()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Data::default()),
        Err(e) => Err(e.to_string()),
    }
}

/// Load sessions for the timer. Like `read`, but a file that can't be read is
/// renamed to `*.corrupt` (so the next save can't destroy it) and an empty
/// store is returned with a warning.
pub fn load(path: &Path) -> Loaded {
    match read(path) {
        Ok(data) => Loaded {
            data,
            warning: None,
        },
        Err(reason) => set_aside(path, &reason),
    }
}

fn set_aside(path: &Path, reason: &str) -> Loaded {
    let mut aside = path.as_os_str().to_owned();
    aside.push(".corrupt");
    let warning = match fs::rename(path, &aside) {
        Ok(()) => format!(
            "could not read sessions ({reason}); kept as {}",
            Path::new(&aside).display()
        ),
        Err(e) => format!("could not read sessions ({reason}); could not move it aside: {e}"),
    };
    Loaded {
        data: Data::default(),
        warning: Some(warning),
    }
}

/// Take the lock that keeps two copies of cubetimer from overwriting each
/// other's solves: an exclusive lock on `sessions.lock` next to `path`, held
/// for as long as the returned file is open (the OS drops it if we crash).
/// `Ok(None)` means another copy holds it.
pub fn lock(path: &Path) -> io::Result<Option<File>> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(path.with_extension("lock"))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(e)) => Err(e),
    }
}

/// Write atomically: temp file in the same directory, fsync, rename.
pub fn save(path: &Path, data: &Data) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut file = File::create(&tmp)?;
    file.write_all(&serde_json::to_vec_pretty(data).map_err(io::Error::other)?)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("cubetimer-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> Data {
        let mut data = Data::default();
        data.solves_mut().push(Solve {
            ms: 12_340,
            penalty: Penalty::Plus2,
            scramble: "R U".into(),
            ts: 7,
        });
        data.solves_mut().push(Solve {
            ms: 9_990,
            penalty: Penalty::Dnf,
            scramble: "F2".into(),
            ts: 8,
        });
        data.sessions.push(Session::default());
        data.current = 1;
        data
    }

    #[test]
    fn round_trip() {
        let dir = temp_dir("roundtrip");
        let path = dir.join("nested").join("sessions.json");
        save(&path, &sample()).unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.data, sample());
        assert!(loaded.warning.is_none());
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn file_format() {
        let text = serde_json::to_string(&sample().sessions[0].solves[0]).unwrap();
        assert_eq!(
            text,
            r#"{"ms":12340,"penalty":"plus2","scramble":"R U","ts":7}"#
        );
    }

    #[test]
    fn settings_default_and_round_trip() {
        let dir = temp_dir("settings");
        let path = dir.join("sessions.json");
        // a file from before settings existed still loads, with defaults
        fs::write(&path, r#"{"sessions":[{"solves":[]}],"current":0}"#).unwrap();
        assert_eq!(load(&path).data.settings, Settings::default());
        // and partial settings fill in the rest
        fs::write(
            &path,
            r#"{"sessions":[{"solves":[]}],"settings":{"font":"pixel"}}"#,
        )
        .unwrap();
        let s = load(&path).data.settings;
        assert_eq!(
            (s.font.as_str(), s.inspection, s.bell),
            ("pixel", true, false)
        );
        assert_eq!(
            s.look, "image",
            "a file from before looks existed gets the default"
        );

        let mut data = sample();
        data.settings = Settings {
            font: "plain".into(),
            inspection: false,
            bell: true,
            look: "ascii".into(),
        };
        save(&path, &data).unwrap();
        assert_eq!(load(&path).data.settings, data.settings);
    }

    #[test]
    fn read_never_moves_a_damaged_file() {
        let dir = temp_dir("read");
        let path = dir.join("sessions.json");
        fs::write(&path, "{ not json").unwrap();
        assert!(read(&path).is_err());
        assert!(path.exists());
        assert!(!dir.join("sessions.json.corrupt").exists());
        assert_eq!(read(&dir.join("missing.json")).unwrap(), Data::default());
    }

    #[test]
    fn csv_export() {
        let csv = to_csv(&sample());
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(
            lines[0],
            "session,puzzle,solve,time,penalty,result,scramble,date"
        );
        assert_eq!(
            lines[1],
            "1,3x3,1,12.340,+2,14.340,R U,1970-01-01T00:00:07Z"
        );
        assert_eq!(lines[2], "1,3x3,2,9.990,dnf,DNF,F2,1970-01-01T00:00:08Z");
        assert_eq!(lines.len(), 3); // the second session is empty
    }

    #[test]
    fn csv_quotes_awkward_fields() {
        let mut data = Data::default();
        data.solves_mut().push(Solve {
            ms: 5,
            penalty: Penalty::Ok,
            scramble: "a,\"b\"".into(),
            ts: 0,
        });
        assert!(to_csv(&data).contains(",0.005,\"a,\"\"b\"\"\","));
    }

    #[test]
    fn missing_file_is_an_empty_store() {
        let dir = temp_dir("missing");
        let loaded = load(&dir.join("sessions.json"));
        assert_eq!(loaded.data, Data::default());
        assert!(loaded.warning.is_none());
    }

    #[test]
    fn corrupt_file_is_set_aside_not_overwritten() {
        let dir = temp_dir("corrupt");
        let path = dir.join("sessions.json");
        fs::write(&path, "{ not json").unwrap();
        let loaded = load(&path);
        assert_eq!(loaded.data, Data::default());
        assert!(loaded.warning.is_some());
        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(dir.join("sessions.json.corrupt")).unwrap(),
            "{ not json"
        );
    }

    #[test]
    fn bad_index_and_empty_sessions_are_repaired() {
        let dir = temp_dir("repair");
        let path = dir.join("sessions.json");
        fs::write(&path, r#"{"sessions":[],"current":5}"#).unwrap();
        assert_eq!(load(&path).data, Data::default());
        fs::write(&path, r#"{"sessions":[{"solves":[]}],"current":9}"#).unwrap();
        assert_eq!(load(&path).data.current, 0);
        fs::write(&path, r#"{"sessions":[{"solves":[]}]}"#).unwrap();
        assert_eq!(load(&path).data.current, 0);
    }

    #[test]
    fn only_one_holder_of_the_lock() {
        let dir = temp_dir("lock");
        let path = dir.join("sub").join("sessions.json");
        let first = lock(&path).unwrap();
        assert!(first.is_some());
        assert!(dir.join("sub").join("sessions.lock").exists());
        assert!(lock(&path).unwrap().is_none());
        drop(first);
        assert!(lock(&path).unwrap().is_some());
    }

    #[test]
    fn save_replaces_existing_file() {
        let dir = temp_dir("replace");
        let path = dir.join("sessions.json");
        save(&path, &Data::default()).unwrap();
        save(&path, &sample()).unwrap();
        assert_eq!(load(&path).data, sample());
    }

    #[test]
    fn old_files_without_puzzles_load_as_3x3() {
        let dir = temp_dir("old-format");
        let path = dir.join("sessions.json");
        fs::write(
            &path,
            r#"{"sessions":[{"solves":[{"ms":9000,"penalty":"ok","scramble":"R","ts":1}]},{"solves":[]}],"current":1}"#,
        )
        .unwrap();
        let data = read(&path).unwrap();
        assert!(data.sessions.iter().all(|s| s.puzzle() == Puzzle::Cube3));
        assert_eq!(data.sessions[0].puzzle, "333");
        assert_eq!(data.sessions[0].solves[0].ms, 9_000);
    }

    #[test]
    fn puzzles_are_saved_and_unknown_ones_read_as_3x3() {
        let dir = temp_dir("puzzles");
        let path = dir.join("sessions.json");
        let mut data = Data::default();
        data.sessions.push(Session::new(Puzzle::Cube5));
        data.sessions.push(Session {
            solves: vec![],
            puzzle: "from-the-future".into(),
        });
        save(&path, &data).unwrap();
        let back = read(&path).unwrap();
        assert_eq!(back.sessions[1].puzzle(), Puzzle::Cube5);
        assert_eq!(back.sessions[2].puzzle(), Puzzle::Cube3);
        assert_eq!(back.sessions[2].puzzle, "from-the-future"); // kept as written
        assert_eq!(back.sessions_of(Puzzle::Cube3), vec![0, 2]);
        assert_eq!(back.sessions_of(Puzzle::Cube5), vec![1]);
        assert!(back.sessions_of(Puzzle::Cube2).is_empty());
    }

    #[test]
    fn csv_names_the_puzzle() {
        let mut data = Data::default();
        data.sessions.push(Session::new(Puzzle::Cube4));
        data.current = 1;
        data.solves_mut().push(Solve {
            ms: 50_000,
            penalty: Penalty::Ok,
            scramble: "Rw".into(),
            ts: 0,
        });
        assert!(to_csv(&data).contains("\n2,4x4,1,50.000,ok,50.000,Rw,"));
    }
}
