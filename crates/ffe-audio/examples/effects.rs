//! Manual device check for the effect mixer.
//!
//! `cargo run -p ffe-audio --example effects` opens the default WinMM wave
//! output and plays a short synthetic `bip` through the real mixer. It is a
//! device smoke test, not part of CI; if `waveOutOpen` fails it prints the
//! reason and exits successfully so headless machines stay usable.

use ffe_audio::{EffectLibrary, EffectSample, MixerSampleSink, SampleSink, SAMPLE_RATE};
use std::f64::consts::TAU;
use std::time::Duration;

fn main() {
    let mut effects = EffectLibrary::new();
    let frames = SAMPLE_RATE as usize / 4;
    effects.insert_sample(
        "bip",
        EffectSample {
            pcm: (0..frames)
                .map(|frame| {
                    let phase = frame as f64 * 880.0 * TAU / f64::from(SAMPLE_RATE);
                    (phase.sin() * 8_000.0) as i16
                })
                .collect(),
            sample_rate: SAMPLE_RATE,
        },
    );

    let mut sink = match MixerSampleSink::open(effects) {
        Ok(sink) => sink,
        Err(error) => {
            eprintln!("No wave output device ({error}); the game falls back to a headless mixer.");
            return;
        }
    };
    println!("Playing a 0.25 s 880 Hz bip through the WinMM wave output...");
    sink.play_sample(0x14, 63, -1);
    std::thread::sleep(Duration::from_millis(400));
    sink.stop_all();
    println!("done");
}
