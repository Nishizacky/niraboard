use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Gauge, Paragraph, Wrap};

use crate::app::{App, Card, Focus, LayoutMode, StatusLevel};
use crate::config::{Colors, Steps};
use crate::notifications::{Center, Tab, Urgency};
use crate::system::{Battery, ChargeState, PowerProfile};

/// Smallest terminal width that keeps the header, footer, and a single card
/// readable. Wider layouts add one [`MIN_CARD_WIDTH`] per extra column.
const MIN_WIDTH: u16 = 72;
/// A card needs two border rows plus enough room for its interactive content.
const MIN_CARD_HEIGHT: u16 = 5;
/// Width of one card in a multi-column layout. Three columns plus the two
/// one-cell gaps add up to the 72-cell base width.
const MIN_CARD_WIDTH: u16 = 23;
const HEADER_HEIGHT: u16 = 1;
const FOOTER_HEIGHT: u16 = 2;
const COLUMN_GAP: u16 = 1;

/// Screen rectangles recorded during rendering so mouse events can be mapped
/// back to application actions.
#[derive(Debug, Default)]
pub struct HitAreas {
    cards: Vec<(Rect, Focus)>,
    profile_rows: Vec<(Rect, PowerProfile)>,
    brightness_minus: Option<Rect>,
    brightness_plus: Option<Rect>,
    volume_minus: Option<Rect>,
    volume_plus: Option<Rect>,
    mute: Option<Rect>,
    layout: Option<Rect>,
    wifi: Option<Rect>,
    notification_button: Option<Rect>,
    notification_tabs: Vec<(Rect, Tab)>,
    notification_rows: Vec<(Rect, usize)>,
    notification_window: Option<Rect>,
    steps: Steps,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickAction {
    Focus(Focus),
    SetProfile(PowerProfile),
    StepBrightness(i8),
    StepVolume(i8),
    ToggleMute,
    CycleLayout,
    OpenNetwork,
    ToggleNotifications,
    SelectNotificationTab(Tab),
    SelectNotification(usize),
}

impl HitAreas {
    pub fn action_at(&self, x: u16, y: u16) -> Option<ClickAction> {
        let position = Position::new(x, y);

        if contains(self.notification_button, position) {
            return Some(ClickAction::ToggleNotifications);
        }
        if let Some((_, tab)) = self
            .notification_tabs
            .iter()
            .find(|(rect, _)| rect.contains(position))
        {
            return Some(ClickAction::SelectNotificationTab(*tab));
        }
        if let Some((_, index)) = self
            .notification_rows
            .iter()
            .find(|(rect, _)| rect.contains(position))
        {
            return Some(ClickAction::SelectNotification(*index));
        }
        if contains(self.layout, position) {
            return Some(ClickAction::CycleLayout);
        }
        if contains(self.brightness_minus, position) {
            return Some(ClickAction::StepBrightness(-self.steps.brightness_delta()));
        }
        if contains(self.brightness_plus, position) {
            return Some(ClickAction::StepBrightness(self.steps.brightness_delta()));
        }
        if contains(self.volume_minus, position) {
            return Some(ClickAction::StepVolume(-self.steps.volume_delta()));
        }
        if contains(self.volume_plus, position) {
            return Some(ClickAction::StepVolume(self.steps.volume_delta()));
        }
        if contains(self.mute, position) {
            return Some(ClickAction::ToggleMute);
        }
        if contains(self.wifi, position) {
            return Some(ClickAction::OpenNetwork);
        }

        self.profile_rows
            .iter()
            .find(|(rect, _)| rect.contains(position))
            .map(|(_, profile)| ClickAction::SetProfile(*profile))
            .or_else(|| {
                self.cards
                    .iter()
                    .find(|(rect, _)| rect.contains(position))
                    .map(|(_, focus)| ClickAction::Focus(*focus))
            })
    }

    /// Rectangle occupied by the notification overlay, when it is open.
    pub fn notification_window(&self) -> Option<Rect> {
        self.notification_window
    }
}

fn contains(rect: Option<Rect>, position: Position) -> bool {
    rect.is_some_and(|rect| rect.contains(position))
}

/// Render one frame and return the rectangles that make up the interactive UI.
pub fn draw(frame: &mut Frame, app: &App) -> HitAreas {
    let area = frame.area();
    let colors = app.config.colors;
    let (min_width, min_height) = minimum_size(app.layout, app.config.cards.len());

    if area.width < min_width || area.height < min_height {
        draw_too_small(frame, area, min_width, min_height, colors);
        return HitAreas::default();
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(HEADER_HEIGHT),
        Constraint::Fill(1),
        Constraint::Length(FOOTER_HEIGHT),
    ])
    .areas(area);

    let mut hits = HitAreas {
        steps: app.config.steps,
        ..HitAreas::default()
    };
    draw_header(frame, header, app, &mut hits, colors);

    draw_body(frame, body, app, &mut hits, colors);
    draw_footer(frame, footer, app, &mut hits, colors);

    if app.notifications.is_open() {
        draw_notifications(frame, area, app, &mut hits, colors);
    }

    if app.show_quit_confirmation {
        draw_quit_confirmation(frame, area, colors);
    }

    hits
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App, hits: &mut HitAreas, colors: Colors) {
    let columns = layout_columns(app.layout, app.config.cards.len());

    draw_grid(frame, area, app, hits, columns, colors);
}

/// Number of columns a layout uses for `card_count` configured cards. The grid
/// layouts never use more columns than there are cards to show.
fn layout_columns(layout: LayoutMode, card_count: usize) -> usize {
    let card_count = card_count.max(1);

    match layout {
        LayoutMode::Vertical => 1,
        LayoutMode::Horizontal => card_count,
        LayoutMode::TwoColumns => 2.min(card_count),
        LayoutMode::ThreeColumns => 3.min(card_count),
    }
}

/// Smallest terminal size that can show every configured card in `layout`
/// without squeezing rows below their usable height. Each layout has a
/// different bottleneck: `Vertical` needs height for its card rows, while
/// `Horizontal` needs width for its card columns.
fn minimum_size(layout: LayoutMode, card_count: usize) -> (u16, u16) {
    let card_count = card_count.max(1);
    let columns = layout_columns(layout, card_count);
    let rows = card_count.div_ceil(columns);
    let gap = if columns > 1 { COLUMN_GAP } else { 0 };

    let columns = u16::try_from(columns).unwrap_or(u16::MAX);
    let rows = u16::try_from(rows).unwrap_or(u16::MAX);
    let width = columns
        .saturating_mul(MIN_CARD_WIDTH)
        .saturating_add(gap.saturating_mul(columns.saturating_sub(1)))
        .max(MIN_WIDTH);
    let height = HEADER_HEIGHT
        .saturating_add(FOOTER_HEIGHT)
        .saturating_add(rows.saturating_mul(MIN_CARD_HEIGHT));

    (width, height)
}

