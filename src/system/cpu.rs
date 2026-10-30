use std::fs;
use std::path::Path;
use std::thread;

use super::sysfs;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cpu {
    pub usage_percent: Option<u8>,
    pub temp_c: Option<f32>,
    pub freq_mhz: Option<u32>,
    pub threads: usize,
}

/// Stateful sampler for `/proc/stat`.
///
/// CPU utilisation only makes sense as a delta between two samples, so the
/// first call deliberately returns `None`.
#[derive(Debug, Default)]
pub struct Sampler {
    previous: Option<Sample>,
}

#[derive(Debug, Clone, Copy)]
struct Sample {
    total: u64,
    idle: u64,
}

impl Sampler {
    pub fn sample(&mut self) -> Option<u8> {
        let stat = fs::read_to_string("/proc/stat").ok()?;
        let sample = parse_stat(&stat)?;
        self.update(sample)
    }

    fn update(&mut self, sample: Sample) -> Option<u8> {
        let usage = self.previous.and_then(|previous| {
            let total = sample.total.saturating_sub(previous.total);
            let idle = sample.idle.saturating_sub(previous.idle);
            if total == 0 {
                return None;
            }

            let busy = total.saturating_sub(idle);
            Some(
                ((busy as f64 / total as f64) * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as u8,
            )
        });

        self.previous = Some(sample);
        usage
    }
}

pub fn read(sampler: &mut Sampler) -> Cpu {
    Cpu {
        usage_percent: sampler.sample(),
        temp_c: read_temp_c(),
        freq_mhz: read_freq_mhz(),
        threads: thread::available_parallelism()
            .map(|threads| threads.get())
            .unwrap_or(0),
    }
}

fn parse_stat(text: &str) -> Option<Sample> {
    let mut fields = text.lines().next()?.split_whitespace();
    if fields.next()? != "cpu" {
        return None;
    }

    let mut values = [0_u64; 8];
    let mut count = 0;
    for field in fields.take(8) {
        values[count] = field.parse().ok()?;
        count += 1;
    }

    if count < 4 {
        return None;
    }

    let total = values[..count].iter().sum();
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    Some(Sample { total, idle })
}

fn read_freq_mhz() -> Option<u32> {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").ok()?;
    let mut total = 0.0;
    let mut count = 0_u32;

    for line in cpuinfo.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim() != "cpu MHz" {
            continue;
        }
        if let Ok(frequency) = value.trim().parse::<f64>() {
            total += frequency;
            count += 1;
        }
    }

    (count > 0).then(|| (total / f64::from(count)).round() as u32)
}

fn read_temp_c() -> Option<f32> {
    hwmon_temperature().or_else(thermal_temperature)
}

fn hwmon_temperature() -> Option<f32> {
    for directory in sysfs::entries("/sys/class/hwmon") {
        let Some(name) = sysfs::read_string(directory.join("name")) else {
            continue;
        };
        if !matches!(
            name.as_str(),
            "coretemp" | "k10temp" | "zenpower" | "cpu_thermal"
        ) {
            continue;
        }
        if let Some(temperature) = temperature_from_hwmon(&directory) {
            return Some(temperature);
        }
    }
    None
}

fn temperature_from_hwmon(directory: &Path) -> Option<f32> {
    let mut package = None;
    let mut highest = None;

    for entry in sysfs::entries(directory) {
        let Some(file_name) = entry.file_name() else {
            continue;
        };
        let file_name = file_name.to_string_lossy();
        if !file_name.starts_with("temp") || !file_name.ends_with("_input") {
            continue;
        }

        let Some(temperature) = read_temperature_file(&entry) else {
            continue;
        };
        highest = Some(highest.map_or(temperature, |current: f32| current.max(temperature)));

        let label_path = entry.with_file_name(file_name.replace("_input", "_label"));
        let label = sysfs::read_string(label_path).unwrap_or_default();
        if label.contains("Package") || label.contains("Tctl") {
            package = Some(temperature);
        }
    }

    package.or(highest)
}

fn thermal_temperature() -> Option<f32> {
    let mut acpitz = None;

    for directory in sysfs::entries("/sys/class/thermal") {
        let Some(kind) = sysfs::read_string(directory.join("type")) else {
            continue;
        };

        match kind.as_str() {
            "x86_pkg_temp" | "cpu-thermal" => {
                if let Some(temperature) = read_temperature_file(&directory.join("temp")) {
                    return Some(temperature);
                }
            }
            "acpitz" if acpitz.is_none() => {
                acpitz = read_temperature_file(&directory.join("temp"));
            }
            _ => {}
        }
    }

    acpitz
}

fn read_temperature_file(path: &Path) -> Option<f32> {
    let raw = sysfs::read_number::<i64>(path)?;
    millidegrees_to_celsius(raw)
}

fn millidegrees_to_celsius(raw: i64) -> Option<f32> {
    let celsius = raw as f32 / 1000.0;
    (1.0..=150.0).contains(&celsius).then_some(celsius)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: &str = "cpu  100 0 100 800 0 0 0 0 0 0\ncpu0 1 2 3 4\n";
    const SECOND: &str = "cpu  200 0 150 1250 0 0 0 0 0 0\n";

    #[test]
    fn parses_first_proc_stat_line() {
        let sample = parse_stat(FIRST).expect("sample");
        assert_eq!(sample.total, 1000);
        assert_eq!(sample.idle, 800);
    }

    #[test]
    fn computes_usage_from_two_samples() {
        let first = parse_stat(FIRST).expect("first");
        let second = parse_stat(SECOND).expect("second");
        let mut sampler = Sampler::default();

        assert_eq!(sampler.update(first), None);
        assert_eq!(sampler.update(second), Some(25));
    }

    #[test]
    fn converts_millidegrees() {
        assert_eq!(millidegrees_to_celsius(42_000), Some(42.0));
        assert_eq!(millidegrees_to_celsius(42), None);
    }
}
