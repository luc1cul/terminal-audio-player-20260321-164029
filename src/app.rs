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
                "Tab switches focus · Enter opens/plays · n/p next/prev · s stop · q quit",
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
                        String::from("Browser focus: j/k navigate · Enter opens or plays")
                    }
                    FocusPane::Player => {
                        String::from("Player focus: j/k volume down/up · Space toggles playback")
                    }
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
            KeyCode::Char(' ') => self.send(AudioCommand::TogglePause),
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
