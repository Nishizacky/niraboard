use std::io::ErrorKind;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

/// Run a program with argument-based execution (never through a shell).
///
/// stdout is returned on success. On failure, the first line of stderr is
/// included in the error so it can be shown in the status bar.
pub fn run(program: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(program).args(args).output().map_err(|error| {
        if error.kind() == ErrorKind::NotFound {
            anyhow::anyhow!("{program} not found in PATH")
        } else {
            anyhow::Error::new(error).context(format!("failed to run {program}"))
        }
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().next().unwrap_or("").trim();
        if detail.is_empty() {
            bail!("{program} exited with {}", output.status);
        }
        bail!("{program}: {detail}");
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run an interactive program with the terminal's own stdin/stdout/stderr.
///
/// This is used for full-screen tools such as `nmtui`, so output must not be
/// captured. The caller is responsible for restoring the terminal first.
pub fn run_interactive(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|error| {
            if error.kind() == ErrorKind::NotFound {
                anyhow::anyhow!("{program} not found in PATH")
            } else {
                anyhow::Error::new(error).context(format!("failed to run {program}"))
            }
        })?;

    if !status.success() {
        bail!("{program} exited with {status}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_program_is_reported_without_panicking() {
        let error = run("niraboard-command-that-does-not-exist", &[]).unwrap_err();
        assert!(error.to_string().contains("not found"));
    }

    #[test]
    fn missing_interactive_program_is_reported_without_panicking() {
        let error = run_interactive("niraboard-command-that-does-not-exist", &[]).unwrap_err();
        assert!(error.to_string().contains("not found"));
    }
}
