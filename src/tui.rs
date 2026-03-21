use std::io;
use std::time::Duration;

use crossterm::{
    cursor,
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{App, FocusPane};
use crate::audio_engine::{PlaybackStatus, PlayerState};
use crate::browser::{BrowserEntry, EntryKind};

const UI_TICK_MS: u64 = 100;
const XP_BLUE: Color = Color::Rgb(38, 84, 172);
const XP_BLUE_DEEP: Color = Color::Rgb(14, 49, 120);
const XP_BLUE_MID: Color = Color::Rgb(77, 134, 216);
const XP_GLASS: Color = Color::Rgb(104, 170, 246);
const XP_SKY: Color = Color::Rgb(144, 200, 255);
const XP_HIGHLIGHT: Color = Color::Rgb(255, 225, 125);
const XP_SILVER: Color = Color::Rgb(222, 231, 244);
const XP_PANEL: Color = Color::Rgb(198, 218, 245);
const XP_PANEL_DARK: Color = Color::Rgb(131, 165, 214);
const XP_TEXT_DARK: Color = Color::Rgb(8, 32, 86);
const XP_TEXT_LIGHT: Color = Color::Rgb(247, 251, 255);
const XP_MINT: Color = Color::Rgb(119, 247, 208);

pub fn run(app: &mut App) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let run_result = run_loop(&mut terminal, app);

    drop(terminal);
    disable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen, cursor::Show)?;

    run_result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> anyhow::Result<()> {
    loop {
        app.drain_engine_events();
        terminal.draw(|frame| draw(frame, app))?;

        if app.should_quit() {
            break;
        }

        if event::poll(Duration::from_millis(UI_TICK_MS))?
            && let Event::Key(key) = event::read()?
        {
            app.on_key(key)?;
        }
    }

    Ok(())
}

fn draw(frame: &mut ratatui::Frame<'_>, app: &App) {
    let shell = Block::default()
        .style(Style::default().bg(XP_PANEL).fg(XP_TEXT_DARK))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(XP_BLUE_DEEP));
    frame.render_widget(shell, frame.area());

    let content = frame.area().inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(12),
            Constraint::Length(3),
        ])
        .split(content);

    render_title_bar(frame, app, outer[0]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(39), Constraint::Percentage(61)])
        .split(outer[1]);

    render_browser(frame, app, main[0]);
    render_player(frame, app, main[1]);
    render_status_bar(frame, app, outer[2]);
}

fn render_title_bar(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let transport = match app.player().status {
        PlaybackStatus::Playing => " FLOW ",
        PlaybackStatus::Paused => " HOLD ",
        PlaybackStatus::Stopped => " READY ",
    };
    let focus = match app.focus() {
        FocusPane::Browser => " LIBRARY ",
        FocusPane::Player => " PLAYER ",
    };

    let text = Line::from(vec![
        Span::styled(
            " Terminal Audio Player ",
            Style::default()
                .fg(XP_TEXT_LIGHT)
                .bg(XP_BLUE_DEEP)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "  Windows XP mood • glossy shell • layered wave deck  ",
            Style::default().fg(XP_TEXT_LIGHT).bg(XP_BLUE),
        ),
        Span::styled(
            focus,
            Style::default()
                .fg(XP_TEXT_DARK)
                .bg(XP_SILVER)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            transport,
            Style::default()
                .fg(XP_TEXT_DARK)
                .bg(XP_HIGHLIGHT)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let widget = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(XP_BLUE_DEEP))
            .style(Style::default().bg(XP_BLUE)),
    );
    frame.render_widget(widget, area);
}

fn render_browser(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let entries = app.browser().entries();
    let (directory_count, file_count) = browser_counts(entries);
    let root_name = app
        .browser()
        .root()
        .file_name()
        .map(|value| clean_label(&value.to_string_lossy()))
        .unwrap_or_else(|| app.browser().root().display().to_string());
    let title = match app.focus() {
        FocusPane::Browser => {
            format!(" Media Library ◆ {root_name} · {directory_count} dirs · {file_count} files ")
        }
        FocusPane::Player => {
            format!(" Media Library ◇ {root_name} · {directory_count} dirs · {file_count} files ")
        }
    };

    let columns = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(6)])
        .split(area);

    let items = if entries.is_empty() {
        vec![ListItem::new(Line::from(vec![Span::styled(
            " No audio files or folders found ",
            Style::default().fg(XP_TEXT_DARK),
        )]))]
    } else {
        entries.iter().map(browser_item).collect::<Vec<_>>()
    };

    let list = List::new(items)
        .block(xp_panel(&title, app.focus() == FocusPane::Browser))
        .highlight_symbol("▸ ")
        .highlight_style(
            Style::default()
                .fg(XP_TEXT_LIGHT)
                .bg(XP_BLUE_MID)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !entries.is_empty() {
        state.select(Some(app.browser().selected_index()));
    }

    frame.render_stateful_widget(list, columns[0], &mut state);
    render_browser_summary(frame, app, columns[1], directory_count, file_count);
}