/// Arrange the configured cards in a row-major grid, in the order given by the
/// `cards` array in the configuration file. Columns always receive exactly the
/// same width so two- and three-column layouts line up cleanly; any leftover
/// cells are absorbed by the gaps rather than by one of the cards.
///
/// Cards that would fall outside the screen are skipped instead of being
/// squeezed into the visible area.
fn draw_grid(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    hits: &mut HitAreas,
    columns: usize,
    colors: Colors,
) {
    let cards = &app.config.cards;

    if area.is_empty() || cards.is_empty() {
        return;
    }

    for (card, card_area) in cards.iter().zip(card_rects(area, columns, cards.len())) {
        draw_card(frame, *card, card_area, app, hits, colors);
    }
}

/// Compute the rectangles for the configured cards in row-major order.
///
/// Cards that would fall outside the screen are omitted instead of being
/// squeezed into the visible area.
fn card_rects(area: Rect, columns: usize, count: usize) -> Vec<Rect> {
    if area.is_empty() || count == 0 {
        return Vec::new();
    }

    let columns = columns.clamp(1, count);
    let rows = count.div_ceil(columns);
    let column_gap = if columns > 1 { COLUMN_GAP } else { 0 };
    let column_spans = equal_spans(area.x, area.width, columns, column_gap);
    // Only draw whole rows that fit within the body. `rows` can exceed the
    // available height, so cards below the last visible row are omitted.
    let visible_rows = rows.min(area.height as usize);
    let row_spans = equal_spans(area.y, area.height, visible_rows, 0);

    let mut rects = Vec::with_capacity(count);

    for index in 0..count {
        let Some(&(y, height)) = row_spans.get(index / columns) else {
            continue;
        };
        let Some(&(x, width)) = column_spans.get(index % columns) else {
            continue;
        };
        let rect = Rect::new(x, y, width, height);

        if !rect.is_empty() {
            rects.push(rect);
        }
    }

    rects
}

fn draw_card(
    frame: &mut Frame,
    card: Card,
    area: Rect,
    app: &App,
    hits: &mut HitAreas,
    colors: Colors,
) {
    match card {
        Card::Battery => draw_battery(frame, area, app, colors),
        Card::Cpu => draw_cpu(frame, area, app, colors),
        Card::Memory => draw_memory(frame, area, app, colors),
        Card::Disk => draw_disk(frame, area, app, colors),
        Card::Network => draw_wifi(frame, area, app, hits, colors),
        Card::Power => draw_power(frame, area, app, hits, colors),
        Card::Brightness => draw_brightness(frame, area, app, hits, colors),
        Card::Audio => draw_audio(frame, area, app, hits, colors),
    }
}

/// Split `total` cells into `count` equal-size spans, using `gap` empty cells
/// between them. Extra cells widen the gaps so every span keeps the same size.
fn equal_spans(start: u16, total: u16, count: usize, gap: u16) -> Vec<(u16, u16)> {
    if count == 0 {
        return Vec::new();
    }

    let count = count as u16;
    // If the area is too small even for one cell per span plus the requested
    // gaps, drop the gaps rather than letting the final span overflow.
    let gap = if total >= count.saturating_add(gap.saturating_mul(count - 1)) {
        gap
    } else {
        0
    };
    let gap_total = gap.saturating_mul(count - 1);
    let available = total.saturating_sub(gap_total);
    let size = available / count;
    let extra = available % count;

    let mut spans = Vec::with_capacity(count as usize);
    let mut position = start;

    for index in 0..count {
        spans.push((position, size));
        position = position.saturating_add(size);

        if index + 1 < count {
            let extra_gap = u16::from(index < extra);
            position = position.saturating_add(gap + extra_gap);
        }
    }

    spans
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App, hits: &mut HitAreas, colors: Colors) {
    let columns = Layout::horizontal([Constraint::Fill(1), Constraint::Length(22)]).split(area);

    let badge_width = " NIRABOARD ".chars().count();
    let mut title = Vec::with_capacity(4);

    // Keep the clock to the left of the badge, but never let a long format or
    // a narrow terminal push the title off screen.
    if let Some(clock) = app.clock_text() {
        let clock_width = clock.chars().count() + 2;
        if clock_width.saturating_add(badge_width).saturating_add(2) <= columns[0].width as usize {
            title.push(Span::styled(
                format!(" {clock} "),
                Style::default().fg(colors.text),
            ));
        }
    }

    title.push(Span::styled(
        " NIRABOARD ",
        Style::default()
            .fg(Color::Black)
            .bg(colors.main)
            .add_modifier(Modifier::BOLD),
    ));
    title.push(Span::raw(" "));
    title.push(Span::styled(
        app.identity.as_str(),
        Style::default()
            .fg(colors.text)
            .add_modifier(Modifier::BOLD),
    ));
    frame.render_widget(Paragraph::new(Line::from(title)), columns[0]);

    let right = Paragraph::new(Line::from(vec![
        Span::styled(
            app.layout.label(),
            Style::default()
                .fg(colors.main)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · 2s tick", Style::default().fg(colors.muted)),
    ]))
    .alignment(Alignment::Right);
    hits.layout = Some(columns[1]);
    frame.render_widget(right, columns[1]);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, hits: &mut HitAreas, colors: Colors) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);
    let columns = Layout::horizontal([Constraint::Fill(1), Constraint::Length(23)]).split(rows[0]);

    let help = Line::from(vec![
        key("tab", colors),
        Span::raw(" focus  "),
        key("←/→", colors),
        Span::raw(" adjust  "),
        key("m", colors),
        Span::raw(" mute  "),
        key("v", colors),
        Span::raw(" layout  "),
        key("w", colors),
        Span::raw(" nmtui  "),
        key("1-3", colors),
        Span::raw(" profile  "),
        key("r", colors),
        Span::raw(" refresh  "),
        key("q", colors),
        Span::raw(" quit"),
    ]);
    frame.render_widget(
        Paragraph::new(help).style(Style::default().fg(colors.muted)),
        columns[0],
    );

    let (label, style) = if app.notifications.is_open() {
        (
            "[n] close".to_owned(),
            Style::default()
                .fg(colors.main)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            "[n] notifications".to_owned(),
            Style::default().fg(colors.muted),
        )
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(label, style))).alignment(Alignment::Right),
        columns[1],
    );
    hits.notification_button = Some(columns[1]);

    let (text, color) = match app.status_text() {
        Some((text, StatusLevel::Info)) => (text, colors.ok),
        Some((text, StatusLevel::Error)) => (text, colors.bad),
        None => ("ready · click cards or buttons", colors.muted),
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(text, Style::default().fg(color)))),
        rows[1],
    );
}

