//! Step-1 sanity check: prints every key event with its kind.
//! Run in kitty: `cargo run --example keytest`. Press/hold/release space; `q` quits.
//! Set `KEYTEST_LOG=<file>` to also write the same lines to a file.

#[path = "../src/term.rs"]
mod term;

use std::fs::File;
use std::io::{self, Write};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};

fn main() -> io::Result<()> {
    let guard = term::enter()?;
    term::restore_on_signals()?;
    let mut out = io::stdout();
    let mut log = std::env::var_os("KEYTEST_LOG").and_then(|p| File::create(p).ok());
    // Raw mode has no newline translation, so use \r\n.
    write!(
        out,
        "keyboard enhancement: {}\r\npress keys; space = hold/release test; q or Ctrl-C quits\r\n\r\n",
        if guard.enhanced {
            "ON (press/release events)"
        } else {
            "OFF (basic mode)"
        },
    )?;
    out.flush()?;
    if let Some(log) = log.as_mut() {
        writeln!(log, "enhanced={}", guard.enhanced)?;
    }

    let t0 = std::time::Instant::now();
    loop {
        if let Event::Key(k) = event::read()? {
            let kind = match k.kind {
                KeyEventKind::Press => "Press  ",
                KeyEventKind::Repeat => "Repeat ",
                KeyEventKind::Release => "Release",
            };
            write!(
                out,
                "{:>8.3}s  {kind}  {:?}  {:?}\r\n",
                t0.elapsed().as_secs_f64(),
                k.code,
                k.modifiers
            )?;
            out.flush()?;
            if let Some(log) = log.as_mut() {
                writeln!(log, "{kind} {:?} {:?}", k.code, k.modifiers)?;
            }
            let quit = k.kind == KeyEventKind::Press
                && (k.code == KeyCode::Char('q')
                    || (k.code == KeyCode::Char('c')
                        && k.modifiers.contains(KeyModifiers::CONTROL)));
            if quit {
                break;
            }
        }
    }
    Ok(())
}