fn render_player(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10),
            Constraint::Length(9),
            Constraint::Length(3),
            Constraint::Length(5),
            Constraint::Min(6),
        ])
        .split(area);

    render_now_playing(
        frame,
        app.player(),
        app.focus() == FocusPane::Player,
        right[0],
    );
    render_visualizer(frame, app.player(), right[1]);
    render_progress(frame, app.player(), right[2]);
    render_keys(frame, right[3]);
    render_queue(frame, app.player(), right[4]);
}

fn render_now_playing(
    frame: &mut ratatui::Frame<'_>,
    player: &PlayerState,
    focused: bool,
    area: Rect,
) {
    let title = if focused {
        " Now Playing ◆ "
    } else {
        " Now Playing ◇ "
    };
    let details = track_details(player);
    let title_width = area.width.saturating_sub(14) as usize;
    let phase = animation_phase(player, 180);
    let title_display = marquee_text(&details.title, title_width.max(20), phase);
    let queue_text = match (player.queue_index, player.queue.is_empty()) {
        (Some(index), false) => format!("{} of {}", index + 1, player.queue.len()),
        _ => String::from("0 of 0"),
    };
    let remaining = player
        .duration
        .map(|duration| {
            format!(
                "-{}",
                format_duration(duration.saturating_sub(player.position))
            )
        })
        .unwrap_or_else(|| String::from("--:--"));

    let mut top_row = vec![status_chip(player), Span::raw("  ")];
    top_row.extend(transport_spans(player));

    let lines = vec![
        Line::from(top_row),
        Line::from(vec![
            label_span(" Track  "),
            Span::styled(
                title_display,
                Style::default()
                    .fg(XP_TEXT_DARK)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            label_span(" Artist "),
            Span::raw(details.artist),
            Span::raw("   "),
            label_span(" Album "),
            Span::raw(details.album),
        ]),
        Line::from(vec![
            label_span(" Format "),
            Span::raw(details.extension),
            Span::raw("   "),
            label_span(" Queue "),
            Span::raw(queue_text),
        ]),
        Line::from(vec![
            label_span(" Level  "),
            Span::styled(
                volume_meter(player.volume, 10),
                Style::default().fg(XP_BLUE_DEEP),
            ),
            Span::raw(format!(" {:>3.0}%", player.volume * 100.0)),
        ]),
        Line::from(vec![
            label_span(" Time   "),
            Span::raw(format!(
                "{} / {}",
                format_duration(player.position),
                player
                    .duration
                    .map(format_duration)
                    .unwrap_or_else(|| String::from("--:--"))
            )),
            Span::raw("   "),
            label_span(" Remain "),
            Span::raw(remaining),
        ]),
        Line::from(vec![
            label_span(" Mood   "),
            Span::styled(playback_mood(player), Style::default().fg(XP_BLUE)),
        ]),
    ];

    let widget = Paragraph::new(lines)
        .block(xp_panel(title, focused))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_visualizer(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let width = area.width.saturating_sub(4) as usize;
    let lines = vec![
        Line::from(vec![
            Span::styled(
                "  signal deck",
                Style::default()
                    .fg(XP_TEXT_LIGHT)
                    .add_modifier(Modifier::ITALIC),
            ),
            Span::raw("   "),
            Span::styled(
                transport_state_label(player),
                Style::default().fg(XP_HIGHLIGHT),
            ),
        ]),
        make_spectrum_line(player, width),
        make_wave_line(player, width, 5.4, 0.0, [XP_GLASS, XP_MINT, XP_HIGHLIGHT]),
        make_wave_line(player, width, 3.1, 1.8, [XP_PANEL_DARK, XP_SKY, XP_SILVER]),
        make_glow_line(player, width),
        make_reflection_line(player, width),
        Line::from(vec![Span::styled(
            visualizer_caption(player),
            Style::default().fg(XP_SILVER),
        )]),
    ];

    let widget = Paragraph::new(lines).block(
        Block::default()
            .title(" Wave View ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(XP_BLUE_DEEP))
            .style(Style::default().bg(XP_BLUE_DEEP)),
    );

    frame.render_widget(widget, area);
}

fn render_progress(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let (progress_ratio, progress_label) = progress_info(player);
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Transport Ribbon ")
                .border_style(Style::default().fg(XP_BLUE_DEEP))
                .style(Style::default().bg(XP_SILVER)),
        )
        .gauge_style(
            Style::default()
                .fg(XP_HIGHLIGHT)
                .bg(XP_PANEL_DARK)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(progress_ratio)
        .label(progress_label);
    frame.render_widget(gauge, area);
}

fn render_keys(frame: &mut ratatui::Frame<'_>, area: Rect) {
    let help = Paragraph::new(help_lines())
        .block(xp_panel(" Controls ", false))
        .wrap(Wrap { trim: false });
    frame.render_widget(help, area);
}

fn render_queue(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let lines = if player.queue.is_empty() {
        vec![
            Line::from(vec![Span::styled(
                " Queue is empty — pick a track from the library.",
                Style::default().fg(XP_TEXT_DARK),
            )]),
            Line::from(vec![Span::styled(
                " The carousel will show the current song and the next few jumps.",
                Style::default().fg(XP_BLUE),
            )]),
        ]
    } else {
        let current_index = player.queue_index.unwrap_or(0);
        let start = current_index.saturating_sub(1);
        let end = (start + 6).min(player.queue.len());

        player.queue[start..end]
            .iter()
            .enumerate()
            .map(|(offset, path)| {
                let absolute_index = start + offset;
                let name = path
                    .file_stem()
                    .map(|name| clean_label(&name.to_string_lossy()))
                    .unwrap_or_else(|| clean_label(&path.display().to_string()));
                let prefix = match Some(absolute_index) == player.queue_index {
                    true => " NOW ",
                    false if absolute_index > current_index => " NEXT",
                    false => " PREV",
                };
                let style = match Some(absolute_index) == player.queue_index {
                    true => Style::default()
                        .fg(XP_TEXT_LIGHT)
                        .bg(XP_BLUE_MID)
                        .add_modifier(Modifier::BOLD),
                    false if absolute_index > current_index => Style::default().fg(XP_BLUE_DEEP),
                    false => Style::default().fg(XP_PANEL_DARK),
                };

                Line::from(vec![
                    Span::styled(prefix, style.add_modifier(Modifier::BOLD)),
                    Span::raw(" "),
                    Span::styled(format!("{:>2}. {}", absolute_index + 1, name), style),
                ])
            })
            .collect()
    };

    let widget = Paragraph::new(lines)
        .block(xp_panel(" Queue Carousel ", false))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_status_bar(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let status = Paragraph::new(Line::from(vec![
        Span::styled(
            " Status ",
            Style::default()
                .fg(XP_TEXT_LIGHT)
                .bg(XP_BLUE_DEEP)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            app.status_line().to_string(),
            Style::default().fg(XP_TEXT_DARK),
        ),
        Span::raw("   "),
        Span::styled(
            match app.focus() {
                FocusPane::Browser => " Focus: library ",
                FocusPane::Player => " Focus: player ",
            },
            Style::default().fg(XP_BLUE_DEEP),
        ),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(XP_BLUE_DEEP))
            .style(Style::default().bg(XP_SILVER)),
    );
    frame.render_widget(status, area);
}

fn xp_panel(title: &str, focused: bool) -> Block<'static> {
    let border = if focused { XP_HIGHLIGHT } else { XP_BLUE_DEEP };
    let title_style = if focused {
        Style::default()
            .fg(XP_TEXT_DARK)
            .bg(XP_HIGHLIGHT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
            .fg(XP_TEXT_LIGHT)
            .bg(XP_BLUE)
            .add_modifier(Modifier::BOLD)
    };

    Block::default()
        .title(Line::from(vec![Span::styled(
            title.to_string(),
            title_style,
        )]))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(XP_SILVER))
}

fn browser_item(entry: &BrowserEntry) -> ListItem<'static> {
    let indent = "  ".repeat(entry.depth);
    let icon = match entry.kind {
        EntryKind::Directory if entry.expanded => "▾",
        EntryKind::Directory => "▸",
        EntryKind::File => "♪",
    };

    let style = match entry.kind {
        EntryKind::Directory => Style::default()
            .fg(XP_BLUE_DEEP)
            .add_modifier(Modifier::BOLD),
        EntryKind::File => Style::default().fg(XP_TEXT_DARK),
    };

    ListItem::new(Line::from(vec![
        Span::raw(indent),
        Span::styled(format!("{icon} {}", entry.name), style),
    ]))
}

fn render_browser_summary(
    frame: &mut ratatui::Frame<'_>,
    app: &App,
    area: Rect,
    directory_count: usize,
    file_count: usize,
) {
    let selected = app.browser().selected_entry();
    let selected_name = selected
        .map(|entry| clean_label(&entry.name))
        .unwrap_or_else(|| String::from("Nothing selected"));
    let selected_kind = selected
        .map(|entry| match entry.kind {
            EntryKind::Directory => "folder",
            EntryKind::File => "track",
        })
        .unwrap_or("idle");
    let selected_path = selected
        .map(|entry| {
            tail_clip(
                &entry.path.display().to_string(),
                area.width.saturating_sub(12) as usize,
            )
        })
        .unwrap_or_else(|| {
            tail_clip(
                &app.browser().root().display().to_string(),
                area.width.saturating_sub(12) as usize,
            )
        });
    let depth = selected.map(|entry| entry.depth).unwrap_or(0);

    let lines = vec![
        Line::from(vec![
            label_span(" Selected "),
            Span::styled(
                selected_name,
                Style::default()
                    .fg(XP_TEXT_DARK)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            label_span(" Kind     "),
            Span::raw(selected_kind),
            Span::raw("   "),
            label_span(" Depth "),
            Span::raw(depth.to_string()),
        ]),
        Line::from(vec![
            label_span(" Library  "),
            Span::raw(format!("{directory_count} dirs · {file_count} files")),
        ]),
        Line::from(vec![label_span(" Path     "), Span::raw(selected_path)]),
    ];

    let widget = Paragraph::new(lines)
        .block(xp_panel(" Selection ", false))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn browser_counts(entries: &[BrowserEntry]) -> (usize, usize) {
    entries
        .iter()
        .fold((0, 0), |(directories, files), entry| match entry.kind {
            EntryKind::Directory => (directories + 1, files),
            EntryKind::File => (directories, files + 1),
        })
}

fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(" Space play-pause · q quit · Tab switch focus ".fg(XP_TEXT_DARK)),
        Line::from(" Browser: j/k move · Enter open-play · ←/→ collapse-expand ".fg(XP_TEXT_DARK)),
        Line::from(" Player: j/k volume · +/- fine tune · h/l seek · n/p jump ".fg(XP_TEXT_DARK)),
    ]
}

fn progress_info(player: &PlayerState) -> (f64, String) {
    match player.duration {
        Some(duration) if !duration.is_zero() => (
            (player.position.as_secs_f64() / duration.as_secs_f64()).clamp(0.0, 1.0),
            format!(
                "{} / {}",
                format_duration(player.position),
                format_duration(duration)
            ),
        ),
        _ => (0.0, String::from("--:-- / --:--")),
    }
}

fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02}")
}

fn playback_mood(player: &PlayerState) -> &'static str {
    match player.status {
        PlaybackStatus::Playing => "blue-glass momentum",
        PlaybackStatus::Paused => "frozen shimmer",
        PlaybackStatus::Stopped => "waiting for a signal",
    }
}

