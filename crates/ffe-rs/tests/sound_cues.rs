//! Headless cue wiring: the real `AudioEngine` with a recording sample sink,
//! no MIDI device and no music folder. The test drives the same cue sequence a
//! short combat/launch scene posts and checks the sample-table indices, the
//! preference gates and the volume scaling.

use ffe_rs::audio::{AudioEngine, MusicLibrary, NullMidiSink, RecordingSampleSink};
use ffe_rs::prefs::Prefs;
use ffe_rs::sound::{sample, Cue, GameAudio};

fn engine(sink: &RecordingSampleSink) -> GameAudio {
    GameAudio::new(AudioEngine::new(
        MusicLibrary::new("missing-music"),
        NullMidiSink,
        sink.clone(),
    ))
}

#[test]
fn a_combat_and_launch_sequence_records_the_original_sample_codes() {
    let sink = RecordingSampleSink::new();
    let mut game = engine(&sink);

    game.play(Cue::Laser { bank: 0 }); // F930, laser1
    game.play(Cue::ShieldHit); // F953, 0x1c
    game.play(Cue::HullDamage); // F953, 0x1d
    game.play(Cue::ShipExplosion); // F942, 0x05
    game.play(Cue::LargeExplosion); // F942, 0x1f
    game.play(Cue::Message); // F351, 0x16
    game.play(Cue::Launch); // F1098, 0x20
    game.play(Cue::Hyperspace); // F908, 0x21
    game.play(Cue::Camera); // F1125, 0x27

    let calls = sink.calls();
    assert_eq!(
        calls.iter().map(|call| call.code).collect::<Vec<_>>(),
        vec![
            sample::LASER_1,
            sample::MISSILE_EXPLOSION_SHIELD,
            sample::DAMAGE,
            sample::SHIP_EXPLOSION,
            sample::BOOM,
            sample::BONG,
            sample::LAUNCH,
            sample::HYPERSPACE,
            sample::CAMERA
        ]
    );
    assert!(calls
        .iter()
        .all(|call| call.volume == 63 && call.pitch == -1));
}

#[test]
fn the_preference_page_mutes_and_the_volume_scale_reach_the_sink() {
    let sink = RecordingSampleSink::new();
    let mut game = engine(&sink);

    let mut prefs = Prefs {
        sound: [false, true, true, true, true, true, true, true, true, true],
        ..Prefs::default()
    };
    game.apply_prefs(&prefs);
    game.play(Cue::Laser { bank: 0 });
    assert!(sink.calls().is_empty());

    prefs.sound[0] = true;
    prefs.sound[8] = false; // Icon beeps off
    game.apply_prefs(&prefs);
    assert!(!game.play(Cue::UiBeep));
    assert!(sink.calls().is_empty());

    game.set_volumes(100, 50);
    game.play(Cue::Laser { bank: 0 });
    assert_eq!(sink.calls().last().map(|call| call.volume), Some(31));
}