fn draw_battery(frame: &mut Frame, area: Rect, app: &App, colors: Colors) {
    let block = card("Battery", false, colors);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(battery) = app.monitor.metrics.battery.as_ref() else {
        centered_message(frame, inner, &["Battery unavailable"], colors.muted);
        return;
    };
    if inner.is_empty() {
        return;
    }

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
    let color = battery_color(battery, colors);
    render_gauge(
        frame,
        rows[0],
        battery.percent.into(),
        &format!("{}%", battery.percent),
        color,
    );

    let energy = battery
        .energy_now_wh
        .map(|energy| format!("{energy:.1} Wh"))
        .unwrap_or_else(|| "—".to_owned());
    let draw = battery
        .power_w
        .map(|power| format!("{power:.1} W"))
        .unwrap_or_else(|| "—".to_owned());
    let health = match (battery.health_percent, battery.energy_full_wh) {
        (Some(health), Some(full)) => format!("{health}% · {full:.1} Wh"),
        (Some(health), None) => format!("{health}%"),
        (None, Some(full)) => format!("— · {full:.1} Wh"),
        (None, None) => "—".to_owned(),
    };
    let ac_text = if battery.ac_online {
        "AC connected"
    } else {
        "On battery"
    };
    let ac_color = if battery.ac_online {
        colors.ok
    } else {
        colors.warn
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{:>3}%", battery.percent),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(battery.state.label(), Style::default().fg(colors.text)),
        ]),
        Line::from(vec![
            Span::styled(ac_text, Style::default().fg(ac_color)),
            Span::styled(" · ", Style::default().fg(colors.muted)),
            Span::styled(energy, Style::default().fg(colors.muted)),
        ]),
        Line::from(vec![
            Span::styled("Draw   ", Style::default().fg(colors.muted)),
            Span::styled(draw, Style::default().fg(colors.text)),
        ]),
        Line::from(vec![
            Span::styled("Health ", Style::default().fg(colors.muted)),
            Span::styled(health, Style::default().fg(colors.text)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), rows[1]);
}