fn transport_state_label(player: &PlayerState) -> &'static str {
    match player.status {
        PlaybackStatus::Playing => "dual-wave scan is live",
        PlaybackStatus::Paused => "scanline frozen in place",
        PlaybackStatus::Stopped => "deck idling on a soft glow",
    }
}

fn visualizer_caption(player: &PlayerState) -> &'static str {
    match player.status {
        PlaybackStatus::Playing => "Layered bands ride playback time instead of looping blindly.",
        PlaybackStatus::Paused => "Phase is preserved while the transport holds.",
        PlaybackStatus::Stopped => "Idle chrome stays lit until a track wakes the panel.",
    }
}

fn status_chip(player: &PlayerState) -> Span<'static> {
    match player.status {
        PlaybackStatus::Playing => Span::styled(
            " PLAYING ",
            Style::default()
                .fg(XP_TEXT_DARK)
                .bg(XP_MINT)
                .add_modifier(Modifier::BOLD),
        ),
        PlaybackStatus::Paused => Span::styled(
            " PAUSED ",
            Style::default()
                .fg(XP_TEXT_DARK)
                .bg(XP_HIGHLIGHT)
                .add_modifier(Modifier::BOLD),
        ),
        PlaybackStatus::Stopped => Span::styled(
            " STOPPED ",
            Style::default()
                .fg(XP_TEXT_LIGHT)
                .bg(XP_BLUE_DEEP)
                .add_modifier(Modifier::BOLD),
        ),
    }
}

