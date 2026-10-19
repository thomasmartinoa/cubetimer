mod app;
mod chart;
mod cube;
mod fmt;
mod font;
mod nxn;
mod puzzle;
mod scramble;
mod stats;
mod statsview;
mod store;
mod term;
mod twobytwo;
mod twophase;
mod ui;

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
    Unknown(String),
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Action {
    match args.into_iter().next().as_deref() {
        None => Action::Run,
        Some("-h" | "--help") => Action::Help,
        Some("-V" | "--version") => Action::Version,
        Some("--export") => Action::Export,
        Some(other) => Action::Unknown(other.to_string()),
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
        assert_eq!(args(&["--bogus"]), Action::Unknown("--bogus".into()));
    }
}
