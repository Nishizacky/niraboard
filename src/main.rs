mod actions;
mod app;
mod config;
mod event;
mod spawn;
mod system;
mod ui;

use std::env;
use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::panic;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow, bail};
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

const USAGE: &str = "\
niraboard - a compact laptop power dashboard for the terminal

Usage: niraboard [OPTION]

Options:
  -h, --help                  Print this help and exit
  -V, --version               Print the version and exit
  -c, --config <FILE>         Read configuration from FILE
      --spawn-terminal        Open a terminal emulator for the dashboard when
                              no terminal is attached
      --print-default-config  Print the default configuration as TOML and exit
";

fn main() -> Result<()> {
    match Command::from_args()? {
        Command::Run {
            config,
            spawn_terminal,
        } => run_dashboard(config, spawn_terminal),
        Command::Help => {
            print!("{USAGE}");
            Ok(())
        }
        Command::Version => {
            println!("niraboard {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::PrintDefaultConfig => {
            print!("{}", Config::default().to_toml()?);
            Ok(())
        }
    }
}

fn run_dashboard(config_path: Option<PathBuf>, spawn_terminal: bool) -> Result<()> {
    let launch = spawn::decide(
        io::stdin().is_terminal(),
        io::stdout().is_terminal(),
        spawn_terminal,
        spawn::in_graphical_session(),
        spawn::is_terminal_child(),
    );

    match launch {
        spawn::Launch::Direct => {}
        spawn::Launch::Relaunch => {
            spawn::relaunch(config_path.as_deref())?;
            return Ok(());
        }
        spawn::Launch::Refuse(refusal) => bail!("{}", spawn::refusal_message(refusal)),
    }

    install_panic_hook();

    let mut session = TerminalSession::start()?;
    let result = run(&mut session, config_path.as_deref());
    let restore_result = session.finish();

    result?;
    restore_result
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Run {
        config: Option<PathBuf>,
        spawn_terminal: bool,
    },
    Help,
    Version,
    PrintDefaultConfig,
}

impl Command {
    fn from_args() -> Result<Self> {
        Self::parse(env::args_os().skip(1))
    }

    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self> {
        let args = args
            .into_iter()
            .map(|arg| {
                arg.into_string().map_err(|arg| {
                    anyhow!("argument is not valid UTF-8: {}", arg.to_string_lossy())
                })
            })
            .collect::<Result<Vec<_>>>()?;

        if let [arg] = args.as_slice() {
            match arg.as_str() {
                "-h" | "--help" => return Ok(Self::Help),
                "-V" | "--version" => return Ok(Self::Version),
                "--print-default-config" => return Ok(Self::PrintDefaultConfig),
                _ => {}
            }
        }

        let mut config = None;
        let mut spawn_terminal = false;
        let mut index = 0;

        while let Some(arg) = args.get(index) {
            index += 1;
            match arg.as_str() {
                "-c" | "--config" => {
                    let Some(path) = args.get(index) else {
                        bail!("option `{arg}` requires a file path\n\n{USAGE}");
                    };
                    if path.starts_with('-') {
                        bail!("option `{arg}` requires a file path\n\n{USAGE}");
                    }
                    index += 1;
                    config = Some(parse_config_path(path)?);
                }
                other if other.starts_with("--config=") => {
                    let path = &other["--config=".len()..];
                    config = Some(parse_config_path(path)?);
                }
                "--spawn-terminal" => spawn_terminal = true,
                other if other.starts_with('-') => {
                    bail!("unknown option `{other}`\n\n{USAGE}")
                }
                other => bail!("unexpected argument `{other}`\n\n{USAGE}"),
            }
        }

        Ok(Self::Run {
            config,
            spawn_terminal,
        })
    }
}

fn parse_config_path(path: &str) -> Result<PathBuf> {
    if path.is_empty() {
        bail!("option `--config` requires a file path\n\n{USAGE}");
    }

    Ok(PathBuf::from(path))
}

fn run(session: &mut TerminalSession, config_path: Option<&Path>) -> Result<()> {
    let mut app = build_app(config_path);
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
fn build_app(config_path: Option<&Path>) -> App {
    match Config::load(config_path) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command> {
        Command::parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_runs_the_dashboard() {
        assert_eq!(
            parse(&[]).expect("no arguments"),
            Command::Run {
                config: None,
                spawn_terminal: false,
            }
        );
    }

    #[test]
    fn parses_cli_options() {
        assert_eq!(parse(&["--help"]).expect("help"), Command::Help);
        assert_eq!(parse(&["-h"]).expect("help"), Command::Help);
        assert_eq!(parse(&["--version"]).expect("version"), Command::Version);
        assert_eq!(
            parse(&["--print-default-config"]).expect("defaults"),
            Command::PrintDefaultConfig
        );
    }

    #[test]
    fn parses_config_path() {
        let expected = Command::Run {
            config: Some(PathBuf::from("/tmp/niraboard.toml")),
            spawn_terminal: false,
        };

        assert_eq!(
            parse(&["--config", "/tmp/niraboard.toml"]).expect("long option"),
            expected
        );
        assert_eq!(
            parse(&["-c", "/tmp/niraboard.toml"]).expect("short option"),
            expected
        );
        assert_eq!(
            parse(&["--config=/tmp/niraboard.toml"]).expect("inline value"),
            expected
        );
    }

    #[test]
    fn parses_spawn_terminal_flag() {
        assert_eq!(
            parse(&["--spawn-terminal"]).expect("flag"),
            Command::Run {
                config: None,
                spawn_terminal: true,
            }
        );

        let expected = Command::Run {
            config: Some(PathBuf::from("/tmp/niraboard.toml")),
            spawn_terminal: true,
        };

        assert_eq!(
            parse(&["--spawn-terminal", "--config", "/tmp/niraboard.toml"]).expect("flag first"),
            expected
        );
        assert_eq!(
            parse(&["--config", "/tmp/niraboard.toml", "--spawn-terminal"]).expect("flag last"),
            expected
        );
    }

    #[test]
    fn rejects_unknown_and_extra_arguments() {
        assert!(parse(&["--wat"]).is_err());
        assert!(parse(&["--help", "--version"]).is_err());
        assert!(parse(&["--config"]).is_err());
        assert!(parse(&["--config="]).is_err());
        assert!(parse(&["--config", ""]).is_err());
        assert!(parse(&["--config", "--help"]).is_err());
        assert!(parse(&["--config", "/tmp/a.toml", "extra"]).is_err());
        assert!(parse(&["--spawn-terminal", "--wat"]).is_err());
        assert!(parse(&["extra"]).is_err());
    }
}