fn transport_spans(player: &PlayerState) -> Vec<Span<'static>> {
    let passive = Style::default().fg(XP_TEXT_LIGHT).bg(XP_PANEL_DARK);
    let accent = match player.status {
        PlaybackStatus::Playing => Style::default().fg(XP_TEXT_DARK).bg(XP_MINT),
        PlaybackStatus::Paused => Style::default().fg(XP_TEXT_DARK).bg(XP_HIGHLIGHT),
        PlaybackStatus::Stopped => Style::default().fg(XP_TEXT_LIGHT).bg(XP_BLUE_MID),
    }
    .add_modifier(Modifier::BOLD);

    vec![
        Span::styled(" ◀◀ ", passive),
        Span::raw(" "),
        Span::styled(
            match player.status {
                PlaybackStatus::Playing => " ▌▌ ",
                PlaybackStatus::Paused | PlaybackStatus::Stopped => " ▶ ",
            },
            accent,
        ),
        Span::raw(" "),
        Span::styled(" ▶▶ ", passive),
        Span::raw(" "),
        Span::styled(" ■ ", passive),
    ]
}

fn label_span(label: &str) -> Span<'static> {
    Span::styled(
        label.to_string(),
        Style::default()
            .fg(XP_BLUE_DEEP)
            .add_modifier(Modifier::BOLD),
    )
}

