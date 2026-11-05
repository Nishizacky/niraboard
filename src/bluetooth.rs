use std::time::{Duration, Instant};

use crate::actions;
use crate::system::bluetooth::{self, Adapter, Bluetooth, Device};

/// The panel is refreshed on demand and then periodically while open.
const REFRESH_INTERVAL: Duration = Duration::from_secs(10);
/// During a scan the device list is sampled more often, without blocking.
const SCAN_REFRESH_INTERVAL: Duration = Duration::from_secs(2);
/// How long BlueZ discovery runs before the panel stops it.
const SCAN_SECONDS: u64 = 8;

/// State for the Bluetooth control panel.
///
/// The panel owns only its view state and the latest snapshot. BlueZ remains
/// the owner of the adapter and pairings; every action uses BlueZ's D-Bus API
/// and is followed by a fresh read.
#[derive(Debug, Default)]
pub struct Panel {
    open: bool,
    data: Option<Bluetooth>,
    selected: usize,
    refreshed: Option<Instant>,
    error: Option<String>,
    notice: Option<String>,
    notice_is_error: bool,
    scanning: bool,
    scan_started: Option<Instant>,
    scan_adapter_path: Option<String>,
}

impl Panel {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Open or close the panel. The caller decides when to read the adapter,
    /// so it can draw a progress message before the blocking command starts.
    pub fn toggle(&mut self) -> bool {
        if self.open {
            self.close();
        } else {
            self.open();
        }
        self.open
    }

    pub fn open(&mut self) {
        self.stop_scan();
        self.open = true;
        self.selected = 0;
        self.scanning = false;
        self.scan_started = None;
        self.notice = None;
        self.notice_is_error = false;
    }

    pub fn close(&mut self) {
        self.stop_scan();
        self.open = false;
        self.scanning = false;
        self.scan_started = None;
        self.notice = None;
        self.notice_is_error = false;
        self.error = None;
    }

    pub fn data(&self) -> Option<&Bluetooth> {
        self.data.as_ref()
    }

    pub fn adapter(&self) -> Option<&Adapter> {
        self.data.as_ref().and_then(|data| data.adapter.as_ref())
    }

    pub fn devices(&self) -> &[Device] {
        self.data
            .as_ref()
            .map(|data| data.devices.as_slice())
            .unwrap_or(&[])
    }

    pub fn connected_count(&self) -> usize {
        self.data
            .as_ref()
            .map(Bluetooth::connected_count)
            .unwrap_or(0)
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, index: usize) {
        self.selected = index.min(self.devices().len().saturating_sub(1));
    }

    pub fn move_selection(&mut self, delta: i32) {
        let len = self.devices().len();
        if len == 0 {
            self.selected = 0;
            return;
        }

        let current = i64::try_from(self.selected).unwrap_or(i64::MAX);
        let target = current.saturating_add(i64::from(delta));
        self.selected = target.clamp(0, (len - 1) as i64) as usize;
    }

    pub fn select_start(&mut self) {
        self.selected = 0;
    }

