use std::time::{Duration, Instant};

use anyhow::Error;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Position;

use crate::actions;
use crate::config::Config;
use crate::event::Event;
pub use crate::notifications::Level as StatusLevel;
use crate::notifications::{Center, Source};
use crate::system::{Metrics, Monitor, PowerProfile, identity};
use crate::ui::{ClickAction, HitAreas};

const STATUS_TTL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Power,
    Brightness,
    Audio,
}

impl Focus {
    const ALL: [Self; 3] = [Self::Power, Self::Brightness, Self::Audio];

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|focus| *focus == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|focus| *focus == self)
            .unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

/// A dashboard card. The order of [`Config::cards`](crate::config::Config::cards)
/// decides where each one is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    Battery,
    Cpu,
    Memory,
    Disk,
    Network,
    Power,
    Brightness,
    Audio,
}

impl Card {
    pub const ALL: [Self; 8] = [
        Self::Battery,
        Self::Cpu,
        Self::Memory,
        Self::Disk,
        Self::Network,
        Self::Power,
        Self::Brightness,
        Self::Audio,
    ];

    /// Parse a card name from the configuration file.
    pub fn from_name(name: &str) -> Option<Self> {
        let normalized = name.to_ascii_lowercase().replace([' ', '-', '_'], "");

        match normalized.as_str() {
            "battery" => Some(Self::Battery),
            "cpu" => Some(Self::Cpu),
            "memory" | "ram" => Some(Self::Memory),
            "disk" | "storage" | "filesystem" => Some(Self::Disk),
            "network" | "wifi" => Some(Self::Network),
            "power" | "profile" | "powerprofile" => Some(Self::Power),
            "brightness" | "backlight" => Some(Self::Brightness),
            "audio" | "volume" => Some(Self::Audio),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Battery => "battery",
            Self::Cpu => "cpu",
            Self::Memory => "memory",
            Self::Disk => "disk",
            Self::Network => "network",
            Self::Power => "power",
            Self::Brightness => "brightness",
            Self::Audio => "audio",
        }
    }
}

/// How the dashboard cards are arranged on screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayoutMode {
    /// One card per row.
    #[default]
    Vertical,
    /// One card per column.
    Horizontal,
    /// A grid with two equal-width columns.
    TwoColumns,
    /// A grid with three equal-width columns.
    ThreeColumns,
}

