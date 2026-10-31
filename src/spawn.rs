use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::system::command;

/// Set on the process we relaunch inside a terminal. It keeps a terminal that
/// fails to provide a TTY from relaunching itself forever.
const CHILD_ENV: &str = "NIRABOARD_TERMINAL_CHILD";

/// Honors the freedesktop default-terminal settings when it is installed.
const XDG_TERMINAL_EXEC: &str = "xdg-terminal-exec";

/// Terminal emulators we know how to drive, in preference order. The options
/// are placed before the command that the terminal should run.
const TERMINALS: &[(&str, &[&str])] = &[
    ("foot", &[]),
    ("kitty", &[]),
    ("ghostty", &["-e"]),
    ("alacritty", &["-e"]),
    ("wezterm", &["start", "--"]),
    ("gnome-terminal", &["--"]),
    ("konsole", &["-e"]),
    ("xfce4-terminal", &["--execute"]),
    ("tilix", &["-e"]),
    ("terminator", &["-x"]),
    ("x-terminal-emulator", &["-e"]),
    ("xterm", &["-e"]),
    ("urxvt", &["-e"]),
    ("st", &["-e"]),
    ("qterminal", &["-e"]),
];

/// Why the dashboard refused to start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Only one of stdin/stdout is a terminal, so opening a new window could
    /// surprise someone who redirected the other one.
    PartialRedirection,
    /// There is no display server to open a terminal on.
    NoGraphicalSession,
    /// We already tried to relaunch inside a terminal and still got no TTY.
    AlreadyRelaunched,
}

/// What to do after inspecting the process' standard streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch {
    /// Both streams are a terminal; run the dashboard in place.
    Direct,
    /// No terminal is attached; open one and rerun there.
    Relaunch,
    /// The dashboard cannot be shown.
    Refuse(Refusal),
}

pub fn decide(
    stdin_tty: bool,
    stdout_tty: bool,
    forced: bool,
    graphical: bool,
    child: bool,
) -> Launch {
    if stdin_tty && stdout_tty {
        return Launch::Direct;
    }
    if child {
        return Launch::Refuse(Refusal::AlreadyRelaunched);
    }
    if forced {
        return Launch::Relaunch;
    }
    if stdin_tty || stdout_tty {
        return Launch::Refuse(Refusal::PartialRedirection);
    }
    if graphical {
        Launch::Relaunch
    } else {
        Launch::Refuse(Refusal::NoGraphicalSession)
    }
}

pub fn refusal_message(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::PartialRedirection => {
            "niraboard needs both stdin and stdout to be a terminal\n\
             unredirect them or pass --spawn-terminal"
        }
        Refusal::NoGraphicalSession => {
            "niraboard must be run from an interactive terminal\n\
             no graphical session was found to open one"
        }
        Refusal::AlreadyRelaunched => {
            "the terminal emulator did not provide an interactive terminal"
        }
    }
}

pub fn is_terminal_child() -> bool {
    env::var_os(CHILD_ENV).is_some_and(|value| !value.is_empty())
}

pub fn in_graphical_session() -> bool {
    ["WAYLAND_DISPLAY", "DISPLAY"]
        .iter()
        .any(|name| env::var_os(name).is_some_and(|value| !value.is_empty()))
}

/// Start a terminal emulator that runs this executable again.
///
/// The configuration path is forwarded so the child sees the same file. The
/// child process inherits [`CHILD_ENV`] and never tries to relaunch again.
pub fn relaunch(config_path: Option<&Path>) -> Result<()> {
    let executable = env::current_exe().context("failed to locate the niraboard executable")?;
    let mut args = Vec::new();
    if let Some(path) = config_path {
        args.push(OsString::from("--config"));
        args.push(path.as_os_str().to_os_string());
    }

    let terminal = env::var("TERMINAL")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let Some((program, command_args)) = resolve(terminal.as_deref(), &executable, &args) else {
        bail!("no terminal emulator was found; install one or set $TERMINAL");
    };

    command::spawn_detached(&program, &command_args, &[(CHILD_ENV, "1")])
        .with_context(|| format!("failed to start {}", program.display()))
}

fn resolve(
    terminal: Option<&str>,
    program: &Path,
    args: &[OsString],
) -> Option<(PathBuf, Vec<OsString>)> {
    let configured = terminal.and_then(|value| configured_terminal(value, program, args));
    if let Some(found) = configured {
        return Some(found);
    }

    if let Some(path) = find_in_path(XDG_TERMINAL_EXEC) {
        return Some((path.clone(), terminal_command(&[], program, args)));
    }

    TERMINALS.iter().find_map(|(name, prefix)| {
        find_in_path(name).map(|path| (path, terminal_command(prefix, program, args)))
    })
}

