use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};

use anyhow::Context;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::audio_engine::{AudioCommand, EngineEvent, PlayerState};
use crate::browser::{EntryKind, FileBrowser};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPane {
    Browser,
    Player,
}

pub struct App {
    browser: FileBrowser,
    player: PlayerState,
    focus: FocusPane,
    status_line: String,
    command_tx: Sender<AudioCommand>,
    event_rx: Receiver<EngineEvent>,
    should_quit: bool,
}

impl App {
    pub fn new(
        root: PathBuf,
        command_tx: Sender<AudioCommand>,
        event_rx: Receiver<EngineEvent>,
    ) -> anyhow::Result<Self> {
        let browser = FileBrowser::new(root).context("failed to initialize file browser")?;

        Ok(Self {
            browser,
            player: PlayerState::default(),
            focus: FocusPane::Browser,
            status_line: String::from(
                "Library lane ready · Enter loads · Tab swaps lanes · n/p step the stack · q quits",
            ),
            command_tx,
            event_rx,
            should_quit: false,
        })
    }

    pub fn browser(&self) -> &FileBrowser {
        &self.browser
    }

    pub fn player(&self) -> &PlayerState {
        &self.player
    }

    pub fn focus(&self) -> FocusPane {
        self.focus
    }

    pub fn status_line(&self) -> &str {
        &self.status_line
    }

    pub fn should_quit(&self) -> bool {
        self.should_quit
    }

    pub fn shutdown(&self) {
        let _ = self.command_tx.send(AudioCommand::Shutdown);
    }

    pub fn drain_engine_events(&mut self) {
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                EngineEvent::StateUpdated(player_state) => {
                    self.player = player_state;
                }
                EngineEvent::Error(message) => {
                    self.status_line = message;
                }
            }
        }
    }

    pub fn on_key(&mut self, key: KeyEvent) -> anyhow::Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    FocusPane::Browser => FocusPane::Player,
                    FocusPane::Player => FocusPane::Browser,
                };
                self.status_line = match self.focus {
                    FocusPane::Browser => {
                        String::from("Library lane active · j/k browse · Enter loads or opens")
                    }
                    FocusPane::Player => String::from(
                        "Playback deck active · j/k trims volume · Space toggles transport",
                    ),
                };
            }
            KeyCode::Enter => self.activate_selected()?,
            KeyCode::Left => {
                self.browser.collapse_selected_directory_or_parent()?;
            }
            KeyCode::Right => {
                self.browser.expand_selected_directory()?;
            }
            KeyCode::Up => self.browser.move_up(),
            KeyCode::Down => self.browser.move_down(),
            KeyCode::Char('j') => match self.focus {
                FocusPane::Browser => self.browser.move_down(),
                FocusPane::Player => self.send(AudioCommand::AdjustVolume(-0.05)),
            },
            KeyCode::Char('k') => match self.focus {
                FocusPane::Browser => self.browser.move_up(),
                FocusPane::Player => self.send(AudioCommand::AdjustVolume(0.05)),
            },
            KeyCode::Char(' ') => self.toggle_pause_or_play_selected(),
            KeyCode::Char('s') => self.send(AudioCommand::Stop),
            KeyCode::Char('n') => self.send(AudioCommand::Next),
            KeyCode::Char('p') => self.send(AudioCommand::Previous),
            KeyCode::Char('h') => self.send(AudioCommand::SeekBy(-5)),
            KeyCode::Char('l') => self.send(AudioCommand::SeekBy(5)),
            KeyCode::Char('+') | KeyCode::Char('=') => self.send(AudioCommand::AdjustVolume(0.05)),
            KeyCode::Char('-') => self.send(AudioCommand::AdjustVolume(-0.05)),
            _ => {}
        }

        Ok(())
    }

    fn toggle_pause_or_play_selected(&mut self) {
        if self.player.current_track.is_none()
            && let Some((path, playlist, index)) = self.browser.selected_audio_selection()
        {
            self.send(AudioCommand::LoadAndPlay {
                path,
                playlist,
                index,
            });
            self.status_line =
                String::from("Playing selection · Space pause/resume · h/l seek · n/p next/prev");
            return;
        }

        self.send(AudioCommand::TogglePause);
    }

    fn activate_selected(&mut self) -> anyhow::Result<()> {
        let Some(entry) = self.browser.selected_entry().cloned() else {
            return Ok(());
        };

        match entry.kind {
            EntryKind::Directory => {
                self.browser.toggle_selected_directory()?;
            }
            EntryKind::File => {
                if let Some((path, playlist, index)) = self.browser.selected_audio_selection() {
                    self.send(AudioCommand::LoadAndPlay {
                        path,
                        playlist,
                        index,
                    });
                    self.status_line = String::from(
                        "Playing selection · Space pause/resume · h/l seek · n/p next/prev",
                    );
                }
            }
        }

        Ok(())
    }

    fn send(&mut self, command: AudioCommand) {
        if let Err(error) = self.command_tx.send(command) {
            self.status_line = format!("audio engine disconnected: {error}");
            self.should_quit = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use crossterm::event::{KeyEvent, KeyModifiers};
    use tempfile::tempdir;

    use super::*;
    use crate::audio_engine::Track;

    #[test]
    fn space_starts_selected_track_when_nothing_is_loaded() {
        let temp = tempdir().unwrap();
        let track_path = temp.path().join("song.wav");
        std::fs::write(&track_path, b"stub").unwrap();
        let expected_path = std::fs::canonicalize(&track_path).unwrap();

        let (command_tx, command_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let mut app = App::new(temp.path().to_path_buf(), command_tx, event_rx).unwrap();

        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
            .unwrap();

        match command_rx.recv().unwrap() {
            AudioCommand::LoadAndPlay {
                path,
                playlist,
                index,
            } => {
                assert_eq!(path, expected_path);
                assert_eq!(playlist, vec![expected_path.clone()]);
                assert_eq!(index, 0);
            }
            other => panic!("expected LoadAndPlay, got {other:?}"),
        }
    }

    #[test]
    fn space_toggles_pause_when_track_is_already_loaded() {
        let temp = tempdir().unwrap();
        let track_path = temp.path().join("song.wav");
        std::fs::write(&track_path, b"stub").unwrap();

        let (command_tx, command_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let mut app = App::new(temp.path().to_path_buf(), command_tx, event_rx).unwrap();
        app.player.current_track = Some(Track {
            path: track_path,
            title: String::from("song.wav"),
        });

        app.on_key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE))
            .unwrap();

        match command_rx.recv().unwrap() {
            AudioCommand::TogglePause => {}
            other => panic!("expected TogglePause, got {other:?}"),
        }
    }

    #[test]
    fn tab_updates_status_line_to_lane_language() {
        let temp = tempdir().unwrap();
        let (command_tx, _command_rx) = mpsc::channel();
        let (_event_tx, event_rx) = mpsc::channel();
        let mut app = App::new(temp.path().to_path_buf(), command_tx, event_rx).unwrap();

        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(
            app.status_line(),
            "Playback deck active · j/k trims volume · Space toggles transport"
        );

        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE))
            .unwrap();
        assert_eq!(
            app.status_line(),
            "Library lane active · j/k browse · Enter loads or opens"
        );
    }
}