impl LayoutMode {
    pub const ALL: [Self; 4] = [
        Self::Vertical,
        Self::Horizontal,
        Self::TwoColumns,
        Self::ThreeColumns,
    ];

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|mode| *mode == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Vertical => "Vertical",
            Self::Horizontal => "Horizontal",
            Self::TwoColumns => "2 columns",
            Self::ThreeColumns => "3 columns",
        }
    }

    /// Name used for this layout in the configuration file.
    pub fn name(self) -> &'static str {
        match self {
            Self::Vertical => "vertical",
            Self::Horizontal => "horizontal",
            Self::TwoColumns => "2-columns",
            Self::ThreeColumns => "3-columns",
        }
    }

    /// Parse a layout name from the configuration file.
    pub fn from_name(name: &str) -> Option<Self> {
        let normalized = name.to_ascii_lowercase().replace([' ', '-', '_'], "");

        match normalized.as_str() {
            "vertical" | "rows" => Some(Self::Vertical),
            "horizontal" | "columns" => Some(Self::Horizontal),
            "2columns" | "twocolumns" => Some(Self::TwoColumns),
            "3columns" | "threecolumns" => Some(Self::ThreeColumns),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub struct StatusMessage {
    pub text: String,
    pub level: StatusLevel,
    created: Instant,
}

impl StatusMessage {
    fn new(text: String, level: StatusLevel) -> Self {
        Self {
            text,
            level,
            created: Instant::now(),
        }
    }

    fn is_expired(&self) -> bool {
        self.created.elapsed() >= STATUS_TTL
    }
}

pub struct App {
    pub monitor: Monitor,
    pub focus: Focus,
    pub layout: LayoutMode,
    pub config: Config,
    pub status: Option<StatusMessage>,
    pub hits: HitAreas,
    pub should_quit: bool,
    pub show_quit_confirmation: bool,
    pub identity: String,
    pub notifications: Center,
    clock_text: String,
    network_requested: bool,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self::with_config(Config::default())
    }

    pub fn with_config(config: Config) -> Self {
        let clock_text = if config.clock.enabled {
            crate::clock::now(&config.clock.format)
        } else {
            String::new()
        };

        Self {
            monitor: Monitor::new(),
            focus: Focus::Power,
            layout: config.layout,
            config,
            status: None,
            hits: HitAreas::default(),
            should_quit: false,
            show_quit_confirmation: false,
            identity: identity(),
            notifications: Center::default(),
            clock_text,
            network_requested: false,
        }
    }

    /// Refresh the header clock. Returns true when the rendered text changed.
    ///
    /// Called more often than [`App::tick`] so formats that include seconds stay
    /// accurate without refreshing every system metric once per second.
    pub fn update_clock(&mut self) -> bool {
        if !self.config.clock.enabled {
            return false;
        }

        let text = crate::clock::now(&self.config.clock.format);
        if text == self.clock_text {
            return false;
        }

        self.clock_text = text;
        true
    }

    /// Clock and date text for the header, or `None` when it is hidden.
    pub fn clock_text(&self) -> Option<&str> {
        if !self.config.clock.enabled || self.clock_text.is_empty() {
            return None;
        }

        Some(self.clock_text.as_str())
    }

    /// Show a configuration problem in the footer without aborting startup.
    pub fn report_config_warning(&mut self, error: Error) {
        self.set_error("Config", error, Source::System, "config-error");
    }

    pub fn refresh_all(&mut self) -> bool {
        let before = self.monitor.metrics.clone();
        let changed = self.monitor.refresh_all();
        changed | self.detect_changes(&before)
    }

    /// Take a pending request to launch the interactive network editor.
    pub fn take_network_request(&mut self) -> bool {
        std::mem::take(&mut self.network_requested)
    }

    /// Report the result of the interactive network editor to the status bar.
    pub fn network_settings_finished(&mut self, result: anyhow::Result<()>) {
        match result {
            Ok(()) => self.set_status(
                "Network settings closed".to_owned(),
                StatusLevel::Info,
                Source::Control,
                "network-settings",
            ),
            Err(error) => self.set_error(
                "Network settings",
                error,
                Source::Control,
                "network-settings-error",
            ),
        }
    }

    /// Called once per tick. Returns true when a redraw is needed.
    pub fn tick(&mut self) -> bool {
        let before = self.monitor.metrics.clone();
        let mut changed = self.monitor.tick();
        changed |= self.detect_changes(&before);

        if self.status.as_ref().is_some_and(StatusMessage::is_expired) {
            self.status = None;
            changed = true;
        }

        // Relative notification timestamps need a fresh frame even when no
        // metric changed.
        if self.notifications.is_open() {
            changed = true;
        }

        changed
    }

    /// Turn transitions in the metrics snapshot into notifications. Only
    /// crossing a threshold or changing state logs an entry, so a low battery
    /// or a hot CPU does not repeat every tick.
    fn detect_changes(&mut self, before: &Metrics) -> bool {
        let mut events: Vec<(String, StatusLevel, Source, &'static str)> = Vec::new();
        let after = &self.monitor.metrics;

        if let (Some(previous), Some(current)) = (&before.battery, &after.battery) {
            if previous.ac_online != current.ac_online {
                let (text, group) = if current.ac_online {
                    ("AC connected", "ac-connected")
                } else {
                    ("Running on battery", "ac-disconnected")
                };
                events.push((text.to_owned(), StatusLevel::Info, Source::System, group));
            }

            if current.percent <= 10 && previous.percent > 10 {
                events.push((
                    format!("Battery critical: {}%", current.percent),
                    StatusLevel::Error,
                    Source::System,
                    "battery-critical",
                ));
            } else if current.percent <= 25 && previous.percent > 25 {
                events.push((
                    format!("Battery low: {}%", current.percent),
                    StatusLevel::Warn,
                    Source::System,
                    "battery-low",
                ));
            }
        }

        let wifi_change = match (&before.wifi, &after.wifi) {
            (Some(previous), Some(current)) if previous.connected != current.connected => {
                Some(current)
            }
            _ => None,
        };
        if let Some(current) = wifi_change {
            if current.connected {
                let ssid = current
                    .ssid
                    .as_deref()
                    .filter(|ssid| !ssid.is_empty())
                    .map(|ssid| format!(": {ssid}"))
                    .unwrap_or_default();
                events.push((
                    format!("Wi-Fi connected{ssid}"),
                    StatusLevel::Info,
                    Source::System,
                    "wifi-connected",
                ));
            } else {
                events.push((
                    "Wi-Fi disconnected".to_owned(),
                    StatusLevel::Warn,
                    Source::System,
                    "wifi-disconnected",
                ));
            }
        }

        let hot_cpu = match (before.cpu.temp_c, after.cpu.temp_c) {
            (Some(previous), Some(current)) if previous < 85.0 && current >= 85.0 => Some(current),
            _ => None,
        };
        if let Some(current) = hot_cpu {
            events.push((
                format!("CPU hot: {current:.1} °C"),
                StatusLevel::Warn,
                Source::System,
                "cpu-hot",
            ));
        }

        let changed = !events.is_empty();
        for (text, level, source, group) in events {
            self.notifications.push(text, level, source, group);
        }

        changed
    }

    pub fn handle_event(&mut self, event: Event) -> bool {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(mouse) => self.handle_mouse(mouse),
            Event::Resize => true,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.kind != KeyEventKind::Press {
            return false;
        }

        if self.show_quit_confirmation {
            // Only y confirms; every other key cancels so stray presses never quit.
            let yes = matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y'));
            return self.confirm_quit(yes);
        }

        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            self.show_quit_confirmation = true;
            return true;
        }

        if self.notifications.is_open() {
            return self.handle_notification_key(key);
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                self.show_quit_confirmation = true;
                true
            }
            KeyCode::Char('y') | KeyCode::Char('Y') => true,
            KeyCode::Tab => {
                self.focus = self.focus.next();
                true
            }
            KeyCode::BackTab => {
                self.focus = self.focus.previous();
                true
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                self.adjust(-1);
                true
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                self.adjust(1);
                true
            }
            KeyCode::Char('m') | KeyCode::Char('M') => {
                self.toggle_mute();
                true
            }
            KeyCode::Char('n') | KeyCode::Char('N') => self.toggle_notifications(),
            KeyCode::Char('v') | KeyCode::Char('V') => {
                self.cycle_layout();
                true
            }
            KeyCode::Char('w') | KeyCode::Char('W') => {
                self.network_requested = true;
                true
            }
            KeyCode::Char('r') | KeyCode::Char('R') => {
                let _ = self.refresh_all();
                self.set_status(
                    "Refreshed".to_owned(),
                    StatusLevel::Info,
                    Source::Control,
                    "refresh",
                );
                true
            }
            KeyCode::Char('1') => {
                self.set_profile(PowerProfile::PowerSaver);
                true
            }
            KeyCode::Char('2') => {
                self.set_profile(PowerProfile::Balanced);
                true
            }
            KeyCode::Char('3') => {
                self.set_profile(PowerProfile::Performance);
                true
            }
            _ => false,
        }
    }

    /// Keys owned by the open notification window. Everything else is ignored
    /// so dashboard shortcuts cannot fire behind the overlay.
    fn handle_notification_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.notifications.close();
                true
            }
            KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                self.notifications.next_tab();
                true
            }
            KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                self.notifications.previous_tab();
                true
            }
            KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('J') => {
                self.notifications.scroll_by(1);
                true
            }
            KeyCode::Up | KeyCode::Char('k') | KeyCode::Char('K') => {
                self.notifications.scroll_by(-1);
                true
            }
            KeyCode::PageDown => {
                self.notifications.scroll_by(5);
                true
            }
            KeyCode::PageUp => {
                self.notifications.scroll_by(-5);
                true
            }
            KeyCode::Home => {
                self.notifications.scroll_to_start();
                true
            }
            KeyCode::End => {
                self.notifications.scroll_to_end();
                true
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.show_quit_confirmation = true;
                true
            }
            _ => false,
        }
    }

    fn toggle_notifications(&mut self) -> bool {
        self.notifications.toggle();
        true
    }

    fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }

        if self.show_quit_confirmation {
            return self.confirm_quit(false);
        }

        let position = Position::new(mouse.column, mouse.row);
        let action = self.hits.action_at(mouse.column, mouse.row);

        if let Some(ClickAction::ToggleNotifications) = action {
            return self.toggle_notifications();
        }

        if self.notifications.is_open() {
            if let Some(ClickAction::SelectNotificationTab(tab)) = action {
                self.notifications.select_tab(tab);
                return true;
            }

            // A click inside the panel is consumed; a click outside closes it
            // without activating whatever card was underneath.
            let inside = self
                .hits
                .notification_window()
                .is_some_and(|rect| rect.contains(position));
            if inside {
                return false;
            }

            self.notifications.close();
            return true;
        }

        let Some(action) = action else {
            return false;
        };

        match action {
            ClickAction::Focus(focus) => {
                if self.focus == focus {
                    false
                } else {
                    self.focus = focus;
                    true
                }
            }
            ClickAction::SetProfile(profile) => {
                self.set_profile(profile);
                true
            }
            ClickAction::StepBrightness(delta) => {
                self.step_brightness(delta);
                true
            }
            ClickAction::StepVolume(delta) => {
                self.step_volume(delta);
                true
            }
            ClickAction::ToggleMute => {
                self.toggle_mute();
                true
            }
            ClickAction::CycleLayout => {
                self.cycle_layout();
                true
            }
            ClickAction::OpenNetwork => {
                self.network_requested = true;
                true
            }
            ClickAction::ToggleNotifications => self.toggle_notifications(),
            ClickAction::SelectNotificationTab(_) => false,
        }
    }

    fn cycle_layout(&mut self) {
        self.layout = self.layout.next();
        self.set_status(
            format!("Layout: {}", self.layout.label()),
            StatusLevel::Info,
            Source::Control,
            "layout",
        );
    }

    fn adjust(&mut self, direction: i8) {
        match self.focus {
            Focus::Power => self.step_profile(direction),
            Focus::Brightness => {
                self.step_brightness(direction * self.config.steps.brightness_delta())
            }
            Focus::Audio => self.step_volume(direction * self.config.steps.volume_delta()),
        }
    }

    fn step_profile(&mut self, direction: i8) {
        let current = self
            .monitor
            .metrics
            .power_profile
            .unwrap_or(PowerProfile::Balanced);
        self.set_profile(current.step(direction));
    }

    fn set_profile(&mut self, profile: PowerProfile) {
        self.focus = Focus::Power;

        let result = actions::set_power_profile(profile);
        self.monitor.refresh_power_profile_now();

        match result {
            Ok(()) => self.set_status(
                format!("Power profile: {}", profile.label()),
                StatusLevel::Info,
                Source::Control,
                "power-profile",
            ),
            Err(error) => self.set_error(
                "Power profile",
                error,
                Source::Control,
                "power-profile-error",
            ),
        }
    }

    fn step_brightness(&mut self, delta: i8) {
        self.focus = Focus::Brightness;

        let result = actions::step_brightness(delta);
        self.monitor.refresh_brightness_now();

        match result {
            Ok(()) => {
                let level = self
                    .monitor
                    .metrics
                    .brightness
                    .as_ref()
                    .map(|brightness| format!("{}%", brightness.percent))
                    .unwrap_or_else(|| "updated".to_owned());
                self.set_status(
                    format!("Brightness: {level}"),
                    StatusLevel::Info,
                    Source::Control,
                    "brightness",
                );
            }
            Err(error) => self.set_error("Brightness", error, Source::Control, "brightness-error"),
        }
    }

    fn step_volume(&mut self, delta: i8) {
        self.focus = Focus::Audio;

        let result = actions::step_volume(delta);
        self.monitor.refresh_audio_now();

        match result {
            Ok(()) => {
                let text = match self.monitor.metrics.audio.as_ref() {
                    Some(audio) if audio.muted => {
                        format!("Volume {}% (muted)", audio.volume_percent)
                    }
                    Some(audio) => format!("Volume {}%", audio.volume_percent),
                    None => "Volume updated".to_owned(),
                };
                self.set_status(text, StatusLevel::Info, Source::Control, "volume");
            }
            Err(error) => self.set_error("Volume", error, Source::Control, "volume-error"),
        }
    }

    fn toggle_mute(&mut self) {
        self.focus = Focus::Audio;

        let result = actions::toggle_mute();
        self.monitor.refresh_audio_now();

        match result {
            Ok(()) => {
                let muted = self
                    .monitor
                    .metrics
                    .audio
                    .as_ref()
                    .is_some_and(|audio| audio.muted);
                let text = if muted { "Muted" } else { "Unmuted" };
                self.set_status(text.to_owned(), StatusLevel::Info, Source::Control, "mute");
            }
            Err(error) => self.set_error("Mute", error, Source::Control, "mute-error"),
        }
    }

    fn set_status(
        &mut self,
        text: String,
        level: StatusLevel,
        source: Source,
        group: &'static str,
    ) {
        self.notifications.push(text.clone(), level, source, group);
        self.status = Some(StatusMessage::new(text, level));
    }

    fn set_error(&mut self, context: &str, error: Error, source: Source, group: &'static str) {
        let detail = format!("{error:#}");
        let detail = detail.lines().next().unwrap_or("action failed");
        let mut text = format!("{context}: {detail}");

        if text.chars().count() > 110 {
            text = text.chars().take(107).collect();
            text.push_str("...");
        }

        self.set_status(text, StatusLevel::Error, source, group);
    }

    /// Confirm or cancel quit when the user presses y/n during confirmation.
    fn confirm_quit(&mut self, yes: bool) -> bool {
        self.show_quit_confirmation = false;
        if yes {
            self.should_quit = true;
        }
        true
    }

    pub fn status_text(&self) -> Option<(&str, StatusLevel)> {
        self.status
            .as_ref()
            .map(|status| (status.text.as_str(), status.level))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::Tab;
    use crate::system::{Battery, ChargeState, Wifi};

    fn battery(percent: u8, ac_online: bool) -> Battery {
        Battery {
            percent,
            state: ChargeState::Discharging,
            power_w: None,
            health_percent: None,
            energy_now_wh: None,
            energy_full_wh: None,
            ac_online,
        }
    }

    fn wifi(connected: bool, ssid: Option<&str>) -> Wifi {
        Wifi {
            connected,
            ssid: ssid.map(str::to_owned),
            signal: Some(80),
            rate_mbps: None,
        }
    }

    #[test]
    fn focus_cycles_in_both_directions() {
        assert_eq!(Focus::Power.next(), Focus::Brightness);
        assert_eq!(Focus::Brightness.next(), Focus::Audio);
        assert_eq!(Focus::Audio.next(), Focus::Power);
        assert_eq!(Focus::Power.previous(), Focus::Audio);
    }

    #[test]
    fn v_key_cycles_layout_and_sets_status() {
        let mut app = App::new();

        assert_eq!(app.layout, LayoutMode::Vertical);

        assert!(app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE)));
        assert_eq!(app.layout, LayoutMode::Horizontal);
        assert_eq!(
            app.status_text(),
            Some(("Layout: Horizontal", StatusLevel::Info))
        );

        assert!(app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE)));
        assert_eq!(app.layout, LayoutMode::TwoColumns);
    }

    #[test]
    fn w_key_requests_network_settings_once() {
        let mut app = App::new();

        assert!(!app.take_network_request());
        assert!(app.handle_key(KeyEvent::new(KeyCode::Char('w'), KeyModifiers::NONE)));
        assert!(app.take_network_request());
        assert!(!app.take_network_request());
    }

    #[test]
    fn layout_cycles_through_every_mode() {
        let mut layout = LayoutMode::default();
        let mut seen = Vec::new();

        for _ in 0..LayoutMode::ALL.len() {
            seen.push(layout);
            layout = layout.next();
        }

        assert_eq!(seen, LayoutMode::ALL);
        assert_eq!(layout, LayoutMode::default());
    }

    #[test]
    fn app_starts_without_running_commands() {
        let app = App::new();
        assert_eq!(app.focus, Focus::Power);
        assert!(!app.should_quit);
        assert!(!app.identity.is_empty());
    }

    #[test]
    fn app_uses_configured_default_layout() {
        let config = Config {
            layout: LayoutMode::TwoColumns,
            ..Config::default()
        };
        let app = App::with_config(config);

        assert_eq!(app.layout, LayoutMode::TwoColumns);
    }

    #[test]
    fn clock_text_uses_the_configured_format() {
        let config = Config {
            clock: crate::config::ClockConfig {
                enabled: true,
                format: "fixed".to_owned(),
            },
            ..Config::default()
        };
        let mut app = App::with_config(config);

        assert_eq!(app.clock_text(), Some("fixed"));
        assert!(!app.update_clock());
    }

    #[test]
    fn disabled_clock_has_no_text_and_does_not_redraw() {
        let config = Config {
            clock: crate::config::ClockConfig {
                enabled: false,
                format: "%H:%M".to_owned(),
            },
            ..Config::default()
        };
        let mut app = App::with_config(config);

        assert_eq!(app.clock_text(), None);
        assert!(!app.update_clock());
    }

    #[test]
    fn n_key_toggles_notifications_and_esc_closes_them() {
        let mut app = App::new();

        assert!(!app.notifications.is_open());
        assert!(app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE)));
        assert!(app.notifications.is_open());

        assert!(app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        assert!(!app.notifications.is_open());
        assert!(!app.show_quit_confirmation);
        assert!(!app.should_quit);
    }

    #[test]
    fn tab_switches_notification_tabs_while_open() {
        let mut app = App::new();
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));

        assert_eq!(app.notifications.tab(), Tab::All);
        assert!(app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(app.notifications.tab(), Tab::System);
        assert!(app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE)));
        assert_eq!(app.notifications.tab(), Tab::All);
    }

    #[test]
    fn status_changes_are_recorded_in_the_history() {
        let mut app = App::new();

        assert!(app.handle_key(KeyEvent::new(KeyCode::Char('v'), KeyModifiers::NONE)));

        let newest = app
            .notifications
            .filtered()
            .next()
            .expect("notification entry");
        assert_eq!(newest.text, "Layout: Horizontal");
        assert_eq!(newest.level, StatusLevel::Info);
        assert_eq!(newest.source, Source::Control);
        assert_eq!(
            app.status_text(),
            Some(("Layout: Horizontal", StatusLevel::Info))
        );
    }

    #[test]
    fn battery_low_is_reported_once_per_crossing() {
        let mut app = App::new();
        app.monitor.metrics.battery = Some(battery(30, false));
        let before = app.monitor.metrics.clone();
        app.monitor.metrics.battery = Some(battery(24, false));

        assert!(app.detect_changes(&before));
        let newest = app
            .notifications
            .filtered()
            .next()
            .expect("notification entry");
        assert_eq!(newest.text, "Battery low: 24%");
        assert_eq!(newest.level, StatusLevel::Warn);

        let before = app.monitor.metrics.clone();
        assert!(!app.detect_changes(&before));
    }

    #[test]
    fn critical_battery_and_ac_changes_are_reported() {
        let mut app = App::new();
        app.monitor.metrics.battery = Some(battery(12, false));
        let before = app.monitor.metrics.clone();
        app.monitor.metrics.battery = Some(battery(9, true));

        assert!(app.detect_changes(&before));
        let texts: Vec<&str> = app
            .notifications
            .filtered()
            .map(|entry| entry.text.as_str())
            .collect();
        assert!(texts.contains(&"AC connected"));
        assert!(texts.contains(&"Battery critical: 9%"));
    }

    #[test]
    fn wifi_disconnect_and_cpu_heat_are_reported() {
        let mut app = App::new();
        app.monitor.metrics.wifi = Some(wifi(false, None));
        let mut before = app.monitor.metrics.clone();
        before.wifi = Some(wifi(true, Some("home")));
        before.cpu.temp_c = Some(70.0);
        app.monitor.metrics.cpu.temp_c = Some(86.5);

        assert!(app.detect_changes(&before));
        let texts: Vec<&str> = app
            .notifications
            .filtered()
            .map(|entry| entry.text.as_str())
            .collect();
        assert!(texts.contains(&"Wi-Fi disconnected"));
        assert!(texts.contains(&"CPU hot: 86.5 °C"));
    }

    #[test]
    fn fully_read_history_has_no_unread_badge() {
        let mut app = App::new();
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        app.notifications.push(
            "entry".to_owned(),
            StatusLevel::Info,
            Source::System,
            "entry",
        );
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));

        assert_eq!(app.notifications.unread(), 0);
    }
}
