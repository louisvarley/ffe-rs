//! Headless mixer test: the real `AudioEngine` with a headless
//! [`MixerSampleSink`] and no device. Unlike `sound_cues.rs`, which records the
//! original sample-table calls, this test renders the mixed 16-bit PCM and
//! checks that decoded effect samples, the mute gate and the volume curve reach
//! the waveform.

use ffe_rs::audio::{
    AudioEngine, AudioSettings, EffectLibrary, EffectSample, MixerSampleSink, MusicLibrary,
    NullMidiSink, SAMPLE_RATE,
};
use ffe_rs::prefs::Prefs;
use ffe_rs::sound::{Cue, GameAudio};

fn constant(name: &str, value: i16, frames: usize) -> EffectLibrary {
    let mut effects = EffectLibrary::new();
    effects.insert_sample(
        name,
        EffectSample {
            pcm: vec![value; frames],
            sample_rate: SAMPLE_RATE,
        },
    );
    effects
}

fn game(sink: MixerSampleSink) -> GameAudio {
    GameAudio::new(AudioEngine::new(
        MusicLibrary::new("missing-music"),
        NullMidiSink,
        sink,
    ))
}

#[test]
fn a_cue_mixes_decoded_sample_data_not_only_a_cue_record() {
    let sink = MixerSampleSink::new(constant("bip", 10_000, 32));
    let mixer = sink.mixer();
    let mut game = game(sink);

    assert!(game.play(Cue::UiBeep));
    let mut output = vec![0i16; 64];
    mixer.lock().expect("mixer").mix_into(&mut output);

    assert_eq!(&output[..32], &[10_000i16; 32]);
    assert!(output[32..].iter().all(|sample| *sample == 0));
    assert_eq!(mixer.lock().expect("mixer").active_voices(), 0);
}

#[test]
fn effects_mute_stops_active_voices_and_suppresses_new_ones() {
    let sink = MixerSampleSink::new(constant("bip", 10_000, 256));
    let mixer = sink.mixer();
    let mut game = game(sink);
    assert!(game.play(Cue::UiBeep));
    let mut first = vec![0i16; 8];
    mixer.lock().expect("mixer").mix_into(&mut first);
    assert!(first.iter().any(|sample| *sample != 0));

    // F1908's DATA_009028: off stops all 16 channels.
    let prefs = Prefs {
        sound: [false, true, true, true, true, true, true, true, true, true],
        ..Prefs::default()
    };
    game.apply_prefs(&prefs);
    let mut muted = vec![0i16; 8];
    mixer.lock().expect("mixer").mix_into(&mut muted);
    assert!(muted.iter().all(|sample| *sample == 0));
    // The cue still resolves; `F1908`'s gate is inside the player, so the
    // engine sees the call and simply mixes nothing.
    assert!(game.play(Cue::UiBeep));
    let mut after = vec![0i16; 8];
    mixer.lock().expect("mixer").mix_into(&mut after);
    assert!(after.iter().all(|sample| *sample == 0));
}

#[test]
fn effects_volume_scales_the_original_level_through_the_centibel_curve() {
    let sink = MixerSampleSink::new(constant("bip", 10_000, 32));
    let mixer = sink.mixer();
    let mut game = game(sink);

    game.set_volumes(100, 100);
    game.play(Cue::UiBeep);
    let mut full = vec![0i16; 32];
    mixer.lock().expect("mixer").mix_into(&mut full);

    // Let the first voice finish so the 50% render has one voice, not two.
    game.set_volumes(100, 50); // F1908 level 63 -> 31
    game.play(Cue::UiBeep);
    let mut half = vec![0i16; 8];
    mixer.lock().expect("mixer").mix_into(&mut half);

    assert_eq!(full[0], 10_000);
    let expected = (10_000.0 * ffe_rs::audio::level_gain(31)) as i16;
    assert_eq!(half[0], expected);
    assert!(half[0] < full[0]);

    let settings = AudioSettings {
        effects_volume: 0,
        ..AudioSettings::default()
    };
    // Finish the previous voice before checking the zero-volume gate.
    let mut drain = vec![0i16; 32];
    mixer.lock().expect("mixer").mix_into(&mut drain);
    game.set_settings(settings);
    game.play(Cue::UiBeep);
    let mut silent = vec![0i16; 8];
    mixer.lock().expect("mixer").mix_into(&mut silent);
    assert!(silent.iter().all(|sample| *sample == 0));
}
