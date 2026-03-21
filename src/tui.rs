use std::io;
use std::path::Path;
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
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
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
const XP_GLASS: Color = Color::Rgb(110, 176, 245);
const XP_HIGHLIGHT: Color = Color::Rgb(255, 225, 125);
const XP_SILVER: Color = Color::Rgb(222, 231, 244);
const XP_PANEL: Color = Color::Rgb(198, 218, 245);
const XP_PANEL_DARK: Color = Color::Rgb(131, 165, 214);
const XP_TEXT_DARK: Color = Color::Rgb(8, 32, 86);
const XP_TEXT_LIGHT: Color = Color::Rgb(247, 251, 255);
const XP_MINT: Color = Color::Rgb(119, 247, 208);
const XP_RED: Color = Color::Rgb(188, 61, 61);

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
            "  XP blue glass • media deck • wave tank  ",
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
    let root_label = fit_text(
        &app.browser().root().display().to_string(),
        area.width.saturating_sub(20) as usize,
    );
    let title = match app.focus() {
        FocusPane::Browser => format!(" Media Library ◆ {root_label}"),
        FocusPane::Player => format!(" Media Library ◇ {root_label}"),
    };

    let shell = xp_panel(&title, app.focus() == FocusPane::Browser);
    frame.render_widget(shell, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.width < 18 || inner.height < 10 {
        return;
    }

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(6), Constraint::Length(8)])
        .split(inner);

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
        .block(
            Block::default()
                .title(" Navigator ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(XP_SKY))
                .style(Style::default().bg(XP_SILVER)),
        )
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

    frame.render_stateful_widget(list, sections[0], &mut state);
    render_browser_inspector(frame, app, sections[1]);
}

