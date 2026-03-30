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
    style::{Color, Modifier, Style},
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
const VISUALIZER_PRESETS: [&str; 4] = ["Bars", "Ocean Mist", "Fire Storm", "Scope"];

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

    let chrome_height = if content.height < 26 { 2 } else { 3 };
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(chrome_height),
            Constraint::Min(12),
            Constraint::Length(chrome_height),
        ])
        .split(content);

    render_title_bar(frame, app, outer[0]);

    let main = if outer[1].width < 110 {
        let browser_height = if outer[1].height < 22 { 10 } else { 12 };
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(browser_height), Constraint::Min(10)])
            .split(outer[1])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(39), Constraint::Percentage(61)])
            .split(outer[1])
    };

    render_browser(frame, app, main[0]);
    render_player(frame, app, main[1]);
    render_status_bar(frame, app, outer[2]);
}

fn render_title_bar(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let mood = if area.width < 72 {
        "  blue-glass deck  "
    } else if area.width < 96 {
        "  blue-glass signal deck  "
    } else {
        "  blue-glass signal deck • cue stack shimmer  "
    };
    let compact_tab = match app.focus() {
        FocusPane::Browser => nav_tab("LIBRARY", true),
        FocusPane::Player => nav_tab("NOW PLAYING", true),
    };
    let stack_chip = match (app.player().queue_index, app.player().queue.is_empty()) {
        (Some(index), false) => chip(
            format!("STACK {}/{}", index + 1, app.player().queue.len()),
            XP_TEXT_DARK,
            XP_PANEL,
        ),
        _ => chip(
            format!("STACK {}", app.player().queue.len()),
            XP_TEXT_DARK,
            XP_PANEL,
        ),
    };

    let state_chip = match app.player().status {
        PlaybackStatus::Playing => chip(
            title_state_text(&app.player().status),
            XP_TEXT_DARK,
            XP_MINT,
        ),
        PlaybackStatus::Paused => chip(
            title_state_text(&app.player().status),
            XP_TEXT_DARK,
            XP_HIGHLIGHT,
        ),
        PlaybackStatus::Stopped => chip(
            title_state_text(&app.player().status),
            XP_TEXT_LIGHT,
            XP_BLUE_DEEP,
        ),
    };

    let mut spans = vec![
        Span::styled(
            " Terminal Audio Player ",
            Style::default()
                .fg(XP_TEXT_LIGHT)
                .bg(XP_BLUE_DEEP)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(mood, Style::default().fg(XP_TEXT_LIGHT).bg(XP_BLUE)),
    ];

    if area.width >= 94 {
        spans.push(nav_tab("NOW PLAYING", app.focus() == FocusPane::Player));
        spans.push(Span::raw(" "));
        spans.push(nav_tab("MEDIA LIBRARY", app.focus() == FocusPane::Browser));
    } else {
        spans.push(compact_tab);
    }

    spans.push(Span::raw(" "));
    spans.push(state_chip);

    if area.width >= 112 {
        spans.push(Span::raw(" "));
        spans.push(stack_chip);
    }

    let text = Line::from(spans);

    if area.height < 3 {
        let widget = Paragraph::new(text).style(Style::default().bg(XP_BLUE));
        frame.render_widget(widget, area);
        return;
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(XP_BLUE_DEEP))
        .style(Style::default().bg(XP_BLUE));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.width < 16 {
        return;
    }

    if inner.width < 72 {
        let widget = Paragraph::new(text).style(Style::default().bg(XP_BLUE));
        frame.render_widget(widget, inner);
        return;
    }

    let control_width = if inner.width >= 120 { 15 } else { 13 };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(12), Constraint::Length(control_width)])
        .split(inner);

    let title = Paragraph::new(text).style(Style::default().bg(XP_BLUE));
    frame.render_widget(title, columns[0]);

    let controls = Paragraph::new(window_controls_line(columns[1].width as usize))
        .alignment(Alignment::Right)
        .style(Style::default().bg(XP_BLUE));
    frame.render_widget(controls, columns[1]);
}

fn title_state_text(status: &PlaybackStatus) -> &'static str {
    match status {
        PlaybackStatus::Playing => "ON AIR",
        PlaybackStatus::Paused => "HELD",
        PlaybackStatus::Stopped => "READY",
    }
}

fn nav_tab(label: &str, active: bool) -> Span<'static> {
    let (fg, bg) = if active {
        (XP_TEXT_DARK, XP_SILVER)
    } else {
        (XP_TEXT_LIGHT, XP_BLUE_MID)
    };

    Span::styled(
        format!(" {label} "),
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    )
}

fn window_controls_line(width: usize) -> Line<'static> {
    if width < 11 {
        return Line::from(vec![chip("×", XP_TEXT_LIGHT, XP_RED)]);
    }

    Line::from(vec![
        chip("–", XP_TEXT_DARK, XP_PANEL),
        Span::raw(" "),
        chip("□", XP_TEXT_DARK, XP_PANEL),
        Span::raw(" "),
        chip("×", XP_TEXT_LIGHT, XP_RED),
    ])
}

fn render_browser(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let compact_browser = area.width < 84 || area.height < 12;
    let root_label = browser_root_label(
        app.browser().root(),
        if compact_browser {
            area.width.saturating_sub(26) as usize
        } else {
            area.width.saturating_sub(20) as usize
        },
    );
    let title = match app.focus() {
        FocusPane::Browser => format!(" Library Lane ◆ {root_label}"),
        FocusPane::Player => format!(" Library Lane ◇ {root_label}"),
    };

    let shell = xp_panel(&title, app.focus() == FocusPane::Browser);
    frame.render_widget(shell, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.width < 18 || inner.height < 4 {
        return;
    }

    let sections = if compact_browser || inner.height < 11 {
        vec![inner]
    } else {
        let inspector_height = if inner.width < 42 || inner.height < 14 {
            5
        } else {
            8
        };
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(inspector_height)])
            .split(inner)
            .to_vec()
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

    let list = if compact_browser {
        List::new(items).highlight_symbol("◆ ").highlight_style(
            Style::default()
                .fg(XP_TEXT_LIGHT)
                .bg(XP_BLUE_MID)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        List::new(items)
            .block(
                Block::default()
                    .title(" Library Tree ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(XP_SKY))
                    .style(Style::default().bg(XP_SILVER)),
            )
            .highlight_symbol("◆ ")
            .highlight_style(
                Style::default()
                    .fg(XP_TEXT_LIGHT)
                    .bg(XP_BLUE_MID)
                    .add_modifier(Modifier::BOLD),
            )
    };

    let mut state = ListState::default();
    if !entries.is_empty() {
        state.select(Some(app.browser().selected_index()));
    }

    frame.render_stateful_widget(list, sections[0], &mut state);
    if sections.len() > 1 {
        render_browser_inspector(frame, app, sections[1]);
    }
}

fn render_browser_inspector(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let entries = app.browser().entries();
    let selected = app.browser().selected_entry();
    let (directory_count, _visible_file_count) = browser_counts(entries);
    let playlist_count = app.browser().playlist_len();
    let selection_name = selected
        .map(|entry| entry.name.clone())
        .unwrap_or_else(|| String::from("Nothing selected"));
    let selection_path = selected
        .map(|entry| display_relative_path(app.browser().root(), &entry.path))
        .unwrap_or_else(|| String::from("."));
    let focus_hint = match app.focus() {
        FocusPane::Browser => "library lane active · j/k glide · Enter load/open",
        FocusPane::Player => "playback deck active · Tab returns to the library lane",
    };

    let compact = area.width < 42 || area.height < 8;
    let lines = if compact {
        vec![
            Line::from(vec![
                browser_kind_chip(selected),
                Span::raw(" "),
                browser_state_chip(selected),
                Span::raw(" "),
                chip(
                    format!("{playlist_count} TRACK"),
                    XP_TEXT_DARK,
                    XP_HIGHLIGHT,
                ),
            ]),
            Line::from(vec![
                browser_label("Cue"),
                Span::styled(
                    fit_text(&selection_name, area.width.saturating_sub(9) as usize),
                    Style::default()
                        .fg(XP_TEXT_DARK)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                browser_label("Shelf"),
                Span::styled(
                    fit_text(&selection_path, area.width.saturating_sub(9) as usize),
                    Style::default().fg(XP_BLUE),
                ),
            ]),
        ]
    } else {
        vec![
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
                browser_label("Cue"),
                Span::styled(
                    fit_text(&selection_name, area.width.saturating_sub(15) as usize),
                    Style::default()
                        .fg(XP_TEXT_DARK)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                browser_label("Shelf"),
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
                browser_label("Move"),
                Span::styled(
                    fit_text(
                        browser_action_hint(selected),
                        area.width.saturating_sub(12) as usize,
                    ),
                    Style::default().fg(XP_TEXT_DARK),
                ),
            ]),
            Line::from(vec![
                browser_label("Lane"),
                Span::styled(
                    fit_text(focus_hint, area.width.saturating_sub(11) as usize),
                    Style::default().fg(XP_BLUE),
                ),
            ]),
        ]
    };

    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Lane Monitor ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(XP_SKY))
                .style(Style::default().bg(XP_SILVER)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(widget, area);
}

fn render_player(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    if area.width >= 72 {
        if area.height < 14 {
            render_compact_player(frame, app, area);
            return;
        }

        if area.height < 20 {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(7),
                    Constraint::Min(4),
                    Constraint::Length(3),
                ])
                .split(area);

            render_now_playing(
                frame,
                app.player(),
                app.focus() == FocusPane::Player,
                rows[0],
            );
            render_visualizer(frame, app.player(), rows[1]);
            render_progress(frame, app.player(), rows[2]);
            return;
        }

        if let Some((now_height, visualizer_height, progress_height, queue_height)) =
            wide_queue_reentry_layout(area.height)
        {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(now_height),
                    Constraint::Length(visualizer_height),
                    Constraint::Length(progress_height),
                    Constraint::Length(queue_height),
                ])
                .split(area);

            render_now_playing(
                frame,
                app.player(),
                app.focus() == FocusPane::Player,
                rows[0],
            );
            render_visualizer(frame, app.player(), rows[1]);
            render_progress(frame, app.player(), rows[2]);
            render_queue(frame, app.player(), rows[3]);
            return;
        }

        if let Some((now_height, visualizer_height, progress_height, guide_height, queue_height)) =
            wide_deck_guide_reentry_layout(area.height)
        {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(now_height),
                    Constraint::Length(visualizer_height),
                    Constraint::Length(progress_height),
                    Constraint::Length(guide_height),
                    Constraint::Length(queue_height),
                ])
                .split(area);

            render_now_playing(
                frame,
                app.player(),
                app.focus() == FocusPane::Player,
                rows[0],
            );
            render_visualizer(frame, app.player(), rows[1]);
            render_progress(frame, app.player(), rows[2]);
            render_keys(frame, rows[3]);
            render_queue(frame, app.player(), rows[4]);
            return;
        }

        let now_height: u16 = if area.height >= 30 { 9 } else { 8 };
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(now_height),
                Constraint::Min(5),
                Constraint::Length(3),
                Constraint::Length(4),
                Constraint::Length(6),
            ])
            .split(area);

        render_now_playing(
            frame,
            app.player(),
            app.focus() == FocusPane::Player,
            rows[0],
        );
        render_visualizer(frame, app.player(), rows[1]);
        render_progress(frame, app.player(), rows[2]);
        render_keys(frame, rows[3]);
        render_queue(frame, app.player(), rows[4]);
        return;
    }

    if area.height < 15 {
        render_compact_player(frame, app, area);
        return;
    }

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Min(4),
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

