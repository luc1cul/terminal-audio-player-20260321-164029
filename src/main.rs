use std::env;
use std::path::PathBuf;

use anyhow::Context;
use terminal_audio_player::app::App;
use terminal_audio_player::audio_engine;
use terminal_audio_player::tui;

fn main() -> anyhow::Result<()> {
    let root = resolve_root()?;
    let (command_tx, event_rx, engine_handle) =
        audio_engine::spawn_engine().context("failed to start audio engine")?;

    let mut app = App::new(root, command_tx, event_rx)?;
    let run_result = tui::run(&mut app);
    app.shutdown();

    if engine_handle.join().is_err() {
        eprintln!("audio engine thread panicked");
    }

    run_result
}

fn resolve_root() -> anyhow::Result<PathBuf> {
    match env::args().nth(1) {
        Some(arg) => Ok(PathBuf::from(arg)),
        None => env::current_dir().context("failed to determine current directory"),
    }
}