fn render_browser_inspector(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let entries = app.browser().entries();
    let selected = app.browser().selected_entry();
    let (directory_count, visible_file_count) = browser_counts(entries);
    let playlist_count = app.browser().playlist_len();
    let selection_name = selected
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| String::from("Nothing selected"));
    let selection_path = selected
        .map(|entry| display_relative_path(app.browser().root(), &entry.path))
        .unwrap_or_else(|| String::from("."));
    let focus_hint = match app.focus() {
        FocusPane::Browser => "browse lane active · j/k move · Enter open/play",
        FocusPane::Player => "player lane active · Tab to return to library",
    };

    let lines = vec![
        Line::from(vec![
            browser_kind_chip(selected),
            Span::raw(" "),
            browser_state_chip(selected),
            Span::raw(" "),
            chip(format!("{directory_count} DIR"), XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            chip(
                format!("{playlist_count} TRACK"),
                XP_TEXT_DARK,
                XP_HIGHLIGHT,
            ),
        ]),
        Line::from(vec![
            browser_label("Selection"),
            Span::styled(
                fit_text(&selection_name, area.width.saturating_sub(15) as usize),
                Style::default()
                    .fg(XP_TEXT_DARK)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            browser_label("Path"),
            Span::styled(
                fit_text(&selection_path, area.width.saturating_sub(10) as usize),
                Style::default().fg(XP_BLUE),
            ),
        ]),
        meter_line(
            "depth",
            browser_depth_ratio(entries, selected),
            area.width.saturating_sub(18) as usize,
            XP_BLUE_MID,
            XP_PANEL_DARK,
        ),
        Line::from(vec![
            browser_label("Action"),
            Span::styled(
                fit_text(
                    browser_action_hint(selected),
                    area.width.saturating_sub(12) as usize,
                ),
                Style::default().fg(XP_TEXT_DARK),
            ),
        ]),
        Line::from(vec![
            browser_label("Focus"),
            Span::styled(
                fit_text(focus_hint, area.width.saturating_sub(11) as usize),
                Style::default().fg(XP_BLUE),
            ),
            Span::raw("  "),
            Span::styled(
                format!("{visible_file_count} visible files"),
                Style::default().fg(XP_PANEL_DARK),
            ),
        ]),
    ];

    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Inspector ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(XP_SKY))
                .style(Style::default().bg(XP_SILVER)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_player(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10),
            Constraint::Length(10),
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

    let block = xp_panel(title, focused);
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    if inner.width < 24 || inner.height < 5 {
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(16), Constraint::Min(24)])
        .split(inner);

    render_album_tile(frame, player, columns[0]);

    let track_name = player
        .current_track
        .as_ref()
        .map(|track| track.title.clone())
        .unwrap_or_else(|| String::from("Drop into the library and press Enter"));

    let queue_text = match (player.queue_index, player.queue.is_empty()) {
        (Some(index), false) => format!("{} / {}", index + 1, player.queue.len()),
        _ => String::from("0 / 0"),
    };

    let status_chip = match player.status {
        PlaybackStatus::Playing => chip("PLAYING", XP_TEXT_DARK, XP_MINT),
        PlaybackStatus::Paused => chip("PAUSED", XP_TEXT_DARK, XP_HIGHLIGHT),
        PlaybackStatus::Stopped => chip("STOPPED", XP_TEXT_LIGHT, XP_BLUE_DEEP),
    };

    let detail_lines = vec![
        Line::from(vec![
            Span::styled(
                " Track  ",
                Style::default()
                    .fg(XP_BLUE_DEEP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                fit_text(&track_name, columns[1].width.saturating_sub(10) as usize),
                Style::default()
                    .fg(XP_TEXT_DARK)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            status_chip,
            Span::raw(" "),
            chip(format!("QUEUE {queue_text}"), XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            chip(format_duration(player.position), XP_TEXT_LIGHT, XP_BLUE_MID),
        ]),
        meter_line(
            "volume",
            player.volume as f64,
            columns[1].width.saturating_sub(18) as usize,
            XP_MINT,
            XP_PANEL_DARK,
        ),
        transport_line(player),
        info_message_line(player, columns[1].width as usize),
    ];

    let widget = Paragraph::new(detail_lines)
        .style(Style::default().bg(XP_SILVER))
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, columns[1]);
}

fn render_album_tile(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let widget = Paragraph::new(album_tile_lines(player))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Deck ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(XP_SKY))
                .style(Style::default().bg(XP_BLUE_MID)),
        )
        .style(Style::default().bg(XP_BLUE_MID));
    frame.render_widget(widget, area);
}

fn render_visualizer(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let block = Block::default()
        .title(" Wave Tank ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(XP_BLUE_DEEP))
        .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    if inner.width < 12 || inner.height < 6 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(4),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);

    let legend = Paragraph::new(Line::from(vec![
        chip("SPECTRUM", XP_TEXT_DARK, XP_HIGHLIGHT),
        Span::raw(" "),
        chip("WAVE", XP_TEXT_LIGHT, XP_BLUE_MID),
        Span::raw(" "),
        chip("GLOW", XP_TEXT_DARK, XP_MINT),
    ]))
    .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(legend, rows[0]);

    let spectrum = Paragraph::new(make_spectrum_lines(
        player,
        rows[1].width as usize,
        rows[1].height as usize,
    ))
    .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(spectrum, rows[1]);

    let wave = Paragraph::new(make_wave_line(player, rows[2].width as usize))
        .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(wave, rows[2]);

    let glow = Paragraph::new(make_glow_line(player, rows[3].width as usize))
        .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(glow, rows[3]);

    let caption = Paragraph::new(Line::from(vec![Span::styled(
        fit_text(&visualizer_caption(player), rows[4].width as usize),
        Style::default().fg(XP_SILVER),
    )]))
    .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(caption, rows[4]);
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
    let block = xp_panel(" Queue Preview ", false);
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.width < 20 || inner.height < 4 {
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(1),
        ])
        .split(inner);

    let summary = Paragraph::new(queue_summary_line(player)).style(Style::default().bg(XP_SILVER));
    frame.render_widget(summary, rows[0]);

    let queue_lines = queue_lines(player, rows[1].width as usize);
    let queue_widget = Paragraph::new(queue_lines)
        .style(Style::default().bg(XP_SILVER))
        .wrap(Wrap { trim: false });
    frame.render_widget(queue_widget, rows[1]);

    let footer = Paragraph::new(queue_footer_line(player, rows[2].width as usize))
        .style(Style::default().bg(XP_SILVER));
    frame.render_widget(footer, rows[2]);
}