fn wide_queue_reentry_layout(area_height: u16) -> Option<(u16, u16, u16, u16)> {
    if !(20..26).contains(&area_height) {
        return None;
    }

    let queue_height = match area_height {
        20 | 21 => 4,
        22..=24 => 5,
        _ => 6,
    };
    let visualizer_height = 8;
    let progress_height = 3;
    let now_height = area_height - visualizer_height - progress_height - queue_height;

    Some((now_height, visualizer_height, progress_height, queue_height))
}

fn wide_deck_guide_reentry_layout(area_height: u16) -> Option<(u16, u16, u16, u16, u16)> {
    if !(26..31).contains(&area_height) {
        return None;
    }

    let queue_height = match area_height {
        26 => 4,
        27 | 28 => 5,
        _ => 6,
    };
    let guide_height = 4;
    let progress_height = 3;
    let visualizer_height = if area_height >= 30 { 9 } else { 8 };
    let now_height =
        area_height - visualizer_height - progress_height - guide_height - queue_height;

    Some((
        now_height,
        visualizer_height,
        progress_height,
        guide_height,
        queue_height,
    ))
}

fn render_compact_player(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let compact = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(7),
            Constraint::Min(3),
            Constraint::Length(3),
        ])
        .split(area);

    render_now_playing(
        frame,
        app.player(),
        app.focus() == FocusPane::Player,
        compact[0],
    );
    render_visualizer(frame, app.player(), compact[1]);
    render_progress(frame, app.player(), compact[2]);
}

fn render_now_playing(
    frame: &mut ratatui::Frame<'_>,
    player: &PlayerState,
    focused: bool,
    area: Rect,
) {
    let title = if focused {
        " Playback Deck ◆ "
    } else {
        " Playback Deck ◇ "
    };

    let block = xp_panel(title, focused);
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    if inner.width < 24 || inner.height < 2 {
        return;
    }

    if inner.height < 5 {
        let widget = Paragraph::new(compact_now_playing_lines(
            player,
            inner.width as usize,
            inner.height as usize,
        ))
        .style(Style::default().bg(XP_SILVER))
        .wrap(Wrap { trim: false });
        frame.render_widget(widget, inner);
        return;
    }

    let compact = inner.width < 52;
    let detail_area = if compact {
        inner
    } else {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(16), Constraint::Min(24)])
            .split(inner);
        render_album_tile(frame, player, columns[0]);
        columns[1]
    };

    let widget = Paragraph::new(now_playing_detail_lines(
        player,
        detail_area.width as usize,
        detail_area.height as usize,
    ))
    .style(Style::default().bg(XP_SILVER))
    .wrap(Wrap { trim: false });
    frame.render_widget(widget, detail_area);
}

fn now_playing_detail_lines(
    player: &PlayerState,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let width = width.max(24);
    let height = height.max(5);
    let track_name = player
        .current_track
        .as_ref()
        .map(|track| track.title.clone())
        .unwrap_or_else(|| String::from("Pick a track in the library lane and press Enter"));
    let queue_text = match (player.queue_index, player.queue.is_empty()) {
        (Some(index), false) => format!("{} / {}", index + 1, player.queue.len()),
        _ => String::from("0 / 0"),
    };

    let status_chip = match player.status {
        PlaybackStatus::Playing => chip("PLAYING", XP_TEXT_DARK, XP_MINT),
        PlaybackStatus::Paused => chip("PAUSED", XP_TEXT_DARK, XP_HIGHLIGHT),
        PlaybackStatus::Stopped => chip("STOPPED", XP_TEXT_LIGHT, XP_BLUE_DEEP),
    };
    let hero_chip = match player.status {
        PlaybackStatus::Playing => chip("ON AIR", XP_TEXT_DARK, XP_HIGHLIGHT),
        PlaybackStatus::Paused => chip("HELD", XP_TEXT_DARK, XP_PANEL),
        PlaybackStatus::Stopped => chip("READY", XP_TEXT_LIGHT, XP_BLUE_DEEP),
    };

    let mut lines = vec![now_playing_header_line(player, width)];
    lines.push(Line::from(vec![
        hero_chip,
        Span::raw(" "),
        Span::styled(
            animated_marquee(player, &track_name, width.saturating_sub(13).max(8), 4.0),
            Style::default()
                .fg(XP_TEXT_DARK)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    if height >= 6 {
        lines.push(deck_source_line(player, width));
    }

    lines.push(now_playing_context_line(player, width));
    lines.push(Line::from(vec![
        status_chip,
        Span::raw(" "),
        chip(format!("QUEUE {queue_text}"), XP_TEXT_DARK, XP_PANEL),
        Span::raw(" "),
        chip(compact_time_label(player), XP_TEXT_LIGHT, XP_BLUE_MID),
    ]));

    if height >= 7 {
        lines.push(meter_line(
            "volume",
            player.volume as f64,
            width.saturating_sub(18),
            XP_MINT,
            XP_PANEL_DARK,
        ));
    }

    if height >= 9 {
        lines.push(meter_line(
            "drive",
            playback_energy(player),
            width.saturating_sub(17),
            XP_HIGHLIGHT,
            XP_PANEL_DARK,
        ));
    }

    lines.push(transport_line(player, width));

    if height >= 8 {
        lines.push(info_message_line(player, width));
    }

    lines
}

fn render_album_tile(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let accent = match player.status {
        PlaybackStatus::Playing => XP_HIGHLIGHT,
        PlaybackStatus::Paused => XP_PANEL,
        PlaybackStatus::Stopped => XP_SKY,
    };

    let block = Block::default()
        .title(" Album Glass ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(XP_BLUE_MID));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.width < 6 || inner.height < 2 {
        return;
    }

    let widget = Paragraph::new(album_tile_lines(
        player,
        inner.width as usize,
        inner.height as usize,
    ))
    .alignment(Alignment::Center)
    .style(Style::default().bg(XP_BLUE_MID));
    frame.render_widget(widget, inner);
}

fn render_visualizer(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let block = Block::default()
        .title(" Signal Deck ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(XP_BLUE_DEEP))
        .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });

    if inner.width < 12 || inner.height < 1 {
        return;
    }
    if inner.width >= 56 && (5..=6).contains(&inner.height) {
        render_visualizer_bridge(frame, player, inner);
        return;
    }

    if inner.height < 6 {
        if inner.width >= 36 && inner.height >= 2 {
            let compact = Paragraph::new(compact_visualizer_lines(
                player,
                inner.width as usize,
                inner.height as usize,
            ))
            .style(Style::default().bg(XP_BLUE_DEEP))
            .wrap(Wrap { trim: false });
            frame.render_widget(compact, inner);
        } else {
            let compact = Paragraph::new(make_wave_line(player, inner.width as usize))
                .style(Style::default().bg(XP_BLUE_DEEP));
            frame.render_widget(compact, inner);
        }
        return;
    }

    if inner.width >= 40 && inner.height == 7 {
        let ladder_width = if inner.width >= 58 { 19 } else { 16 };
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(ladder_width)])
            .split(inner);

        render_visualizer_deck(frame, player, columns[0]);
        render_signal_rise(frame, player, columns[1]);
        return;
    }

    if inner.width >= 40 && inner.height >= 8 {
        let ladder_width = if inner.width >= 58 { 19 } else { 16 };
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(ladder_width)])
            .split(inner);

        render_visualizer_deck(frame, player, columns[0]);
        render_signal_ladder(frame, player, columns[1]);
        return;
    }

    render_visualizer_deck(frame, player, inner);
}

fn render_visualizer_bridge(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let rail_width = if area.width >= 68 { 20 } else { 18 };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(36),
            Constraint::Length(1),
            Constraint::Length(rail_width),
        ])
        .split(area);

    let deck = Paragraph::new(compact_visualizer_lines(
        player,
        columns[0].width as usize,
        columns[0].height as usize,
    ))
    .style(Style::default().bg(XP_BLUE_DEEP))
    .wrap(Wrap { trim: false });
    frame.render_widget(deck, columns[0]);

    let divider = Paragraph::new(signal_bridge_divider_lines(columns[1].height as usize))
        .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(divider, columns[1]);

    let rail = Paragraph::new(signal_rail_lines(
        player,
        columns[2].width as usize,
        columns[2].height as usize,
    ))
    .style(Style::default().bg(XP_BLUE));
    frame.render_widget(rail, columns[2]);
}

fn signal_bridge_divider_lines(height: usize) -> Vec<Line<'static>> {
    (0..height.max(1))
        .map(|index| {
            let accent = if index % 2 == 0 {
                XP_PANEL_DARK
            } else {
                XP_SKY
            };
            Line::from(vec![Span::styled(
                "▏",
                Style::default().fg(accent).bg(XP_BLUE_DEEP),
            )])
        })
        .collect()
}

fn signal_rail_lines(player: &PlayerState, width: usize, height: usize) -> Vec<Line<'static>> {
    let width = width.max(14);
    let height = height.max(4);
    let mut lines = vec![signal_rail_header_line(player, width)];
    let meter_rows = height.saturating_sub(2).clamp(2, 4);
    lines.extend(visualizer_meter_lines(player, width, meter_rows));
    lines.truncate(height.saturating_sub(1));
    lines.push(signal_rail_footer_line(player, width));
    lines
}

fn signal_rail_header_line(player: &PlayerState, width: usize) -> Line<'static> {
    let mut spans = vec![chip("LEVEL RAIL", XP_TEXT_DARK, XP_PANEL)];

    if width >= 20 {
        spans.push(Span::raw(" "));
        spans.push(visualizer_state_chip(player));
    }

    Line::from(spans)
}

fn signal_rail_footer_line(player: &PlayerState, width: usize) -> Line<'static> {
    let footer = match player.status {
        PlaybackStatus::Stopped => String::from("queue to wake rail"),
        PlaybackStatus::Paused => format!(
            "{} · held · {}",
            visualizer_preset(player),
            compact_time_label(player)
        ),
        PlaybackStatus::Playing => {
            format!(
                "{} · crest · {}",
                visualizer_preset(player),
                compact_time_label(player)
            )
        }
    };

    Line::from(vec![Span::styled(
        fit_text(&footer, width),
        Style::default().fg(XP_SILVER),
    )])
}

fn render_signal_rise(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let block = Block::default()
        .title(" Signal Rise ")
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(XP_SKY))
        .style(Style::default().bg(XP_BLUE));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 0,
    });
    if inner.width < 10 || inner.height < 2 {
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

    let header = Paragraph::new(Line::from(vec![
        visualizer_state_chip(player),
        Span::raw(" "),
        Span::styled(
            fit_text(
                &format!(
                    "rail opens · {} · {}",
                    visualizer_preset(player),
                    compact_time_label(player)
                ),
                rows[0].width.saturating_sub(9) as usize,
            ),
            Style::default().fg(XP_SKY).add_modifier(Modifier::BOLD),
        ),
    ]))
    .style(Style::default().bg(XP_BLUE));
    frame.render_widget(header, rows[0]);

    let meters = Paragraph::new(visualizer_meter_lines(
        player,
        rows[1].width as usize,
        rows[1].height as usize,
    ))
    .style(Style::default().bg(XP_BLUE));
    frame.render_widget(meters, rows[1]);

    let footer = Paragraph::new(Line::from(vec![Span::styled(
        fit_text(&signal_rise_footer_text(player), rows[2].width as usize),
        Style::default().fg(XP_HIGHLIGHT),
    )]))
    .style(Style::default().bg(XP_BLUE));
    frame.render_widget(footer, rows[2]);
}

