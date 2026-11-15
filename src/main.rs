mod app;
mod download;
mod ui;

use std::{
    io::{self, Stdout},
    time::Duration,
};

use app::App;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        cursor::Show,
        event::{
            self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste,
            EnableMouseCapture, Event,
        },
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};

fn main() -> io::Result<()> {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture,
            DisableBracketedPaste,
            Show
        );
        default_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste
    )?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.clear()?;

    let mut app = App::new();
    let result = run(&mut terminal, &mut app);
    drop(app);

    let _ = disable_raw_mode();
    let _ = execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        DisableBracketedPaste,
        Show
    );
    let _ = terminal.show_cursor();
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => app.on_key(key),
                Event::Mouse(mouse) => app.on_mouse(mouse),
                Event::Paste(text) => app.on_paste(&text),
                _ => {}
            }
        }
        app.tick();
        if app.quit {
            break;
        }
    }
    Ok(())
}