fn render_status_bar(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(XP_BLUE_DEEP))
        .style(Style::default().bg(XP_SILVER));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 0,
        horizontal: 1,
    });
    if inner.width < 24 {
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(46), Constraint::Min(12)])
        .split(inner);

    let left = Paragraph::new(status_chip_line(app)).style(Style::default().bg(XP_SILVER));
    frame.render_widget(left, columns[0]);

    let message = Paragraph::new(Line::from(vec![Span::styled(
        fit_text(
            app.status_line(),
            columns[1].width.saturating_sub(1) as usize,
        ),
        Style::default().fg(XP_TEXT_DARK),
    )]))
    .alignment(Alignment::Right)
    .style(Style::default().bg(XP_SILVER));
    frame.render_widget(message, columns[1]);
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
    let mut spans = Vec::with_capacity(entry.depth + 3);
    for _ in 0..entry.depth {
        spans.push(Span::styled("│ ", Style::default().fg(XP_PANEL_DARK)));
    }

    let (icon, icon_color, name_style) = match entry.kind {
        EntryKind::Directory if entry.expanded => (
            "▾",
            XP_MINT,
            Style::default()
                .fg(XP_BLUE_DEEP)
                .add_modifier(Modifier::BOLD),
        ),
        EntryKind::Directory => (
            "▸",
            XP_BLUE_MID,
            Style::default()
                .fg(XP_BLUE_DEEP)
                .add_modifier(Modifier::BOLD),
        ),
        EntryKind::File => ("♪", XP_HIGHLIGHT, Style::default().fg(XP_TEXT_DARK)),
    };

    spans.push(Span::styled(
        format!("{icon} "),
        Style::default().fg(icon_color).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(entry.name.clone(), name_style));

    if entry.kind == EntryKind::File {
        if let Some(extension) = entry.path.extension().and_then(|ext| ext.to_str()) {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                extension.to_ascii_uppercase(),
                Style::default().fg(XP_BLUE_MID),
            ));
        }
    }

    ListItem::new(Line::from(spans))
}

fn browser_counts(entries: &[BrowserEntry]) -> (usize, usize) {
    entries
        .iter()
        .fold((0, 0), |(directories, files), entry| match entry.kind {
            EntryKind::Directory => (directories + 1, files),
            EntryKind::File => (directories, files + 1),
        })
}

fn browser_label(text: &str) -> Span<'static> {
    Span::styled(
        format!(" {text:<9}"),
        Style::default()
            .fg(XP_BLUE_DEEP)
            .add_modifier(Modifier::BOLD),
    )
}

fn browser_kind_chip(entry: Option<&BrowserEntry>) -> Span<'static> {
    match entry.map(|entry| entry.kind) {
        Some(EntryKind::Directory) => chip("FOLDER", XP_TEXT_LIGHT, XP_BLUE_DEEP),
        Some(EntryKind::File) => chip("TRACK", XP_TEXT_DARK, XP_MINT),
        None => chip("EMPTY", XP_TEXT_DARK, XP_PANEL),
    }
}

fn browser_state_chip(entry: Option<&BrowserEntry>) -> Span<'static> {
    match entry {
        Some(entry) if entry.kind == EntryKind::Directory && entry.expanded => {
            chip("OPEN", XP_TEXT_DARK, XP_MINT)
        }
        Some(entry) if entry.kind == EntryKind::Directory => {
            chip("CLOSED", XP_TEXT_LIGHT, XP_BLUE_MID)
        }
        Some(_) => chip("READY", XP_TEXT_LIGHT, XP_BLUE),
        None => chip("IDLE", XP_TEXT_LIGHT, XP_BLUE_MID),
    }
}