fn draw_cpu(frame: &mut Frame, area: Rect, app: &App, colors: Colors) {
    let block = card("CPU", false, colors);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }

    let cpu = &app.monitor.metrics.cpu;
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
    let color = cpu
        .usage_percent
        .map(|usage| usage_color(usage, 70, 90, colors))
        .unwrap_or(colors.muted);
    let label = cpu
        .usage_percent
        .map(|usage| format!("{usage}%"))
        .unwrap_or_else(|| "—".to_owned());
    render_gauge(
        frame,
        rows[0],
        cpu.usage_percent.unwrap_or(0).into(),
        &label,
        color,
    );

    let usage = cpu
        .usage_percent
        .map(|usage| format!("{usage}%"))
        .unwrap_or_else(|| "—".to_owned());
    let temperature = cpu
        .temp_c
        .map(|temperature| format!("{temperature:.1} °C"))
        .unwrap_or_else(|| "—".to_owned());
    let temperature_color = cpu
        .temp_c
        .map(|temperature| temp_color(temperature, colors))
        .unwrap_or(colors.muted);
    let frequency = cpu
        .freq_mhz
        .map(|frequency| format!("{:.2} GHz", frequency as f32 / 1000.0))
        .unwrap_or_else(|| "—".to_owned());
    let threads = if cpu.threads > 0 {
        cpu.threads.to_string()
    } else {
        "—".to_owned()
    };
    let model = cpu.model.as_deref().unwrap_or("—");
    let model = fit(model, inner.width.saturating_sub(8));

    let lines = vec![
        Line::from(vec![
            Span::styled("Usage   ", Style::default().fg(colors.muted)),
            Span::styled(
                usage,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Temp    ", Style::default().fg(colors.muted)),
            Span::styled(temperature, Style::default().fg(temperature_color)),
            Span::raw("   "),
            Span::styled("Freq  ", Style::default().fg(colors.muted)),
            Span::styled(frequency, Style::default().fg(colors.text)),
        ]),
        Line::from(vec![
            Span::styled("Threads ", Style::default().fg(colors.muted)),
            Span::styled(threads, Style::default().fg(colors.text)),
        ]),
        Line::from(vec![
            Span::styled("Model   ", Style::default().fg(colors.muted)),
            Span::styled(model, Style::default().fg(colors.text)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), rows[1]);
}

fn draw_memory(frame: &mut Frame, area: Rect, app: &App, colors: Colors) {
    let block = card("Memory", false, colors);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(memory) = app.monitor.metrics.memory.as_ref() else {
        centered_message(frame, inner, &["Memory unavailable"], colors.muted);
        return;
    };
    if inner.is_empty() {
        return;
    }

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
    let color = usage_color(memory.percent, 70, 90, colors);
    render_gauge(
        frame,
        rows[0],
        memory.percent.into(),
        &format!("{}%", memory.percent),
        color,
    );

    let used = gib(memory.used_bytes);
    let total = gib(memory.total_bytes);
    let available = gib(memory.available_bytes);
    let lines = vec![
        Line::from(vec![
            Span::styled("Used  ", Style::default().fg(colors.muted)),
            Span::styled(
                format!("{used:.1} / {total:.1} GiB"),
                Style::default().fg(colors.text),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{available:.1} GiB"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" available", Style::default().fg(colors.muted)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), rows[1]);
}

fn draw_disk(frame: &mut Frame, area: Rect, app: &App, colors: Colors) {
    let block = card("Disk", false, colors);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(disk) = app.monitor.metrics.disk.as_ref() else {
        centered_message(frame, inner, &["Disk unavailable"], colors.muted);
        return;
    };
    if inner.is_empty() {
        return;
    }

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
    let color = usage_color(disk.percent, 70, 90, colors);
    render_gauge(
        frame,
        rows[0],
        disk.percent.into(),
        &format!("{}%", disk.percent),
        color,
    );

    let used = gib(disk.used_bytes);
    let total = gib(disk.total_bytes);
    let filesystem = format!("{} · {}", disk.filesystem, disk.mount);
    let filesystem = fit(&filesystem, inner.width.saturating_sub(8));
    let lines = vec![
        Line::from(vec![
            Span::styled("Used  ", Style::default().fg(colors.muted)),
            Span::styled(
                format!("{used:.1} / {total:.1} GiB"),
                Style::default().fg(colors.text),
            ),
        ]),
        Line::from(vec![
            Span::styled("FS    ", Style::default().fg(colors.muted)),
            Span::styled(filesystem, Style::default().fg(colors.text)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), rows[1]);
}

fn draw_wifi(frame: &mut Frame, area: Rect, app: &App, hits: &mut HitAreas, colors: Colors) {
    let block = card("Network", false, colors);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    hits.wifi = Some(area);

    if inner.is_empty() {
        return;
    }

    let click_hint = Line::from(Span::styled(
        "click / w: nmtui",
        Style::default().fg(colors.main),
    ));

    let Some(wifi) = app.monitor.metrics.wifi.as_ref() else {
        let mut lines = vec![Line::from(Span::styled(
            "NetworkManager unavailable",
            Style::default().fg(colors.muted),
        ))];
        lines.extend(ip_lines(app, colors));
        lines.push(click_hint);
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    };

    if !wifi.connected {
        let mut lines = vec![
            Line::from(Span::styled(
                "○ Disconnected",
                Style::default()
                    .fg(colors.warn)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "No active Wi-Fi connection",
                Style::default().fg(colors.muted),
            )),
        ];
        lines.extend(ip_lines(app, colors));
        lines.push(click_hint);
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    }

    let ssid = wifi
        .ssid
        .as_deref()
        .filter(|ssid| !ssid.is_empty())
        .unwrap_or("(hidden)");
    let signal = wifi
        .signal
        .map(|signal| format!("{signal}%"))
        .unwrap_or_else(|| "—".to_owned());
    let signal_color = wifi
        .signal
        .map(|signal| signal_color(signal, colors))
        .unwrap_or(colors.muted);

    let mut signal_line = vec![
        Span::styled("Signal ", Style::default().fg(colors.muted)),
        Span::styled(signal, Style::default().fg(signal_color)),
    ];
    if let Some(rate) = wifi.rate_mbps {
        signal_line.push(Span::styled(" · ", Style::default().fg(colors.muted)));
        signal_line.push(Span::styled(
            format!("{rate} Mb/s"),
            Style::default().fg(colors.text),
        ));
    }

    let mut lines = vec![
        Line::from(Span::styled(
            "● Connected",
            Style::default().fg(colors.ok).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            ssid,
            Style::default()
                .fg(colors.text)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(signal_line),
    ];
    lines.extend(ip_lines(app, colors));
    lines.push(click_hint);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Local and global IP addresses, shown even when Wi-Fi itself is unavailable.
fn ip_lines(app: &App, colors: Colors) -> Vec<Line<'static>> {
    let local = app
        .monitor
        .metrics
        .local_ip
        .clone()
        .unwrap_or_else(|| "—".to_owned());
    let public = app
        .monitor
        .metrics
        .public_ip
        .clone()
        .unwrap_or_else(|| "—".to_owned());

    vec![
        Line::from(vec![
            Span::styled("Local  ", Style::default().fg(colors.muted)),
            Span::styled(local, Style::default().fg(colors.text)),
        ]),
        Line::from(vec![
            Span::styled("Global ", Style::default().fg(colors.muted)),
            Span::styled(public, Style::default().fg(colors.text)),
        ]),
    ]
}

fn draw_power(frame: &mut Frame, area: Rect, app: &App, hits: &mut HitAreas, colors: Colors) {
    let focused = app.focus == Focus::Power;
    let block = card("Power profile", focused, colors);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    hits.cards.push((area, Focus::Power));

    if inner.is_empty() {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    for (index, profile) in PowerProfile::ALL.iter().enumerate() {
        let row = rows[index];
        let active = app.monitor.metrics.power_profile == Some(*profile);
        let marker = if active { "●" } else { " " };
        let style = if active {
            Style::default().fg(colors.ok).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors.muted)
        };

        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{marker} {}", profile.label()),
                style,
            ))),
            row,
        );
        hits.profile_rows.push((row, *profile));
    }

    let available = app.monitor.metrics.power_profile.is_some();
    let hint = if available {
        "click or ←/→"
    } else {
        "daemon unavailable"
    };
    let hint_color = if available { colors.muted } else { colors.bad };
    let lines = vec![
        Line::from(Span::styled(hint, Style::default().fg(hint_color))),
        Line::from(Span::styled(
            "power-profiles-daemon",
            Style::default().fg(colors.muted),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), rows[3]);
}

fn draw_brightness(frame: &mut Frame, area: Rect, app: &App, hits: &mut HitAreas, colors: Colors) {
    let focused = app.focus == Focus::Brightness;
    let block = card("Brightness", focused, colors);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    hits.cards.push((area, Focus::Brightness));

    let Some(brightness) = app.monitor.metrics.brightness.as_ref() else {
        centered_message(frame, inner, &["backlight unavailable"], colors.muted);
        return;
    };
    if inner.is_empty() {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    render_gauge(
        frame,
        rows[0],
        brightness.percent.into(),
        &format!("{}%", brightness.percent),
        colors.bar,
    );

    let controls = Layout::horizontal([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .split(rows[1]);
    let button_style = if focused {
        Style::default()
            .fg(colors.main)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors.text)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("[-]", button_style))).alignment(Alignment::Center),
        controls[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{}%", brightness.percent),
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        controls[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("[+]", button_style))).alignment(Alignment::Center),
        controls[2],
    );
    hits.brightness_minus = Some(controls[0]);
    hits.brightness_plus = Some(controls[2]);

    let lines = vec![
        Line::from(Span::styled(
            format!("{}% steps", app.config.steps.brightness),
            Style::default().fg(colors.muted),
        )),
        Line::from(Span::styled(
            brightness.device.as_str(),
            Style::default().fg(colors.muted),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), rows[3]);
}

fn draw_audio(frame: &mut Frame, area: Rect, app: &App, hits: &mut HitAreas, colors: Colors) {
    let focused = app.focus == Focus::Audio;
    let block = card("Audio", focused, colors);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    hits.cards.push((area, Focus::Audio));

    let Some(audio) = app.monitor.metrics.audio.as_ref() else {
        centered_message(frame, inner, &["PipeWire unavailable"], colors.muted);
        return;
    };
    if inner.is_empty() {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(inner);

    let color = if audio.muted {
        colors.bad
    } else if audio.volume_percent > 100 {
        colors.warn
    } else {
        colors.bar
    };
    render_gauge(
        frame,
        rows[0],
        audio.volume_percent,
        &format!("{}%", audio.volume_percent),
        color,
    );

    let controls = Layout::horizontal([
        Constraint::Length(3),
        Constraint::Fill(1),
        Constraint::Length(3),
    ])
    .split(rows[1]);
    let button_style = if focused {
        Style::default()
            .fg(colors.main)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors.text)
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("[-]", button_style))).alignment(Alignment::Center),
        controls[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!("{}%", audio.volume_percent),
            Style::default().add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center),
        controls[1],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled("[+]", button_style))).alignment(Alignment::Center),
        controls[2],
    );
    hits.volume_minus = Some(controls[0]);
    hits.volume_plus = Some(controls[2]);

    let mute_style = if audio.muted {
        Style::default()
            .fg(colors.warn)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors.text)
    };
    let mute_label = if audio.muted { "[UNMUTE]" } else { "[MUTE]" };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(mute_label, mute_style)))
            .alignment(Alignment::Center),
        rows[2],
    );
    hits.mute = Some(rows[2]);

    let state = if audio.muted {
        "MUTED · PipeWire"
    } else {
        "PipeWire · default sink"
    };
    let state_color = if audio.muted {
        colors.bad
    } else {
        colors.muted
    };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            state,
            Style::default().fg(state_color),
        ))),
        rows[3],
    );
}

