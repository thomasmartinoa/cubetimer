mod app;
mod fmt;
mod font;
mod scramble;
mod stats;
mod store;
mod term;
mod ui;

use std::io;

use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

fn main() -> io::Result<()> {
    let guard = term::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    terminal.hide_cursor()?;
    let mut app = app::App::new(guard.enhanced);
    if let Some(path) = store::default_path() {
        let loaded = store::load(&path);
        app = app.with_storage(path, loaded);
    }
    let result = app.run(&mut terminal);
    drop(guard);
    result
}