fn signal_rise_footer_text(player: &PlayerState) -> &'static str {
    match player.status {
        PlaybackStatus::Playing => "crest rail rising toward the full ladder",
        PlaybackStatus::Paused => "held rail waiting to reopen the full ladder",
        PlaybackStatus::Stopped => "queue a track to raise the full ladder",
    }
}

fn render_visualizer_deck(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let lush = area.height >= 9;
    let deluxe = area.height >= 7;
    let rows = if lush {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(area)
    } else if deluxe {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
            ])
            .split(area)
    };

    let header = Paragraph::new(visualizer_header_line(player, rows[0].width as usize))
        .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(header, rows[0]);

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

    let (glow_row, caption_row) = if lush {
        let undertow = Paragraph::new(make_wave_undertow_line(player, rows[3].width as usize))
            .style(Style::default().bg(XP_BLUE_DEEP));
        frame.render_widget(undertow, rows[3]);

        let reflection = Paragraph::new(make_reflection_line(player, rows[4].width as usize))
            .style(Style::default().bg(XP_BLUE_DEEP));
        frame.render_widget(reflection, rows[4]);
        (5, 6)
    } else if deluxe {
        let undertow = Paragraph::new(make_wave_undertow_line(player, rows[3].width as usize))
            .style(Style::default().bg(XP_BLUE_DEEP));
        frame.render_widget(undertow, rows[3]);
        (4, 5)
    } else {
        (3, 4)
    };

    let glow = Paragraph::new(make_glow_line(player, rows[glow_row].width as usize))
        .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(glow, rows[glow_row]);

    let caption = Paragraph::new(visualizer_footer_line(
        player,
        rows[caption_row].width as usize,
    ))
    .style(Style::default().bg(XP_BLUE_DEEP));
    frame.render_widget(caption, rows[caption_row]);
}

fn render_signal_ladder(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    // The bloom effect happens at the first stage of the full ladder view.
    // The transition from render_signal_rise occurs when the visualizer's
    // inner height transitions from 7 to 8. This function is called with
    // an area of height 8 in that initial "bloom" frame.
    let is_blooming = area.height == 8;

    let border_style = if is_blooming {
        Style::default().fg(XP_HIGHLIGHT)
    } else {
        Style::default().fg(XP_SKY)
    };

    let block = Block::default()
        .title(" Signal Ladder ")
        .borders(Borders::ALL)
        .border_style(border_style)
        .style(Style::default().bg(XP_BLUE));
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.width < 10 || inner.height < 2 {
        return;
    }

    if inner.height < 5 {
        let meters = Paragraph::new(visualizer_meter_lines(
            player,
            inner.width as usize,
            inner.height as usize,
        ))
        .style(Style::default().bg(XP_BLUE));
        frame.render_widget(meters, inner);
        return;
    }

    let footer_height = if inner.height >= 7 { 2 } else { 1 };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(footer_height),
        ])
        .split(inner);

    let header = Paragraph::new(Line::from(vec![
        visualizer_state_chip(player),
        Span::raw(" "),
        Span::styled(
            fit_text(
                &format!(
                    "{} · {}",
                    visualizer_preset(player),
                    compact_time_label(player)
                ),
                rows[0].width.saturating_sub(9) as usize,
            ),
            Style::default().fg(XP_SKY).add_modifier(Modifier::BOLD),
        ),
    ]))
    .style(Style::default().bg(XP_BLUE));
    frame.render_widget(header, rows[0]);

    let meters = Paragraph::new(visualizer_meter_lines(
        player,
        rows[1].width as usize,
        rows[1].height as usize,
    ))
    .style(Style::default().bg(XP_BLUE));
    frame.render_widget(meters, rows[1]);

    let footer_text = if is_blooming {
        "the full ladder blooms from the signal rise"
    } else {
        playback_mood(player)
    };

    let footer_lines = if rows[2].height > 1 {
        vec![
            Line::from(vec![Span::styled(
                fit_text(footer_text, rows[2].width as usize),
                Style::default().fg(XP_HIGHLIGHT),
            )]),
            Line::from(vec![Span::styled(
                fit_text(&visualizer_caption(player), rows[2].width as usize),
                Style::default().fg(XP_SILVER),
            )]),
        ]
    } else {
        vec![Line::from(vec![Span::styled(
            fit_text(footer_text, rows[2].width as usize),
            Style::default().fg(XP_HIGHLIGHT),
        )])]
    };
    let footer = Paragraph::new(footer_lines).style(Style::default().bg(XP_BLUE));
    frame.render_widget(footer, rows[2]);
}

fn render_progress(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let (progress_ratio, progress_label) = progress_info(player);
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Time Ribbon ")
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
    let block = xp_panel(" Deck Guide ", false);
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.width < 24 || inner.height < 2 {
        return;
    }

    let help = Paragraph::new(help_lines(inner.width as usize, inner.height as usize))
        .style(Style::default().bg(XP_SILVER))
        .wrap(Wrap { trim: false });
    frame.render_widget(help, inner);
}

fn render_queue(frame: &mut ratatui::Frame<'_>, player: &PlayerState, area: Rect) {
    let block = xp_panel(" Cue Stack ", false);
    frame.render_widget(block, area);

    let inner = area.inner(Margin {
        vertical: 1,
        horizontal: 1,
    });
    if inner.width < 20 || inner.height < 2 {
        return;
    }

    if inner.height >= 5 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(1),
            ])
            .split(inner);

        let summary =
            Paragraph::new(queue_summary_line(player)).style(Style::default().bg(XP_SILVER));
        frame.render_widget(summary, rows[0]);

        let marquee = Paragraph::new(queue_marquee_line(player, rows[1].width as usize))
            .style(Style::default().bg(XP_SILVER));
        frame.render_widget(marquee, rows[1]);

        let queue_widget = Paragraph::new(queue_lines(
            player,
            rows[2].width as usize,
            rows[2].height as usize,
        ))
        .style(Style::default().bg(XP_SILVER))
        .wrap(Wrap { trim: false });
        frame.render_widget(queue_widget, rows[2]);

        let footer = Paragraph::new(queue_footer_line(player, rows[3].width as usize))
            .style(Style::default().bg(XP_SILVER));
        frame.render_widget(footer, rows[3]);
        return;
    }

    let summary = Paragraph::new(queue_summary_line(player)).style(Style::default().bg(XP_SILVER));
    frame.render_widget(summary, Rect::new(inner.x, inner.y, inner.width, 1));

    if inner.height == 2 {
        let queue_widget = Paragraph::new(queue_lines(player, inner.width as usize, 1))
            .style(Style::default().bg(XP_SILVER))
            .wrap(Wrap { trim: false });
        frame.render_widget(
            queue_widget,
            Rect::new(inner.x, inner.y + 1, inner.width, 1),
        );
        return;
    }

    if inner.height == 3 {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Length(1),
            ])
            .split(inner);

        let queue_widget = Paragraph::new(queue_lines(player, rows[1].width as usize, 1))
            .style(Style::default().bg(XP_SILVER))
            .wrap(Wrap { trim: false });
        frame.render_widget(queue_widget, rows[1]);

        let footer = Paragraph::new(queue_footer_line(player, rows[2].width as usize))
            .style(Style::default().bg(XP_SILVER));
        frame.render_widget(footer, rows[2]);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(inner);

    let marquee = Paragraph::new(queue_marquee_line(player, rows[1].width as usize))
        .style(Style::default().bg(XP_SILVER));
    frame.render_widget(marquee, rows[1]);

    let queue_widget = Paragraph::new(queue_lines(player, rows[2].width as usize, 1))
        .style(Style::default().bg(XP_SILVER))
        .wrap(Wrap { trim: false });
    frame.render_widget(queue_widget, rows[2]);

    let footer = Paragraph::new(queue_footer_line(player, rows[3].width as usize))
        .style(Style::default().bg(XP_SILVER));
    frame.render_widget(footer, rows[3]);
}

fn render_status_bar(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Status Ribbon ")
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

    if inner.width < 88 {
        let compact_text = if app.player().current_track.is_some() {
            status_transport_text(app.player())
        } else {
            app.status_line().to_string()
        };

        let compact = Paragraph::new(Line::from(vec![
            compact_status_chip(app),
            Span::raw(" "),
            Span::styled(
                fit_text(&compact_text, inner.width.saturating_sub(12) as usize),
                Style::default().fg(XP_TEXT_DARK),
            ),
        ]))
        .style(Style::default().bg(XP_SILVER));
        frame.render_widget(compact, inner);
        return;
    }

    if inner.width >= 118 {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Length(46),
                Constraint::Length(34),
                Constraint::Min(12),
            ])
            .split(inner);

        let left = Paragraph::new(status_chip_line(app)).style(Style::default().bg(XP_SILVER));
        frame.render_widget(left, columns[0]);

        let transport = Paragraph::new(status_transport_line(
            app.player(),
            columns[1].width.saturating_sub(1) as usize,
        ))
        .alignment(Alignment::Center)
        .style(Style::default().bg(XP_SILVER));
        frame.render_widget(transport, columns[1]);

        let message = Paragraph::new(Line::from(vec![Span::styled(
            fit_text(
                app.status_line(),
                columns[2].width.saturating_sub(1) as usize,
            ),
            Style::default().fg(XP_TEXT_DARK),
        )]))
        .alignment(Alignment::Right)
        .style(Style::default().bg(XP_SILVER));
        frame.render_widget(message, columns[2]);
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
            "Enter folds this shelf · ← climbs one level"
        }
        Some(entry) if entry.kind == EntryKind::Directory => "Enter or → opens the folder shelf",
        Some(_) => "Enter loads this track into the deck",
        None => "Point the library lane at a folder with supported audio",
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

fn browser_root_label(root: &Path, max_chars: usize) -> String {
    if max_chars <= 18 {
        return fit_text(
            &root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string()),
            max_chars,
        );
    }

    if max_chars <= 80 {
        return fit_text(&tail_path(root, 2), max_chars);
    }

    fit_text(&root.display().to_string(), max_chars)
}

fn tail_path(path: &Path, component_count: usize) -> String {
    let parts = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>();

    if parts.is_empty() {
        return String::from(".");
    }

    let start = parts.len().saturating_sub(component_count.max(1));
    parts[start..].join("/")
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

fn compact_playback_chip(player: &PlayerState) -> Span<'static> {
    match player.status {
        PlaybackStatus::Playing => chip("PLAY", XP_TEXT_DARK, XP_MINT),
        PlaybackStatus::Paused => chip("PAUSE", XP_TEXT_DARK, XP_HIGHLIGHT),
        PlaybackStatus::Stopped => chip("STOP", XP_TEXT_LIGHT, XP_BLUE_DEEP),
    }
}

