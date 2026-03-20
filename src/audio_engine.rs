use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};
use thiserror::Error;

const DEFAULT_VOLUME: f32 = 0.8;
const ENGINE_TICK_MS: u64 = 150;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaybackStatus {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
}

impl Track {
    fn from_path(path: &Path) -> Self {
        let title = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        Self {
            path: path.to_path_buf(),
            title,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerState {
    pub status: PlaybackStatus,
    pub current_track: Option<Track>,
    pub volume: f32,
    pub position: Duration,
    pub duration: Option<Duration>,
    pub queue: Vec<PathBuf>,
    pub queue_index: Option<usize>,
    pub last_error: Option<String>,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            status: PlaybackStatus::Stopped,
            current_track: None,
            volume: DEFAULT_VOLUME,
            position: Duration::ZERO,
            duration: None,
            queue: Vec::new(),
            queue_index: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone)]
pub enum AudioCommand {
    LoadAndPlay {
        path: PathBuf,
        playlist: Vec<PathBuf>,
        index: usize,
    },
    TogglePause,
    Stop,
    Next,
    Previous,
    SeekBy(i64),
    AdjustVolume(f32),
    Shutdown,
}

#[derive(Debug, Clone)]
pub enum EngineEvent {
    StateUpdated(PlayerState),
    Error(String),
}

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("{0}")]
    Backend(String),
}

pub trait PlaybackBackend {
    fn load(
        &mut self,
        path: &Path,
        start_at: Duration,
        volume: f32,
        paused: bool,
    ) -> Result<Option<Duration>, AudioError>;
    fn play(&mut self) -> Result<(), AudioError>;
    fn pause(&mut self) -> Result<(), AudioError>;
    fn stop(&mut self) -> Result<(), AudioError>;
    fn set_volume(&mut self, volume: f32) -> Result<(), AudioError>;
    fn position(&self) -> Duration;
    fn track_finished(&self) -> bool;
}

pub struct RealAudioBackend {
    device_sink: MixerDeviceSink,
    player: Player,
    current_path: Option<PathBuf>,
    current_duration: Option<Duration>,
    volume: f32,
}

impl RealAudioBackend {
    pub fn new() -> Result<Self, AudioError> {
        let mut device_sink = DeviceSinkBuilder::open_default_sink().map_err(|error| {
            AudioError::Backend(format!("failed to open default audio output: {error}"))
        })?;
        device_sink.log_on_drop(false);
        let player = Player::connect_new(device_sink.mixer());
        player.set_volume(DEFAULT_VOLUME);

        Ok(Self {
            device_sink,
            player,
            current_path: None,
            current_duration: None,
            volume: DEFAULT_VOLUME,
        })
    }

    fn clamp_position(&self, position: Duration) -> Duration {
        match self.current_duration {
            Some(duration) => position.min(duration),
            None => position,
        }
    }
}

impl PlaybackBackend for RealAudioBackend {
    fn load(
        &mut self,
        path: &Path,
        start_at: Duration,
        volume: f32,
        paused: bool,
    ) -> Result<Option<Duration>, AudioError> {
        let _keep_sink_alive = &self.device_sink;
        let file = File::open(path).map_err(|error| {
            AudioError::Backend(format!("failed to open {}: {error}", path.display()))
        })?;
        let decoder = Decoder::try_from(file).map_err(|error| {
            AudioError::Backend(format!("failed to decode {}: {error}", path.display()))
        })?;
        let duration = decoder.total_duration();

        self.player.stop();
        self.volume = volume;
        self.player.set_volume(volume);
        self.player.append(decoder);
        self.player.try_seek(start_at).map_err(|error| {
            AudioError::Backend(format!("failed to seek {}: {error}", path.display()))
        })?;

        if paused {
            self.player.pause();
        } else {
            self.player.play();
        }

        self.current_duration = duration;
        self.current_path = Some(path.to_path_buf());
        Ok(duration)
    }

    fn play(&mut self) -> Result<(), AudioError> {
        self.player.play();
        Ok(())
    }

    fn pause(&mut self) -> Result<(), AudioError> {
        self.player.pause();
        Ok(())
    }

    fn stop(&mut self) -> Result<(), AudioError> {
        self.player.stop();
        self.current_path = None;
        self.current_duration = None;
        Ok(())
    }

    fn set_volume(&mut self, volume: f32) -> Result<(), AudioError> {
        self.volume = volume;
        self.player.set_volume(volume);
        Ok(())
    }

    fn position(&self) -> Duration {
        self.clamp_position(self.player.get_pos())
    }

    fn track_finished(&self) -> bool {
        self.current_path.is_some() && self.player.empty()
    }
}

