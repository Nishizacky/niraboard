use super::command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProfile {
    PowerSaver,
    Balanced,
    Performance,
}

impl PowerProfile {
    pub const ALL: [Self; 3] = [Self::PowerSaver, Self::Balanced, Self::Performance];

    pub fn name(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }

    pub fn label(self) -> &'static str {
        self.name()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "power-saver" => Some(Self::PowerSaver),
            "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance),
            _ => None,
        }
    }

    pub fn step(self, direction: i8) -> Self {
        if direction == 0 {
            return self;
        }

        let index = Self::ALL
            .iter()
            .position(|profile| *profile == self)
            .unwrap_or(1) as isize;
        let length = Self::ALL.len() as isize;
        let offset = if direction > 0 { 1 } else { -1 };
        let next = (index + offset).rem_euclid(length) as usize;
        Self::ALL[next]
    }
}

/// Read the active profile cheaply over D-Bus through `busctl`.
///
/// `powerprofilesctl` is the documented interface for changes, but its Python
/// startup costs roughly half a second. Reading the same property through
/// `busctl` keeps periodic refreshes cheap; `powerprofilesctl get` remains the
/// fallback when `busctl` is unavailable.
pub fn read() -> Option<PowerProfile> {
    if let Ok(output) = command::run(
        "busctl",
        &[
            "--system",
            "get-property",
            "net.hadess.PowerProfiles",
            "/net/hadess/PowerProfiles",
            "net.hadess.PowerProfiles",
            "ActiveProfile",
        ],
    ) {
        if let Some(profile) = parse_busctl(&output) {
            return Some(profile);
        }
    }

    let output = command::run("powerprofilesctl", &["get"]).ok()?;
    PowerProfile::from_name(output.trim())
}

fn parse_busctl(output: &str) -> Option<PowerProfile> {
    let output = output.trim();
    let value = output.strip_prefix('s').unwrap_or(output).trim();
    PowerProfile::from_name(value.trim_matches('"'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_busctl_property_output() {
        assert_eq!(
            parse_busctl("s \"power-saver\"\n"),
            Some(PowerProfile::PowerSaver)
        );
    }

    #[test]
    fn parses_powerprofilesctl_output() {
        assert_eq!(
            PowerProfile::from_name(" balanced \n"),
            Some(PowerProfile::Balanced)
        );
    }

    #[test]
    fn cycles_profiles_in_both_directions() {
        assert_eq!(PowerProfile::PowerSaver.step(1), PowerProfile::Balanced);
        assert_eq!(PowerProfile::PowerSaver.step(-1), PowerProfile::Performance);
        assert_eq!(PowerProfile::PowerSaver.step(0), PowerProfile::PowerSaver);
    }
}
