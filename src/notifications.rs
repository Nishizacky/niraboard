use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::system::command;

/// The mako stack is refreshed only while the window is open. Keeping this
/// interval reasonably long avoids spawning `makoctl` in the background.
const REFRESH_INTERVAL: Duration = Duration::from_secs(10);
const MAX_ENTRIES: usize = 200;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesktopNotification {
    pub id: u32,
    pub app: Option<String>,
    pub summary: String,
    pub body: Option<String>,
    pub urgency: Urgency,
    pub actions: Vec<(String, String)>,
}

/// The two views of mako's stack. Current notifications can be acted on;
/// history is the list of notifications mako already dismissed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Active,
    History,
}

impl Tab {
    pub const ALL: [Self; 2] = [Self::Active, Self::History];

    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::History => "History",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Active => Self::History,
            Self::History => Self::Active,
        }
    }

    pub fn previous(self) -> Self {
        self.next()
    }
}

/// State for the mako notification browser. niraboard never stores a
/// notification itself: mako remains the only owner of the stack.
#[derive(Debug, Default)]
pub struct Center {
    open: bool,
    tab: Tab,
    active: Vec<DesktopNotification>,
    history: Vec<DesktopNotification>,
    selected: usize,
    refreshed: Option<Instant>,
    error: Option<String>,
}

impl Center {
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Open or close the browser. Opening refreshes the stack immediately.
    pub fn toggle(&mut self) -> bool {
        if self.open {
            self.close();
        } else {
            self.open();
        }
        true
    }

    pub fn open(&mut self) {
        self.open = true;
        self.selected = 0;
        self.refresh(true);
    }