fn track_details(player: &PlayerState) -> TrackDetails {
    let Some(track) = player.current_track.as_ref() else {
        return TrackDetails {
            title: String::from("Drop into the library and press Enter"),
            artist: String::from("Library idle"),
            album: String::from("No folder selected"),
            extension: String::from("--"),
        };
    };

    let title = track
        .path
        .file_stem()
        .map(|value| clean_label(&value.to_string_lossy()))
        .unwrap_or_else(|| clean_label(&track.title));
    let album = track
        .path
        .parent()
        .and_then(|path| path.file_name())
        .map(|value| clean_label(&value.to_string_lossy()))
        .unwrap_or_else(|| String::from("Unknown folder"));
    let artist = track
        .path
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.file_name())
        .map(|value| clean_label(&value.to_string_lossy()))
        .filter(|value| value != &album)
        .unwrap_or_else(|| String::from("Library"));
    let extension = track
        .path
        .extension()
        .map(|value| value.to_string_lossy().to_uppercase())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| String::from("AUDIO"));

    TrackDetails {
        title,
        artist,
        album,
        extension,
    }
}

fn clean_label(raw: &str) -> String {
    let text = raw.replace('_', " ");
    let text = text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();

    if text.is_empty() {
        String::from("Untitled")
    } else {
        text
    }
}

