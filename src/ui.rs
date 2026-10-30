use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Gauge, Paragraph};

use crate::app::{App, Focus, LayoutMode, StatusLevel};
use crate::config::{Colors, Steps};
use crate::system::{Battery, ChargeState, PowerProfile};

const OK: Color = Color::Green;
const WARN: Color = Color::Yellow;
const BAD: Color = Color::Red;
const DIM: Color = Color::DarkGray;
const LABEL: Color = Color::Gray;

const MIN_WIDTH: u16 = 72;
const MIN_HEIGHT: u16 = 22;
const CARD_COUNT: usize = 7;
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
}

impl HitAreas {
    pub fn action_at(&self, x: u16, y: u16) -> Option<ClickAction> {
        let position = Position::new(x, y);

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
}

fn contains(rect: Option<Rect>, position: Position) -> bool {
    rect.is_some_and(|rect| rect.contains(position))
}

/// Render one frame and return the rectangles that make up the interactive UI.
pub fn draw(frame: &mut Frame, app: &App) -> HitAreas {
    let area = frame.area();
    let colors = app.config.colors;

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, area);
        return HitAreas::default();
    }

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Fill(1),
        Constraint::Length(2),
    ])
    .areas(area);

    let mut hits = HitAreas {
        steps: app.config.steps,
        ..HitAreas::default()
    };
    draw_header(frame, header, app, &mut hits, colors);

    draw_body(frame, body, app, &mut hits, colors);
    draw_footer(frame, footer, app, colors);

    if app.show_quit_confirmation {
        draw_quit_confirmation(frame, area, colors);
    }

    hits
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App, hits: &mut HitAreas, colors: Colors) {
    match app.layout {
        LayoutMode::Vertical => draw_grid(frame, area, app, hits, 1, colors),
        LayoutMode::Horizontal => draw_grid(frame, area, app, hits, CARD_COUNT, colors),
        LayoutMode::TwoColumns => draw_grid(frame, area, app, hits, 2, colors),
        LayoutMode::ThreeColumns => draw_grid(frame, area, app, hits, 3, colors),
    }
}

/// Arrange every card in a row-major grid. Columns always receive exactly the
/// same width so two- and three-column layouts line up cleanly; any leftover
/// cells are absorbed by the gaps rather than by one of the cards.
fn draw_grid(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    hits: &mut HitAreas,
    columns: usize,
    colors: Colors,
) {
    if area.is_empty() {
        return;
    }

    let columns = columns.clamp(1, CARD_COUNT);
    let rows = CARD_COUNT.div_ceil(columns);
    let column_gap = if columns > 1 { COLUMN_GAP } else { 0 };
    let columns_spans = equal_spans(area.x, area.width, columns, column_gap);
    let row_spans = equal_spans(area.y, area.height, rows, 0);

    for index in 0..CARD_COUNT {
        let (x, width) = columns_spans[index % columns];
        let (y, height) = row_spans[index / columns];
        draw_card(
            frame,
            index,
            Rect::new(x, y, width, height),
            app,
            hits,
            colors,
        );
    }
}