fn browser_action_hint(entry: Option<&BrowserEntry>) -> &'static str {
    match entry {
        Some(entry) if entry.kind == EntryKind::Directory && entry.expanded => {
            "Enter toggles folder · ← collapses upward"
        }
        Some(entry) if entry.kind == EntryKind::Directory => "Enter or → opens folder contents",
        Some(_) => "Enter starts playback from this track",
        None => "Point the player at a folder with supported audio",
    }
}

fn browser_depth_ratio(entries: &[BrowserEntry], selected: Option<&BrowserEntry>) -> f64 {
    let depth_ceiling = entries
        .iter()
        .map(|entry| entry.depth + 1)
        .max()
        .unwrap_or(1);
    let selected_depth = selected.map(|entry| entry.depth + 1).unwrap_or(0);
    (selected_depth as f64 / depth_ceiling as f64).clamp(0.0, 1.0)
}

fn display_relative_path(root: &Path, path: &Path) -> String {
    if path == root {
        return String::from(".");
    }

    path.strip_prefix(root)
        .ok()
        .map(|relative| {
            let label = relative.display().to_string();
            if label.is_empty() {
                String::from(".")
            } else {
                label
            }
        })
        .unwrap_or_else(|| path.display().to_string())
}

fn status_chip_line(app: &App) -> Line<'static> {
    let focus_chip = match app.focus() {
        FocusPane::Browser => chip("LIB", XP_TEXT_DARK, XP_HIGHLIGHT),
        FocusPane::Player => chip("PLAYER", XP_TEXT_LIGHT, XP_BLUE_MID),
    };
    let state_chip = match app.player().status {
        PlaybackStatus::Playing => chip("PLAY", XP_TEXT_DARK, XP_MINT),
        PlaybackStatus::Paused => chip("PAUSE", XP_TEXT_DARK, XP_HIGHLIGHT),
        PlaybackStatus::Stopped => chip("STOP", XP_TEXT_LIGHT, XP_BLUE_DEEP),
    };
    let queue_text = match (app.player().queue_index, app.player().queue.is_empty()) {
        (Some(index), false) => format!("Q {}/{}", index + 1, app.player().queue.len()),
        _ => String::from("Q 0/0"),
    };

    Line::from(vec![
        chip("STATUS", XP_TEXT_LIGHT, XP_BLUE_DEEP),
        Span::raw(" "),
        focus_chip,
        Span::raw(" "),
        state_chip,
        Span::raw(" "),
        chip(
            format!("VOL {:>3.0}%", app.player().volume * 100.0),
            XP_TEXT_DARK,
            XP_PANEL,
        ),
        Span::raw(" "),
        chip(queue_text, XP_TEXT_DARK, XP_SILVER),
    ])
}

