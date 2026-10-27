//! Scripted music contexts through the real tick-driven player and a synthetic
//! 17-song library. No MIDI device and no game `MUSIC/` folder are required.

use ffe_audio::{
    AudioEngine, AudioSettings, MusicLibrary, NullSampleSink, RecordingMidiSink, SONG_COUNT,
};
use ffe_rs::sound::GameAudio;
use std::time::Duration;

fn hmp_delta(value: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut rest = value;
    loop {
        let group = (rest & 0x7f) as u8;
        rest >>= 7;
        if rest == 0 {
            bytes.push(group | 0x80);
            break;
        }
        bytes.push(group);
    }
    bytes
}

fn synthetic_hmp() -> Vec<u8> {
    let mut track = Vec::new();
    track.extend_from_slice(&hmp_delta(0));
    track.extend_from_slice(&[0x90, 64, 90]);
    track.extend_from_slice(&hmp_delta(192));
    track.extend_from_slice(&[0x80, 64, 0]);
    track.extend_from_slice(&hmp_delta(0));
    track.extend_from_slice(&[0xff, 0x2f, 0x00]);

    let mut image = vec![0u8; 0x40];
    image[..14].copy_from_slice(b"HMIMIDIP013195");
    image[0x30] = 2;
    image[0x3c..0x40].copy_from_slice(&2u32.to_le_bytes());
    image.extend_from_slice(&[0xff, 0x2f, 0x00]);
    image.extend_from_slice(&1u32.to_le_bytes());
    image.extend_from_slice(&((track.len() as u32) + 12).to_le_bytes());
    image.extend_from_slice(&[0, 0, 0, 0]);
    image.extend_from_slice(&track);
    image.extend_from_slice(&[0, 0, 0, 0]);
    image
}

fn library() -> MusicLibrary {
    let mut library = MusicLibrary::new("missing-music");
    for index in 0..SONG_COUNT {
        library.insert_hmp(index, synthetic_hmp());
    }
    library
}

#[test]
fn intro_flight_combat_hyperspace_and_dock_start_the_source_songs() {
    let midi = RecordingMidiSink::new();
    let engine = AudioEngine::with_rng_seed(library(), midi.clone(), NullSampleSink, 0x1234_5678);
    let mut game = GameAudio::new(engine);
    let mut log = Vec::new();

    game.front_end_music();
    assert_eq!(game.current_song(), Some(0), "F919 front end");
    log.push(("front end", Some(0), "F919 song 0"));

    // F919's F1904 replay: let the 1.6 s synthetic song finish.
    let note_ons = midi.count(&[0x90, 64, 90]);
    game.tick(Duration::from_millis(2_500));
    assert_eq!(game.current_song(), Some(0), "F919 replay");
    assert!(
        midi.count(&[0x90, 64, 90]) > note_ons,
        "the replay must send MIDI again"
    );
    log.push(("front end done", Some(0), "F919 replays song 0"));

    game.leave_front_end();
    assert_eq!(game.current_song(), None, "F919 stop on leaving");
    log.push(("leave front end", None, "F919 stop"));

    // F232 continuous music: after a stop, a finished song starts a random one.
    let mut flags = game.flags();
    flags.continuous_music = true;
    game.set_flags(flags);
    game.tick(Duration::from_millis(10));
    let flight = game.current_song().expect("continuous music starts a song");
    assert!(flight < 16, "F1902(-1) never chooses song 16");
    log.push(("in flight", Some(flight), "F232 random 0..=15"));

    // F1048/F1091 stop at a docking or launch transition.
    game.stop_for_sequence();
    assert_eq!(game.current_song(), None);
    log.push(("launch transition", None, "F1048/F1091 stop"));

    // F750/F236 battle start and F770 end.
    game.start_battle();
    assert_eq!(game.current_song(), Some(14), "F750 battle");
    log.push(("combat", Some(14), "F750 song 14"));
    game.end_battle();
    assert_eq!(game.current_song(), None, "F770 combat end");
    log.push(("combat end", None, "F770 stop"));

    // F1094 hyperspace exit: a random song.
    game.hyperspace_music();
    let jump = game.current_song().expect("F1094 random song");
    assert!(jump < 16, "F1902(-2) uses the 0..=15 random pool");
    log.push(("hyperspace", Some(jump), "F1094 random 0..=15"));

    // F1048 stop, then F656's clearance music.
    game.stop_for_sequence();
    game.clearance_music();
    assert_eq!(game.current_song(), Some(15), "F656 Blue Danube");
    log.push(("clearance", Some(15), "F656 song 15"));

    // F770/F941: death stops the song and clears the scene state.
    game.player_death();
    assert_eq!(game.current_song(), None, "player death");
    log.push(("player death", None, "F770/F941 stop"));

    println!("music event log:");
    for (scene, song, rule) in &log {
        println!("  {scene:20} song={song:?}  {rule}");
    }
}

#[test]
fn music_off_is_silent_and_music_volume_sets_the_midi_channel_level() {
    let midi = RecordingMidiSink::new();
    let engine = AudioEngine::with_rng_seed(library(), midi.clone(), NullSampleSink, 7);
    let mut game = GameAudio::new(engine);

    game.set_settings(AudioSettings {
        music_volume: 50,
        ..AudioSettings::default()
    });
    game.front_end_music();
    assert_eq!(game.current_song(), Some(0));
    assert!(
        midi.contains(&[0xb0, 7, 50]),
        "50% of General MIDI's reset level 100 is CC7 = 50"
    );

    game.set_settings(AudioSettings {
        music_on: false,
        music_volume: 100,
        ..AudioSettings::default()
    });
    assert_eq!(
        game.current_song(),
        None,
        "turning music off stops the song"
    );
    game.front_end_music();
    game.start_battle();
    game.hyperspace_music();
    game.clearance_music();
    assert_eq!(game.current_song(), None, "every scene start is gated");
    assert!(game.song_done());
}

#[test]
fn every_song_index_is_loadable_from_the_library() {
    let library = library();
    assert_eq!(
        library.cached_indices().collect::<Vec<_>>(),
        (0..SONG_COUNT).collect::<Vec<_>>()
    );
    for index in 0..SONG_COUNT {
        assert!(library.load(index).is_ok(), "song {index}");
    }
}