fn card(title: &str, focused: bool, colors: Colors) -> Block<'static> {
    let border_color = if focused { colors.main } else { colors.muted };
    let title_style = if focused {
        Style::default()
            .fg(colors.main)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(colors.text)
    };

    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .title(Span::styled(format!(" {title} "), title_style))
}

fn key(label: &'static str, colors: Colors) -> Span<'static> {
    Span::styled(
        label,
        Style::default()
            .fg(colors.main)
            .add_modifier(Modifier::BOLD),
    )
}

fn render_gauge(frame: &mut Frame, area: Rect, percent: u16, label: &str, color: Color) {
    if area.is_empty() {
        return;
    }

    let gauge = Gauge::default()
        .ratio(f64::from(percent.min(100)) / 100.0)
        .label(label.to_owned())
        .gauge_style(Style::default().fg(color))
        .use_unicode(true);
    frame.render_widget(gauge, Rect::new(area.x, area.y, area.width, 1));
}

fn centered_message(frame: &mut Frame, area: Rect, lines: &[&str], color: Color) {
    if area.is_empty() {
        return;
    }

    let height = (lines.len() as u16).min(area.height);
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .areas(area);

    let lines: Vec<Line> = lines
        .iter()
        .map(|line| Line::from(Span::styled(*line, Style::default().fg(color))))
        .collect();
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), middle);
}

fn draw_too_small(frame: &mut Frame, area: Rect, width: u16, height: u16, colors: Colors) {
    let required = format!("need at least {width} x {height}");
    centered_message(
        frame,
        area,
        &["Terminal too small", required.as_str(), "press q to quit"],
        colors.warn,
    );
}

fn draw_notifications(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    hits: &mut HitAreas,
    colors: Colors,
) {
    if area.is_empty() {
        return;
    }

    let width = area.width.saturating_sub(4).clamp(40, 78).min(area.width);
    let height = area.height.saturating_sub(4).clamp(9, 22).min(area.height);
    let popup = Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    );

    hits.notification_window = Some(popup);
    frame.render_widget(Clear, popup);

    let center = &app.notifications;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors.main))
        .title(Span::styled(
            format!(
                " mako · {} active · {} history ",
                center.active_count(),
                center.history_count()
            ),
            Style::default()
                .fg(colors.main)
                .add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    if inner.height < 3 {
        return;
    }

    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .split(inner);

    draw_notification_tabs(frame, rows[0], app, hits, colors);

    let list_area = rows[1];
    let visible = list_area.height as usize;
    let entries = center.entries();

    if let Some(error) = center.error() {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                error.to_owned(),
                Style::default().fg(colors.bad),
            )))
            .wrap(Wrap { trim: true }),
            list_area,
        );
    } else if entries.is_empty() {
        let message = match center.tab() {
            Tab::Active => "No active notifications",
            Tab::History => "No notification history",
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                message,
                Style::default().fg(colors.muted),
            )))
            .alignment(Alignment::Center),
            list_area,
        );
    } else {
        let start = selection_offset(entries.len(), center.selected(), visible);
        let text_width = list_area.width.saturating_sub(2);
        let lines: Vec<Line> = entries
            .iter()
            .enumerate()
            .skip(start)
            .take(visible)
            .map(|(index, entry)| {
                let selected = index == center.selected();
                let (default_marker, urgency_color) = match entry.urgency {
                    Urgency::Low => ("·", colors.muted),
                    Urgency::Normal => ("●", colors.main),
                    Urgency::Critical => ("!", colors.bad),
                };
                let marker = if selected { "▸" } else { default_marker };

                let mut text = String::new();
                if let Some(app_name) = entry.app.as_deref() {
                    text.push_str(app_name);
                    text.push_str(": ");
                }
                text.push_str(&entry.summary);
                if let Some(body) = entry.body.as_deref() {
                    text.push_str(" — ");
                    text.push_str(body);
                }

                let text_style = if selected {
                    Style::default()
                        .fg(colors.main)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(colors.text)
                };
                Line::from(vec![
                    Span::styled(
                        format!("{marker} "),
                        Style::default()
                            .fg(if selected { colors.main } else { urgency_color })
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(fit(&text, text_width), text_style),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), list_area);

        for (row, (index, _)) in entries
            .iter()
            .enumerate()
            .skip(start)
            .take(visible)
            .enumerate()
        {
            let rect = Rect::new(
                list_area.x,
                list_area.y.saturating_add(row as u16),
                list_area.width,
                1,
            );
            hits.notification_rows.push((rect, index));
        }
    }

    draw_notification_hint(frame, rows[2], center, colors);
}

/// First entry shown so the selected one stays inside the visible window.
fn selection_offset(len: usize, selected: usize, visible: usize) -> usize {
    if visible == 0 || len <= visible {
        return 0;
    }

    selected
        .saturating_sub(visible / 2)
        .min(len.saturating_sub(visible))
}

fn draw_notification_tabs(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    hits: &mut HitAreas,
    colors: Colors,
) {
    if area.is_empty() {
        return;
    }

    let mut x = area.x;

    for tab in Tab::ALL {
        let label = format!(" {} ", tab.label());
        let width = label.chars().count() as u16;
        if x.saturating_add(width) > area.right() {
            break;
        }

        let rect = Rect::new(x, area.y, width, 1);
        let style = if app.notifications.tab() == tab {
            Style::default()
                .fg(Color::Black)
                .bg(colors.main)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(colors.muted)
        };
        frame.render_widget(Paragraph::new(Line::from(Span::styled(label, style))), rect);
        hits.notification_tabs.push((rect, tab));
        x = x.saturating_add(width).saturating_add(1);
    }
}

fn draw_notification_hint(frame: &mut Frame, area: Rect, center: &Center, colors: Colors) {
    if area.is_empty() {
        return;
    }

    let hint = Line::from(vec![
        key("↵", colors),
        Span::raw(" invoke  "),
        key("d", colors),
        Span::raw(" dismiss  "),
        key("D", colors),
        Span::raw(" all  "),
        key("o", colors),
        Span::raw(" restore  "),
        key("r", colors),
        Span::raw(" refresh  "),
        key("n/Esc", colors),
        Span::raw(" close"),
    ]);

    let total = center.entries().len();
    let position = if total == 0 {
        "0 / 0".to_owned()
    } else {
        format!("{} / {total}", center.selected() + 1)
    };
    let columns = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(position.chars().count() as u16),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(colors.muted)),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            position,
            Style::default().fg(colors.muted),
        )))
        .alignment(Alignment::Right),
        columns[1],
    );
}

