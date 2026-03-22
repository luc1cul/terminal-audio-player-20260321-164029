# terminal-audio-player

A lightweight Rust terminal audio player with a decoupled audio engine and TUI frontend.

## Features

- Pure terminal UI with `ratatui` + `crossterm`
- Audio playback powered by `rodio`
- Supports **MP3, FLAC, WAV, OGG**
- Left-hand library lane with tree + lane-monitor chrome
- Playback deck controls, richer cue-stack chrome, and a status ribbon
- Windows XP-inspired glossy media deck with blue-glass chrome, animated marquee text, and responsive narrow-layout fallbacks
- Animated spectrum, crest reflection, glow, and signal ladder visualizer keyed to playback state
- Decoupled architecture using `std::sync::mpsc` channels
- Unit-tested audio engine state transitions

## Keybindings

- `Space` — play / pause (or start the selected file if nothing is loaded)
- `Tab` — switch focus between browser and player
- `j / k` — browser: move down/up, player: volume down/up
- `Enter` — expand directory or play selected file
- `← / →` — collapse / expand directories
- `h / l` — seek backward / forward by 5 seconds
- `n / p` — next / previous track
- `s` — stop
- `q` — quit

## Run

```bash
cargo run --release -- /path/to/music
```

If no path is supplied, the current working directory is used as the browser root.

## Architecture

- `src/audio_engine.rs` — playback backend, engine controller, channels, tests
- `src/browser.rs` — directory tree and playlist discovery
- `src/app.rs` — application state and key handling
- `src/tui.rs` — terminal rendering and event loop
- `src/main.rs` — bootstraps engine + UI

The backend and UI communicate asynchronously through channels so the rendering loop stays isolated from playback control.
