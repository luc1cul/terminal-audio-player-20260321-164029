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
    let title = match app.player().status {
        PlaybackStatus::Playing => "Now Playing",
        PlaybackStatus::Paused => "Paused",
        PlaybackStatus::Stopped => "Library Ready",
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
            "  Windows XP mood • glossy shell • wave view  ",
            Style::default().fg(XP_TEXT_LIGHT).bg(XP_BLUE),
        ),
        Span::styled(
            format!(" {} ", title),
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
    let title = match app.focus() {
        FocusPane::Browser => format!(" Media Library ◆ {}", app.browser().root().display()),
        FocusPane::Player => format!(" Media Library ◇ {}", app.browser().root().display()),
    };

    let entries = app.browser().entries();
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

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_player(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Min(5),
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
    let track_name = player
        .current_track
        .as_ref()
        .map(|track| track.title.clone())
        .unwrap_or_else(|| String::from("Drop into the library and press Enter"));

    let status_chip = match player.status {
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
    };

    let queue_text = match (player.queue_index, player.queue.is_empty()) {
        (Some(index), false) => format!("{} of {}", index + 1, player.queue.len()),
        _ => String::from("0 of 0"),
    };

    let lines = vec![
        Line::from(vec![status_chip]),
        Line::from(vec![
            Span::styled(
                " Track  ",
                Style::default()
                    .fg(XP_BLUE_DEEP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                track_name,
                Style::default()
                    .fg(XP_TEXT_DARK)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                " Volume ",
                Style::default()
                    .fg(XP_BLUE_DEEP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!("{:>3.0}%", player.volume * 100.0)),
            Span::raw("   "),
            Span::styled(
                " Queue ",
                Style::default()
                    .fg(XP_BLUE_DEEP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(queue_text),
        ]),
        Line::from(vec![
            Span::styled(
                " Time   ",
                Style::default()
                    .fg(XP_BLUE_DEEP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(format!(
                "{} / {}",
                format_duration(player.position),
                player
                    .duration
                    .map(format_duration)
                    .unwrap_or_else(|| String::from("--:--"))
            )),
        ]),
        Line::from(vec![
            Span::styled(
                " Mood   ",
                Style::default()
                    .fg(XP_BLUE_DEEP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(playback_mood(player), Style::default().fg(XP_BLUE)),
        ]),
    ];

    let widget = Paragraph::new(lines)
        .block(xp_panel(title, focused))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_visualizer(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let wave = make_wave_line(player, area.width.saturating_sub(4) as usize);
    let glow = make_glow_line(player, area.width.saturating_sub(4) as usize);

    let lines = vec![
        Line::from(vec![Span::styled(
            "  analogue wave sweep",
            Style::default()
                .fg(XP_TEXT_LIGHT)
                .add_modifier(Modifier::ITALIC),
        )]),
        wave,
        glow,
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
                .title(" Seek Ribbon ")
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
        vec![Line::from(vec![Span::styled(
            " Queue is empty — pick a track from the library.",
            Style::default().fg(XP_TEXT_DARK),
        )])]
    } else {
        let current_index = player.queue_index.unwrap_or(0);
        let start = current_index.saturating_sub(2);
        let end = (start + 5).min(player.queue.len());

        player.queue[start..end]
            .iter()
            .enumerate()
            .map(|(offset, path)| {
                let absolute_index = start + offset;
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string());
                let style = if Some(absolute_index) == player.queue_index {
                    Style::default()
                        .fg(XP_TEXT_LIGHT)
                        .bg(XP_BLUE_MID)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(XP_TEXT_DARK)
                };

                let prefix = if Some(absolute_index) == player.queue_index {
                    "▶"
                } else {
                    "•"
                };

                Line::from(vec![Span::styled(
                    format!(" {prefix} {:>2}. {name}", absolute_index + 1),
                    style,
                )])
            })
            .collect()
    };

    let widget = Paragraph::new(lines)
        .block(xp_panel(" Queue Preview ", false))
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

fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from(" Space play/pause · q quit · Tab switch focus ".fg(XP_TEXT_DARK)),
        Line::from(" Browser: j/k move · Enter open/play · ←/→ collapse/expand ".fg(XP_TEXT_DARK)),
        Line::from(" Player: j/k volume · +/- fine tune · h/l seek ".fg(XP_TEXT_DARK)),
        Line::from(" Queue travel: n/p next-prev · s stop ".fg(XP_TEXT_DARK)),
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

fn visualizer_caption(player: &PlayerState) -> String {
    match player.status {
        PlaybackStatus::Playing => String::from("Sweeping wave is live and tied to playback time."),
        PlaybackStatus::Paused => String::from("Wave is holding position until playback resumes."),
        PlaybackStatus::Stopped => String::from("Idle glow — select a track to wake the panel."),
    }
}

fn make_wave_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(12);
    let phase = if player.status == PlaybackStatus::Stopped {
        0.0
    } else {
        player.position.as_secs_f64() * 5.2
    };

    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut spans = Vec::with_capacity(width);

    for i in 0..width {
        let x = i as f64 / width as f64;
        let base = ((x * 10.0 + phase).sin() + (x * 24.0 + phase * 0.7).sin() * 0.45) * 0.5;
        let normalized = ((base + 1.0) / 2.0).clamp(0.0, 1.0);
        let idx = (normalized * (chars.len() - 1) as f64).round() as usize;
        let color = if normalized > 0.72 {
            XP_HIGHLIGHT
        } else if normalized > 0.45 {
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

fn make_glow_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(12);
    let sweep = if player.status == PlaybackStatus::Stopped {
        width / 3
    } else {
        ((player.position.as_millis() / 90) as usize) % width
    };

    let mut spans = Vec::with_capacity(width);
    for i in 0..width {
        let distance = i.abs_diff(sweep);
        let ch = if distance == 0 {
            '✦'
        } else if distance <= 2 {
            '•'
        } else {
            '·'
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