fn compact_now_playing_lines(
    player: &PlayerState,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let width = width.max(16);
    let track_name = player
        .current_track
        .as_ref()
        .map(|track| track.title.clone())
        .unwrap_or_else(|| String::from("Pick a track in the library lane and press Enter"));
    let queue_text = match (player.queue_index, player.queue.is_empty()) {
        (Some(index), false) => format!("Q {} / {}", index + 1, player.queue.len()),
        _ => String::from("Q 0 / 0"),
    };

    let mut lines = vec![Line::from(vec![Span::styled(
        fit_text(&track_name, width.saturating_sub(1)),
        Style::default()
            .fg(XP_TEXT_DARK)
            .add_modifier(Modifier::BOLD),
    )])];

    if height >= 3 {
        lines.push(Line::from(vec![Span::styled(
            fit_text(&compact_track_context(player), width.saturating_sub(1)),
            Style::default().fg(XP_BLUE),
        )]));
    }

    let time_chip = chip(compact_time_label(player), XP_TEXT_LIGHT, XP_BLUE_MID);
    let queue_chip = chip(queue_text, XP_TEXT_DARK, XP_PANEL);
    let bottom = if width >= 42 {
        vec![
            compact_playback_chip(player),
            Span::raw(" "),
            time_chip,
            Span::raw(" "),
            queue_chip,
        ]
    } else if width >= 30 {
        vec![compact_playback_chip(player), Span::raw(" "), time_chip]
    } else {
        vec![compact_playback_chip(player)]
    };
    lines.push(Line::from(bottom));

    lines
}

fn compact_track_context(player: &PlayerState) -> String {
    match (&player.last_error, player.current_track.as_ref()) {
        (Some(error), _) => format!("error · {error}"),
        (_, Some(track)) => track
            .path
            .parent()
            .and_then(|parent| parent.file_name())
            .map(|name| format!("folder · {}", name.to_string_lossy()))
            .unwrap_or_else(|| String::from("library selection loaded")),
        _ => String::from("select a track to light the deck"),
    }
}

fn compact_time_label(player: &PlayerState) -> String {
    match (player.current_track.as_ref(), player.duration) {
        (Some(_), Some(duration)) if !duration.is_zero() => {
            format!(
                "{} / {}",
                format_duration(player.position),
                format_duration(duration)
            )
        }
        (Some(_), _) if !player.position.is_zero() => format_duration(player.position),
        _ => String::from("--:--"),
    }
}

fn track_folder_label(player: &PlayerState) -> Option<String> {
    player
        .current_track
        .as_ref()
        .and_then(|track| track.path.parent())
        .and_then(|parent| parent.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

fn track_shelf_label(player: &PlayerState) -> Option<String> {
    player
        .current_track
        .as_ref()
        .and_then(|track| track.path.parent())
        .and_then(|parent| parent.parent())
        .and_then(|grandparent| grandparent.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
}

fn track_codec_label(player: &PlayerState) -> Option<String> {
    player
        .current_track
        .as_ref()
        .and_then(|track| track.path.extension())
        .map(|ext| ext.to_string_lossy().to_uppercase())
        .filter(|ext| !ext.is_empty())
}

fn push_spaced_span(spans: &mut Vec<Span<'static>>, span: Span<'static>) {
    if !spans.is_empty() {
        spans.push(Span::raw(" "));
    }
    spans.push(span);
}

fn deck_source_line(player: &PlayerState, width: usize) -> Line<'static> {
    let mut spans = Vec::new();

    if width >= 58
        && let Some(shelf) = track_shelf_label(player)
    {
        push_spaced_span(
            &mut spans,
            chip(
                format!("SHELF {}", fit_text(&shelf, 12)),
                XP_TEXT_DARK,
                XP_PANEL,
            ),
        );
    }

    if width >= 40
        && let Some(folder) = track_folder_label(player)
    {
        push_spaced_span(
            &mut spans,
            chip(
                format!("FOLDER {}", fit_text(&folder, 12)),
                XP_TEXT_DARK,
                XP_GLASS,
            ),
        );
    }

    if let Some(codec) = track_codec_label(player) {
        push_spaced_span(
            &mut spans,
            chip(
                format!("CODEC {}", fit_text(&codec, 5)),
                XP_TEXT_LIGHT,
                XP_BLUE_DEEP,
            ),
        );
    }

    if width >= 54 {
        let tail_chip = match player.duration {
            Some(duration) if player.current_track.is_some() => chip(
                format!("LEN {}", format_duration(duration)),
                XP_TEXT_DARK,
                XP_HIGHLIGHT,
            ),
            _ if player.current_track.is_some() => chip("LEN --:--", XP_TEXT_DARK, XP_HIGHLIGHT),
            _ => chip("PRESS ENTER", XP_TEXT_DARK, XP_HIGHLIGHT),
        };
        push_spaced_span(&mut spans, tail_chip);
    }

    if spans.is_empty() {
        push_spaced_span(&mut spans, chip("LIBRARY LANE", XP_TEXT_DARK, XP_PANEL));
        if width >= 42 {
            push_spaced_span(&mut spans, chip("PRESS ENTER", XP_TEXT_DARK, XP_HIGHLIGHT));
        }
    }

    Line::from(spans)
}

fn visualizer_collection() -> &'static str {
    "BARS + WAVES"
}

fn visualizer_preset_index(player: &PlayerState) -> usize {
    if player.current_track.is_some() {
        track_seed(player) as usize % VISUALIZER_PRESETS.len()
    } else {
        1
    }
}

fn visualizer_preset(player: &PlayerState) -> &'static str {
    VISUALIZER_PRESETS[visualizer_preset_index(player)]
}

fn deck_rating(player: &PlayerState) -> usize {
    if player.current_track.is_none() {
        return 0;
    }

    let base = 3 + ((track_seed(player) as usize / 11) % 3);
    match player.status {
        PlaybackStatus::Playing => base.min(5),
        PlaybackStatus::Paused => base.saturating_sub(1).max(2),
        PlaybackStatus::Stopped => 2,
    }
}

fn deck_rating_text(player: &PlayerState) -> String {
    if player.current_track.is_none() {
        return String::from("UNRATED");
    }

    let filled = deck_rating(player).min(5);
    let stars = (0..5)
        .map(|index| if index < filled { '★' } else { '☆' })
        .collect::<String>();
    format!("RATING {stars}")
}

fn now_playing_header_line(player: &PlayerState, width: usize) -> Line<'static> {
    let mut spans = vec![
        chip("NOW PLAYING", XP_TEXT_LIGHT, XP_BLUE_MID),
        Span::raw(" "),
        chip(visualizer_collection(), XP_TEXT_DARK, XP_PANEL),
    ];

    if width >= 40 {
        spans.push(Span::raw(" "));
        spans.push(chip(visualizer_preset(player), XP_TEXT_DARK, XP_HIGHLIGHT));
    }

    if width >= 62 {
        spans.push(Span::raw(" "));
        spans.push(chip(deck_rating_text(player), XP_TEXT_DARK, XP_MINT));
    }

    Line::from(spans)
}

fn visualizer_state_chip(player: &PlayerState) -> Span<'static> {
    match player.status {
        PlaybackStatus::Playing => chip("LIVE", XP_TEXT_DARK, XP_MINT),
        PlaybackStatus::Paused => chip("HOLD", XP_TEXT_DARK, XP_HIGHLIGHT),
        PlaybackStatus::Stopped => chip("IDLE", XP_TEXT_LIGHT, XP_BLUE_DEEP),
    }
}

fn compact_visualizer_header_line(player: &PlayerState, width: usize) -> Line<'static> {
    let mut spans = vec![visualizer_state_chip(player), Span::raw(" ")];
    spans.push(chip(visualizer_preset(player), XP_TEXT_DARK, XP_HIGHLIGHT));

    if width >= 46 {
        spans.push(Span::raw(" "));
        spans.push(chip(visualizer_collection(), XP_TEXT_DARK, XP_PANEL));
    }

    if width >= 62 {
        spans.push(Span::raw(" "));
        spans.push(chip(compact_time_label(player), XP_TEXT_LIGHT, XP_BLUE_MID));
    }

    Line::from(spans)
}

fn compact_visualizer_caption_text(player: &PlayerState) -> String {
    match player.status {
        PlaybackStatus::Playing => {
            format!(
                "crest / undertow / mirror / glow · {}",
                compact_time_label(player)
            )
        }
        PlaybackStatus::Paused => {
            format!(
                "held crest / undertow / mirror · {}",
                compact_time_label(player)
            )
        }
        PlaybackStatus::Stopped => String::from("idle glass / queue a track"),
    }
}

fn compact_visualizer_caption_line(player: &PlayerState, width: usize) -> Line<'static> {
    Line::from(vec![
        chip("WASH", XP_TEXT_DARK, XP_PANEL),
        Span::raw(" "),
        Span::styled(
            fit_text(
                &compact_visualizer_caption_text(player),
                width.saturating_sub(9).max(8),
            ),
            Style::default().fg(XP_SILVER),
        ),
    ])
}

fn compact_visualizer_wave_caption_line(player: &PlayerState, width: usize) -> Line<'static> {
    let gap = if width >= 56 { 3 } else { 2 };
    let caption_width = if width >= 68 {
        34
    } else if width >= 54 {
        28
    } else {
        20
    };
    let wave_width = width.saturating_sub(caption_width + gap).max(12);
    let caption_width = width.saturating_sub(wave_width + gap).max(10);

    let mut spans = make_wave_line(player, wave_width).spans;
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(compact_visualizer_caption_line(player, caption_width).spans);
    Line::from(spans)
}

fn compact_visualizer_lines(
    player: &PlayerState,
    width: usize,
    height: usize,
) -> Vec<Line<'static>> {
    let width = width.max(36);

    match height.max(1) {
        1 => vec![make_wave_line(player, width)],
        2 => vec![
            compact_visualizer_header_line(player, width),
            compact_visualizer_wave_caption_line(player, width),
        ],
        3 => vec![
            compact_visualizer_header_line(player, width),
            make_wave_line(player, width),
            compact_visualizer_caption_line(player, width),
        ],
        4 => vec![
            compact_visualizer_header_line(player, width),
            make_signature_line(player, width),
            make_wave_line(player, width),
            compact_visualizer_caption_line(player, width),
        ],
        _ => {
            let include_glow = height >= 7;
            let effect_rows = if include_glow { 3 } else { 2 };
            let spectrum_rows = height.saturating_sub(2 + effect_rows).max(1);
            let mut lines = vec![compact_visualizer_header_line(player, width)];
            lines.extend(make_spectrum_lines(player, width, spectrum_rows));
            lines.push(make_wave_line(player, width));
            lines.push(make_wave_undertow_line(player, width));
            if include_glow {
                lines.push(make_glow_line(player, width));
            }
            lines.push(compact_visualizer_caption_line(player, width));
            lines
        }
    }
}

fn span_text_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.chars().count()).sum()
}

fn visualizer_footer_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(12);

    if width < 26 {
        return Line::from(vec![Span::styled(
            fit_text(&compact_visualizer_caption_text(player), width),
            Style::default().fg(XP_SILVER),
        )]);
    }

    if width < 46 {
        return Line::from(vec![
            chip("◄", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            chip(
                fit_text(visualizer_preset(player), width.saturating_sub(12).max(6)),
                XP_TEXT_DARK,
                XP_HIGHLIGHT,
            ),
            Span::raw(" "),
            chip("►", XP_TEXT_DARK, XP_PANEL),
        ]);
    }

    let mut spans = vec![
        chip("◄", XP_TEXT_DARK, XP_PANEL),
        Span::raw(" "),
        chip(
            format!(
                "VIS {:02}/{:02}",
                visualizer_preset_index(player) + 1,
                VISUALIZER_PRESETS.len()
            ),
            XP_TEXT_DARK,
            XP_GLASS,
        ),
        Span::raw(" "),
        chip(visualizer_preset(player), XP_TEXT_DARK, XP_HIGHLIGHT),
        Span::raw(" "),
        chip("►", XP_TEXT_DARK, XP_PANEL),
    ];

    if width >= 58 {
        spans.push(Span::raw(" "));
        spans.push(visualizer_state_chip(player));
    }

    let used_width = span_text_width(&spans);
    let remaining = width.saturating_sub(used_width + 1);
    if remaining >= 12 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            fit_text(&compact_visualizer_caption_text(player), remaining),
            Style::default().fg(XP_SILVER),
        ));
    }

    Line::from(spans)
}

