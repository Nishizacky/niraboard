use std::path::Path;

use super::command;
use super::sysfs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChargeState {
    Charging,
    Discharging,
    Full,
    NotCharging,
    Unknown,
}

impl ChargeState {
    fn from_status(status: &str) -> Self {
        match status.trim().to_ascii_lowercase().as_str() {
            "charging" => Self::Charging,
            "discharging" => Self::Discharging,
            "full" | "fully-charged" => Self::Full,
            "not charging" | "pending-charge" => Self::NotCharging,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Charging => "Charging",
            Self::Discharging => "Discharging",
            Self::Full => "Full",
            Self::NotCharging => "Not charging",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Battery {
    pub percent: u8,
    pub state: ChargeState,
    pub power_w: Option<f32>,
    pub health_percent: Option<u8>,
    pub energy_now_wh: Option<f32>,
    pub energy_full_wh: Option<f32>,
    pub ac_online: bool,
}

/// Read the first present battery from `/sys/class/power_supply`.
pub fn read() -> Option<Battery> {
    let supplies = sysfs::entries("/sys/class/power_supply");

    let ac_online = supplies.iter().any(|path| {
        sysfs::read_string(path.join("type")).is_some_and(|kind| kind == "Mains")
            && sysfs::read_number::<u8>(path.join("online")) == Some(1)
    });

    for path in &supplies {
        if sysfs::read_string(path.join("type")).as_deref() != Some("Battery") {
            continue;
        }
        if sysfs::read_number::<u8>(path.join("present")) == Some(0) {
            continue;
        }
        if let Some(battery) = read_sysfs(path, ac_online) {
            return Some(battery);
        }
    }

    None
}

fn read_sysfs(path: &Path, ac_online: bool) -> Option<Battery> {
    let status = sysfs::read_string(path.join("status")).unwrap_or_default();
    let state = ChargeState::from_status(&status);

    let voltage_now = sysfs::read_number::<f64>(path.join("voltage_now"));
    let voltage_design = sysfs::read_number::<f64>(path.join("voltage_min_design"));
    let voltage = voltage_now.or(voltage_design);

    let energy_now_wh = energy_wh(path, "energy_now", voltage);
    let energy_full_wh = energy_wh(path, "energy_full", voltage);
    let energy_design_wh = energy_wh(path, "energy_full_design", voltage);

    let health_percent = match (energy_full_wh, energy_design_wh) {
        (Some(full), Some(design)) if design > 0.0 => Some(as_percent(full / design)),
        _ => {
            let full = sysfs::read_number::<f64>(path.join("charge_full"));
            let design = sysfs::read_number::<f64>(path.join("charge_full_design"));
            match (full, design) {
                (Some(full), Some(design)) if design > 0.0 => Some(as_percent(full / design)),
                _ => None,
            }
        }
    };

    let power_w = read_power_w(path, voltage_now).filter(|power| *power > 0.0);

    let percent = sysfs::read_number::<u8>(path.join("capacity")).or_else(|| {
        match (energy_now_wh, energy_full_wh) {
            (Some(now), Some(full)) if full > 0.0 => Some(as_percent(now / full)),
            _ => None,
        }
    })?;

    Some(Battery {
        percent,
        state,
        power_w,
        health_percent,
        energy_now_wh,
        energy_full_wh,
        ac_online,
    })
}

fn energy_wh(path: &Path, field: &str, voltage_uv: Option<f64>) -> Option<f32> {
    if let Some(value) = sysfs::read_number::<f64>(path.join(field)) {
        return Some((value / 1_000_000.0) as f32);
    }

    let charge_field = field.replace("energy", "charge");
    let charge_uh = sysfs::read_number::<f64>(path.join(charge_field))?;
    let voltage_uv = voltage_uv?;
    Some((charge_uh * voltage_uv / 1_000_000_000_000.0) as f32)
}

fn read_power_w(path: &Path, voltage_uv: Option<f64>) -> Option<f32> {
    if let Some(power_uw) = sysfs::read_number::<f64>(path.join("power_now")) {
        return Some((power_uw / 1_000_000.0).abs() as f32);
    }

    let current_ua = sysfs::read_number::<f64>(path.join("current_now"))?;
    let voltage_uv = voltage_uv?;
    Some((current_ua * voltage_uv / 1_000_000_000_000.0).abs() as f32)
}

fn as_percent(ratio: impl Into<f64>) -> u8 {
    let ratio = ratio.into();
    (ratio * 100.0).round().clamp(0.0, 100.0) as u8
}

/// Fallback for machines without a battery entry in sysfs.
pub fn read_upower() -> Option<Battery> {
    let devices = command::run("upower", &["-e"]).ok()?;
    let path = devices.lines().find(|line| line.contains("/battery_"))?;
    let info = command::run("upower", &["-i", path]).ok()?;
    parse_upower(&info)
}

fn parse_upower(info: &str) -> Option<Battery> {
    let mut percent_value = None;
    let mut state = ChargeState::Unknown;
    let mut power_w = None;
    let mut energy_now = None;
    let mut energy_full = None;
    let mut energy_design = None;

    for line in info.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };

        match key.trim() {
            "percentage" => percent_value = parse_percent(value),
            "state" => state = ChargeState::from_status(value),
            "energy-rate" => power_w = parse_watts(value),
            "energy" => energy_now = parse_wh(value),
            "energy-full" => energy_full = parse_wh(value),
            "energy-full-design" => energy_design = parse_wh(value),
            _ => {}
        }
    }

    let health_percent = match (energy_full, energy_design) {
        (Some(full), Some(design)) if design > 0.0 => Some(as_percent(full / design)),
        _ => None,
    };

    let percent = percent_value.or_else(|| match (energy_now, energy_full) {
        (Some(now), Some(full)) if full > 0.0 => Some(as_percent(now / full)),
        _ => None,
    })?;

    Some(Battery {
        percent,
        state,
        power_w: power_w.filter(|power| *power > 0.0),
        health_percent,
        energy_now_wh: energy_now,
        energy_full_wh: energy_full,
        ac_online: matches!(
            state,
            ChargeState::Charging | ChargeState::Full | ChargeState::NotCharging
        ),
    })
}

fn parse_percent(value: &str) -> Option<u8> {
    let value = value.trim().trim_end_matches('%').parse::<f64>().ok()?;
    Some(value.round().clamp(0.0, 100.0) as u8)
}

fn parse_wh(value: &str) -> Option<f32> {
    value.trim().strip_suffix("Wh")?.trim().parse::<f32>().ok()
}

fn parse_watts(value: &str) -> Option<f32> {
    value.trim().strip_suffix('W')?.trim().parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const UPOWER_SAMPLE: &str = "\
  native-path:          BAT0
  battery
    present:             yes
    state:               discharging
    energy:              29.95 Wh
    energy-full:         37.93 Wh
    energy-full-design:  45.73 Wh
    energy-rate:         6.2 W
    percentage:          79%
";

    #[test]
    fn parses_upower_battery() {
        let battery = parse_upower(UPOWER_SAMPLE).expect("battery");
        assert_eq!(battery.percent, 79);
        assert_eq!(battery.state, ChargeState::Discharging);
        assert_eq!(battery.power_w, Some(6.2));
        assert_eq!(battery.health_percent, Some(83));
        assert_eq!(battery.energy_now_wh, Some(29.95));
        assert!(!battery.ac_online);
    }

    #[test]
    fn maps_not_charging_state() {
        assert_eq!(
            ChargeState::from_status("Not charging"),
            ChargeState::NotCharging
        );
    }
}
