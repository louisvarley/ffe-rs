//! Play an original HMP song through the Windows system MIDI synthesizer.
//!
//! Usage: `cargo run -p ffe-audio --example play -- <MUSIC directory> [index]`
//! The default directory is `MUSIC` under the current directory, matching the
//! original `[DIR] SongPath` default.

use ffe_audio::{Audio, AudioEngine, MusicLibrary, SONG_NAMES};
use std::time::{Duration, Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let directory = args.next().unwrap_or_else(|| "MUSIC".to_owned());
    let index = args
        .next()
        .map(|value| value.parse::<usize>())
        .transpose()?
        .unwrap_or(0);
    if index >= SONG_NAMES.len() {
        return Err(format!("index must be 0..={}", SONG_NAMES.len() - 1).into());
    }

    let library = MusicLibrary::new(&directory);
    let mut engine = AudioEngine::system(library);
    engine.play_song_index(index)?;
    println!(
        "playing {} ({}) from {}",
        index,
        engine
            .player()
            .duration_seconds()
            .map_or("unknown length".to_owned(), |seconds| format!(
                "{seconds:.1}s"
            )),
        directory
    );

    let mut previous = Instant::now();
    while !engine.song_done() {
        let now = Instant::now();
        engine.tick(now.duration_since(previous));
        previous = now;
        std::thread::sleep(Duration::from_millis(10));
    }
    println!("done");
    Ok(())
}