fn visualizer_header_line(player: &PlayerState, width: usize) -> Line<'static> {
    let mut spans = vec![visualizer_state_chip(player), Span::raw(" ")];
    spans.push(chip(visualizer_collection(), XP_TEXT_DARK, XP_PANEL));

    if width >= 34 {
        spans.push(Span::raw(" "));
        spans.push(chip(visualizer_preset(player), XP_TEXT_DARK, XP_HIGHLIGHT));
    }

    if width >= 54 {
        spans.push(Span::raw(" "));
        spans.push(chip("REFLECT + GLOW", XP_TEXT_DARK, XP_GLASS));
    }

    Line::from(spans)
}

fn compact_status_chip(app: &App) -> Span<'static> {
    let text = match (app.focus(), app.player().status.clone()) {
        (FocusPane::Browser, PlaybackStatus::Playing) => " LIB • PLAY ",
        (FocusPane::Browser, PlaybackStatus::Paused) => " LIB • PAUSE ",
        (FocusPane::Browser, PlaybackStatus::Stopped) => " LIB • STOP ",
        (FocusPane::Player, PlaybackStatus::Playing) => " DECK • PLAY ",
        (FocusPane::Player, PlaybackStatus::Paused) => " DECK • PAUSE ",
        (FocusPane::Player, PlaybackStatus::Stopped) => " DECK • STOP ",
    };

    Span::styled(
        text,
        Style::default()
            .fg(XP_TEXT_LIGHT)
            .bg(XP_BLUE_DEEP)
            .add_modifier(Modifier::BOLD),
    )
}

fn status_chip_line(app: &App) -> Line<'static> {
    let focus_chip = match app.focus() {
        FocusPane::Browser => chip("LIB", XP_TEXT_DARK, XP_HIGHLIGHT),
        FocusPane::Player => chip("DECK", XP_TEXT_LIGHT, XP_BLUE_MID),
    };
    let state_chip = match app.player().status {
        PlaybackStatus::Playing => chip("PLAY", XP_TEXT_DARK, XP_MINT),
        PlaybackStatus::Paused => chip("PAUSE", XP_TEXT_DARK, XP_HIGHLIGHT),
        PlaybackStatus::Stopped => chip("STOP", XP_TEXT_LIGHT, XP_BLUE_DEEP),
    };
    let stack_text = match (app.player().queue_index, app.player().queue.is_empty()) {
        (Some(index), false) => format!("STACK {}/{}", index + 1, app.player().queue.len()),
        _ => String::from("STACK 0/0"),
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
        chip(stack_text, XP_TEXT_DARK, XP_SILVER),
    ])
}

fn status_transport_text(player: &PlayerState) -> String {
    match &player.current_track {
        Some(track) => format!("{} · {}", track.title, compact_time_label(player)),
        None if !player.queue.is_empty() => {
            format!(
                "Stack loaded · {} tracks waiting in the deck",
                player.queue.len()
            )
        }
        None => String::from("Standby deck — load something from the library lane"),
    }
}

fn status_transport_line(player: &PlayerState, width: usize) -> Line<'static> {
    match &player.current_track {
        Some(track) => Line::from(vec![
            chip("DECK A", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            Span::styled(
                animated_marquee(player, &track.title, width.saturating_sub(18).max(6), 4.4),
                Style::default()
                    .fg(XP_TEXT_DARK)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            chip(compact_time_label(player), XP_TEXT_LIGHT, XP_BLUE_DEEP),
        ]),
        None if !player.queue.is_empty() => Line::from(vec![
            chip("STACK", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            Span::styled(
                animated_marquee(
                    player,
                    &status_transport_text(player),
                    width.saturating_sub(11).max(6),
                    2.8,
                ),
                Style::default().fg(XP_TEXT_DARK),
            ),
        ]),
        None => Line::from(vec![
            chip("STANDBY", XP_TEXT_LIGHT, XP_BLUE_DEEP),
            Span::raw(" "),
            Span::styled(
                animated_marquee(
                    player,
                    &status_transport_text(player),
                    width.saturating_sub(13).max(6),
                    2.4,
                ),
                Style::default().fg(XP_TEXT_DARK),
            ),
        ]),
    }
}

fn queue_summary_line(player: &PlayerState) -> Line<'static> {
    match (player.queue_index, player.queue.is_empty()) {
        (_, true) => Line::from(vec![
            chip("CABINET", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            chip("EMPTY", XP_TEXT_LIGHT, XP_BLUE_DEEP),
            Span::raw(" "),
            chip("LOAD TRACK", XP_TEXT_DARK, XP_HIGHLIGHT),
        ]),
        (Some(index), false) => {
            let remaining = player.queue.len().saturating_sub(index + 1);
            Line::from(vec![
                chip(
                    format!("STACK {}/{}", index + 1, player.queue.len()),
                    XP_TEXT_DARK,
                    XP_PANEL,
                ),
                Span::raw(" "),
                chip("ON AIR", XP_TEXT_LIGHT, XP_BLUE_MID),
                Span::raw(" "),
                chip(format!("NEXT {remaining}"), XP_TEXT_DARK, XP_MINT),
                Span::raw(" "),
                chip(
                    format_duration(player.position),
                    XP_TEXT_LIGHT,
                    XP_BLUE_DEEP,
                ),
            ])
        }
        (None, false) => Line::from(vec![
            chip(
                format!("STACK {}", player.queue.len()),
                XP_TEXT_DARK,
                XP_PANEL,
            ),
            Span::raw(" "),
            chip("STAGED", XP_TEXT_LIGHT, XP_BLUE_DEEP),
            Span::raw(" "),
            chip("PRESS PLAY", XP_TEXT_DARK, XP_HIGHLIGHT),
        ]),
    }
}

fn queue_marquee_text(player: &PlayerState) -> String {
    match &player.current_track {
        Some(track) => {
            let context = now_playing_context_text(player).replace("   ", " · ");
            if context.is_empty() {
                track.title.clone()
            } else {
                format!("{} · {}", track.title, context)
            }
        }
        None if !player.queue.is_empty() => {
            format!(
                "Stack loaded with {} tracks — press play to light the deck.",
                player.queue.len()
            )
        }
        None => String::from("Pick a track from the library lane to seed the playback stack."),
    }
}

fn queue_marquee_line(player: &PlayerState, width: usize) -> Line<'static> {
    let (label, fg, bg) = match (player.current_track.is_some(), player.queue.is_empty()) {
        (true, _) => ("AIR", XP_TEXT_LIGHT, XP_BLUE_MID),
        (false, false) => ("STAGED", XP_TEXT_DARK, XP_HIGHLIGHT),
        (false, true) => ("EMPTY", XP_TEXT_DARK, XP_PANEL),
    };

    Line::from(vec![
        chip(label, fg, bg),
        Span::raw(" "),
        Span::styled(
            animated_marquee(
                player,
                &queue_marquee_text(player),
                width.saturating_sub(label.chars().count() + 7).max(8),
                3.6,
            ),
            Style::default().fg(XP_TEXT_DARK),
        ),
    ])
}

fn queue_lines(player: &PlayerState, width: usize, rows: usize) -> Vec<Line<'static>> {
    if player.queue.is_empty() {
        return vec![Line::from(vec![Span::styled(
            " Cue stack is empty — pick a track from the library lane.",
            Style::default().fg(XP_TEXT_DARK),
        )])];
    }

    let visible_rows = rows.max(1);
    let current_index = player
        .queue_index
        .unwrap_or(0)
        .min(player.queue.len().saturating_sub(1));
    let mut start = current_index.saturating_sub(visible_rows.saturating_sub(1).min(2));
    let mut end = (start + visible_rows).min(player.queue.len());
    if end.saturating_sub(start) < visible_rows {
        start = end.saturating_sub(visible_rows);
        end = (start + visible_rows).min(player.queue.len());
    }
    let width = width.saturating_sub(15).max(8);

    player.queue[start..end]
        .iter()
        .enumerate()
        .map(|(offset, path)| {
            let absolute_index = start + offset;
            let name = queue_track_name(path);
            let (label, style) = if Some(absolute_index) == player.queue_index {
                (
                    " AIR ",
                    Style::default()
                        .fg(XP_TEXT_LIGHT)
                        .bg(XP_BLUE_MID)
                        .add_modifier(Modifier::BOLD),
                )
            } else if absolute_index == current_index.saturating_add(1) {
                (
                    " CUE ",
                    Style::default()
                        .fg(XP_TEXT_DARK)
                        .bg(XP_MINT)
                        .add_modifier(Modifier::BOLD),
                )
            } else if absolute_index < current_index {
                (" HOLD", Style::default().fg(XP_BLUE_DEEP).bg(XP_PANEL))
            } else {
                (" DEEP", Style::default().fg(XP_TEXT_LIGHT).bg(XP_BLUE_DEEP))
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
    let text = match (&player.status, player.queue.is_empty()) {
        (_, true) => "Enter in the library lane seeds the playback stack.",
        (PlaybackStatus::Playing, false) => {
            "n/p glides through the stack · Enter in the library lane reseeds the deck."
        }
        (PlaybackStatus::Paused, false) => {
            "Space relights the deck · Enter in the library lane swaps the stack."
        }
        (PlaybackStatus::Stopped, false) => {
            "Space starts the staged stack · Enter in the library lane rebuilds the deck."
        }
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

fn deck_guide_line(label: &str, fg: Color, bg: Color, text: &str, width: usize) -> Line<'static> {
    Line::from(vec![
        chip(label, fg, bg),
        Span::raw(" "),
        Span::styled(
            fit_text(text, width.saturating_sub(label.chars().count() + 7).max(8)),
            Style::default().fg(XP_TEXT_DARK),
        ),
    ])
}

fn help_lines(width: usize, rows: usize) -> Vec<Line<'static>> {
    let groups: Vec<(&str, Color, Color, &str)> = match rows.max(1).min(4) {
        1 => vec![(
            "FLOW",
            XP_TEXT_DARK,
            XP_PANEL,
            "Enter cue/play · Tab lane↔deck · Space play/pause · j/k browse/vol",
        )],
        2 => vec![
            (
                "TRANSPORT",
                XP_TEXT_DARK,
                XP_HIGHLIGHT,
                "Space play/pause · s stop deck · n/p step stack",
            ),
            (
                "FLOW",
                XP_TEXT_DARK,
                XP_PANEL,
                "Enter cue/play · Tab lane↔deck · j/k browse/vol · h/l seek ribbon",
            ),
        ],
        3 => vec![
            (
                "TRANSPORT",
                XP_TEXT_DARK,
                XP_HIGHLIGHT,
                "Space play/pause · s stop deck · n/p step stack",
            ),
            (
                "BROWSER",
                XP_TEXT_LIGHT,
                XP_BLUE_DEEP,
                "j/k move tree · Enter open/play · ←/→ fold folders",
            ),
            (
                "DECK",
                XP_TEXT_DARK,
                XP_MINT,
                "Tab lane↔deck · j/k volume · +/- trim glass · h/l seek ribbon",
            ),
        ],
        _ => vec![
            (
                "TRANSPORT",
                XP_TEXT_DARK,
                XP_HIGHLIGHT,
                "Space play/pause · s stop deck · n/p step stack",
            ),
            (
                "BROWSER",
                XP_TEXT_LIGHT,
                XP_BLUE_DEEP,
                "j/k move tree · Enter open/play · ←/→ fold folders",
            ),
            (
                "DECK",
                XP_TEXT_DARK,
                XP_MINT,
                "j/k volume · +/- fine tune · h/l seek ribbon",
            ),
            (
                "FOCUS",
                XP_TEXT_DARK,
                XP_PANEL,
                "Tab lane↔deck focus · q exits the deck",
            ),
        ],
    };

    groups
        .into_iter()
        .map(|(label, fg, bg, text)| deck_guide_line(label, fg, bg, text, width))
        .collect()
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
        PlaybackStatus::Playing => "crest and undertow rolling under blue glass",
        PlaybackStatus::Paused => "last crest and undertow held under blue glass",
        PlaybackStatus::Stopped => "blue glass waiting on a cue",
    }
}

fn visualizer_caption(player: &PlayerState) -> String {
    let preset = visualizer_preset(player);
    match player.status {
        PlaybackStatus::Playing => {
            format!(
                "{preset} throws crest / undertow / mirror / blue-glass glow across the signal deck."
            )
        }
        PlaybackStatus::Paused => {
            format!("{preset} holds crest / undertow / mirror / blue-glass glow on the last frame.")
        }
        PlaybackStatus::Stopped => format!(
            "{preset} waits in {} — queue a track to wake the blue-glass undertow and glow.",
            visualizer_collection()
        ),
    }
}

fn album_tile_monogram(player: &PlayerState) -> String {
    let source = player
        .current_track
        .as_ref()
        .map(|track| track.title.clone())
        .or_else(|| track_folder_label(player))
        .unwrap_or_else(|| String::from("Terminal Audio Player"));

    let mut monogram = source
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|part| !part.is_empty())
        .take(2)
        .filter_map(|part| part.chars().next())
        .flat_map(|ch| ch.to_uppercase())
        .collect::<String>();

    if monogram.is_empty() {
        monogram = source
            .chars()
            .filter(|ch| ch.is_alphanumeric())
            .take(2)
            .flat_map(|ch| ch.to_uppercase())
            .collect();
    }

    if monogram.is_empty() {
        String::from("TAP")
    } else {
        monogram.chars().take(3).collect()
    }
}

fn album_tile_meta_text(player: &PlayerState, width: usize) -> String {
    if player.current_track.is_none() {
        return fit_text("queue to cue", width.max(1));
    }

    let mut parts = Vec::new();
    if let Some(codec) = track_codec_label(player) {
        parts.push(codec);
    }
    if width >= 10 {
        parts.push(visualizer_preset(player).to_string());
    }

    if parts.is_empty() {
        parts.push(title_state_text(&player.status).to_string());
    }

    fit_text(&parts.join(" • "), width.max(1))
}

fn album_tile_lines(player: &PlayerState, width: usize, height: usize) -> Vec<Line<'static>> {
    let width = width.max(6);
    let height = height.max(2);
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

    let brand_line = Line::from(vec![Span::styled(
        "MEDIA 9",
        Style::default()
            .fg(XP_TEXT_LIGHT)
            .add_modifier(Modifier::BOLD),
    )]);

    let state_line = Line::from(vec![Span::styled(
        title_state_text(&player.status),
        Style::default().fg(XP_SKY).add_modifier(Modifier::BOLD),
    )]);

    let monogram_line = Line::from(vec![Span::styled(
        fit_text(&album_tile_monogram(player), width),
        Style::default()
            .fg(XP_HIGHLIGHT)
            .add_modifier(Modifier::BOLD),
    )]);

    let signature_line = make_signature_line(player, width.min(10).max(6));

    let meta_line = Line::from(vec![Span::styled(
        album_tile_meta_text(player, width),
        Style::default().fg(XP_SILVER).add_modifier(Modifier::BOLD),
    )]);

    let volume_line = Line::from(vec![Span::styled(
        format!(" {:>3.0}% VOL ", player.volume * 100.0),
        Style::default()
            .fg(badge_fg)
            .bg(badge_bg)
            .add_modifier(Modifier::BOLD),
    )]);

    let time_line = Line::from(vec![Span::styled(
        format!(" {} ", compact_time_label(player)),
        Style::default()
            .fg(XP_TEXT_LIGHT)
            .bg(XP_BLUE_DEEP)
            .add_modifier(Modifier::BOLD),
    )]);

    match height {
        2 => vec![monogram_line, time_line],
        3 => vec![brand_line, monogram_line, time_line],
        4 => vec![brand_line, monogram_line, meta_line, time_line],
        5 => vec![brand_line, state_line, monogram_line, meta_line, time_line],
        6 => vec![
            brand_line,
            state_line,
            monogram_line,
            signature_line,
            meta_line,
            time_line,
        ],
        _ => vec![
            brand_line,
            state_line,
            monogram_line,
            signature_line,
            meta_line,
            volume_line,
            time_line,
        ],
    }
}