    pub fn select_end(&mut self) {
        self.selected = self.devices().len().saturating_sub(1);
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn notice(&self) -> Option<(&str, bool)> {
        self.notice
            .as_deref()
            .map(|notice| (notice, self.notice_is_error))
    }

    pub fn is_scanning(&self) -> bool {
        self.scanning
    }

    /// Time left in the current discovery run, if one is active.
    pub fn scan_remaining(&self) -> Option<Duration> {
        if !self.scanning {
            return None;
        }

        self.scan_started
            .map(|started| Duration::from_secs(SCAN_SECONDS).saturating_sub(started.elapsed()))
    }

    /// Whether [`Panel::tick`] is about to perform a blocking read.
    pub fn refresh_due(&self) -> bool {
        if !self.open {
            return false;
        }

        let interval = if self.scanning {
            SCAN_REFRESH_INTERVAL
        } else {
            REFRESH_INTERVAL
        };

        self.refreshed
            .is_none_or(|refreshed| refreshed.elapsed() >= interval)
    }

    /// Label and connection state of the selected device, used to describe a
    /// pending connect or disconnect in the info bar.
    pub fn selected_label(&self) -> Option<(&str, bool)> {
        self.selected_device()
            .map(|device| (device.label(), device.connected))
    }

    /// Refresh if the panel is open and the normal interval has elapsed.
    pub fn tick(&mut self) -> bool {
        if !self.open {
            return false;
        }

        if self.scanning {
            let mut changed = false;

            if self
                .refreshed
                .is_some_and(|refreshed| refreshed.elapsed() >= SCAN_REFRESH_INTERVAL)
            {
                changed |= self.refresh(true);
            }

            if self
                .scan_started
                .is_some_and(|started| started.elapsed() >= Duration::from_secs(SCAN_SECONDS))
            {
                self.stop_scan();
                self.scanning = false;
                self.scan_started = None;
                self.set_notice("Scan complete".to_owned(), false);
                self.refresh(true);
                changed = true;
            }

            return changed;
        }

        self.refresh(false)
    }

    pub fn refresh_now(&mut self) -> bool {
        self.refresh(true)
    }

    /// Turn the adapter on or off, then reload the device list.
    pub fn toggle_power(&mut self) {
        let Some((adapter_path, powered)) = self
            .adapter()
            .map(|adapter| (adapter.path.clone(), adapter.powered))
        else {
            self.set_notice("No Bluetooth adapter".to_owned(), true);
            return;
        };

        if self.scanning {
            self.stop_scan();
            self.scanning = false;
            self.scan_started = None;
        }

        match actions::set_bluetooth_power(&adapter_path, !powered) {
            Ok(()) => {
                self.refresh(true);
                let state = if powered {
                    "Bluetooth off"
                } else {
                    "Bluetooth on"
                };
                self.set_notice(state.to_owned(), false);
            }
            Err(error) => self.set_error("Bluetooth power", error),
        }
    }

    /// Connect or disconnect the selected device.
    pub fn toggle_selected(&mut self) {
        let Some(device) = self.selected_device().cloned() else {
            return;
        };

        let connected = device.connected;
        let label = device.label().to_owned();
        let result = if connected {
            actions::disconnect_bluetooth(&device.path)
        } else {
            actions::connect_bluetooth(&device.path)
        };

        match result {
            Ok(()) => {
                self.refresh(true);
                let state = if connected {
                    "Disconnected"
                } else {
                    "Connected"
                };
                self.set_notice(format!("{state}: {label}"), false);
            }
            Err(error) => {
                let context = if connected {
                    "Bluetooth disconnect"
                } else {
                    "Bluetooth connect"
                };
                self.set_error(context, error);
            }
        }
    }

    /// Start a discovery run through BlueZ. The read loop keeps running while
    /// the adapter discovers devices.
    pub fn scan(&mut self) {
        if self.scanning {
            return;
        }

        let Some((adapter_path, powered)) = self
            .adapter()
            .map(|adapter| (adapter.path.clone(), adapter.powered))
        else {
            self.set_notice("No Bluetooth adapter".to_owned(), true);
            return;
        };
        if !powered {
            self.set_notice("Turn Bluetooth on before scanning".to_owned(), true);
            return;
        }

        match actions::start_bluetooth_scan(&adapter_path) {
            Ok(()) => {
                self.scanning = true;
                self.scan_started = Some(Instant::now());
                self.scan_adapter_path = Some(adapter_path);
                self.set_notice("Scanning for devices...".to_owned(), false);
            }
            Err(error) => self.set_error("Bluetooth scan", error),
        }
    }

    fn stop_scan(&mut self) {
        if let Some(adapter_path) = self.scan_adapter_path.take() {
            let _ = actions::stop_bluetooth_scan(&adapter_path);
        }
    }

    fn selected_device(&self) -> Option<&Device> {
        self.devices().get(self.selected)
    }

    /// Read the adapter and device list. With `force` false the call is skipped
    /// until [`REFRESH_INTERVAL`] has passed.
    fn refresh(&mut self, force: bool) -> bool {
        if !self.open && !force {
            return false;
        }
        if !force
            && self
                .refreshed
                .is_some_and(|refreshed| refreshed.elapsed() < REFRESH_INTERVAL)
        {
            return false;
        }

        self.refreshed = Some(Instant::now());

        match bluetooth::read() {
            Ok(data) => {
                let changed = self.data.as_ref() != Some(&data) || self.error.take().is_some();
                self.data = Some(data);
                self.clamp_selection();
                changed
            }
            Err(error) => {
                let message = one_line(&format!("{error:#}"));
                let changed =
                    self.error.as_deref() != Some(message.as_str()) || self.data.take().is_some();
                self.error = Some(message);
                self.selected = 0;
                changed
            }
        }
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.devices().len().saturating_sub(1));
    }

