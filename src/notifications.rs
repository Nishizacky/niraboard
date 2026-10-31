use std::time::{Duration, Instant};

/// Maximum number of notifications kept in memory. The oldest entry is
/// dropped once the list is full.
pub const MAX_ENTRIES: usize = 200;

/// Notifications in the same group are merged when they arrive within this
/// window, so holding a volume key does not fill the history with one row per
/// step.
const COALESCE_WINDOW: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Level {
    #[default]
    Info,
    Warn,
    Error,
}

/// Where a notification came from. This decides which source tab shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    System,
    Control,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    All,
    System,
    Controls,
    Issues,
}

impl Tab {
    pub const ALL: [Self; 4] = [Self::All, Self::System, Self::Controls, Self::Issues];

    pub fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::System => "System",
            Self::Controls => "Controls",
            Self::Issues => "Issues",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }

    pub fn previous(self) -> Self {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0);
        Self::ALL[(index + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    fn accepts(self, entry: &Entry) -> bool {
        match self {
            Self::All => true,
            Self::System => entry.source == Source::System,
            Self::Controls => entry.source == Source::Control,
            // Warnings and errors are worth surfacing regardless of the tab
            // they were logged from.
            Self::Issues => entry.level != Level::Info,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: u64,
    pub text: String,
    pub level: Level,
    pub source: Source,
    group: &'static str,
    created: Instant,
}

impl Entry {
    /// Short relative age, e.g. `now`, `12s`, `3m`, or `1h`.
    pub fn age_label(&self, now: Instant) -> String {
        let seconds = now.saturating_duration_since(self.created).as_secs();

        if seconds < 5 {
            "now".to_owned()
        } else if seconds < 60 {
            format!("{seconds}s")
        } else if seconds < 3600 {
            format!("{}m", seconds / 60)
        } else {
            format!("{}h", seconds / 3600)
        }
    }
}

/// In-memory notification history plus the state of the notification window.
#[derive(Debug)]
pub struct Center {
    entries: Vec<Entry>,
    open: bool,
    tab: Tab,
    offset: usize,
    last_read_id: u64,
    next_id: u64,
}

impl Default for Center {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            open: false,
            tab: Tab::default(),
            offset: 0,
            // The first notification gets id 1, so id 0 always means "read".
            last_read_id: 0,
            next_id: 1,
        }
    }
}

impl Center {
    /// Record a notification. A repeat of the most recent group within
    /// [`COALESCE_WINDOW`] updates that row instead of adding another one.
    pub fn push(&mut self, text: String, level: Level, source: Source, group: &'static str) {
        let text = text.replace(['\n', '\r'], " ");
        let merged = self.entries.last().is_some_and(|entry| {
            entry.group == group
                && entry.level == level
                && entry.created.elapsed() < COALESCE_WINDOW
        });

        if merged {
            if let Some(entry) = self.entries.last_mut() {
                entry.text = text;
                entry.created = Instant::now();

                // A merged row is still the newest notification. Give it a
                // fresh id while the window is closed so updating an older,
                // already-read row brings the unread badge back.
                if !self.open {
                    entry.id = self.next_id;
                    self.next_id = self.next_id.wrapping_add(1);
                }
            }
            return;
        }

        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.entries.push(Entry {
            id,
            text,
            level,
            source,
            group,
            created: Instant::now(),
        });

        if self.entries.len() > MAX_ENTRIES {
            self.entries.remove(0);
        }
        if self.open {
            self.mark_read();
        }

        // Keep the rows the user is looking at stable while new entries
        // arrive above them.
        if self.open && self.offset > 0 {
            self.offset = self.offset.saturating_add(1);
        }
        self.clamp_offset();
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Open or close the window. Both directions mark everything as read.
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.mark_read();

        if self.open {
            self.offset = 0;
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.mark_read();
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    pub fn select_tab(&mut self, tab: Tab) {
        if self.tab != tab {
            self.tab = tab;
            self.offset = 0;
        }
    }

    pub fn next_tab(&mut self) {
        self.select_tab(self.tab.next());
    }

    pub fn previous_tab(&mut self) {
        self.select_tab(self.tab.previous());
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn scroll_by(&mut self, delta: i32) {
        let len = self.filtered_count();
        if len == 0 {
            self.offset = 0;
            return;
        }

        let max = len.saturating_sub(1);
        let target = i64::try_from(self.offset).unwrap_or(i64::MAX) + i64::from(delta);
        self.offset = target.clamp(0, i64::try_from(max).unwrap_or(i64::MAX)) as usize;
    }

    pub fn scroll_to_start(&mut self) {
        self.offset = 0;
    }

    pub fn scroll_to_end(&mut self) {
        self.offset = self.filtered_count().saturating_sub(1);
    }

    /// Number of unread notifications, always zero while the window is open
    /// because looking at the list marks it read.
    pub fn unread(&self) -> usize {
        if self.open {
            return 0;
        }

        self.entries
            .iter()
            .filter(|entry| entry.id > self.last_read_id)
            .count()
    }

    /// Newest first, filtered by the selected tab.
    pub fn filtered(&self) -> impl Iterator<Item = &Entry> {
        let tab = self.tab;
        self.entries
            .iter()
            .rev()
            .filter(move |entry| tab.accepts(entry))
    }

    pub fn filtered_count(&self) -> usize {
        self.filtered().count()
    }

    fn mark_read(&mut self) {
        if let Some(entry) = self.entries.last() {
            self.last_read_id = entry.id;
        }
    }

    fn clamp_offset(&mut self) {
        let max = self.filtered_count().saturating_sub(1);
        self.offset = self.offset.min(max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push(center: &mut Center, text: &str, level: Level, source: Source, group: &'static str) {
        center.push(text.to_owned(), level, source, group);
    }

    #[test]
    fn new_notifications_are_unread_until_the_window_opens() {
        let mut center = Center::default();
        assert_eq!(center.unread(), 0);

        push(&mut center, "one", Level::Info, Source::System, "one");
        push(&mut center, "two", Level::Info, Source::System, "two");
        assert_eq!(center.unread(), 2);

        center.toggle();
        assert!(center.is_open());
        assert_eq!(center.unread(), 0);

        push(&mut center, "three", Level::Info, Source::System, "three");
        assert_eq!(center.unread(), 0);

        center.close();
        assert_eq!(center.unread(), 0);

        push(&mut center, "four", Level::Info, Source::System, "four");
        assert_eq!(center.unread(), 1);
    }

    #[test]
    fn coalesces_repeats_of_the_same_group() {
        let mut center = Center::default();

        push(
            &mut center,
            "Volume 40%",
            Level::Info,
            Source::Control,
            "volume",
        );
        push(
            &mut center,
            "Volume 45%",
            Level::Info,
            Source::Control,
            "volume",
        );

        assert_eq!(center.filtered_count(), 1);
        assert_eq!(center.filtered().next().expect("entry").text, "Volume 45%");

        push(
            &mut center,
            "Volume failed",
            Level::Error,
            Source::Control,
            "volume",
        );
        assert_eq!(center.filtered_count(), 2);
    }

    #[test]
    fn merged_updates_become_unread_again() {
        let mut center = Center::default();
        push(
            &mut center,
            "Volume 40%",
            Level::Info,
            Source::Control,
            "volume",
        );

        center.toggle();
        center.close();
        assert_eq!(center.unread(), 0);

        push(
            &mut center,
            "Volume 45%",
            Level::Info,
            Source::Control,
            "volume",
        );

        assert_eq!(center.filtered_count(), 1);
        assert_eq!(center.unread(), 1);
    }

    #[test]
    fn tabs_filter_by_source_and_severity() {
        let mut center = Center::default();

        push(
            &mut center,
            "Battery low",
            Level::Warn,
            Source::System,
            "battery",
        );
        push(
            &mut center,
            "Volume",
            Level::Info,
            Source::Control,
            "volume",
        );
        push(
            &mut center,
            "Audio failed",
            Level::Error,
            Source::Control,
            "audio",
        );

        center.select_tab(Tab::All);
        assert_eq!(center.filtered_count(), 3);

        center.select_tab(Tab::System);
        assert_eq!(center.filtered_count(), 1);

        center.select_tab(Tab::Controls);
        assert_eq!(center.filtered_count(), 2);

        center.select_tab(Tab::Issues);
        assert_eq!(center.filtered_count(), 2);
    }

    #[test]
    fn oldest_entries_are_dropped_at_the_cap() {
        let mut center = Center::default();

        for index in 0..(MAX_ENTRIES + 5) {
            // Alternate the group so adjacent entries never coalesce.
            let group = if index % 2 == 0 { "even" } else { "odd" };
            push(
                &mut center,
                &format!("entry {index}"),
                Level::Info,
                Source::System,
                group,
            );
        }

        assert_eq!(center.filtered_count(), MAX_ENTRIES);
    }

    #[test]
    fn tabs_cycle_in_both_directions() {
        assert_eq!(Tab::All.next(), Tab::System);
        assert_eq!(Tab::Issues.next(), Tab::All);
        assert_eq!(Tab::All.previous(), Tab::Issues);
    }

    #[test]
    fn switching_tabs_resets_the_scroll_offset() {
        let mut center = Center::default();
        for index in 0..10 {
            push(
                &mut center,
                &format!("entry {index}"),
                Level::Info,
                Source::System,
                match index % 2 {
                    0 => "even",
                    _ => "odd",
                },
            );
        }

        center.scroll_by(4);
        assert_eq!(center.offset(), 4);

        center.next_tab();
        assert_eq!(center.offset(), 0);
    }

    #[test]
    fn scrolling_stays_within_the_filtered_list() {
        let mut center = Center::default();
        push(&mut center, "one", Level::Info, Source::System, "one");

        center.scroll_by(10);
        assert_eq!(center.offset(), 0);

        center.scroll_to_end();
        assert_eq!(center.offset(), 0);

        for index in 0..5 {
            let group = if index % 2 == 0 {
                "more-even"
            } else {
                "more-odd"
            };
            push(
                &mut center,
                &format!("entry {index}"),
                Level::Info,
                Source::System,
                group,
            );
        }

        center.scroll_by(-5);
        assert_eq!(center.offset(), 0);
        center.scroll_by(3);
        assert_eq!(center.offset(), 3);
        center.scroll_by(100);
        assert_eq!(center.offset(), 5);
    }

    #[test]
    fn age_labels_are_relative() {
        let mut center = Center::default();
        push(&mut center, "entry", Level::Info, Source::System, "entry");
        let entry = center.filtered().next().expect("entry");

        assert_eq!(entry.age_label(Instant::now()), "now");
    }
}
