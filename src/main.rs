mod actions;
mod app;
mod config;
mod event;
mod system;
mod ui;

use std::io::{self, IsTerminal};
use std::panic;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use ratatui::DefaultTerminal;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};

use app::App;
use config::Config;
use ui::HitAreas;

const TICK: Duration = Duration::from_secs(2);

fn main() -> Result<()> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("niraboard must be run from an interactive terminal");
    }

    install_panic_hook();

    let mut session = TerminalSession::start()?;
    let result = run(&mut session);
    let restore_result = session.finish();

    result?;
    restore_result
}

fn run(session: &mut TerminalSession) -> Result<()> {
    let mut app = build_app();
    app.refresh_all();

    let mut dirty = true;
    let mut last_tick = Instant::now();

    loop {
        if app.should_quit {
            break;
        }

        if dirty {
            let mut hits = HitAreas::default();
            session.terminal.draw(|frame| {
                hits = ui::draw(frame, &app);
            })?;
            app.hits = hits;
            dirty = false;
        }

        let timeout = TICK.saturating_sub(last_tick.elapsed());
        if let Some(event) = event::poll(timeout)? {
            dirty |= app.handle_event(event);
        }

        if app.take_network_request() {
            let result = session.run_network_settings();
            app.monitor.refresh_wifi_now();
            app.network_settings_finished(result);
            dirty = true;
        }

        if last_tick.elapsed() >= TICK {
            last_tick = Instant::now();
            dirty |= app.tick();
        }
    }

    Ok(())
}

/// Load configuration, falling back to defaults and showing the error in the
/// footer if the file cannot be read or parsed.
fn build_app() -> App {
    match Config::load() {
        Ok(config) => App::with_config(config),
        Err(error) => {
            let mut app = App::new();
            app.report_config_warning(error);
            app
        }
    }
}

struct TerminalSession {
    terminal: DefaultTerminal,
    restored: bool,
}

impl TerminalSession {
    fn start() -> Result<Self> {
        enable_raw_mode().context("failed to enable raw mode")?;

        let mut stdout = io::stdout();
        if let Err(error) = execute!(&mut stdout, EnterAlternateScreen, EnableMouseCapture, Hide) {
            let _ = disable_raw_mode();
            return Err(error).context("failed to set up the terminal");
        }

        let backend = CrosstermBackend::new(stdout);
        match Terminal::new(backend) {
            Ok(terminal) => Ok(Self {
                terminal,
                restored: false,
            }),
            Err(error) => {
                let _ = restore_terminal();
                Err(error).context("failed to create the terminal")
            }
        }
    }

    fn finish(mut self) -> Result<()> {
        self.restored = true;
        restore_terminal()
    }

    /// Leave niraboard's terminal mode, run `nmtui`, then re-enter it.
    fn run_network_settings(&mut self) -> Result<()> {
        self.suspend()?;
        let action_result = actions::open_network_settings();
        let resume_result = self.resume();

        action_result?;
        resume_result
    }

    fn suspend(&mut self) -> Result<()> {
        restore_terminal()?;
        self.restored = true;
        Ok(())
    }

    fn resume(&mut self) -> Result<()> {
        enable_raw_mode().context("failed to enable raw mode")?;
        self.restored = false;

        let mut stdout = io::stdout();
        execute!(&mut stdout, EnterAlternateScreen, EnableMouseCapture, Hide)
            .context("failed to restore the terminal")?;

        self.terminal.clear()?;
        Ok(())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        if !self.restored {
            let _ = restore_terminal();
        }
    }
}

fn restore_terminal() -> Result<()> {
    let mut first_error = None;

    if let Err(error) = disable_raw_mode() {
        first_error = Some(error);
    }

    let mut stdout = io::stdout();
    if let Err(error) = execute!(&mut stdout, DisableMouseCapture, Show, LeaveAlternateScreen) {
        if first_error.is_none() {
            first_error = Some(error);
        }
    }

    match first_error {
        Some(error) => Err(error).context("failed to restore the terminal"),
        None => Ok(()),
    }
}

fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        previous(info);
    }));
}