pub struct EngineController<B> {
    backend: B,
    state: PlayerState,
}

impl<B: PlaybackBackend> EngineController<B> {
    pub fn new(mut backend: B) -> Self {
        let state = PlayerState::default();
        let _ = backend.set_volume(state.volume);

        Self { backend, state }
    }

    pub fn snapshot(&self) -> PlayerState {
        self.state.clone()
    }

    pub fn handle_command(&mut self, command: AudioCommand) -> Vec<EngineEvent> {
        let result = match command {
            AudioCommand::LoadAndPlay {
                path,
                playlist,
                index,
            } => self.load_track(&path, playlist, index, false),
            AudioCommand::TogglePause => self.toggle_pause(),
            AudioCommand::Stop => self.stop(),
            AudioCommand::Next => self.advance_queue(1),
            AudioCommand::Previous => self.previous_track(),
            AudioCommand::SeekBy(seconds) => self.seek_by(seconds),
            AudioCommand::AdjustVolume(delta) => self.adjust_volume(delta),
            AudioCommand::Shutdown => Ok(()),
        };

        self.state.position = self.backend.position();

        match result {
            Ok(()) => {
                self.state.last_error = None;
                vec![EngineEvent::StateUpdated(self.state.clone())]
            }
            Err(error) => {
                let message = error.to_string();
                self.state.last_error = Some(message.clone());
                vec![
                    EngineEvent::Error(message),
                    EngineEvent::StateUpdated(self.state.clone()),
                ]
            }
        }
    }

    pub fn tick(&mut self) -> Vec<EngineEvent> {
        let previous_position = self.state.position;
        self.state.position = self.backend.position();

        if self.state.status == PlaybackStatus::Playing && self.backend.track_finished() {
            match self.advance_queue(1) {
                Ok(()) if self.state.status == PlaybackStatus::Playing => {
                    return vec![EngineEvent::StateUpdated(self.state.clone())];
                }
                Ok(()) => {}
                Err(error) => {
                    self.state.last_error = Some(error.to_string());
                    return vec![
                        EngineEvent::Error(error.to_string()),
                        EngineEvent::StateUpdated(self.state.clone()),
                    ];
                }
            }
        }

        if self.state.position != previous_position {
            vec![EngineEvent::StateUpdated(self.state.clone())]
        } else {
            Vec::new()
        }
    }

    fn load_track(
        &mut self,
        path: &Path,
        playlist: Vec<PathBuf>,
        index: usize,
        paused: bool,
    ) -> Result<(), AudioError> {
        let duration = self
            .backend
            .load(path, Duration::ZERO, self.state.volume, paused)?;
        let queue_index = if playlist.is_empty() {
            None
        } else {
            Some(index.min(playlist.len().saturating_sub(1)))
        };

        self.state.current_track = Some(Track::from_path(path));
        self.state.duration = duration;
        self.state.position = Duration::ZERO;
        self.state.status = if paused {
            PlaybackStatus::Paused
        } else {
            PlaybackStatus::Playing
        };
        self.state.queue = playlist;
        self.state.queue_index = queue_index;
        Ok(())
    }

    fn restart_current(&mut self, paused: bool) -> Result<(), AudioError> {
        let Some(track) = self.state.current_track.clone() else {
            return Ok(());
        };

        let duration = self
            .backend
            .load(&track.path, Duration::ZERO, self.state.volume, paused)?;
        self.state.duration = duration;
        self.state.position = Duration::ZERO;
        self.state.status = if paused {
            PlaybackStatus::Paused
        } else {
            PlaybackStatus::Playing
        };
        Ok(())
    }

    fn toggle_pause(&mut self) -> Result<(), AudioError> {
        match self.state.status {
            PlaybackStatus::Playing => {
                self.backend.pause()?;
                self.state.position = self.backend.position();
                self.state.status = PlaybackStatus::Paused;
            }
            PlaybackStatus::Paused => {
                self.backend.play()?;
                self.state.status = PlaybackStatus::Playing;
            }
            PlaybackStatus::Stopped => {
                self.restart_current(false)?;
            }
        }

        Ok(())
    }

    fn stop(&mut self) -> Result<(), AudioError> {
        self.backend.stop()?;
        self.state.status = PlaybackStatus::Stopped;
        self.state.position = Duration::ZERO;
        Ok(())
    }

    fn adjust_volume(&mut self, delta: f32) -> Result<(), AudioError> {
        self.state.volume = (self.state.volume + delta).clamp(0.0, 2.0);
        self.backend.set_volume(self.state.volume)?;
        Ok(())
    }