fn queue_summary_line(player: &PlayerState) -> Line<'static> {
    match (player.queue_index, player.queue.is_empty()) {
        (_, true) => Line::from(vec![
            chip("EMPTY", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            chip("PICK A TRACK", XP_TEXT_LIGHT, XP_BLUE_DEEP),
        ]),
        (Some(index), false) => {
            let remaining = player.queue.len().saturating_sub(index + 1);
            Line::from(vec![
                chip(
                    format!("NOW {}/{}", index + 1, player.queue.len()),
                    XP_TEXT_DARK,
                    XP_HIGHLIGHT,
                ),
                Span::raw(" "),
                chip(format!("NEXT {remaining}"), XP_TEXT_DARK, XP_MINT),
                Span::raw(" "),
                chip(format_duration(player.position), XP_TEXT_LIGHT, XP_BLUE_MID),
            ])
        }
        (None, false) => Line::from(vec![
            chip("QUEUE LOADED", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            chip("READY TO PLAY", XP_TEXT_LIGHT, XP_BLUE_DEEP),
        ]),
    }
}

fn queue_lines(player: &PlayerState, width: usize) -> Vec<Line<'static>> {
    if player.queue.is_empty() {
        return vec![Line::from(vec![Span::styled(
            " Queue is empty — pick a track from the library.",
            Style::default().fg(XP_TEXT_DARK),
        )])];
    }

    let current_index = player.queue_index.unwrap_or(0);
    let start = current_index.saturating_sub(2);
    let end = (start + 6).min(player.queue.len());
    let width = width.saturating_sub(14).max(8);

    player.queue[start..end]
        .iter()
        .enumerate()
        .map(|(offset, path)| {
            let absolute_index = start + offset;
            let name = queue_track_name(path);
            let (label, style) = if Some(absolute_index) == player.queue_index {
                (
                    " NOW ",
                    Style::default()
                        .fg(XP_TEXT_LIGHT)
                        .bg(XP_BLUE_MID)
                        .add_modifier(Modifier::BOLD),
                )
            } else if absolute_index == current_index.saturating_add(1) {
                (
                    " NEXT",
                    Style::default()
                        .fg(XP_TEXT_DARK)
                        .bg(XP_MINT)
                        .add_modifier(Modifier::BOLD),
                )
            } else if absolute_index < current_index {
                (" PREV", Style::default().fg(XP_BLUE_DEEP).bg(XP_PANEL))
            } else {
                (" LATE", Style::default().fg(XP_TEXT_LIGHT).bg(XP_BLUE_DEEP))
            };

            Line::from(vec![
                Span::styled(label, style),
                Span::raw(" "),
                Span::styled(
                    format!("{:>2}.", absolute_index + 1),
                    Style::default().fg(XP_BLUE_DEEP),
                ),
                Span::raw(" "),
                Span::styled(
                    fit_text(&name, width),
                    if Some(absolute_index) == player.queue_index {
                        Style::default()
                            .fg(XP_TEXT_DARK)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(XP_TEXT_DARK)
                    },
                ),
            ])
        })
        .collect()
}

fn queue_footer_line(player: &PlayerState, width: usize) -> Line<'static> {
    let text = if player.queue.is_empty() {
        "Enter from the library builds the active playback order."
    } else {
        "n/p moves through the queue · Enter from library re-roots playback order."
    };

    Line::from(vec![Span::styled(
        fit_text(text, width),
        Style::default().fg(XP_BLUE),
    )])
}

fn queue_track_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
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
        PlaybackStatus::Playing => "blue-glass lift",
        PlaybackStatus::Paused => "held chrome shimmer",
        PlaybackStatus::Stopped => "sleeping in the dock",
    }
}

fn visualizer_caption(player: &PlayerState) -> String {
    match player.status {
        PlaybackStatus::Playing => {
            String::from("Time-driven spectrum is keyed to track, volume, and playback position.")
        }
        PlaybackStatus::Paused => {
            String::from("Spectrum is frozen at the current playback posture.")
        }
        PlaybackStatus::Stopped => {
            String::from("Idle deck shimmer — select a track to light the tank.")
        }
    }
}

fn album_tile_lines(player: &PlayerState) -> Vec<Line<'static>> {
    let glyph = match player.status {
        PlaybackStatus::Playing => "♫",
        PlaybackStatus::Paused => "♪",
        PlaybackStatus::Stopped => "♬",
    };
    let badge_bg = match player.status {
        PlaybackStatus::Playing => XP_MINT,
        PlaybackStatus::Paused => XP_HIGHLIGHT,
        PlaybackStatus::Stopped => XP_PANEL,
    };
    let badge_fg = if player.status == PlaybackStatus::Stopped {
        XP_BLUE_DEEP
    } else {
        XP_TEXT_DARK
    };

    vec![
        Line::from(vec![Span::styled(
            "XP GLASS",
            Style::default()
                .fg(XP_TEXT_LIGHT)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![Span::styled(
            glyph,
            Style::default()
                .fg(XP_HIGHLIGHT)
                .add_modifier(Modifier::BOLD),
        )]),
        make_signature_line(player, 8),
        Line::from(vec![Span::styled(
            format!(" {:>3.0}% VOL ", player.volume * 100.0),
            Style::default()
                .fg(badge_fg)
                .bg(badge_bg)
                .add_modifier(Modifier::BOLD),
        )]),
    ]
}

fn chip(text: impl Into<String>, fg: Color, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", text.into()),
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    )
}

