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

pub fn set_bluetooth_power(adapter_path: &str, powered: bool) -> Result<()> {
    command::run(
        "busctl",
        &[
            "--system",
            "--timeout=10",
            "set-property",
            "org.bluez",
            adapter_path,
            "org.bluez.Adapter1",
            "Powered",
            "b",
            if powered { "true" } else { "false" },
        ],
    )?;
    Ok(())
}

pub fn connect_bluetooth(device_path: &str) -> Result<()> {
    command::run(
        "busctl",
        &[
            "--system",
            "--timeout=15",
            "call",
            "org.bluez",
            device_path,
            "org.bluez.Device1",
            "Connect",
        ],
    )?;
    Ok(())
}

pub fn disconnect_bluetooth(device_path: &str) -> Result<()> {
    command::run(
        "busctl",
        &[
            "--system",
            "--timeout=15",
            "call",
            "org.bluez",
            device_path,
            "org.bluez.Device1",
            "Disconnect",
        ],
    )?;
    Ok(())
}

/// Start a discovery run through BlueZ. The panel stops it after its scan
/// window; [`stop_bluetooth_scan`] is also called on close and drop.
pub fn start_bluetooth_scan(adapter_path: &str) -> Result<()> {
    command::run(
        "busctl",
        &[
            "--system",
            "--timeout=5",
            "call",
            "org.bluez",
            adapter_path,
            "org.bluez.Adapter1",
            "StartDiscovery",
        ],
    )?;
    Ok(())
}

pub fn stop_bluetooth_scan(adapter_path: &str) -> Result<()> {
    command::run(
        "busctl",
        &[
            "--system",
            "--timeout=5",
            "call",
            "org.bluez",
            adapter_path,
            "org.bluez.Adapter1",
            "StopDiscovery",
        ],
    )?;
    Ok(())
}