fn tail_clip(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= width {
        return text.to_string();
    }

    if width <= 3 {
        return "…".repeat(width);
    }

    let tail = chars
        .into_iter()
        .rev()
        .take(width - 1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    format!("…{tail}")
}

fn marquee_text(text: &str, width: usize, phase: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= width {
        return text.to_string();
    }

    let gap = 5;
    let mut extended = chars.clone();
    extended.extend(std::iter::repeat_n(' ', gap));
    extended.extend(chars.iter().copied());
    let cycle = chars.len() + gap;
    let start = phase % cycle;

    extended.into_iter().skip(start).take(width).collect()
}

fn volume_meter(volume: f32, width: usize) -> String {
    let width = width.max(1);
    let filled = (volume.clamp(0.0, 1.0) * width as f32).round() as usize;
    let filled = filled.min(width);
    let mut meter = String::with_capacity(width);

    for index in 0..width {
        meter.push(if index < filled { '█' } else { '░' });
    }

    meter
}

fn animation_phase(player: &PlayerState, step_ms: u128) -> usize {
    if matches!(player.status, PlaybackStatus::Stopped) {
        0
    } else {
        (player.position.as_millis() / step_ms) as usize
    }
}

fn make_spectrum_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(16);
    let phase = player.position.as_secs_f64() * 4.6;
    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut spans = Vec::with_capacity(width);

    for index in 0..width {
        let x = index as f64 / width as f64;
        let energy =
            ((x * 18.0 + phase).sin() * 0.55 + (x * 33.0 + phase * 0.6).cos() * 0.35 + 1.0) / 2.0;
        let idx = (energy.clamp(0.0, 1.0) * (chars.len() - 1) as f64).round() as usize;
        let color = if energy > 0.72 {
            XP_HIGHLIGHT
        } else if energy > 0.48 {
            XP_MINT
        } else {
            XP_SKY
        };
        spans.push(Span::styled(
            chars[idx].to_string(),
            Style::default().fg(color),
        ));
    }

    Line::from(spans)
}

fn make_wave_line(
    player: &PlayerState,
    width: usize,
    speed: f64,
    phase_offset: f64,
    palette: [Color; 3],
) -> Line<'static> {
    let width = width.max(16);
    let phase = player.position.as_secs_f64() * speed + phase_offset;
    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut spans = Vec::with_capacity(width);

    for index in 0..width {
        let x = index as f64 / width as f64;
        let base = ((x * 11.0 + phase).sin() + (x * 26.0 + phase * 0.7).sin() * 0.45) * 0.5;
        let normalized = ((base + 1.0) / 2.0).clamp(0.0, 1.0);
        let idx = (normalized * (chars.len() - 1) as f64).round() as usize;
        let color = if normalized > 0.72 {
            palette[2]
        } else if normalized > 0.45 {
            palette[1]
        } else {
            palette[0]
        };
        spans.push(Span::styled(
            chars[idx].to_string(),
            Style::default().fg(color),
        ));
    }

    Line::from(spans)
}

fn make_glow_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(16);
    let sweep = if matches!(player.status, PlaybackStatus::Stopped) {
        width / 3
    } else {
        ((player.position.as_millis() / 80) as usize) % width
    };

    let mut spans = Vec::with_capacity(width);
    for index in 0..width {
        let distance = index.abs_diff(sweep);
        let ch = if distance == 0 {
            '✦'
        } else if distance <= 2 {
            '•'
        } else if distance <= 5 {
            '·'
        } else {
            ' '
        };
        let color = if distance == 0 {
            XP_HIGHLIGHT
        } else if distance <= 2 {
            XP_SILVER
        } else {
            XP_PANEL_DARK
        };
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
    }

    Line::from(spans)
}

fn make_reflection_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(16);
    let phase = player.position.as_secs_f64() * 3.2;
    let mut spans = Vec::with_capacity(width);

    for index in 0..width {
        let x = index as f64 / width as f64;
        let normalized = (((x * 9.0 + phase).cos() * 0.6) + 1.0) / 2.0;
        let ch = if normalized > 0.76 {
            '▀'
        } else if normalized > 0.52 {
            '▔'
        } else if normalized > 0.34 {
            '·'
        } else {
            ' '
        };
        let color = if matches!(player.status, PlaybackStatus::Stopped) {
            XP_PANEL_DARK
        } else if normalized > 0.68 {
            XP_SKY
        } else {
            XP_PANEL_DARK
        };
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
    }

    Line::from(spans)
}

struct TrackDetails {
    title: String,
    artist: String,
    album: String,
    extension: String,
}