fn meter_line(
    label: &str,
    ratio: f64,
    width: usize,
    active_color: Color,
    inactive_color: Color,
) -> Line<'static> {
    let width = width.max(8);
    let ratio = ratio.clamp(0.0, 1.0);
    let filled = (ratio * width as f64).round() as usize;

    let mut spans = vec![Span::styled(
        format!(" {} ", label.to_uppercase()),
        Style::default()
            .fg(XP_BLUE_DEEP)
            .add_modifier(Modifier::BOLD),
    )];

    for index in 0..width {
        let (ch, color) = if index < filled {
            ('█', active_color)
        } else {
            ('░', inactive_color)
        };
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
    }

    spans.push(Span::raw(" "));
    spans.push(Span::styled(
        format!("{:>3.0}%", ratio * 100.0),
        Style::default().fg(XP_TEXT_DARK),
    ));

    Line::from(spans)
}

fn transport_line(player: &PlayerState) -> Line<'static> {
    let center_label = match player.status {
        PlaybackStatus::Playing => "PAUSE",
        PlaybackStatus::Paused | PlaybackStatus::Stopped => "PLAY",
    };
    let center_bg = match player.status {
        PlaybackStatus::Playing => XP_HIGHLIGHT,
        PlaybackStatus::Paused | PlaybackStatus::Stopped => XP_MINT,
    };

    Line::from(vec![
        chip("◄◄", XP_TEXT_DARK, XP_PANEL),
        Span::raw(" "),
        chip(center_label, XP_TEXT_DARK, center_bg),
        Span::raw(" "),
        chip("►►", XP_TEXT_DARK, XP_PANEL),
        Span::raw(" "),
        chip(
            player
                .duration
                .map(format_duration)
                .unwrap_or_else(|| String::from("--:--")),
            XP_TEXT_LIGHT,
            XP_BLUE_DEEP,
        ),
    ])
}

fn info_message_line(player: &PlayerState, width: usize) -> Line<'static> {
    match &player.last_error {
        Some(error) => Line::from(vec![
            Span::styled(
                " Error ",
                Style::default()
                    .fg(XP_TEXT_LIGHT)
                    .bg(XP_RED)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                fit_text(error, width.saturating_sub(9)),
                Style::default().fg(XP_RED),
            ),
        ]),
        None => Line::from(vec![
            Span::styled(
                " Signal ",
                Style::default()
                    .fg(XP_TEXT_LIGHT)
                    .bg(XP_BLUE_DEEP)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                fit_text(playback_mood(player), width.saturating_sub(10)),
                Style::default().fg(XP_BLUE),
            ),
        ]),
    }
}

fn fit_text(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    if max_chars == 1 {
        return String::from("…");
    }

    let truncated = text.chars().take(max_chars - 1).collect::<String>();
    format!("{truncated}…")
}

fn track_seed(player: &PlayerState) -> u64 {
    let title_seed = player
        .current_track
        .as_ref()
        .map(|track| {
            track.title.bytes().fold(0_u64, |acc, byte| {
                acc.wrapping_mul(131).wrapping_add(byte as u64)
            })
        })
        .unwrap_or(17);

    let duration_seed = player
        .duration
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    title_seed ^ duration_seed.rotate_left(7)
}

fn playback_energy(player: &PlayerState) -> f64 {
    match player.status {
        PlaybackStatus::Playing => 0.52 + (player.volume as f64 * 0.38),
        PlaybackStatus::Paused => 0.26 + (player.volume as f64 * 0.08),
        PlaybackStatus::Stopped => 0.12,
    }
}

