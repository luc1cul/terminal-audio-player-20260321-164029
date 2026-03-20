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
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::app::{App, FocusPane};
use crate::audio_engine::{PlaybackStatus, PlayerState};
use crate::browser::{BrowserEntry, EntryKind};

const UI_TICK_MS: u64 = 100;

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
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(frame.area());

    render_browser(frame, app, chunks[0]);
    render_player(frame, app, chunks[1]);
}

fn render_browser(frame: &mut ratatui::Frame<'_>, app: &App, area: ratatui::layout::Rect) {
    let title = match app.focus() {
        FocusPane::Browser => format!("Browser ● {}", app.browser().root().display()),
        FocusPane::Player => format!("Browser ○ {}", app.browser().root().display()),
    };

    let entries = app.browser().entries();
    let items = if entries.is_empty() {
        vec![ListItem::new(Line::from(
            "No audio files or directories found",
        ))]
    } else {
        entries.iter().map(browser_item).collect::<Vec<_>>()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !entries.is_empty() {
        state.select(Some(app.browser().selected_index()));
    }

    frame.render_stateful_widget(list, area, &mut state);
}

fn render_player(frame: &mut ratatui::Frame<'_>, app: &App, area: ratatui::layout::Rect) {
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Length(7),
            Constraint::Min(4),
        ])
        .split(area);

    let player_title = match app.focus() {
        FocusPane::Player => "Player ●",
        FocusPane::Browser => "Player ○",
    };

    let player_info = Paragraph::new(player_lines(app.player()))
        .block(Block::default().borders(Borders::ALL).title(player_title))
        .wrap(Wrap { trim: false });
    frame.render_widget(player_info, right_chunks[0]);

    let (progress_ratio, progress_label) = progress_info(app.player());
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("Progress"))
        .gauge_style(
            Style::default()
                .fg(Color::Green)
                .bg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .ratio(progress_ratio)
        .label(progress_label);
    frame.render_widget(gauge, right_chunks[1]);

    let help = Paragraph::new(help_lines())
        .block(Block::default().borders(Borders::ALL).title("Keys"))
        .wrap(Wrap { trim: false });
    frame.render_widget(help, right_chunks[2]);

    let status = Paragraph::new(vec![Line::from(app.status_line().to_string())])
        .block(Block::default().borders(Borders::ALL).title("Status"))
        .wrap(Wrap { trim: false });
    frame.render_widget(status, right_chunks[3]);
}

fn browser_item(entry: &BrowserEntry) -> ListItem<'static> {
    let indent = "  ".repeat(entry.depth);
    let icon = match entry.kind {
        EntryKind::Directory if entry.expanded => "▾",
        EntryKind::Directory => "▸",
        EntryKind::File => "♪",
    };

    let style = match entry.kind {
        EntryKind::Directory => Style::default().fg(Color::Yellow),
        EntryKind::File => Style::default().fg(Color::White),
    };

    ListItem::new(Line::from(vec![
        Span::raw(indent),
        Span::styled(format!("{icon} {}", entry.name), style),
    ]))
}

fn player_lines(player: &PlayerState) -> Vec<Line<'static>> {
    let track_name = player
        .current_track
        .as_ref()
        .map(|track| track.title.clone())
        .unwrap_or_else(|| String::from("Nothing loaded"));

    let status_text = match player.status {
        PlaybackStatus::Stopped => "Stopped",
        PlaybackStatus::Playing => "Playing",
        PlaybackStatus::Paused => "Paused",
    };

    let queue_text = match (player.queue_index, player.queue.is_empty()) {
        (Some(index), false) => format!("{} / {}", index + 1, player.queue.len()),
        _ => String::from("0 / 0"),
    };

    vec![
        Line::from(vec![
            Span::styled("Track: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(track_name),
        ]),
        Line::from(vec![
            Span::styled("State: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(status_text),
        ]),
        Line::from(vec![
            Span::styled("Volume: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!("{:.0}%", player.volume * 100.0)),
        ]),
        Line::from(vec![
            Span::styled("Queue: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(queue_text),
        ]),
        Line::from(vec![
            Span::styled("Time: ", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(
                "{} / {}",
                format_duration(player.position),
                player
                    .duration
                    .map(format_duration)
                    .unwrap_or_else(|| String::from("--:--"))
            )),
        ]),
        Line::from(match &player.last_error {
            Some(error) => format!("Last error: {error}"),
            None => String::from("Last error: none"),
        }),
    ]
}

fn help_lines() -> Vec<Line<'static>> {
    vec![
        Line::from("Space play/pause · q quit · Tab switch focus"),
        Line::from("Browser focus: j/k move · Enter open/play · ←/→ collapse/expand"),
        Line::from("Player focus: j/k volume down/up · +/- volume fine-tune"),
        Line::from("h/l seek -/+5s · n/p next/prev · s stop"),
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
