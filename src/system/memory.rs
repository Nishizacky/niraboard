use std::fs;

#[derive(Debug, Clone, PartialEq)]
pub struct Memory {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub percent: u8,
}

pub fn read() -> Option<Memory> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    parse(&text)
}

fn parse(text: &str) -> Option<Memory> {
    let mut total = None;
    let mut available = None;
    let mut free = None;
    let mut buffers = 0_u64;
    let mut cached = 0_u64;

    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let Some(number) = value
            .split_whitespace()
            .next()
            .and_then(|number| number.parse::<u64>().ok())
        else {
            continue;
        };

        match key.trim() {
            "MemTotal" => total = Some(number),
            "MemAvailable" => available = Some(number),
            "MemFree" => free = Some(number),
            "Buffers" => buffers = number,
            "Cached" => cached = number,
            _ => {}
        }
    }

    let total_kb = total?;
    if total_kb == 0 {
        return None;
    }

    let available_kb = match available {
        Some(value) => value,
        None => free? + buffers + cached,
    };
    let available_kb = available_kb.min(total_kb);
    let used_kb = total_kb.saturating_sub(available_kb);
    let percent = ((used_kb as f64 / total_kb as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8;

    Some(Memory {
        total_bytes: total_kb * 1024,
        used_bytes: used_kb * 1024,
        available_bytes: available_kb * 1024,
        percent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MEMINFO: &str = "\
MemTotal:       16128664 kB
MemFree:         5039624 kB
MemAvailable:   13033248 kB
Buffers:          123456 kB
Cached:          4567890 kB
";

    #[test]
    fn parses_meminfo_using_available() {
        let memory = parse(MEMINFO).expect("memory");
        assert_eq!(memory.total_bytes, 16_128_664 * 1024);
        assert_eq!(memory.used_bytes, (16_128_664 - 13_033_248) * 1024);
        assert_eq!(memory.percent, 19);
    }

    #[test]
    fn falls_back_to_free_buffers_and_cache() {
        let text = "MemTotal: 1000 kB\nMemFree: 400 kB\nBuffers: 100 kB\nCached: 200 kB\n";
        let memory = parse(text).expect("memory");
        assert_eq!(memory.available_bytes, 700 * 1024);
        assert_eq!(memory.percent, 30);
    }
}