/// Interpret `$TERMINAL`, which may carry options such as `foot -e`.
fn configured_terminal(
    value: &str,
    program: &Path,
    args: &[OsString],
) -> Option<(PathBuf, Vec<OsString>)> {
    let mut tokens = value.split_whitespace();
    let name = tokens.next()?;
    let mut prefix = tokens.map(str::to_owned).collect::<Vec<_>>();
    let path = find_in_path(name)?;

    if !prefix.iter().any(|token| is_separator(token)) {
        let defaults = default_prefix(name);
        prefix.extend(defaults.iter().map(|token| (*token).to_owned()));
    }

    let prefix = prefix.iter().map(String::as_str).collect::<Vec<_>>();
    Some((path, terminal_command(&prefix, program, args)))
}

fn default_prefix(name: &str) -> &'static [&'static str] {
    let base = Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(name);

    TERMINALS
        .iter()
        .find(|(terminal, _)| *terminal == base)
        .map(|(_, prefix)| *prefix)
        .unwrap_or(&["-e"])
}

fn is_separator(token: &str) -> bool {
    matches!(token, "-e" | "--" | "-x" | "--execute" | "-E")
}

fn terminal_command(prefix: &[&str], program: &Path, args: &[OsString]) -> Vec<OsString> {
    let mut command = Vec::with_capacity(prefix.len() + args.len() + 1);
    command.extend(prefix.iter().map(OsString::from));
    command.push(program.as_os_str().to_os_string());
    command.extend(args.iter().cloned());
    command
}

fn find_in_path(name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().count() > 1 {
        return is_executable(candidate).then(|| candidate.to_path_buf());
    }

    env::split_paths(&env::var_os("PATH")?)
        .map(|directory| directory.join(name))
        .find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable() -> PathBuf {
        PathBuf::from("/usr/bin/niraboard")
    }

    fn args() -> Vec<OsString> {
        vec![OsString::from("--config"), OsString::from("/tmp/a.toml")]
    }

    #[test]
    fn tty_streams_run_in_place() {
        assert_eq!(decide(true, true, false, true, false), Launch::Direct);
        assert_eq!(decide(true, true, true, false, true), Launch::Direct);
    }

    #[test]
    fn graphical_invocation_without_terminal_relaunches() {
        assert_eq!(decide(false, false, false, true, false), Launch::Relaunch);
    }

    #[test]
    fn forced_relaunch_ignores_the_display_heuristic() {
        assert_eq!(decide(false, false, true, false, false), Launch::Relaunch);
    }

    #[test]
    fn no_terminal_and_no_display_is_refused() {
        assert_eq!(
            decide(false, false, false, false, false),
            Launch::Refuse(Refusal::NoGraphicalSession)
        );
    }

    #[test]
    fn partially_redirected_streams_are_refused() {
        assert_eq!(
            decide(true, false, false, true, false),
            Launch::Refuse(Refusal::PartialRedirection)
        );
        assert_eq!(
            decide(false, true, false, true, false),
            Launch::Refuse(Refusal::PartialRedirection)
        );
    }

    #[test]
    fn relaunched_child_never_loops() {
        assert_eq!(
            decide(false, false, true, true, true),
            Launch::Refuse(Refusal::AlreadyRelaunched)
        );
    }

    #[test]
    fn builds_command_from_prefix() {
        let command = terminal_command(&["-e"], &executable(), &args());
        assert_eq!(
            command,
            vec![
                OsString::from("-e"),
                OsString::from("/usr/bin/niraboard"),
                OsString::from("--config"),
                OsString::from("/tmp/a.toml"),
            ]
        );
    }

    #[test]
    fn direct_style_terminals_get_no_separator() {
        let command = terminal_command(default_prefix("foot"), &executable(), &[]);
        assert_eq!(command, vec![OsString::from("/usr/bin/niraboard")]);
    }

    #[test]
    fn terminal_env_without_separator_gets_a_default() {
        let Some((program, command)) = configured_terminal("sh", &executable(), &args()) else {
            panic!("sh should be in PATH");
        };

        assert_eq!(program, find_in_path("sh").expect("sh in PATH"));
        assert_eq!(
            command,
            vec![
                OsString::from("-e"),
                OsString::from("/usr/bin/niraboard"),
                OsString::from("--config"),
                OsString::from("/tmp/a.toml"),
            ]
        );
    }

    #[test]
    fn terminal_env_keeps_its_own_separator() {
        let Some((_, command)) = configured_terminal("sh -e", &executable(), &[]) else {
            panic!("sh should be in PATH");
        };
        assert_eq!(
            command,
            vec![OsString::from("-e"), OsString::from("/usr/bin/niraboard")]
        );

        let Some((_, command)) = configured_terminal("sh --", &executable(), &[]) else {
            panic!("sh should be in PATH");
        };
        assert_eq!(
            command,
            vec![OsString::from("--"), OsString::from("/usr/bin/niraboard")]
        );
    }

    #[test]
    fn terminal_env_must_exist() {
        assert!(configured_terminal("niraboard-no-such-terminal", &executable(), &[]).is_none());
    }

    #[test]
    fn finds_programs_in_path() {
        assert!(find_in_path("sh").is_some());
        assert!(find_in_path("niraboard-command-that-does-not-exist").is_none());
    }
}