    fn seek_by(&mut self, seconds: i64) -> Result<(), AudioError> {
        if self.state.status == PlaybackStatus::Stopped {
            return Ok(());
        }

        let Some(track) = self.state.current_track.clone() else {
            return Ok(());
        };

        let current_position = self.backend.position();
        let target = clamp_seek_target(current_position, seconds, self.state.duration);
        let paused = self.state.status == PlaybackStatus::Paused;
        let duration = self
            .backend
            .load(&track.path, target, self.state.volume, paused)?;

        self.state.position = target;
        self.state.duration = duration;
        Ok(())
    }

    fn previous_track(&mut self) -> Result<(), AudioError> {
        if self.backend.position() >= Duration::from_secs(3) {
            let paused = self.state.status == PlaybackStatus::Paused;
            return self.restart_current(paused);
        }

        self.advance_queue(-1)
    }

    fn advance_queue(&mut self, offset: isize) -> Result<(), AudioError> {
        let Some(current_index) = self.state.queue_index else {
            return Ok(());
        };

        let Some(next_index) = current_index.checked_add_signed(offset) else {
            return Ok(());
        };

        if next_index >= self.state.queue.len() {
            self.backend.stop()?;
            self.state.status = PlaybackStatus::Stopped;
            self.state.position = Duration::ZERO;
            return Ok(());
        }

        let next_path = self.state.queue[next_index].clone();
        let duration = self
            .backend
            .load(&next_path, Duration::ZERO, self.state.volume, false)?;

        self.state.current_track = Some(Track::from_path(&next_path));
        self.state.duration = duration;
        self.state.position = Duration::ZERO;
        self.state.status = PlaybackStatus::Playing;
        self.state.queue_index = Some(next_index);
        Ok(())
    }
}

pub type EngineChannels = (Sender<AudioCommand>, Receiver<EngineEvent>, JoinHandle<()>);

pub fn spawn_engine() -> Result<EngineChannels, AudioError> {
    let backend = RealAudioBackend::new()?;
    let controller = EngineController::new(backend);
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();

    let engine_handle = thread::spawn(move || run_engine_loop(controller, command_rx, event_tx));

    Ok((command_tx, event_rx, engine_handle))
}

