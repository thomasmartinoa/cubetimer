//! Solve records and session persistence.
//!
//! Sessions live in `<data dir>/cubetimer/sessions.json` and are written
//! atomically (temp file, then rename).

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

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
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        Self { ms, penalty, scramble, ts }
    }

    pub fn result(&self) -> TimeResult {
        TimeResult::new(self.ms, self.penalty)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    pub solves: Vec<Solve>,
}

/// Everything saved to disk. Always holds at least one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Data {
    pub sessions: Vec<Session>,
    /// Index of the session being timed.
    #[serde(default)]
    pub current: usize,
}

impl Default for Data {
    fn default() -> Self {
        Self { sessions: vec![Session::default()], current: 0 }
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

    pub fn solves_mut(&mut self) -> &mut Vec<Solve> {
        &mut self.sessions[self.current].solves
    }
}

pub struct Loaded {
    pub data: Data,
    /// Set when the file was unreadable and set aside.
    pub warning: Option<String>,
}

pub fn default_path() -> Option<PathBuf> {
    dirs::data_dir().map(|d| d.join("cubetimer").join("sessions.json"))
}

/// Load sessions. A missing file is an empty store. A file that can't be
/// parsed is renamed to `*.corrupt` (so the next save can't destroy it) and an
/// empty store is returned with a warning.
pub fn load(path: &Path) -> Loaded {
    let text = match fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Loaded { data: Data::default(), warning: None };
        }
        Err(e) => return set_aside(path, &e.to_string()),
    };
    match serde_json::from_str::<Data>(&text) {
        Ok(data) => Loaded { data: data.normalize(), warning: None },
        Err(e) => set_aside(path, &e.to_string()),
    }
}

fn set_aside(path: &Path, reason: &str) -> Loaded {
    let mut aside = path.as_os_str().to_owned();
    aside.push(".corrupt");
    let warning = match fs::rename(path, &aside) {
        Ok(()) => format!("could not read sessions ({reason}); kept as {}", Path::new(&aside).display()),
        Err(e) => format!("could not read sessions ({reason}); could not move it aside: {e}"),
    };
    Loaded { data: Data::default(), warning: Some(warning) }
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
        let dir = std::env::temp_dir().join(format!("cubetimer-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> Data {
        let mut data = Data::default();
        data.solves_mut().push(Solve { ms: 12_340, penalty: Penalty::Plus2, scramble: "R U".into(), ts: 7 });
        data.solves_mut().push(Solve { ms: 9_990, penalty: Penalty::Dnf, scramble: "F2".into(), ts: 8 });
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
        assert_eq!(text, r#"{"ms":12340,"penalty":"plus2","scramble":"R U","ts":7}"#);
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
        assert_eq!(fs::read_to_string(dir.join("sessions.json.corrupt")).unwrap(), "{ not json");
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
    fn save_replaces_existing_file() {
        let dir = temp_dir("replace");
        let path = dir.join("sessions.json");
        save(&path, &Data::default()).unwrap();
        save(&path, &sample()).unwrap();
        assert_eq!(load(&path).data, sample());
    }
}
