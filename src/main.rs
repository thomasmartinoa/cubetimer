mod alg;
mod algs;
mod algview;
mod anim;
mod app;
mod chart;
mod cube;
mod fmt;
mod font;
mod import;
mod nxn;
mod puzzle;
mod render3d;
mod scramble;
mod stats;
mod statsview;
mod store;
mod term;
mod trainer;
mod twobytwo;
mod twophase;
mod ui;
mod view3d;

use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

const USAGE: &str = "\
Full-screen speedcube timer with WCA inspection, hold-to-start and
random-state scrambles.

Usage: cubetimer [OPTIONS]

Options:
      --export   Print every solve as CSV (for spreadsheets) and exit
      --import FILE [--dry-run]
                 Add the sessions of a csTimer export to your solves
                 (--dry-run only says what would be imported)
  -h, --help     Print this help
  -V, --version  Print the version

Press ? inside the timer for the keys. Solves are saved in
$XDG_DATA_HOME/cubetimer/sessions.json (usually ~/.local/share).";

#[derive(Debug, PartialEq, Eq)]
enum Action {
    Run,
    Help,
    Version,
    Export,
    Import { path: String, dry_run: bool },
    Unknown(String),
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Action {
    let args: Vec<String> = args.into_iter().collect();
    match args.first().map(String::as_str) {
        None => Action::Run,
        Some("-h" | "--help") => Action::Help,
        Some("-V" | "--version") => Action::Version,
        Some("--export") => Action::Export,
        Some("--import") => {
            let dry_run = args.iter().any(|a| a == "--dry-run");
            match args[1..].iter().find(|a| !a.starts_with("--")) {
                Some(path) => Action::Import {
                    path: path.clone(),
                    dry_run,
                },
                None => Action::Unknown("--import (needs a file)".into()),
            }
        }
        Some(other) => Action::Unknown(other.to_string()),
    }
}

/// `--import`: read a csTimer export and add its sessions to the saved data.
fn import(path: &str, dry_run: bool) -> ExitCode {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cubetimer: can't read {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let (sessions, summary) = match import::parse(&text) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("cubetimer: {path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    println!("{}", summary.describe(dry_run));
    if dry_run || sessions.is_empty() {
        return ExitCode::SUCCESS;
    }
    let Some(data_path) = store::default_path() else {
        eprintln!("cubetimer: no data directory");
        return ExitCode::FAILURE;
    };
    // writing your data: not while the timer is running
    let _lock = match store::lock(&data_path) {
        Ok(Some(lock)) => lock,
        Ok(None) => {
            eprintln!("cubetimer: the timer is running; close it first, then import");
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("cubetimer: can't lock {}: {e}", data_path.display());
            return ExitCode::FAILURE;
        }
    };
    let mut data = match store::read(&data_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cubetimer: can't read {}: {e}", data_path.display());
            return ExitCode::FAILURE;
        }
    };
    import::merge(&mut data, sessions);
    match store::save(&data_path, &data) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cubetimer: can't save {}: {e}", data_path.display());
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Action::Run => {}
        Action::Help => {
            println!("cubetimer {}\n{USAGE}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Action::Version => {
            println!("cubetimer {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Action::Export => {
            // Reading needs no lock: saves replace the file atomically.
            let Some(path) = store::default_path() else {
                eprintln!("cubetimer: no data directory");
                return ExitCode::FAILURE;
            };
            return match store::read(&path) {
                Ok(data) => {
                    print!("{}", store::to_csv(&data));
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("cubetimer: can't read {}: {e}", path.display());
                    ExitCode::FAILURE
                }
            };
        }
        Action::Import { path, dry_run } => return import(&path, dry_run),
        Action::Unknown(arg) => {
            eprintln!("cubetimer: unknown option '{arg}'\nTry 'cubetimer --help'.");
            return ExitCode::from(2);
        }
    }

    // Take the lock before touching the terminal, so a second copy can say
    // why it won't start on a normal screen.
    let path = store::default_path();
    let _lock = match path.as_deref().map(store::lock) {
        Some(Ok(Some(lock))) => Some(lock),
        Some(Ok(None)) => {
            eprintln!(
                "cubetimer: already running (another copy is using {})",
                path.as_deref()
                    .unwrap_or_else(|| "sessions.json".as_ref())
                    .display()
            );
            return ExitCode::FAILURE;
        }
        // No lock (read-only disk, say): still run; saving reports its own errors.
        Some(Err(_)) | None => None,
    };

    match run(path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("cubetimer: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(path: Option<PathBuf>) -> io::Result<()> {
    let guard = term::enter()?;
    term::restore_on_signals()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.hide_cursor()?;
    let mut app = app::App::new(guard.enhanced);
    if let Some(path) = path {
        let loaded = store::load(&path);
        app = app.with_storage(path, loaded);
    }
    let result = app.run(&mut terminal);
    drop(guard);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Action {
        parse_args(a.iter().map(|s| s.to_string()))
    }

    #[test]
    fn command_line() {
        assert_eq!(args(&[]), Action::Run);
        assert_eq!(args(&["-h"]), Action::Help);
        assert_eq!(args(&["--help"]), Action::Help);
        assert_eq!(args(&["-V"]), Action::Version);
        assert_eq!(args(&["--version"]), Action::Version);
        assert_eq!(args(&["--export"]), Action::Export);
        assert_eq!(
            args(&["--import", "a.json"]),
            Action::Import {
                path: "a.json".into(),
                dry_run: false
            }
        );
        assert_eq!(
            args(&["--import", "--dry-run", "a.json"]),
            Action::Import {
                path: "a.json".into(),
                dry_run: true
            }
        );
        assert!(matches!(args(&["--import"]), Action::Unknown(_)));
        assert!(matches!(
            args(&["--import", "--dry-run"]),
            Action::Unknown(_)
        ));
        assert_eq!(args(&["--bogus"]), Action::Unknown("--bogus".into()));
    }
}
