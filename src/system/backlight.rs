use super::sysfs;

#[derive(Debug, Clone, PartialEq)]
pub struct Brightness {
    pub percent: u8,
    pub raw: u32,
    pub max: u32,
    pub device: String,
}

pub fn read() -> Option<Brightness> {
    for path in sysfs::entries("/sys/class/backlight") {
        let Some(max) = sysfs::read_number::<u32>(path.join("max_brightness")) else {
            continue;
        };
        if max == 0 {
            continue;
        }

        let Some(raw) = sysfs::read_number::<u32>(path.join("actual_brightness"))
            .or_else(|| sysfs::read_number::<u32>(path.join("brightness")))
        else {
            continue;
        };

        let Some(device) = path.file_name() else {
            continue;
        };

        return Some(Brightness {
            percent: percent(raw, max),
            raw,
            max,
            device: device.to_string_lossy().into_owned(),
        });
    }

    None
}

fn percent(raw: u32, max: u32) -> u8 {
    ((raw as f64 / max as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calculates_brightness_percent() {
        assert_eq!(percent(375, 7500), 5);
        assert_eq!(percent(7500, 7500), 100);
        assert_eq!(percent(9000, 7500), 100);
    }
}
