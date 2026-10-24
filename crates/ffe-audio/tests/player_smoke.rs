//! End-to-end smoke playback without an audio device: a synthetic HMP on disk,
//! the null backend, and the caller-driven clock.

use ffe_audio::{Audio, AudioEngine, HmpSong, MusicLibrary};
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

#[test]
fn silent_engine_plays_a_synthetic_song_to_completion() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("QQFRONT.HMP"), synthetic_hmp()).unwrap();

    let library = MusicLibrary::new(directory.path());
    let parsed = library.load(0).unwrap();
    assert_eq!(parsed.header_track_count, 2);
    assert_eq!(HmpSong::parse(&synthetic_hmp()).unwrap(), parsed);

    let mut engine = AudioEngine::silent(library);
    engine.play_song_index(0).unwrap();
    assert!(!engine.song_done());
    // 192 ticks at the original tempo is about 1.606 seconds.
    engine.tick(Duration::from_millis(1_700));
    assert!(engine.song_done());

    // A missing device is not required: the null backend is entirely deterministic.
    let mut engine = AudioEngine::silent(MusicLibrary::new(directory.path()));
    let mut flags = ffe_audio::SoundFlags {
        songs: [false; ffe_audio::SONG_COUNT],
        ..ffe_audio::SoundFlags::default()
    };
    flags.songs[0] = true;
    engine.set_sound_flags(flags);
    for _ in 0..100 {
        if !engine.song_done() {
            break;
        }
        engine.play_song(-1).unwrap();
    }
    assert!(!engine.song_done());
}
