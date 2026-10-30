use anyhow::Result;

use crate::system::{PowerProfile, command};

pub fn set_power_profile(profile: PowerProfile) -> Result<()> {
    command::run("powerprofilesctl", &["set", profile.name()])?;
    Ok(())
}

pub fn step_brightness(delta: i8) -> Result<()> {
    if delta == 0 {
        return Ok(());
    }

    let amount = delta.unsigned_abs();
    let argument = format!("{amount}%{}", if delta > 0 { "+" } else { "-" });
    command::run("brightnessctl", &["set", &argument])?;
    Ok(())
}

pub fn step_volume(delta: i8) -> Result<()> {
    if delta == 0 {
        return Ok(());
    }

    let amount = delta.unsigned_abs();
    let argument = format!("{amount}%{}", if delta > 0 { "+" } else { "-" });
    command::run(
        "wpctl",
        &["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", &argument],
    )?;
    Ok(())
}

pub fn toggle_mute() -> Result<()> {
    command::run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"])?;
    Ok(())
}

/// Hand the terminal to NetworkManager's interactive TUI. The caller restores
/// niraboard's terminal state before and after this call.
pub fn open_network_settings() -> Result<()> {
    command::run_interactive("nmtui", &[])
}