fn run_engine_loop<B: PlaybackBackend>(
    mut controller: EngineController<B>,
    command_rx: Receiver<AudioCommand>,
    event_tx: Sender<EngineEvent>,
) {
    let _ = event_tx.send(EngineEvent::StateUpdated(controller.snapshot()));

    loop {
        match command_rx.recv_timeout(Duration::from_millis(ENGINE_TICK_MS)) {
            Ok(AudioCommand::Shutdown) => break,
            Ok(command) => {
                for event in controller.handle_command(command) {
                    if event_tx.send(event).is_err() {
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                for event in controller.tick() {
                    if event_tx.send(event).is_err() {
                        return;
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn clamp_seek_target(
    current_position: Duration,
    seconds: i64,
    track_duration: Option<Duration>,
) -> Duration {
    let shifted = if seconds.is_negative() {
        current_position.saturating_sub(Duration::from_secs(seconds.unsigned_abs()))
    } else {
        current_position.saturating_add(Duration::from_secs(seconds as u64))
    };

    match track_duration {
        Some(duration) => shifted.min(duration),
        None => shifted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct MockBackend {
        volume: f32,
        position: Duration,
        finished: bool,
        loads: Vec<(PathBuf, Duration, f32, bool)>,
        play_calls: usize,
        pause_calls: usize,
        stop_calls: usize,
    }

    impl PlaybackBackend for MockBackend {
        fn load(
            &mut self,
            path: &Path,
            start_at: Duration,
            volume: f32,
            paused: bool,
        ) -> Result<Option<Duration>, AudioError> {
            self.position = start_at;
            self.finished = false;
            self.volume = volume;
            self.loads
                .push((path.to_path_buf(), start_at, volume, paused));
            Ok(Some(Duration::from_secs(180)))
        }

        fn play(&mut self) -> Result<(), AudioError> {
            self.play_calls += 1;
            Ok(())
        }

        fn pause(&mut self) -> Result<(), AudioError> {
            self.pause_calls += 1;
            Ok(())
        }

        fn stop(&mut self) -> Result<(), AudioError> {
            self.stop_calls += 1;
            self.position = Duration::ZERO;
            Ok(())
        }

        fn set_volume(&mut self, volume: f32) -> Result<(), AudioError> {
            self.volume = volume;
            Ok(())
        }

        fn position(&self) -> Duration {
            self.position
        }

        fn track_finished(&self) -> bool {
            self.finished
        }
    }

    fn sample_playlist() -> Vec<PathBuf> {
        vec![
            PathBuf::from("one.mp3"),
            PathBuf::from("two.flac"),
            PathBuf::from("three.ogg"),
        ]
    }

    #[test]
    fn load_and_play_updates_state() {
        let backend = MockBackend::default();
        let mut controller = EngineController::new(backend);
        let playlist = sample_playlist();

        controller.handle_command(AudioCommand::LoadAndPlay {
            path: playlist[0].clone(),
            playlist: playlist.clone(),
            index: 0,
        });

        assert_eq!(controller.state.status, PlaybackStatus::Playing);
        assert_eq!(controller.state.queue_index, Some(0));
        assert_eq!(controller.state.queue, playlist);
        assert_eq!(
            controller
                .state
                .current_track
                .as_ref()
                .map(|track| track.title.as_str()),
            Some("one.mp3")
        );
    }

    #[test]
    fn toggle_pause_round_trip_updates_state() {
        let backend = MockBackend::default();
        let mut controller = EngineController::new(backend);
        let playlist = sample_playlist();

        controller.handle_command(AudioCommand::LoadAndPlay {
            path: playlist[0].clone(),
            playlist,
            index: 0,
        });
        controller.handle_command(AudioCommand::TogglePause);
        assert_eq!(controller.state.status, PlaybackStatus::Paused);

        controller.handle_command(AudioCommand::TogglePause);
        assert_eq!(controller.state.status, PlaybackStatus::Playing);
    }

    #[test]
    fn seek_clamps_to_zero() {
        let backend = MockBackend {
            position: Duration::from_secs(3),
            ..MockBackend::default()
        };
        let mut controller = EngineController::new(backend);
        let playlist = sample_playlist();

        controller.handle_command(AudioCommand::LoadAndPlay {
            path: playlist[0].clone(),
            playlist,
            index: 0,
        });
        controller.backend.position = Duration::from_secs(3);
        controller.handle_command(AudioCommand::SeekBy(-5));

        assert_eq!(controller.state.position, Duration::ZERO);
        assert_eq!(
            controller.backend.loads.last().map(|entry| entry.1),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn next_and_previous_walk_the_queue() {
        let backend = MockBackend::default();
        let mut controller = EngineController::new(backend);
        let playlist = sample_playlist();

        controller.handle_command(AudioCommand::LoadAndPlay {
            path: playlist[1].clone(),
            playlist,
            index: 1,
        });
        controller.handle_command(AudioCommand::Next);
        assert_eq!(controller.state.queue_index, Some(2));
        assert_eq!(
            controller
                .state
                .current_track
                .as_ref()
                .map(|track| track.title.as_str()),
            Some("three.ogg")
        );

        controller.backend.position = Duration::from_secs(0);
        controller.handle_command(AudioCommand::Previous);
        assert_eq!(controller.state.queue_index, Some(1));
        assert_eq!(
            controller
                .state
                .current_track
                .as_ref()
                .map(|track| track.title.as_str()),
            Some("two.flac")
        );
    }

    #[test]
    fn previous_restarts_current_track_when_already_in_progress() {
        let backend = MockBackend {
            position: Duration::from_secs(12),
            ..MockBackend::default()
        };
        let mut controller = EngineController::new(backend);
        let playlist = sample_playlist();

        controller.handle_command(AudioCommand::LoadAndPlay {
            path: playlist[1].clone(),
            playlist,
            index: 1,
        });
        controller.backend.position = Duration::from_secs(12);

        controller.handle_command(AudioCommand::Previous);

        assert_eq!(controller.state.queue_index, Some(1));
        assert_eq!(controller.state.position, Duration::ZERO);
        assert_eq!(
            controller
                .backend
                .loads
                .last()
                .map(|entry| (&entry.0, entry.1)),
            Some((&PathBuf::from("two.flac"), Duration::ZERO))
        );
    }

    #[test]
    fn load_and_play_clamps_invalid_queue_index() {
        let backend = MockBackend::default();
        let mut controller = EngineController::new(backend);
        let playlist = sample_playlist();

        controller.handle_command(AudioCommand::LoadAndPlay {
            path: playlist[2].clone(),
            playlist,
            index: usize::MAX,
        });

        assert_eq!(controller.state.queue_index, Some(2));
    }

    #[test]
    fn volume_adjustment_is_clamped() {
        let backend = MockBackend::default();
        let mut controller = EngineController::new(backend);

        controller.handle_command(AudioCommand::AdjustVolume(-2.0));
        assert_eq!(controller.state.volume, 0.0);

        controller.handle_command(AudioCommand::AdjustVolume(3.0));
        assert_eq!(controller.state.volume, 2.0);
    }
}