fn make_signature_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(6);
    let seed = track_seed(player) as f64 * 0.019;
    let energy = playback_energy(player);
    let phase = match player.status {
        PlaybackStatus::Stopped => 0.0,
        PlaybackStatus::Paused => player.position.as_secs_f64() * 1.7,
        PlaybackStatus::Playing => player.position.as_secs_f64() * 4.0,
    };
    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇'];

    let mut spans = Vec::with_capacity(width);
    for index in 0..width {
        let x = index as f64 / width as f64;
        let wave = ((x * 10.0 + phase + seed).sin() * 0.6 + (x * 21.0 - seed).cos() * 0.4) * energy;
        let normalized = ((wave + 1.0) / 2.0).clamp(0.0, 1.0);
        let char_index = (normalized * (chars.len() - 1) as f64).round() as usize;
        let color = if normalized > 0.72 {
            XP_HIGHLIGHT
        } else if normalized > 0.48 {
            XP_MINT
        } else {
            XP_SKY
        };
        spans.push(Span::styled(
            chars[char_index].to_string(),
            Style::default().fg(color),
        ));
    }

    Line::from(spans)
}

fn make_spectrum_lines(player: &PlayerState, width: usize, rows: usize) -> Vec<Line<'static>> {
    let width = width.max(16);
    let rows = rows.max(2);
    let energy = playback_energy(player);
    let seed = track_seed(player) as f64 * 0.013;
    let progress = progress_info(player).0;
    let phase = match player.status {
        PlaybackStatus::Stopped => 0.0,
        PlaybackStatus::Paused => player.position.as_secs_f64() * 1.8,
        PlaybackStatus::Playing => player.position.as_secs_f64() * (3.6 + progress * 2.2),
    };

    let heights = (0..width)
        .map(|index| {
            let x = index as f64 / width as f64;
            let wave_a = (x * 13.0 + phase + seed).sin();
            let wave_b = (x * 25.0 - phase * 0.7 + seed * 0.4).cos() * 0.55;
            let wave_c = ((index as f64 * 0.33) + phase * 1.3 + seed).sin() * 0.2;
            let pulse = ((phase * 0.75 + x * 4.0).sin() * 0.5 + 0.5) * 0.18;
            let normalized = (((wave_a + wave_b + wave_c) * 0.5) + 0.5).clamp(0.0, 1.0);
            let boosted =
                (normalized * 0.7 + energy * 0.2 + pulse + progress * 0.1).clamp(0.0, 1.0);
            ((boosted * rows as f64).ceil() as usize).min(rows)
        })
        .collect::<Vec<_>>();

    (1..=rows)
        .rev()
        .map(|level| {
            let color = match rows - level {
                0 => XP_HIGHLIGHT,
                1 => XP_MINT,
                2 => XP_GLASS,
                _ => XP_SKY,
            };

            let spans = heights
                .iter()
                .enumerate()
                .map(|(index, height)| {
                    if *height >= level {
                        Span::styled("█".to_string(), Style::default().fg(color))
                    } else if level == 1 && index % 6 == 0 {
                        Span::styled("·".to_string(), Style::default().fg(XP_PANEL_DARK))
                    } else {
                        Span::styled(" ".to_string(), Style::default().fg(XP_PANEL_DARK))
                    }
                })
                .collect::<Vec<_>>();

            Line::from(spans)
        })
        .collect()
}

