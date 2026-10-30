pub mod audio;
pub mod backlight;
pub mod battery;
pub mod command;
pub mod cpu;
pub mod memory;
pub mod network;
pub mod power_profile;
mod sysfs;

pub use audio::Audio;
pub use backlight::Brightness;
pub use battery::{Battery, ChargeState};
pub use cpu::Cpu;
pub use memory::Memory;
pub use network::Wifi;
pub use power_profile::PowerProfile;

use std::time::{Duration, Instant};

const AUDIO_INTERVAL: Duration = Duration::from_secs(4);
const WIFI_INTERVAL: Duration = Duration::from_secs(15);
const PROFILE_INTERVAL: Duration = Duration::from_secs(30);
const UPOWER_INTERVAL: Duration = Duration::from_secs(30);
const PUBLIC_IP_INTERVAL: Duration = Duration::from_secs(300);

/// A complete snapshot of everything the dashboard displays.
///
/// `None` means the data source is unavailable on this system. A snapshot can
/// be compared with `==`; the application uses that to avoid redundant redraws.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metrics {
    pub battery: Option<Battery>,
    pub cpu: Cpu,
    pub memory: Option<Memory>,
    pub wifi: Option<Wifi>,
    pub local_ip: Option<String>,
    pub public_ip: Option<String>,
    pub power_profile: Option<PowerProfile>,
    pub brightness: Option<Brightness>,
    pub audio: Option<Audio>,
}

/// Collects metrics and owns the time-based refresh schedule.
///
/// Cheap sysfs/procfs reads run every tick. Process-based integrations are
/// staggered so that expensive commands are not spawned at a high rate.
pub struct Monitor {
    pub metrics: Metrics,
    cpu: cpu::Sampler,
    last_audio: Option<Instant>,
    last_wifi: Option<Instant>,
    last_profile: Option<Instant>,
    last_upower: Option<Instant>,
    last_public_ip: Option<Instant>,
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            metrics: Metrics::default(),
            cpu: cpu::Sampler::default(),
            last_audio: None,
            last_wifi: None,
            last_profile: None,
            last_upower: None,
            last_public_ip: None,
        }
    }

    /// Refresh every source, regardless of the normal schedule.
    pub fn refresh_all(&mut self) -> bool {
        let before = self.metrics.clone();

        self.read_fast();
        self.read_audio();
        self.read_wifi();
        self.read_profile();
        self.read_public_ip();

        let now = Instant::now();
        self.last_audio = Some(now);
        self.last_wifi = Some(now);
        self.last_profile = Some(now);
        self.last_public_ip = Some(now);

        self.metrics != before
    }

    /// Refresh due sources. Returns true when the visible snapshot changed.
    pub fn tick(&mut self) -> bool {
        let before = self.metrics.clone();

        self.read_fast();

        if due(self.last_audio, AUDIO_INTERVAL) {
            self.read_audio();
            self.last_audio = Some(Instant::now());
        }
        if due(self.last_wifi, WIFI_INTERVAL) {
            self.read_wifi();
            self.last_wifi = Some(Instant::now());
        }
        if due(self.last_profile, PROFILE_INTERVAL) {
            self.read_profile();
            self.last_profile = Some(Instant::now());
        }
        if due(self.last_public_ip, PUBLIC_IP_INTERVAL) {
            self.read_public_ip();
            self.last_public_ip = Some(Instant::now());
        }

        self.metrics != before
    }

    fn read_fast(&mut self) {
        self.metrics.cpu = cpu::read(&mut self.cpu);
        self.metrics.memory = memory::read();
        self.metrics.brightness = backlight::read();
        self.read_battery();
    }

    fn read_battery(&mut self) {
        if let Some(battery) = battery::read() {
            self.metrics.battery = Some(battery);
            self.last_upower = None;
            return;
        }

        if due(self.last_upower, UPOWER_INTERVAL) {
            self.metrics.battery = battery::read_upower();
            self.last_upower = Some(Instant::now());
        }
    }

    fn read_audio(&mut self) {
        self.metrics.audio = audio::read();
    }

    fn read_wifi(&mut self) {
        self.metrics.wifi = network::read();
        self.metrics.local_ip = network::local_ip();
    }

    fn read_public_ip(&mut self) {
        self.metrics.public_ip = network::public_ip();
    }

    fn read_profile(&mut self) {
        self.metrics.power_profile = power_profile::read();
    }

    pub fn refresh_audio_now(&mut self) {
        self.read_audio();
        self.last_audio = Some(Instant::now());
    }

    pub fn refresh_brightness_now(&mut self) {
        self.metrics.brightness = backlight::read();
    }

    pub fn refresh_power_profile_now(&mut self) {
        self.read_profile();
        self.last_profile = Some(Instant::now());
    }

    pub fn refresh_wifi_now(&mut self) {
        self.read_wifi();
        self.last_wifi = Some(Instant::now());
    }
}

fn due(last: Option<Instant>, interval: Duration) -> bool {
    last.is_none_or(|last| last.elapsed() >= interval)
}

/// Best-effort human-readable machine name, used in the header.
pub fn identity() -> String {
    for path in [
        "/sys/devices/virtual/dmi/id/product_version",
        "/sys/devices/virtual/dmi/id/product_name",
    ] {
        if let Some(value) = sysfs::read_string(path) {
            if !value.is_empty() && value != "Not Specified" && value != "System Product Name" {
                return value;
            }
        }
    }

    sysfs::read_string("/proc/sys/kernel/hostname")
        .filter(|hostname| !hostname.is_empty())
        .unwrap_or_else(|| "laptop".to_owned())
}