fn draw_quit_confirmation(frame: &mut Frame, area: Rect, colors: Colors) {
    let [_, middle, _] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(3),
        Constraint::Fill(1),
    ])
    .areas(area);

    let [_, inner, _] = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(42),
        Constraint::Fill(1),
    ])
    .areas(middle);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(colors.main))
        .style(Style::default().bg(Color::Black));
    let inner_area = block.inner(inner);
    frame.render_widget(block, inner);

    let lines = vec![
        Line::from(vec![Span::styled(
            "Quit? y",
            Style::default()
                .fg(colors.warn)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "y",
                Style::default().fg(colors.ok).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" confirm  ", Style::default().fg(colors.muted)),
            Span::styled(
                "n",
                Style::default().fg(colors.ok).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" cancel", Style::default().fg(colors.muted)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        inner_area,
    );
}

fn usage_color(percent: u8, warning: u8, critical: u8, colors: Colors) -> Color {
    if percent >= critical {
        colors.bad
    } else if percent >= warning {
        colors.warn
    } else {
        colors.bar
    }
}

fn battery_color(battery: &Battery, colors: Colors) -> Color {
    if battery.state == ChargeState::Charging {
        colors.main
    } else if battery.percent <= 10 {
        colors.bad
    } else if battery.percent <= 25 {
        colors.warn
    } else {
        colors.bar
    }
}

fn temp_color(temperature: f32, colors: Colors) -> Color {
    if temperature >= 85.0 {
        colors.bad
    } else if temperature >= 70.0 {
        colors.warn
    } else {
        colors.ok
    }
}

fn signal_color(signal: u8, colors: Colors) -> Color {
    if signal >= 70 {
        colors.ok
    } else if signal >= 40 {
        colors.warn
    } else {
        colors.bad
    }
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / 1024.0
}

/// Clamp a value to `width` columns, marking the cut with an ellipsis.
fn fit(text: &str, width: u16) -> String {
    let width = width as usize;
    let length = text.chars().count();
    if length <= width {
        return text.to_owned();
    }
    if width == 0 {
        return String::new();
    }

    let mut fitted: String = text.chars().take(width - 1).collect();
    fitted.push('…');
    fitted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::notifications::{DesktopNotification, Urgency};
    use crate::system::{Audio, Battery, Brightness, ChargeState, Disk};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn sample(id: u32, app_name: &str, summary: &str, body: Option<&str>) -> DesktopNotification {
        DesktopNotification {
            id,
            app: Some(app_name.to_owned()),
            summary: summary.to_owned(),
            body: body.map(str::to_owned),
            urgency: Urgency::Normal,
            actions: Vec::new(),
        }
    }

    fn open_with(
        app: &mut App,
        active: Vec<DesktopNotification>,
        history: Vec<DesktopNotification>,
    ) {
        // Bypass `makoctl`: tests only exercise the rendering of a populated
        // stack, not the daemon connection.
        app.notifications.force_open();
        app.notifications.set_entries(active, history);
    }

    fn render(width: u16, height: u16) -> (String, HitAreas) {
        render_layout(width, height, LayoutMode::Vertical)
    }

    fn render_layout(width: u16, height: u16, layout: LayoutMode) -> (String, HitAreas) {
        render_cards(width, height, layout, Card::ALL.to_vec())
    }

    fn render_cards(
        width: u16,
        height: u16,
        layout: LayoutMode,
        cards: Vec<Card>,
    ) -> (String, HitAreas) {
        let mut app = test_app();
        app.layout = layout;
        app.config.cards = cards;

        render_app(&app, width, height)
    }

    fn render_app(app: &App, width: u16, height: u16) -> (String, HitAreas) {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut hits = HitAreas::default();
        terminal
            .draw(|frame| {
                hits = draw(frame, app);
            })
            .expect("draw");

        (format!("{}", terminal.backend()), hits)
    }

    fn test_app() -> App {
        let mut app = App::new();
        app.monitor.metrics.battery = Some(Battery {
            percent: 79,
            state: ChargeState::Discharging,
            power_w: Some(4.2),
            health_percent: Some(83),
            energy_now_wh: Some(29.9),
            energy_full_wh: Some(37.9),
            ac_online: false,
        });
        app.monitor.metrics.audio = Some(Audio {
            volume_percent: 40,
            muted: false,
        });
        app.monitor.metrics.cpu.model = Some("AMD Ryzen 7 5800X".to_owned());
        app.monitor.metrics.cpu.threads = 16;
        app.monitor.metrics.disk = Some(Disk {
            mount: "/".to_owned(),
            filesystem: "ext4".to_owned(),
            total_bytes: 240 * 1024 * 1024 * 1024,
            used_bytes: 96 * 1024 * 1024 * 1024,
            available_bytes: 138 * 1024 * 1024 * 1024,
            percent: 40,
        });
        app.monitor.metrics.brightness = Some(Brightness {
            percent: 50,
            raw: 375,
            max: 7500,
            device: "intel_backlight".to_owned(),
        });
        app.monitor.metrics.local_ip = Some("192.168.1.42".to_owned());
        app.monitor.metrics.public_ip = Some("203.0.113.7".to_owned());
        app
    }

    #[test]
    fn renders_all_cards_at_vertical_minimum() {
        let (screen, hits) = render(72, 43);

        for label in [
            "NIRABOARD",
            "Battery",
            "CPU",
            "Memory",
            "Disk",
            "Network",
            "Power profile",
            "Brightness",
            "Audio",
            "tab",
        ] {
            assert!(screen.contains(label), "missing {label} in\n{screen}");
        }

        assert_eq!(hits.profile_rows.len(), 3);
        assert!(hits.brightness_plus.is_some());
        assert!(hits.volume_plus.is_some());
        assert!(hits.mute.is_some());
    }

    #[test]
    fn renders_cards_in_configured_order() {
        let (screen, _) = render_cards(
            120,
            60,
            LayoutMode::Vertical,
            vec![Card::Audio, Card::Cpu, Card::Battery],
        );

        let audio = screen.find("Audio").expect("audio card");
        let cpu = screen.find("CPU").expect("cpu card");
        let battery = screen.find("Battery").expect("battery card");

        assert!(
            audio < cpu && cpu < battery,
            "wrong card order in\n{screen}"
        );
        assert!(!screen.contains("Memory"), "unconfigured card rendered");
        assert!(!screen.contains("Network"), "unconfigured card rendered");
    }

    #[test]
    fn cards_below_the_screen_are_omitted() {
        let area = Rect::new(0, 0, 80, 10);
        let rects = card_rects(area, 1, 20);

        assert_eq!(rects.len(), 10);
        assert!(rects.iter().all(|rect| rect.height == 1));
        assert_eq!(rects.last().unwrap().bottom(), area.bottom());
    }

    #[test]
    fn cards_are_omitted_from_partially_filled_rows() {
        // Four cards in three columns need two rows. With only one row of
        // space, the first three cards stay visible and the last is hidden.
        let area = Rect::new(0, 0, 80, 1);
        let rects = card_rects(area, 3, 4);

        assert_eq!(rects.len(), 3);
        assert_eq!(rects[0].width + rects[1].width + rects[2].width, 78);
    }

    #[test]
    fn renders_ip_addresses_in_the_network_card() {
        let (screen, _) = render_layout(120, 60, LayoutMode::Vertical);

        assert!(screen.contains("Local  192.168.1.42"));
        assert!(screen.contains("Global 203.0.113.7"));
    }

    #[test]
    fn shows_message_when_terminal_is_too_small() {
        let (screen, hits) = render(60, 18);
        assert!(screen.contains("Terminal too small"));
        assert!(screen.contains("need at least 72 x 43"));
        assert!(hits.profile_rows.is_empty());
    }

    #[test]
    fn minimum_size_depends_on_layout() {
        assert_eq!(minimum_size(LayoutMode::Vertical, 8), (72, 43));
        assert_eq!(minimum_size(LayoutMode::Horizontal, 8), (191, 8));
        assert_eq!(minimum_size(LayoutMode::TwoColumns, 8), (72, 23));
        assert_eq!(minimum_size(LayoutMode::ThreeColumns, 8), (72, 18));
    }

    #[test]
    fn minimum_size_follows_card_count() {
        assert_eq!(minimum_size(LayoutMode::Vertical, 3), (72, 18));
        assert_eq!(minimum_size(LayoutMode::Horizontal, 3), (72, 8));
        assert_eq!(minimum_size(LayoutMode::TwoColumns, 3), (72, 13));
        assert_eq!(minimum_size(LayoutMode::ThreeColumns, 3), (72, 8));
    }

    #[test]
    fn horizontal_layout_needs_extra_width() {
        // Tall enough for every card, but far too narrow for eight columns.
        let (screen, hits) = render_layout(120, 60, LayoutMode::Horizontal);

        assert!(screen.contains("Terminal too small"));
        assert!(screen.contains("need at least 191 x 8"));
        assert!(hits.profile_rows.is_empty());
    }

    #[test]
    fn renders_cpu_model_and_threads() {
        let (screen, _) = render_layout(120, 60, LayoutMode::Vertical);

        assert!(screen.contains("Threads 16"));
        assert!(screen.contains("AMD Ryzen 7 5800X"));
    }

    #[test]
    fn renders_disk_usage() {
        let (screen, _) = render_layout(120, 60, LayoutMode::Vertical);

        assert!(screen.contains("Used  96.0 / 240.0 GiB"));
        assert!(screen.contains("FS    ext4 · /"));
    }

    #[test]
    fn applies_configured_colours() {
        let mut app = test_app();
        app.monitor
            .metrics
            .battery
            .as_mut()
            .expect("battery")
            .ac_online = true;
        app.monitor
            .metrics
            .audio
            .as_mut()
            .expect("audio")
            .volume_percent = 120;
        app.config.colors = Colors {
            main: Color::Rgb(1, 2, 3),
            bar: Color::Rgb(4, 5, 6),
            text: Color::Rgb(7, 8, 9),
            muted: Color::Rgb(10, 11, 12),
            ok: Color::Rgb(13, 14, 15),
            warn: Color::Rgb(16, 17, 18),
            bad: Color::Rgb(19, 20, 21),
        };

        let backend = TestBackend::new(120, 60);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                draw(frame, &app);
            })
            .expect("draw");

        let cells = terminal.backend().buffer().content();
        let used: Vec<Color> = cells.iter().flat_map(|cell| [cell.fg, cell.bg]).collect();

        for (name, color) in [
            ("main", app.config.colors.main),
            ("bar", app.config.colors.bar),
            ("text", app.config.colors.text),
            ("muted", app.config.colors.muted),
            ("ok", app.config.colors.ok),
            ("warn", app.config.colors.warn),
            ("bad", app.config.colors.bad),
        ] {
            assert!(used.contains(&color), "missing {name} colour");
        }
    }

    #[test]
    fn maps_click_targets_to_actions() {
        let (_, hits) = render_layout(120, 60, LayoutMode::Vertical);

        let wifi = hits.wifi.expect("wifi card");
        assert_eq!(
            hits.action_at(wifi.x, wifi.y),
            Some(ClickAction::OpenNetwork)
        );

        let (rect, profile) = hits.profile_rows[1];
        assert_eq!(
            hits.action_at(rect.x, rect.y),
            Some(ClickAction::SetProfile(profile))
        );

        let rect = hits.brightness_minus.expect("brightness button");
        assert_eq!(
            hits.action_at(rect.x, rect.y),
            Some(ClickAction::StepBrightness(-5))
        );

        let rect = hits.layout.expect("layout switcher");
        assert_eq!(
            hits.action_at(rect.x, rect.y),
            Some(ClickAction::CycleLayout)
        );
    }

    #[test]
    fn footer_notification_button_is_clickable() {
        let (_, hits) = render_layout(120, 60, LayoutMode::Vertical);
        let rect = hits.notification_button.expect("notification button");

        assert_eq!(
            hits.action_at(rect.x, rect.y),
            Some(ClickAction::ToggleNotifications)
        );
    }

    #[test]
    fn renders_notification_window_with_tabs_and_entries() {
        let mut app = test_app();
        open_with(
            &mut app,
            vec![sample(1, "Telegram", "Alice", Some("hello"))],
            vec![sample(2, "Battery", "Battery low: 24%", None)],
        );

        let (screen, hits) = render_app(&app, 120, 60);

        for label in [
            "mako",
            "1 active · 1 history",
            "Active",
            "History",
            "Telegram: Alice — hello",
            "invoke",
            "dismiss",
            "restore",
            "close",
        ] {
            assert!(screen.contains(label), "missing {label} in\n{screen}");
        }

        assert!(hits.notification_window.is_some());
        assert_eq!(hits.notification_tabs.len(), Tab::ALL.len());
        assert_eq!(hits.notification_rows.len(), 1);
    }

    #[test]
    fn history_tab_renders_history_entries() {
        let mut app = test_app();
        open_with(
            &mut app,
            vec![sample(1, "Telegram", "Alice", Some("hello"))],
            vec![sample(2, "Battery", "Battery low: 24%", None)],
        );
        app.notifications.select_tab(Tab::History);

        let (screen, hits) = render_app(&app, 120, 60);

        assert!(screen.contains("Battery: Battery low: 24%"));
        assert!(!screen.contains("Telegram: Alice"));
        assert_eq!(hits.notification_rows.len(), 1);
    }

    #[test]
    fn notification_tabs_and_rows_are_clickable() {
        let mut app = test_app();
        open_with(
            &mut app,
            vec![sample(1, "Telegram", "Alice", Some("hello"))],
            vec![sample(2, "Battery", "Battery low: 24%", None)],
        );

        let (_, hits) = render_app(&app, 120, 60);

        let (rect, tab) = hits
            .notification_tabs
            .iter()
            .find(|(_, tab)| *tab == Tab::History)
            .copied()
            .expect("history tab");
        assert_eq!(
            hits.action_at(rect.x, rect.y),
            Some(ClickAction::SelectNotificationTab(tab))
        );

        let (rect, index) = hits.notification_rows[0];
        assert_eq!(
            hits.action_at(rect.x, rect.y),
            Some(ClickAction::SelectNotification(index))
        );
    }

    #[test]
    fn empty_notification_window_shows_a_hint() {
        let mut app = test_app();
        open_with(&mut app, Vec::new(), Vec::new());

        let (screen, _) = render_app(&app, 120, 60);
        assert!(screen.contains("No active notifications"));
        assert!(screen.contains("0 / 0"));
    }

    #[test]
    fn notification_error_is_shown_in_the_window() {
        let mut app = test_app();
        app.notifications.force_open();
        // No mako daemon is running in the test environment, so forcing a
        // refresh records the failure instead of panicking.
        app.notifications.refresh(true);

        let (screen, _) = render_app(&app, 120, 60);
        assert!(screen.contains("makoctl"));
    }

    #[test]
    fn header_clock_appears_left_of_the_title() {
        let mut app = test_app();
        app.config.clock.format = "CLOCK".to_owned();
        assert!(app.update_clock());

        let (screen, _) = render_app(&app, 120, 60);
        let header = screen.lines().next().expect("header");
        let clock = header.find("CLOCK").expect("clock in header");
        let title = header.find("NIRABOARD").expect("title in header");

        assert!(clock < title, "clock should be left of the title: {header}");
    }

    #[test]
    fn header_clock_can_be_disabled() {
        let mut app = test_app();
        app.config.clock.enabled = false;
        app.config.clock.format = "CLOCK".to_owned();
        assert!(!app.update_clock());

        let (screen, _) = render_app(&app, 120, 60);

        assert!(!screen.contains("CLOCK"));
    }

    #[test]
    fn header_clock_is_omitted_when_it_would_hide_the_title() {
        let mut app = test_app();
        app.config.clock.format = "CLOCK".repeat(20);
        assert!(app.update_clock());

        let (screen, _) = render_app(&app, 72, 43);
        let header = screen.lines().next().expect("header");

        assert!(!header.contains("CLOCK"));
        assert!(header.contains("NIRABOARD"));
    }

    #[test]
    fn every_layout_renders_all_cards() {
        for layout in LayoutMode::ALL {
            let (width, height) = minimum_size(layout, Card::ALL.len());
            let (screen, _) = render_layout(width, height, layout);
            for label in [
                "NIRABOARD",
                "Battery",
                "CPU",
                "Memory",
                "Disk",
                "Network",
                "Power profile",
                "Brightness",
                "Audio",
                layout.label(),
            ] {
                assert!(
                    screen.contains(label),
                    "missing {label} in {} layout\n{screen}",
                    layout.label()
                );
            }
        }
    }

    #[test]
    fn click_steps_follow_config() {
        let mut hits = HitAreas {
            steps: Steps {
                brightness: 10,
                volume: 20,
            },
            ..HitAreas::default()
        };
        hits.brightness_minus = Some(Rect::new(0, 0, 3, 1));
        hits.volume_plus = Some(Rect::new(4, 0, 3, 1));

        assert_eq!(hits.action_at(0, 0), Some(ClickAction::StepBrightness(-10)));
        assert_eq!(hits.action_at(4, 0), Some(ClickAction::StepVolume(20)));
    }

    #[test]
    fn two_and_three_column_layouts_use_equal_widths() {
        for columns in [2, 3] {
            for width in [72, 80, 81, 120] {
                let spans = equal_spans(0, width, columns, COLUMN_GAP);
                assert_eq!(spans.len(), columns);

                let card_width = spans[0].1;
                assert!(card_width > 0, "columns collapsed at {width}");
                assert!(
                    spans
                        .iter()
                        .all(|(_, span_width)| *span_width == card_width),
                    "unequal widths for {columns} columns at {width}: {spans:?}"
                );

                let (last_x, last_width) = spans[columns - 1];
                assert!(last_x + last_width <= width);
            }
        }
    }
}
