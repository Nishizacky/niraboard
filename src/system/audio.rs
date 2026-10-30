use super::command;

#[derive(Debug, Clone, PartialEq)]
pub struct Audio {
    pub volume_percent: u16,
    pub muted: bool,
}

/// Read the default PipeWire sink through WirePlumber's `wpctl`.
pub fn read() -> Option<Audio> {
    let output = command::run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]).ok()?;
    parse(&output)
}

fn parse(output: &str) -> Option<Audio> {
    let first_line = output.lines().next()?.trim();
    let value = first_line
        .strip_prefix("Volume:")?
        .split_whitespace()
        .next()?;
    let value = value.parse::<f32>().ok()?;

    if !value.is_finite() || value < 0.0 {
        return None;
    }

    Some(Audio {
        volume_percent: (value * 100.0).round().clamp(0.0, 999.0) as u16,
        muted: output.contains("[MUTED]"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_volume_and_mute_state() {
        let audio = parse("Volume: 0.40\n").expect("audio");
        assert_eq!(audio.volume_percent, 40);
        assert!(!audio.muted);

        let muted = parse("Volume: 0.40 [MUTED]\n").expect("audio");
        assert!(muted.muted);
    }

    #[test]
    fn accepts_amplified_volume() {
        let audio = parse("Volume: 1.20\n").expect("audio");
        assert_eq!(audio.volume_percent, 120);
    }
}