    fn set_notice(&mut self, text: String, is_error: bool) {
        self.notice = Some(text);
        self.notice_is_error = is_error;
    }

    fn set_error(&mut self, context: &str, error: anyhow::Error) {
        let detail = one_line(&format!("{error:#}"));
        self.set_notice(format!("{context}: {detail}"), true);
    }

    #[cfg(test)]
    pub fn force_open(&mut self) {
        self.open = true;
    }

    #[cfg(test)]
    pub fn set_data(&mut self, data: Bluetooth) {
        self.data = Some(data);
        self.error = None;
        self.selected = 0;
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        // Discovery outlives a client on the BlueZ side, so make a best effort
        // to stop it even if the panel is dropped without a normal close.
        self.stop_scan();
    }
}

fn one_line(text: &str) -> String {
    let mut first = text.lines().next().unwrap_or("").trim().to_owned();
    if first.chars().count() > 120 {
        first = first.chars().take(117).collect();
        first.push_str("...");
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(address: &str, name: &str, connected: bool) -> Device {
        Device {
            path: format!(
                "/org/bluez/hci0/dev_{}",
                address.replace(':', "_").to_ascii_uppercase()
            ),
            address: address.to_owned(),
            name: Some(name.to_owned()),
            connected,
        }
    }

    fn sample() -> Bluetooth {
        Bluetooth {
            adapter: Some(Adapter {
                path: "/org/bluez/hci0".to_owned(),
                address: "AA:BB:CC:DD:EE:FF".to_owned(),
                alias: Some("laptop".to_owned()),
                powered: true,
                discoverable: false,
                discovering: false,
            }),
            devices: vec![
                device("00:00:00:00:00:01", "one", false),
                device("00:00:00:00:00:02", "two", true),
            ],
        }
    }

    #[test]
    fn selection_stays_within_the_device_list() {
        let mut panel = Panel::default();
        panel.set_data(sample());

        panel.move_selection(100);
        assert_eq!(panel.selected(), 1);
        panel.move_selection(-100);
        assert_eq!(panel.selected(), 0);
        panel.select_end();
        assert_eq!(panel.selected(), 1);
    }

    #[test]
    fn selection_is_clamped_after_a_refresh_shrinks_the_list() {
        let mut panel = Panel::default();
        panel.set_data(sample());
        panel.select_end();

        let mut data = sample();
        data.devices.pop();
        panel.data = Some(data);
        panel.clamp_selection();

        assert_eq!(panel.selected(), 0);
    }

    #[test]
    fn connected_count_only_counts_connected_devices() {
        let panel = {
            let mut panel = Panel::default();
            panel.set_data(sample());
            panel
        };

        assert_eq!(panel.connected_count(), 1);
    }

    #[test]
    fn scan_remaining_is_visible_only_while_scanning() {
        let mut panel = Panel::default();
        assert_eq!(panel.scan_remaining(), None);

        panel.scanning = true;
        panel.scan_started = Some(Instant::now());

        let remaining = panel.scan_remaining().expect("scan countdown");
        assert!(remaining <= Duration::from_secs(SCAN_SECONDS));
        assert!(!remaining.is_zero());
    }

    #[test]
    fn refresh_due_tracks_the_open_panel_interval() {
        let mut panel = Panel::default();
        assert!(!panel.refresh_due());

        panel.open = true;
        panel.refreshed = Some(Instant::now());
        assert!(!panel.refresh_due());

        panel.refreshed = Some(Instant::now() - REFRESH_INTERVAL);
        assert!(panel.refresh_due());
    }
}