fn draw_card(
    frame: &mut Frame,
    index: usize,
    area: Rect,
    app: &App,
    hits: &mut HitAreas,
    colors: Colors,
) {
    match index {
        0 => draw_battery(frame, area, app, colors),
        1 => draw_cpu(frame, area, app, colors),
        2 => draw_memory(frame, area, app, colors),
        3 => draw_wifi(frame, area, app, hits, colors),
        4 => draw_power(frame, area, app, hits, colors),
        5 => draw_brightness(frame, area, app, hits, colors),
        6 => draw_audio(frame, area, app, hits, colors),
        _ => {}
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

    let title = Line::from(vec![
        Span::styled(
            " NIRABOARD ",
            Style::default()
                .fg(Color::Black)
                .bg(colors.main)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            app.identity.as_str(),
            Style::default().fg(LABEL).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ·  laptop power", Style::default().fg(DIM)),
    ]);
    frame.render_widget(Paragraph::new(title), columns[0]);

    let right = Paragraph::new(Line::from(vec![
        Span::styled(
            app.layout.label(),
            Style::default()
                .fg(colors.main)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" · 2s tick", Style::default().fg(DIM)),
    ]))
    .alignment(Alignment::Right);
    hits.layout = Some(columns[1]);
    frame.render_widget(right, columns[1]);
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App, colors: Colors) {
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

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
        Paragraph::new(help).style(Style::default().fg(DIM)),
        rows[0],
    );

    let (text, color) = match app.status_text() {
        Some((text, StatusLevel::Info)) => (text, OK),
        Some((text, StatusLevel::Error)) => (text, BAD),
        None => ("ready · click cards or buttons", DIM),
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
        centered_message(frame, inner, &["Battery unavailable"], DIM);
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
    let ac_color = if battery.ac_online { OK } else { WARN };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{:>3}%", battery.percent),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(battery.state.label(), Style::default().fg(LABEL)),
        ]),
        Line::from(vec![
            Span::styled(ac_text, Style::default().fg(ac_color)),
            Span::styled(" · ", Style::default().fg(DIM)),
            Span::styled(energy, Style::default().fg(DIM)),
        ]),
        Line::from(vec![
            Span::styled("Draw   ", Style::default().fg(DIM)),
            Span::styled(draw, Style::default().fg(LABEL)),
        ]),
        Line::from(vec![
            Span::styled("Health ", Style::default().fg(DIM)),
            Span::styled(health, Style::default().fg(LABEL)),
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
        .map(|usage| usage_color(usage, 70, 90, colors.bar))
        .unwrap_or(DIM);
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
    let temperature_color = cpu.temp_c.map(temp_color).unwrap_or(DIM);
    let frequency = cpu
        .freq_mhz
        .map(|frequency| format!("{:.2} GHz", frequency as f32 / 1000.0))
        .unwrap_or_else(|| "—".to_owned());
    let threads = if cpu.threads > 0 {
        format!("{} threads", cpu.threads)
    } else {
        "—".to_owned()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Usage ", Style::default().fg(DIM)),
            Span::styled(
                usage,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Temp  ", Style::default().fg(DIM)),
            Span::styled(temperature, Style::default().fg(temperature_color)),
            Span::raw("   "),
            Span::styled("Freq  ", Style::default().fg(DIM)),
            Span::styled(frequency, Style::default().fg(LABEL)),
        ]),
        Line::from(vec![
            Span::styled("Cores ", Style::default().fg(DIM)),
            Span::styled(threads, Style::default().fg(LABEL)),
        ]),
    ];
    frame.render_widget(Paragraph::new(lines), rows[1]);
}

fn draw_memory(frame: &mut Frame, area: Rect, app: &App, colors: Colors) {
    let block = card("Memory", false, colors);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(memory) = app.monitor.metrics.memory.as_ref() else {
        centered_message(frame, inner, &["Memory unavailable"], DIM);
        return;
    };
    if inner.is_empty() {
        return;
    }

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner);
    let color = usage_color(memory.percent, 70, 90, colors.bar);
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
            Span::styled("Used  ", Style::default().fg(DIM)),
            Span::styled(
                format!("{used:.1} / {total:.1} GiB"),
                Style::default().fg(LABEL),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{available:.1} GiB"),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" available", Style::default().fg(DIM)),
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
            Style::default().fg(DIM),
        ))];
        lines.extend(ip_lines(app));
        lines.push(click_hint);
        frame.render_widget(Paragraph::new(lines), inner);
        return;
    };

    if !wifi.connected {
        let mut lines = vec![
            Line::from(Span::styled(
                "○ Disconnected",
                Style::default().fg(WARN).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "No active Wi-Fi connection",
                Style::default().fg(DIM),
            )),
        ];
        lines.extend(ip_lines(app));
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
    let signal_color = wifi.signal.map(signal_color).unwrap_or(DIM);

    let mut signal_line = vec![
        Span::styled("Signal ", Style::default().fg(DIM)),
        Span::styled(signal, Style::default().fg(signal_color)),
    ];
    if let Some(rate) = wifi.rate_mbps {
        signal_line.push(Span::styled(" · ", Style::default().fg(DIM)));
        signal_line.push(Span::styled(
            format!("{rate} Mb/s"),
            Style::default().fg(LABEL),
        ));
    }

    let mut lines = vec![
        Line::from(Span::styled(
            "● Connected",
            Style::default().fg(OK).add_modifier(Modifier::BOLD),
        )),
        Line::from(Span::styled(
            ssid,
            Style::default().fg(LABEL).add_modifier(Modifier::BOLD),
        )),
        Line::from(signal_line),
    ];
    lines.extend(ip_lines(app));
    lines.push(click_hint);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// Local and global IP addresses, shown even when Wi-Fi itself is unavailable.
fn ip_lines(app: &App) -> Vec<Line<'static>> {
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
            Span::styled("Local  ", Style::default().fg(DIM)),
            Span::styled(local, Style::default().fg(LABEL)),
        ]),
        Line::from(vec![
            Span::styled("Global ", Style::default().fg(DIM)),
            Span::styled(public, Style::default().fg(LABEL)),
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
            Style::default().fg(OK).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(DIM)
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
    let hint_color = if available { DIM } else { BAD };
    let lines = vec![
        Line::from(Span::styled(hint, Style::default().fg(hint_color))),
        Line::from(Span::styled(
            "power-profiles-daemon",
            Style::default().fg(DIM),
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
        centered_message(frame, inner, &["backlight unavailable"], DIM);
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
        Style::default().fg(LABEL)
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
            Style::default().fg(DIM),
        )),
        Line::from(Span::styled(
            brightness.device.as_str(),
            Style::default().fg(DIM),
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
        centered_message(frame, inner, &["PipeWire unavailable"], DIM);
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
        BAD
    } else if audio.volume_percent > 100 {
        WARN
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
        Style::default().fg(LABEL)
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
        Style::default().fg(WARN).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(LABEL)
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
    let state_color = if audio.muted { BAD } else { DIM };
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            state,
            Style::default().fg(state_color),
        ))),
        rows[3],
    );
}

