use std::time::{Duration, Instant};

use anyhow::Error;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

use crate::actions;
use crate::config::Config;
use crate::event::Event;
use crate::system::{Monitor, PowerProfile, identity};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusLevel {
    Info,
    Error,
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
            network_requested: false,
        }
    }

    /// Show a configuration problem in the footer without aborting startup.
    pub fn report_config_warning(&mut self, error: Error) {
        self.set_error("Config", error);
    }

    pub fn refresh_all(&mut self) -> bool {
        self.monitor.refresh_all()
    }

    /// Take a pending request to launch the interactive network editor.
    pub fn take_network_request(&mut self) -> bool {
        std::mem::take(&mut self.network_requested)
    }

    /// Report the result of the interactive network editor to the status bar.
    pub fn network_settings_finished(&mut self, result: anyhow::Result<()>) {
        match result {
            Ok(()) => self.set_status("Network settings closed".to_owned(), StatusLevel::Info),
            Err(error) => self.set_error("Network settings", error),
        }
    }

    /// Called once per tick. Returns true when a redraw is needed.
    pub fn tick(&mut self) -> bool {
        let mut changed = self.monitor.tick();

        if self.status.as_ref().is_some_and(StatusMessage::is_expired) {
            self.status = None;
            changed = true;
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

        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                self.show_quit_confirmation = true;
                true
            }
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Char('n') | KeyCode::Char('N') => {
                true
            }
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
                self.set_status("Refreshed".to_owned(), StatusLevel::Info);
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

    fn handle_mouse(&mut self, mouse: MouseEvent) -> bool {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return false;
        }

        if self.show_quit_confirmation {
            return self.confirm_quit(false);
        }

        let Some(action) = self.hits.action_at(mouse.column, mouse.row) else {
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
        }
    }

    fn cycle_layout(&mut self) {
        self.layout = self.layout.next();
        self.set_status(
            format!("Layout: {}", self.layout.label()),
            StatusLevel::Info,
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
            ),
            Err(error) => self.set_error("Power profile", error),
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
                self.set_status(format!("Brightness: {level}"), StatusLevel::Info);
            }
            Err(error) => self.set_error("Brightness", error),
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
                self.set_status(text, StatusLevel::Info);
            }
            Err(error) => self.set_error("Volume", error),
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
                self.set_status(text.to_owned(), StatusLevel::Info);
            }
            Err(error) => self.set_error("Mute", error),
        }
    }

    fn set_status(&mut self, text: String, level: StatusLevel) {
        self.status = Some(StatusMessage::new(text, level));
    }

    fn set_error(&mut self, context: &str, error: Error) {
        let detail = format!("{error:#}");
        let detail = detail.lines().next().unwrap_or("action failed");
        let mut text = format!("{context}: {detail}");

        if text.chars().count() > 110 {
            text = text.chars().take(107).collect();
            text.push_str("...");
        }

        self.set_status(text, StatusLevel::Error);
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
}
