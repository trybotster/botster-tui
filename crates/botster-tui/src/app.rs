use std::{
    io::{self, Stdout},
    time::Duration,
};

use crossterm::{
    cursor::Show,
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Frame, Terminal, backend::CrosstermBackend};

use crate::renderer::{self, HitMap};

pub const SMOKE_MESSAGE: &str = "botster-tui smoke ok";

pub fn smoke_message() -> &'static str {
    SMOKE_MESSAGE
}

pub fn run() -> io::Result<()> {
    let mut terminal = setup_terminal()?;
    let run_result = run_loop(&mut terminal);
    let restore_result = restore_terminal(&mut terminal);

    match (run_result, restore_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();
    if let Err(error) = execute!(stdout, EnterAlternateScreen) {
        let _ = disable_raw_mode();
        return Err(error);
    }

    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    let leave_result = execute!(terminal.backend_mut(), LeaveAlternateScreen, Show);
    let raw_result = disable_raw_mode();
    let cursor_result = terminal.show_cursor();

    leave_result?;
    raw_result?;
    cursor_result
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
    loop {
        let mut hit_map = HitMap::default();
        terminal.draw(|frame| draw(frame, &mut hit_map))?;

        if event::poll(Duration::from_millis(250))? {
            let event = event::read()?;
            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press && should_quit(key) => break,
                _ => {
                    let _dispatch = renderer::dispatch_event(event, &hit_map);
                }
            }
        }
    }

    Ok(())
}

fn draw(frame: &mut Frame<'_>, hit_map: &mut HitMap) {
    let node = renderer::demo_ui_node();
    renderer::render_node(frame, frame.area(), &node, hit_map);
}

fn should_quit(key: KeyEvent) -> bool {
    key.code == KeyCode::Esc
        || matches!(key.code, KeyCode::Char('q' | 'Q'))
        || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_message_names_the_scaffold() {
        assert_eq!(smoke_message(), "botster-tui smoke ok");
    }

    #[test]
    fn quit_keys_match_documented_exit_path() {
        assert!(should_quit(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(should_quit(KeyEvent::new(
            KeyCode::Char('q'),
            KeyModifiers::NONE
        )));
        assert!(should_quit(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::CONTROL
        )));
        assert!(!should_quit(KeyEvent::new(
            KeyCode::Char('c'),
            KeyModifiers::NONE
        )));
    }
}
