//! Terminal setup/teardown, including the kitty keyboard protocol.
//!
//! `enter()` returns a `Guard` whose `Drop` restores the terminal, and a panic
//! hook does the same so a crash never leaves kitty in a broken state.

use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::crossterm::{
    cursor::Show,
    event::{
        DisableFocusChange, EnableFocusChange, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
        supports_keyboard_enhancement,
    },
};

/// Whether the enhancement flags are currently pushed (so restore pops exactly once).
static ENHANCED: AtomicBool = AtomicBool::new(false);
static ACTIVE: AtomicBool = AtomicBool::new(false);

pub struct Guard {
    /// True when the terminal reports press/release events (full mode).
    pub enhanced: bool,
}

/// Enter raw mode and the alternate screen, and push the kitty keyboard flags
/// when the terminal supports them. Also installs the panic hook.
pub fn enter() -> io::Result<Guard> {
    install_panic_hook();
    enable_raw_mode()?;
    let mut out = io::stdout();
    // Focus events let the app forget a key that was let go in another window.
    execute!(out, EnterAlternateScreen, EnableFocusChange)?;
    ACTIVE.store(true, Ordering::SeqCst);

    let enhanced = supports_keyboard_enhancement().unwrap_or(false);
    if enhanced {
        execute!(
            out,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
            )
        )?;
        ENHANCED.store(true, Ordering::SeqCst);
    }
    Ok(Guard { enhanced })
}

/// Undo everything `enter()` did. Safe to call more than once.
pub fn restore() {
    let mut out = io::stdout();
    if ENHANCED.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, PopKeyboardEnhancementFlags);
    }
    if ACTIVE.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, DisableFocusChange, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
    let _ = execute!(out, Show);
    let _ = out.flush();
}

/// Restore the terminal and exit on SIGTERM, SIGHUP, SIGQUIT or SIGINT. These
/// would otherwise end the process without running `Guard`'s `Drop`, leaving
/// the terminal in raw mode on the alternate screen. Solves are already on
/// disk (every change is saved atomically), so exiting here loses nothing.
pub fn restore_on_signals() -> io::Result<()> {
    use signal_hook::consts::{SIGHUP, SIGINT, SIGQUIT, SIGTERM};
    let mut signals = signal_hook::iterator::Signals::new([SIGTERM, SIGHUP, SIGQUIT, SIGINT])?;
    std::thread::spawn(move || {
        if let Some(signal) = signals.forever().next() {
            restore();
            std::process::exit(128 + signal);
        }
    });
    Ok(())
}

fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        restore();
        default(info);
    }));
}

impl Drop for Guard {
    fn drop(&mut self) {
        restore();
    }
}