    pub fn close(&mut self) {
        self.open = false;
        self.error = None;
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn select_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.selected = 0;
        }
    }

    pub fn next_tab(&mut self) {
        self.select_tab(self.tab.next());
    }

    pub fn previous_tab(&mut self) {
        self.select_tab(self.tab.previous());
    }

    pub fn entries(&self) -> &[DesktopNotification] {
        match self.tab {
            Tab::Active => &self.active,
            Tab::History => &self.history,
        }
    }

    pub fn active_count(&self) -> usize {
        self.active.len()
    }

    pub fn history_count(&self) -> usize {
        self.history.len()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn select(&mut self, index: usize) {
        self.selected = index.min(self.entries().len().saturating_sub(1));
    }

    pub fn move_selection(&mut self, delta: i32) {
        let len = self.entries().len();
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
        self.selected = self.entries().len().saturating_sub(1);
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Refresh if the window is open and the refresh interval elapsed.
    pub fn tick(&mut self) -> bool {
        if !self.open {
            return false;
        }

        self.refresh(false)
    }

    /// Read mako's active and historical notifications. When `force` is false
    /// the call is skipped until [`REFRESH_INTERVAL`] has passed.
    pub fn refresh(&mut self, force: bool) -> bool {
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

        match read_stack() {
            Ok((active, history)) => {
                let changed =
                    self.active != active || self.history != history || self.error.take().is_some();
                self.active = active;
                self.history = history;
                self.clamp_selection();
                changed
            }
            Err(error) => {
                let message = one_line(&format!("{error:#}"));
                let changed = self.error.as_deref() != Some(message.as_str())
                    || !self.active.is_empty()
                    || !self.history.is_empty();
                self.active.clear();
                self.history.clear();
                self.error = Some(message);
                self.selected = 0;
                changed
            }
        }
    }

    /// Invoke the default action of the selected active notification.
    pub fn invoke_selected(&mut self) -> bool {
        let Some(notification) = self.selected_active().cloned() else {
            return false;
        };

        self.run_action(&["invoke", "-n", &notification.id.to_string()])
    }

    /// Dismiss the selected active notification. mako adds it to history.
    pub fn dismiss_selected(&mut self) -> bool {
        let Some(notification) = self.selected_active().cloned() else {
            return false;
        };

        self.run_action(&["dismiss", "-n", &notification.id.to_string()])
    }

    pub fn dismiss_all(&mut self) -> bool {
        self.run_action(&["dismiss", "-a"])
    }

    /// Restore mako's most recently dismissed notification.
    pub fn restore(&mut self) -> bool {
        self.run_action(&["restore"])
    }

    fn selected_active(&self) -> Option<&DesktopNotification> {
        if self.tab != Tab::Active {
            return None;
        }
        self.active.get(self.selected)
    }

    fn run_action(&mut self, args: &[&str]) -> bool {
        match command::run("makoctl", args) {
            Ok(_) => {
                self.refresh(true);
            }
            Err(error) => {
                self.error = Some(one_line(&format!("{error:#}")));
            }
        }

        // The window is open while an action is running, so it always needs a
        // frame to show the result (or the refresh that followed it).
        true
    }

    fn clamp_selection(&mut self) {
        self.selected = self.selected.min(self.entries().len().saturating_sub(1));
    }

    #[cfg(test)]
    pub fn set_entries(
        &mut self,
        active: Vec<DesktopNotification>,
        history: Vec<DesktopNotification>,
    ) {
        self.active = active;
        self.history = history;
        self.selected = 0;
        self.error = None;
    }

    #[cfg(test)]
    pub fn force_open(&mut self) {
        self.open = true;
    }
}

fn read_stack() -> Result<(Vec<DesktopNotification>, Vec<DesktopNotification>)> {
    let active = read_list("list")?;
    let history = read_list("history")?;

    Ok((active, history))
}

fn read_list(subcommand: &str) -> Result<Vec<DesktopNotification>> {
    let output = command::run("makoctl", &[subcommand, "-j"])
        .with_context(|| format!("failed to read mako {subcommand}"))?;
    let raw: Vec<RawNotification> =
        serde_json::from_str(&output).context("failed to parse makoctl JSON")?;

    Ok(raw
        .into_iter()
        .take(MAX_ENTRIES)
        .map(DesktopNotification::from)
        .collect())
}

#[derive(Debug, Deserialize)]
struct RawNotification {
    #[serde(default)]
    id: u32,
    #[serde(default)]
    app_name: Option<String>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    urgency: Option<String>,
    #[serde(default)]
    actions: BTreeMap<String, Option<String>>,
}

impl From<RawNotification> for DesktopNotification {
    fn from(notification: RawNotification) -> Self {
        Self {
            id: notification.id,
            app: clean_optional(notification.app_name),
            summary: clean(notification.summary.as_deref().unwrap_or("")),
            body: clean_optional(notification.body),
            urgency: match notification.urgency.as_deref() {
                Some("low") => Urgency::Low,
                Some("critical") => Urgency::Critical,
                _ => Urgency::Normal,
            },
            actions: notification
                .actions
                .into_iter()
                .map(|(key, title)| {
                    let key = clean(&key);
                    let title = title
                        .as_deref()
                        .map(clean)
                        .filter(|title| !title.is_empty())
                        .unwrap_or_else(|| key.clone());
                    (key, title)
                })
                .collect(),
        }
    }
}

fn clean_optional(value: Option<String>) -> Option<String> {
    let value = clean(value.as_deref().unwrap_or(""));
    if value.is_empty() { None } else { Some(value) }
}

/// Notification text is external input. Drop control characters (including
/// terminal escape sequences) and flatten newlines/tabs so a notification
/// cannot disturb the layout or inject terminal control codes.
fn clean(text: &str) -> String {
    let mut cleaned = String::with_capacity(text.len());

    for character in text.chars() {
        match character {
            '\n' | '\r' | '\t' => cleaned.push(' '),
            character if character.is_control() => {}
            character => cleaned.push(character),
        }
    }

    cleaned.trim().to_owned()
}

fn one_line(text: &str) -> String {
    let text = clean(text);
    let mut first = text.lines().next().unwrap_or("").to_owned();
    if first.chars().count() > 120 {
        first = first.chars().take(117).collect();
        first.push_str("...");
    }
    first
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(id: u32, summary: &str) -> DesktopNotification {
        DesktopNotification {
            id,
            app: Some("test".to_owned()),
            summary: summary.to_owned(),
            body: None,
            urgency: Urgency::Normal,
            actions: Vec::new(),
        }
    }

    #[test]
    fn parses_mako_json() {
        let json = r#"[
          {
            "id": 7,
            "app_name": "Telegram",
            "app_icon": "telegram",
            "category": "im.received",
            "desktop_entry": "telegram",
            "summary": "Alice",
            "body": "hello\nworld",
            "urgency": "critical",
            "actions": {"default": "Open", "reply": "Reply"}
          }
        ]"#;

        let raw: Vec<RawNotification> = serde_json::from_str(json).expect("valid JSON");
        let notification = DesktopNotification::from(raw.into_iter().next().expect("entry"));

        assert_eq!(notification.id, 7);
        assert_eq!(notification.app.as_deref(), Some("Telegram"));
        assert_eq!(notification.summary, "Alice");
        assert_eq!(notification.body.as_deref(), Some("hello world"));
        assert_eq!(notification.urgency, Urgency::Critical);
        assert_eq!(notification.actions.len(), 2);
    }

    #[test]
    fn missing_optional_fields_are_tolerated() {
        let json = r#"[{"id": 1, "summary": "Only a summary", "actions": {"default": null}}]"#;
        let raw: Vec<RawNotification> = serde_json::from_str(json).expect("valid JSON");
        let notification = DesktopNotification::from(raw.into_iter().next().expect("entry"));

        assert_eq!(notification.app, None);
        assert_eq!(notification.body, None);
        assert_eq!(notification.urgency, Urgency::Normal);
        // mako prints an empty action title as JSON null.
        assert_eq!(
            notification.actions,
            vec![("default".to_owned(), "default".to_owned())]
        );
    }

    #[test]
    fn control_characters_are_removed() {
        let json = r#"[{"id": 2, "summary": "a\u001b[31mb", "body": "c\nd\te"}]"#;
        let raw: Vec<RawNotification> = serde_json::from_str(json).expect("valid JSON");
        let notification = DesktopNotification::from(raw.into_iter().next().expect("entry"));

        assert_eq!(notification.summary, "a[31mb");
        assert_eq!(notification.body.as_deref(), Some("c d e"));
    }

    #[test]
    fn tabs_alternate() {
        assert_eq!(Tab::Active.next(), Tab::History);
        assert_eq!(Tab::History.next(), Tab::Active);
        assert_eq!(Tab::Active.previous(), Tab::History);
    }

    #[test]
    fn switching_tabs_resets_the_selection() {
        let mut center = Center::default();
        center.set_entries(
            vec![sample(1, "one"), sample(2, "two")],
            vec![sample(3, "three")],
        );
        center.select(1);
        assert_eq!(center.selected(), 1);

        center.select_tab(Tab::History);
        assert_eq!(center.selected(), 0);
        assert_eq!(center.entries().len(), 1);
    }

    #[test]
    fn selection_stays_within_the_current_tab() {
        let mut center = Center::default();
        center.set_entries(
            vec![sample(1, "one"), sample(2, "two")],
            vec![sample(3, "three")],
        );

        center.move_selection(100);
        assert_eq!(center.selected(), 1);
        center.move_selection(-100);
        assert_eq!(center.selected(), 0);
        center.select_end();
        assert_eq!(center.selected(), 1);
    }

    #[test]
    fn actions_only_target_active_notifications() {
        let mut center = Center::default();
        center.set_entries(vec![sample(1, "one")], vec![sample(2, "two")]);
        center.select_tab(Tab::History);

        // History entries cannot be invoked or dismissed by id in mako.
        assert!(!center.invoke_selected());
        assert!(!center.dismiss_selected());
    }

    #[test]
    fn failed_refresh_reports_an_error_and_clears_entries() {
        let mut center = Center::default();
        center.set_entries(vec![sample(1, "one")], vec![sample(2, "two")]);
        center.force_open();

        // No mako daemon is available in the test environment.
        center.refresh(true);

        assert!(!center.error().unwrap_or("").is_empty());
        assert_eq!(center.active_count(), 0);
        assert_eq!(center.history_count(), 0);
    }
}