fn now_playing_context_line(player: &PlayerState, width: usize) -> Line<'static> {
    Line::from(vec![Span::styled(
        animated_marquee(player, &now_playing_context_text(player), width.max(1), 3.0),
        Style::default().fg(XP_BLUE),
    )])
}

fn now_playing_context_text(player: &PlayerState) -> String {
    match (&player.last_error, player.current_track.as_ref()) {
        (Some(error), _) => format!("signal fault · {error}"),
        (_, Some(_)) => {
            let mut parts = Vec::new();
            if let Some(shelf) = track_shelf_label(player) {
                parts.push(format!("shelf · {shelf}"));
            }
            if let Some(folder) = track_folder_label(player) {
                parts.push(format!("folder · {folder}"));
            }
            if let Some(codec) = track_codec_label(player) {
                parts.push(format!("codec · {codec}"));
            }

            if parts.is_empty() {
                String::from("library selection loaded")
            } else {
                parts.join("   ")
            }
        }
        _ => String::from("select a track to wake the blue-glass deck"),
    }
}

fn mini_meter_line(label: &str, ratio: f64, width: usize, active_color: Color) -> Line<'static> {
    let ratio = ratio.clamp(0.0, 1.0);
    let bar_width = width.max(8).saturating_sub(label.len() + 1).max(4);
    let filled = (ratio * bar_width as f64).round() as usize;

    let mut spans = vec![Span::styled(
        format!("{label} "),
        Style::default().fg(XP_SILVER).add_modifier(Modifier::BOLD),
    )];

    for index in 0..bar_width {
        let (glyph, color) = if index < filled {
            ('▮', active_color)
        } else {
            ('▯', XP_PANEL_DARK)
        };
        spans.push(Span::styled(glyph.to_string(), Style::default().fg(color)));
    }

    Line::from(spans)
}

fn visualizer_meter_lines(player: &PlayerState, width: usize, rows: usize) -> Vec<Line<'static>> {
    let progress = progress_info(player).0;
    let energy = playback_energy(player);
    let queue_ratio = match (player.queue_index, player.queue.len()) {
        (Some(index), len) if len > 0 => (index + 1) as f64 / len as f64,
        _ => 0.0,
    };
    let pulse = match player.status {
        PlaybackStatus::Stopped => 0.14,
        PlaybackStatus::Paused => {
            ((track_seed(player) as f64 * 0.0007).sin() * 0.25 + 0.45).clamp(0.0, 1.0)
        }
        PlaybackStatus::Playing => {
            ((player.position.as_secs_f64() * 3.2 + track_seed(player) as f64 * 0.0007).sin() * 0.5
                + 0.5)
                .clamp(0.0, 1.0)
        }
    };
    let position_ratio = if progress > 0.0 {
        progress
    } else if player.status == PlaybackStatus::Stopped {
        0.0
    } else {
        pulse * 0.65
    };

    let meters = [
        (
            "BARS",
            (0.18 + player.volume as f64 * 0.36 + pulse * 0.28).clamp(0.0, 1.0),
            XP_MINT,
        ),
        ("WAVE", energy, XP_HIGHLIGHT),
        (
            "AIR",
            (0.24 + progress * 0.28 + pulse * 0.36).clamp(0.0, 1.0),
            XP_GLASS,
        ),
        ("POS", position_ratio.max(queue_ratio * 0.2), XP_SKY),
    ];

    meters
        .into_iter()
        .take(rows.max(1))
        .map(|(label, ratio, color)| mini_meter_line(label, ratio, width, color))
        .collect()
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