fn make_wave_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(12);
    let seed = track_seed(player) as f64 * 0.011;
    let energy = playback_energy(player);
    let progress = progress_info(player).0;
    let phase = match player.status {
        PlaybackStatus::Stopped => 0.0,
        PlaybackStatus::Paused => player.position.as_secs_f64() * 2.2,
        PlaybackStatus::Playing => player.position.as_secs_f64() * (5.2 + progress * 1.8),
    };

    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let mut spans = Vec::with_capacity(width);

    for index in 0..width {
        let x = index as f64 / width as f64;
        let base = (x * 11.0 + phase + seed).sin() * 0.58
            + (x * 27.0 - phase * 0.65 + seed * 0.7).cos() * 0.24
            + (progress * 6.0 + x * 3.0).sin() * 0.18;
        let normalized = (((base * energy) + 1.0) / 2.0).clamp(0.0, 1.0);
        let char_index = (normalized * (chars.len() - 1) as f64).round() as usize;
        let color = if normalized > 0.78 {
            XP_HIGHLIGHT
        } else if normalized > 0.5 {
            XP_MINT
        } else if normalized > 0.26 {
            XP_SKY
        } else {
            XP_PANEL
        };
        spans.push(Span::styled(
            chars[char_index].to_string(),
            Style::default().fg(color),
        ));
    }

    Line::from(spans)
}

fn make_glow_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(12);
    let (progress, _) = progress_info(player);
    let phase = if player.status == PlaybackStatus::Stopped {
        width / 4
    } else {
        (((progress * width as f64) + player.position.as_secs_f64() * 6.0).round() as usize) % width
    };

    let mut spans = Vec::with_capacity(width);
    for index in 0..width {
        let distance = index.abs_diff(phase);
        let (ch, color) = if distance == 0 {
            ('◉', XP_HIGHLIGHT)
        } else if distance <= 2 {
            ('•', XP_SILVER)
        } else if distance <= 5 {
            ('·', XP_PANEL_DARK)
        } else {
            ('·', XP_BLUE_MID)
        };
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::audio_engine::Track;

    fn sample_player() -> PlayerState {
        PlayerState {
            status: PlaybackStatus::Playing,
            current_track: Some(Track {
                path: PathBuf::from("song.wav"),
                title: String::from("Ocean Avenue After Midnight"),
            }),
            volume: 0.82,
            position: Duration::from_secs(73),
            duration: Some(Duration::from_secs(212)),
            queue: vec![
                PathBuf::from("one.wav"),
                PathBuf::from("two.wav"),
                PathBuf::from("three.wav"),
            ],
            queue_index: Some(1),
            last_error: None,
        }
    }

    #[test]
    fn fit_text_adds_ellipsis_when_needed() {
        assert_eq!(fit_text("abcdef", 4), "abc…");
        assert_eq!(fit_text("ok", 8), "ok");
    }

    #[test]
    fn spectrum_lines_match_requested_geometry() {
        let lines = make_spectrum_lines(&sample_player(), 18, 4);
        assert_eq!(lines.len(), 4);
        assert!(lines.iter().all(|line| line.spans.len() == 18));
    }

    #[test]
    fn wave_line_enforces_minimum_width() {
        let line = make_wave_line(&sample_player(), 4);
        assert_eq!(line.spans.len(), 12);
    }

    #[test]
    fn browser_counts_split_directories_and_files() {
        let entries = vec![
            BrowserEntry {
                path: PathBuf::from("music"),
                name: String::from("music"),
                depth: 0,
                kind: EntryKind::Directory,
                expanded: true,
            },
            BrowserEntry {
                path: PathBuf::from("music/song.mp3"),
                name: String::from("song.mp3"),
                depth: 1,
                kind: EntryKind::File,
                expanded: false,
            },
            BrowserEntry {
                path: PathBuf::from("music/album"),
                name: String::from("album"),
                depth: 1,
                kind: EntryKind::Directory,
                expanded: false,
            },
        ];

        assert_eq!(browser_counts(&entries), (2, 1));
    }

    #[test]
    fn display_relative_path_prefers_root_relative_form() {
        let root = Path::new("/music");
        let child = Path::new("/music/ambient/dreams/song.flac");
        assert_eq!(
            display_relative_path(root, child),
            "ambient/dreams/song.flac"
        );
        assert_eq!(display_relative_path(root, root), ".");
    }
}