fn card(title: &str, focused: bool, colors: Colors) -> Block<'static> {
    let border_color = if focused { colors.main } else { DIM };
    let title_style = if focused {
        Style::default()
            .fg(colors.main)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(LABEL)
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

fn draw_too_small(frame: &mut Frame, area: Rect) {
    centered_message(
        frame,
        area,
        &[
            "Terminal too small",
            "need at least 72 x 22",
            "press q to quit",
        ],
        WARN,
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
            "Quit? (q|y)/n",
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::raw("  "),
            Span::styled("y", Style::default().fg(OK).add_modifier(Modifier::BOLD)),
            Span::styled(" confirm  ", Style::default().fg(DIM)),
            Span::styled("n", Style::default().fg(OK).add_modifier(Modifier::BOLD)),
            Span::styled(" cancel", Style::default().fg(DIM)),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center),
        inner_area,
    );
}

fn usage_color(percent: u8, warning: u8, critical: u8, ok: Color) -> Color {
    if percent >= critical {
        BAD
    } else if percent >= warning {
        WARN
    } else {
        ok
    }
}

fn battery_color(battery: &Battery, colors: Colors) -> Color {
    if battery.state == ChargeState::Charging {
        colors.main
    } else if battery.percent <= 10 {
        BAD
    } else if battery.percent <= 25 {
        WARN
    } else {
        colors.bar
    }
}

fn temp_color(temperature: f32) -> Color {
    if temperature >= 85.0 {
        BAD
    } else if temperature >= 70.0 {
        WARN
    } else {
        OK
    }
}

fn signal_color(signal: u8) -> Color {
    if signal >= 70 {
        OK
    } else if signal >= 40 {
        WARN
    } else {
        BAD
    }
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / 1024.0 / 1024.0 / 1024.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use crate::system::{Audio, Battery, Brightness, ChargeState};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn render(width: u16, height: u16) -> (String, HitAreas) {
        render_layout(width, height, LayoutMode::Vertical)
    }

    fn render_layout(width: u16, height: u16, layout: LayoutMode) -> (String, HitAreas) {
        let mut app = App::new();
        app.layout = layout;
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
        app.monitor.metrics.brightness = Some(Brightness {
            percent: 50,
            raw: 375,
            max: 7500,
            device: "intel_backlight".to_owned(),
        });
        app.monitor.metrics.local_ip = Some("192.168.1.42".to_owned());
        app.monitor.metrics.public_ip = Some("203.0.113.7".to_owned());

        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let mut hits = HitAreas::default();
        terminal
            .draw(|frame| {
                hits = draw(frame, &app);
            })
            .expect("draw");

        (format!("{}", terminal.backend()), hits)
    }

    #[test]
    fn renders_all_cards_at_80x24() {
        let (screen, hits) = render(80, 24);

        for label in [
            "NIRABOARD",
            "Battery",
            "CPU",
            "Memory",
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
    fn renders_ip_addresses_in_the_network_card() {
        let (screen, _) = render_layout(120, 60, LayoutMode::Vertical);

        assert!(screen.contains("Local  192.168.1.42"));
        assert!(screen.contains("Global 203.0.113.7"));
    }

    #[test]
    fn shows_message_when_terminal_is_too_small() {
        let (screen, hits) = render(60, 18);
        assert!(screen.contains("Terminal too small"));
        assert!(hits.profile_rows.is_empty());
    }

    #[test]
    fn maps_click_targets_to_actions() {
        let (_, hits) = render_layout(120, 40, LayoutMode::Vertical);

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
    fn every_layout_renders_all_cards() {
        for layout in LayoutMode::ALL {
            let (screen, _) = render_layout(120, 40, layout);
            for label in [
                "NIRABOARD",
                "Battery",
                "CPU",
                "Memory",
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