fn transport_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(24);
    let center_label = match player.status {
        PlaybackStatus::Playing => "PAUSE",
        PlaybackStatus::Paused | PlaybackStatus::Stopped => "PLAY",
    };
    let center_bg = match player.status {
        PlaybackStatus::Playing => XP_HIGHLIGHT,
        PlaybackStatus::Paused | PlaybackStatus::Stopped => XP_MINT,
    };
    let time_chip = chip(compact_time_label(player), XP_TEXT_LIGHT, XP_BLUE_DEEP);

    if width >= 58 {
        return Line::from(vec![
            chip("PREV", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            chip(center_label, XP_TEXT_DARK, center_bg),
            Span::raw(" "),
            chip("STOP", XP_TEXT_LIGHT, XP_BLUE_DEEP),
            Span::raw(" "),
            chip("NEXT", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            time_chip,
        ]);
    }

    if width >= 40 {
        return Line::from(vec![
            chip("PREV", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            chip(center_label, XP_TEXT_DARK, center_bg),
            Span::raw(" "),
            chip("NEXT", XP_TEXT_DARK, XP_PANEL),
            Span::raw(" "),
            time_chip,
        ]);
    }

    Line::from(vec![
        chip(center_label, XP_TEXT_DARK, center_bg),
        Span::raw(" "),
        time_chip,
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

fn animated_marquee(player: &PlayerState, text: &str, width: usize, speed: f64) -> String {
    if width == 0 {
        return String::new();
    }

    let char_count = text.chars().count();
    if char_count <= width {
        return text.to_string();
    }

    let separator = "   ✦   ";
    let cycle_text = format!("{text}{separator}{text}");
    let cycle_chars = cycle_text.chars().collect::<Vec<_>>();
    let cycle_len = text.chars().count() + separator.chars().count();
    let offset = marquee_offset(player, cycle_len, speed);

    cycle_chars
        .into_iter()
        .cycle()
        .skip(offset)
        .take(width)
        .collect()
}

fn marquee_offset(player: &PlayerState, cycle_len: usize, speed: f64) -> usize {
    if cycle_len == 0 {
        return 0;
    }

    match player.status {
        PlaybackStatus::Stopped => 0,
        PlaybackStatus::Paused => (track_seed(player) as usize) % cycle_len,
        PlaybackStatus::Playing => {
            (((player.position.as_secs_f64() * speed).floor() as usize)
                + (track_seed(player) as usize % cycle_len))
                % cycle_len
        }
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

    let raw_heights = (0..width)
        .map(|index| {
            let x = index as f64 / width as f64;
            let wave_a = (x * 13.0 + phase + seed).sin();
            let wave_b = (x * 25.0 - phase * 0.7 + seed * 0.4).cos() * 0.55;
            let wave_c = ((index as f64 * 0.33) + phase * 1.3 + seed).sin() * 0.2;
            let pulse = ((phase * 0.75 + x * 4.0).sin() * 0.5 + 0.5) * 0.18;
            let bass_bias = (1.0 - x.powf(0.7)) * 0.12;
            let block_bias = if index % 2 == 0 { 0.04 } else { 0.0 };
            let normalized = (((wave_a + wave_b + wave_c) * 0.5) + 0.5).clamp(0.0, 1.0);
            let boosted = (normalized * 0.62
                + energy * 0.18
                + pulse
                + progress * 0.08
                + bass_bias
                + block_bias)
                .clamp(0.0, 1.0);
            ((boosted * rows as f64).round() as usize).min(rows)
        })
        .collect::<Vec<_>>();

    let heights = raw_heights
        .iter()
        .enumerate()
        .map(|(index, current)| {
            let left = if index > 0 {
                raw_heights[index - 1]
            } else {
                *current
            };
            let right = if index + 1 < raw_heights.len() {
                raw_heights[index + 1]
            } else {
                *current
            };
            let far_left = if index > 1 {
                raw_heights[index - 2]
            } else {
                left
            };
            let far_right = if index + 2 < raw_heights.len() {
                raw_heights[index + 2]
            } else {
                right
            };
            let blended = ((*current as f64 * 0.46)
                + (left as f64 * 0.2)
                + (right as f64 * 0.2)
                + (far_left as f64 * 0.07)
                + (far_right as f64 * 0.07))
                .round() as usize;
            blended.clamp(0, rows)
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
                    let left = if index > 0 {
                        heights[index - 1]
                    } else {
                        *height
                    };
                    let right = if index + 1 < heights.len() {
                        heights[index + 1]
                    } else {
                        *height
                    };
                    let crest = *height == level;
                    let shoulder = left < level || right < level;

                    if *height >= level {
                        let (ch, tint) = if crest && shoulder {
                            match rows - level {
                                0 => ('▀', XP_HIGHLIGHT),
                                1 => ('▆', XP_MINT),
                                _ => ('▅', color),
                            }
                        } else if crest {
                            match rows - level {
                                0 => ('▇', XP_HIGHLIGHT),
                                1 => ('▇', XP_MINT),
                                _ => ('▇', color),
                            }
                        } else if shoulder && rows - level <= 1 {
                            ('▇', color)
                        } else {
                            ('█', color)
                        };

                        Span::styled(ch.to_string(), Style::default().fg(tint))
                    } else if *height + 1 == level && (left >= level || right >= level) {
                        let ghost = if rows - level <= 1 { '▪' } else { '·' };
                        Span::styled(ghost.to_string(), Style::default().fg(XP_PANEL_DARK))
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

fn make_wave_undertow_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(12);
    let seed = track_seed(player) as f64 * 0.009;
    let energy = playback_energy(player) * 0.88;
    let progress = progress_info(player).0;
    let phase = match player.status {
        PlaybackStatus::Stopped => seed * 0.4,
        PlaybackStatus::Paused => player.position.as_secs_f64() * 1.8 + seed * 0.4,
        PlaybackStatus::Playing => player.position.as_secs_f64() * (4.1 + progress * 1.4) + seed,
    };

    let chars = ['·', '▪', '▫', '▁', '▂', '▃', '▄'];
    let mut spans = Vec::with_capacity(width);

    for index in 0..width {
        let x = index as f64 / width as f64;
        let swell = (x * 8.0 + phase * 0.92 + seed * 0.6).sin() * 0.46;
        let undertow = (x * 18.0 - phase * 0.48 + seed).cos() * 0.24;
        let drift = (progress * 5.0 + x * 2.4 + seed * 0.3).sin() * 0.18;
        let bass_bias = (1.0 - x.powf(0.72)) * 0.1;
        let normalized = ((((swell + undertow + drift) * energy) + bass_bias) + 1.0) / 2.0;
        let normalized = normalized.clamp(0.0, 1.0);
        let char_index = (normalized * (chars.len() - 1) as f64).round() as usize;
        let color = if normalized > 0.78 {
            XP_SILVER
        } else if normalized > 0.54 {
            XP_GLASS
        } else if normalized > 0.32 {
            XP_SKY
        } else {
            XP_PANEL_DARK
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
        let shimmer = ((index as f64 * 0.45) + player.position.as_secs_f64() * 3.1).sin();
        let (ch, color) = if distance == 0 {
            ('◉', XP_HIGHLIGHT)
        } else if distance <= 2 {
            ('•', XP_SILVER)
        } else if distance <= 5 {
            ('·', XP_PANEL_DARK)
        } else if shimmer > 0.72 {
            ('·', XP_GLASS)
        } else {
            ('·', XP_BLUE_MID)
        };
        spans.push(Span::styled(ch.to_string(), Style::default().fg(color)));
    }

    Line::from(spans)
}

fn make_reflection_line(player: &PlayerState, width: usize) -> Line<'static> {
    let width = width.max(12);
    let seed = track_seed(player) as f64 * 0.017;
    let energy = playback_energy(player);
    let progress = progress_info(player).0;
    let phase = match player.status {
        PlaybackStatus::Stopped => seed * 0.15,
        PlaybackStatus::Paused => player.position.as_secs_f64() * 1.6 + seed,
        PlaybackStatus::Playing => player.position.as_secs_f64() * (3.4 + progress * 1.2) + seed,
    };

    let chars = [' ', '.', '·', '▪', '▫', '◦', '•'];
    let mut spans = Vec::with_capacity(width);
    for index in 0..width {
        let x = index as f64 / width as f64;
        let ripple = (x * 9.0 + phase).sin() * 0.42
            + (x * 18.0 - phase * 0.55).cos() * 0.25
            + (x * 31.0 + seed * 0.8).sin() * 0.11;
        let normalized = (((ripple * energy * 0.9) + 1.0) / 2.0).clamp(0.0, 1.0);
        let char_index = (normalized * (chars.len() - 1) as f64).round() as usize;
        let color = if normalized > 0.78 {
            XP_SILVER
        } else if normalized > 0.56 {
            XP_GLASS
        } else if normalized > 0.34 {
            XP_SKY
        } else {
            XP_PANEL_DARK
        };
        spans.push(Span::styled(
            chars[char_index].to_string(),
            Style::default().fg(color),
        ));
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::mpsc;

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use tempfile::tempdir;

    use super::*;
    use crate::audio_engine::{EngineEvent, Track};

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

    fn render_snapshot(width: u16, height: u16, app: &App) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| draw(frame, app)).unwrap();

        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                let mut line = String::new();
                for x in 0..width {
                    line.push_str(buffer[(x, y)].symbol());
                }
                line.trim_end().to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn render_player_focus_snapshot(width: u16, height: u16) -> String {
        let temp = tempdir().unwrap();
        let (command_tx, _command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let mut app = App::new(temp.path().to_path_buf(), command_tx, event_rx).unwrap();

        event_tx
            .send(EngineEvent::StateUpdated(sample_player()))
            .unwrap();
        app.drain_engine_events();
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();

        render_snapshot(width, height, &app)
    }

    #[test]
    fn fit_text_adds_ellipsis_when_needed() {
        assert_eq!(fit_text("abcdef", 4), "abc…");
        assert_eq!(fit_text("ok", 8), "ok");
    }

    #[test]
    fn animated_marquee_leaves_short_text_untouched() {
        assert_eq!(
            animated_marquee(&sample_player(), "short", 12, 4.0),
            "short"
        );
    }

    #[test]
    fn animated_marquee_advances_for_playing_tracks() {
        let mut player = sample_player();
        player.position = Duration::from_secs(0);
        let first = animated_marquee(&player, "Ocean Avenue After Midnight", 10, 4.0);
        player.position = Duration::from_secs(3);
        let later = animated_marquee(&player, "Ocean Avenue After Midnight", 10, 4.0);

        assert_eq!(first.chars().count(), 10);
        assert_eq!(later.chars().count(), 10);
        assert_ne!(first, later);
    }

    #[test]
    fn title_state_text_matches_playback_status() {
        assert_eq!(title_state_text(&PlaybackStatus::Playing), "ON AIR");
        assert_eq!(title_state_text(&PlaybackStatus::Paused), "HELD");
        assert_eq!(title_state_text(&PlaybackStatus::Stopped), "READY");
    }

    #[test]
    fn tall_wide_title_bar_surfaces_mode_tabs_and_window_controls() {
        let temp = tempdir().unwrap();
        let (command_tx, _command_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let app = App::new(temp.path().to_path_buf(), command_tx, event_rx).unwrap();

        let screen = render_snapshot(120, 30, &app);
        assert!(screen.contains("NOW PLAYING"));
        assert!(screen.contains("MEDIA LIBRARY"));
        assert!(screen.contains("×"));
    }

    #[test]
    fn spectrum_lines_match_requested_geometry() {
        let lines = make_spectrum_lines(&sample_player(), 18, 4);
        assert_eq!(lines.len(), 4);
        assert!(lines.iter().all(|line| line.spans.len() == 18));
    }

    #[test]
    fn spectrum_lines_include_soft_crest_edges() {
        let lines = make_spectrum_lines(&sample_player(), 18, 4);
        let text = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(text.contains('▀') || text.contains('▆') || text.contains('▅'));
    }

    #[test]
    fn reflection_line_enforces_minimum_width() {
        let line = make_reflection_line(&sample_player(), 4);
        assert_eq!(line.spans.len(), 12);
    }

    #[test]
    fn wave_line_enforces_minimum_width() {
        let line = make_wave_line(&sample_player(), 4);
        assert_eq!(line.spans.len(), 12);
    }

    #[test]
    fn wave_undertow_line_enforces_minimum_width() {
        let line = make_wave_undertow_line(&sample_player(), 4);
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
    fn browser_action_hint_for_track_mentions_the_deck() {
        let entry = BrowserEntry {
            path: PathBuf::from("music/song.mp3"),
            name: String::from("song.mp3"),
            depth: 1,
            kind: EntryKind::File,
            expanded: false,
        };

        assert_eq!(
            browser_action_hint(Some(&entry)),
            "Enter loads this track into the deck"
        );
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

    #[test]
    fn browser_root_label_prefers_tail_on_tight_widths() {
        let root = Path::new("/Users/lucw/.openclaw/workspace/terminal-audio-player");
        assert_eq!(browser_root_label(root, 16), "terminal-audio-…");
        assert_eq!(browser_root_label(root, 28), "workspace/terminal-audio-pl…");
    }

    #[test]
    fn compact_track_context_prefers_parent_folder() {
        let mut player = sample_player();
        player.current_track = Some(Track {
            path: PathBuf::from("albums/night-drive/ocean-avenue.wav"),
            title: String::from("Ocean Avenue After Midnight"),
        });

        assert_eq!(compact_track_context(&player), "folder · night-drive");
    }

    #[test]
    fn now_playing_context_text_uses_shelf_folder_and_codec() {
        let mut player = sample_player();
        player.current_track = Some(Track {
            path: PathBuf::from("albums/night-drive/ocean-avenue.flac"),
            title: String::from("Ocean Avenue After Midnight"),
        });

        assert_eq!(
            now_playing_context_text(&player),
            "shelf · albums   folder · night-drive   codec · FLAC"
        );
    }

    #[test]
    fn deck_source_line_surfaces_track_origin_and_length() {
        let mut player = sample_player();
        player.current_track = Some(Track {
            path: PathBuf::from("albums/night-drive/ocean-avenue.flac"),
            title: String::from("Ocean Avenue After Midnight"),
        });

        let line = deck_source_line(&player, 80);
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains("SHELF albums"))
        );
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains("FOLDER night-drive"))
        );
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains("CODEC FLAC"))
        );
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains("LEN 03:32"))
        );
    }

    #[test]
    fn album_tile_lines_include_state_meta_and_time_badges() {
        let lines = album_tile_lines(&sample_player(), 14, 6);
        assert!(lines[1].spans[0].content.contains("ON AIR"));
        assert!(lines[2].spans[0].content.contains("OA"));
        assert!(lines[4].spans[0].content.contains("WAV"));
        assert!(lines[5].spans[0].content.contains("01:13 / 03:32"));
    }

    #[test]
    fn album_tile_lines_expand_with_available_height() {
        let short = album_tile_lines(&sample_player(), 14, 3);
        let tall = album_tile_lines(&sample_player(), 14, 7);

        assert_eq!(short.len(), 3);
        assert_eq!(tall.len(), 7);
        assert!(tall[5].spans[0].content.contains("VOL"));
    }

    #[test]
    fn now_playing_header_line_surfaces_collection_preset_and_rating() {
        let line = now_playing_header_line(&sample_player(), 80);
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains("NOW PLAYING"))
        );
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains(visualizer_collection()))
        );
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains(visualizer_preset(&sample_player())))
        );
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains("RATING"))
        );
    }

    #[test]
    fn wide_short_layout_keeps_now_playing_header_visible() {
        let temp = tempdir().unwrap();
        let (command_tx, _command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let mut app = App::new(temp.path().to_path_buf(), command_tx, event_rx).unwrap();

        event_tx
            .send(EngineEvent::StateUpdated(sample_player()))
            .unwrap();
        app.drain_engine_events();
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();

        let screen = render_snapshot(120, 20, &app);
        assert!(screen.contains("Playback Deck"));
        assert!(screen.contains("NOW PLAYING"));
        assert!(screen.contains("Signal Deck"));
    }

    #[test]
    fn compact_visualizer_lines_add_header_and_wash_caption() {
        let lines = compact_visualizer_lines(&sample_player(), 64, 3);
        assert_eq!(lines.len(), 3);
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|span| span.content.contains("LIVE"))
        );
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|span| span.content.contains(visualizer_preset(&sample_player())))
        );
        assert!(
            lines[2]
                .spans
                .iter()
                .any(|span| span.content.contains("WASH"))
        );
        assert!(
            lines[2]
                .spans
                .iter()
                .any(|span| span.content.contains("crest / undertow / mirror / glow"))
        );
    }

    #[test]
    fn signal_rail_lines_fill_requested_height() {
        let lines = signal_rail_lines(&sample_player(), 20, 6);
        assert_eq!(lines.len(), 6);
        assert!(
            lines[0]
                .spans
                .iter()
                .any(|span| span.content.contains("LEVEL RAIL"))
        );
        assert!(
            lines
                .last()
                .unwrap()
                .spans
                .iter()
                .any(|span| span.content.contains(visualizer_preset(&sample_player())))
        );
    }

    #[test]
    fn compact_deck_guide_lines_keep_lane_deck_and_focus_controls_visible() {
        let lines = help_lines(120, 2);
        assert_eq!(lines.len(), 2);

        let text = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(text.contains("Enter cue/play"));
        assert!(text.contains("h/l seek"));
        assert!(text.contains("lane↔deck"));
    }

    #[test]
    fn wide_short_layout_signal_deck_shows_compact_wash_caption() {
        let temp = tempdir().unwrap();
        let (command_tx, _command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let mut app = App::new(temp.path().to_path_buf(), command_tx, event_rx).unwrap();

        event_tx
            .send(EngineEvent::StateUpdated(sample_player()))
            .unwrap();
        app.drain_engine_events();
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();

        let screen = render_snapshot(120, 20, &app);
        assert!(screen.contains("WASH"));
        assert!(screen.contains("undertow"));
    }

    #[test]
    fn wide_midheight_signal_deck_uses_level_rail_bridge() {
        let screen = render_player_focus_snapshot(120, 24);
        assert!(screen.contains("LEVEL RAIL"));
        assert!(!screen.contains("Signal Ladder"));
    }

    #[test]
    fn wide_queue_reentry_layout_preserves_the_bridge_band() {
        assert_eq!(wide_queue_reentry_layout(19), None);
        assert_eq!(wide_queue_reentry_layout(20), Some((5, 8, 3, 4)));
        assert_eq!(wide_queue_reentry_layout(21), Some((6, 8, 3, 4)));
        assert_eq!(wide_queue_reentry_layout(22), Some((6, 8, 3, 5)));
        assert_eq!(wide_queue_reentry_layout(23), Some((7, 8, 3, 5)));
        assert_eq!(wide_queue_reentry_layout(24), Some((8, 8, 3, 5)));
        assert_eq!(wide_queue_reentry_layout(25), Some((8, 8, 3, 6)));
        assert_eq!(wide_queue_reentry_layout(26), None);
    }

    #[test]
    fn wide_deck_guide_reentry_layout_preserves_the_bridge_band() {
        assert_eq!(wide_deck_guide_reentry_layout(25), None);
        assert_eq!(wide_deck_guide_reentry_layout(26), Some((7, 8, 3, 4, 4)));
        assert_eq!(wide_deck_guide_reentry_layout(27), Some((7, 8, 3, 4, 5)));
        assert_eq!(wide_deck_guide_reentry_layout(28), Some((8, 8, 3, 4, 5)));
        assert_eq!(wide_deck_guide_reentry_layout(29), Some((8, 8, 3, 4, 6)));
        assert_eq!(wide_deck_guide_reentry_layout(30), Some((8, 9, 3, 4, 6)));
        assert_eq!(wide_deck_guide_reentry_layout(31), None);
    }

    #[test]
    fn wide_queue_reentry_keeps_level_rail_when_cue_stack_returns() {
        let screen = render_player_focus_snapshot(120, 26);
        assert!(screen.contains("Cue Stack"));
        assert!(screen.contains("LEVEL RAIL"));
        assert!(screen.contains("STACK 2/3"));
        assert!(screen.contains("AIR   2."));
        assert!(!screen.contains("Signal Ladder"));
    }

    #[test]
    fn wide_deck_guide_reentry_keeps_level_rail_when_five_band_layout_returns() {
        let screen = render_player_focus_snapshot(120, 34);
        assert!(screen.contains("Deck Guide"));
        assert!(screen.contains("Cue Stack"));
        assert!(screen.contains("LEVEL RAIL"));
        assert!(screen.contains("Enter cue/play"));
        assert!(screen.contains("lane↔deck"));
        assert!(!screen.contains("Signal Ladder"));
    }

    #[test]
    fn wide_first_tall_signal_band_uses_signal_rise() {
        let screen = render_player_focus_snapshot(120, 25);
        assert!(screen.contains("Signal Rise"));
        assert!(!screen.contains("LEVEL RAIL"));
        assert!(!screen.contains("Signal Ladder"));
    }

    #[test]
    fn wide_taller_signal_deck_graduates_to_signal_ladder() {
        let screen = render_player_focus_snapshot(120, 40);
        assert!(screen.contains("Signal Ladder"));
        assert!(!screen.contains("Signal Rise"));
        assert!(!screen.contains("LEVEL RAIL"));
    }

    #[test]
    fn visualizer_header_line_mentions_current_preset() {
        let line = visualizer_header_line(&sample_player(), 60);
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains(visualizer_collection()))
        );
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains(visualizer_preset(&sample_player())))
        );
        assert!(
            line.spans
                .iter()
                .any(|span| span.content.contains("REFLECT + GLOW"))
        );
    }

    #[test]
    fn visualizer_footer_line_adds_preset_controls_and_caption() {
        let line = visualizer_footer_line(&sample_player(), 90);
        let text = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(text.contains("◄"));
        assert!(text.contains("VIS"));
        assert!(text.contains(visualizer_preset(&sample_player())));
        assert!(text.contains("crest / undertow / mirror / glow"));
    }

    #[test]
    fn visualizer_caption_references_selected_preset() {
        let caption = visualizer_caption(&sample_player());
        assert!(caption.contains(visualizer_preset(&sample_player())));
        assert!(caption.contains("undertow"));
        assert!(caption.contains("blue-glass glow"));
    }

    #[test]
    fn visualizer_meter_lines_match_requested_height() {
        let lines = visualizer_meter_lines(&sample_player(), 14, 3);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].spans[0].content.contains("BARS"));
        assert!(lines[1].spans[0].content.contains("WAVE"));
        assert!(lines[2].spans[0].content.contains("AIR"));
    }

    #[test]
    fn compact_now_playing_lines_expand_with_available_height() {
        let mut player = sample_player();
        player.current_track = Some(Track {
            path: PathBuf::from("albums/night-drive/ocean-avenue.wav"),
            title: String::from("Ocean Avenue After Midnight"),
        });

        let short = compact_now_playing_lines(&player, 40, 2);
        let taller = compact_now_playing_lines(&player, 40, 4);

        assert_eq!(short.len(), 2);
        assert_eq!(taller.len(), 3);
        assert!(
            short[1]
                .spans
                .iter()
                .any(|span| span.content.contains("01:13 / 03:32"))
        );
        assert!(
            taller[1]
                .spans
                .iter()
                .any(|span| span.content.contains("folder · night-drive"))
        );
    }

    #[test]
    fn queue_marquee_text_blends_title_and_context() {
        let mut player = sample_player();
        player.current_track = Some(Track {
            path: PathBuf::from("albums/night-drive/ocean-avenue.flac"),
            title: String::from("Ocean Avenue After Midnight"),
        });

        let text = queue_marquee_text(&player);
        assert!(text.contains("Ocean Avenue After Midnight"));
        assert!(text.contains("night-drive"));
        assert!(text.contains("FLAC"));
    }

    #[test]
    fn status_transport_text_reports_staged_queue() {
        let mut player = sample_player();
        player.current_track = None;
        player.queue_index = None;

        assert_eq!(
            status_transport_text(&player),
            "Stack loaded · 3 tracks waiting in the deck"
        );
    }

    #[test]
    fn transport_line_expands_into_a_transport_cluster_when_space_allows() {
        let line = transport_line(&sample_player(), 80);
        let text = line
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();

        assert!(text.contains("PREV"));
        assert!(text.contains("NEXT"));
        assert!(text.contains("STOP"));
        assert!(text.contains("01:13 / 03:32"));
    }

    #[test]
    fn queue_footer_line_empty_mentions_library_lane() {
        let player = PlayerState::default();
        let footer = queue_footer_line(&player, 80);
        assert!(footer.spans[0].content.contains("library lane"));
    }

    #[test]
    fn compact_time_label_shows_placeholder_when_nothing_is_loaded() {
        let player = PlayerState::default();
        assert_eq!(compact_time_label(&player), "--:--");
    }
}
