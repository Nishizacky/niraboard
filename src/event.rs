use std::io;
use std::time::Duration;

use ratatui::crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};

/// The small set of terminal events the application cares about.
#[derive(Debug)]
pub enum Event {
    Key(KeyEvent),
    Mouse(MouseEvent),
    Resize,
}

/// Wait for one terminal event, or return `None` when the timeout elapses.
pub fn poll(timeout: Duration) -> io::Result<Option<Event>> {
    if !event::poll(timeout)? {
        return Ok(None);
    }

    let event = match event::read()? {
        CrosstermEvent::Key(key) => Event::Key(key),
        CrosstermEvent::Mouse(mouse) => Event::Mouse(mouse),
        CrosstermEvent::Resize(_, _) => Event::Resize,
        // Focus, paste, and other terminal events are intentionally ignored.
        _ => return Ok(None),
    };

    Ok(Some(event))
}
