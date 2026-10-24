//! Parse all 17 shipped HMP songs when the player's `MUSIC` directory is
//! available. CI does not contain the copyrighted game files, so the test skips
//! cleanly when they are absent; set `FFE_MUSIC_DIR` to run it locally.

use ffe_audio::{HmpSong, MusicLibrary, SONG_COUNT, SONG_NAMES};
use std::path::{Path, PathBuf};

fn music_dir() -> Option<PathBuf> {
    if let Ok(directory) = std::env::var("FFE_MUSIC_DIR") {
        let path = PathBuf::from(directory);
        if path.is_dir() {
            return Some(path);
        }
        panic!("FFE_MUSIC_DIR is not a directory: {}", path.display());
    }
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    [
        manifest.join("../../game/MUSIC"),
        manifest.join("../../../game/MUSIC"),
        PathBuf::from("game/MUSIC"),
    ]
    .into_iter()
    .find(|path| path.is_dir())
}

#[test]
fn parses_all_seventeen_original_songs() {
    let Some(directory) = music_dir() else {
        eprintln!("skipping the 17-HMP test: set FFE_MUSIC_DIR to the game's MUSIC folder");
        return;
    };

    let library = MusicLibrary::new(&directory);
    let mut total_events = 0usize;
    for (index, name) in SONG_NAMES.iter().enumerate() {
        let path = library.path_for(index).unwrap_or_else(|| {
            panic!(
                "{}.HMP is missing from {}",
                name.to_ascii_uppercase(),
                directory.display()
            )
        });
        let bytes = std::fs::read(&path).unwrap();
        let song = HmpSong::parse(&bytes).unwrap_or_else(|error| {
            panic!("{} failed to parse: {error}", path.display());
        });
        assert_eq!(
            usize::from(song.header_track_count),
            song.data_track_count() + 1,
            "{name}: header track count must include the conductor"
        );
        assert!(song.data_track_count() > 0, "{name} has no data tracks");
        assert!(song.event_count() > 0, "{name} has no events");
        assert!(song.duration_ticks() > 0, "{name} has no duration");
        assert_eq!(song.division, 192);
        assert_eq!(song.tempo_us_per_quarter, 0x18_8000);

        let ratio = song.duration_seconds() / f64::from(song.advertised_duration_seconds.max(1));
        assert!(
            (0.4..=2.0).contains(&ratio),
            "{name}: event duration {:.1}s vs advertised {}s is implausible",
            song.duration_seconds(),
            song.advertised_duration_seconds
        );

        let smf = song.to_smf();
        assert_eq!(&smf[..4], b"MThd");
        assert_eq!(
            u16::from_be_bytes([smf[10], smf[11]]) as usize,
            song.data_track_count() + 1
        );
        assert_eq!(u16::from_be_bytes([smf[12], smf[13]]), song.division);

        total_events += song.event_count();
        println!(
            "{index:2} {name:9} header_tracks={:2} data_tracks={:2} events={:5} ticks={:6} calc={:7.2}s advertised={:4}s ratio={:.2}",
            song.header_track_count,
            song.data_track_count(),
            song.event_count(),
            song.duration_ticks(),
            song.duration_seconds(),
            song.advertised_duration_seconds,
            ratio,
        );
    }
    assert_eq!(SONG_NAMES.len(), SONG_COUNT);
    assert!(total_events > 10_000);
}

#[test]
fn the_shipped_directory_contains_the_table_names() {
    let Some(directory) = music_dir() else {
        eprintln!("skipping the directory-name test: set FFE_MUSIC_DIR");
        return;
    };
    let mut found = 0usize;
    for name in SONG_NAMES {
        let upper = directory.join(format!("{}.HMP", name.to_ascii_uppercase()));
        if upper.is_file() {
            found += 1;
        }
    }
    assert_eq!(
        found,
        SONG_COUNT,
        "expected all 17 HMP files in {}",
        directory.display()
    );
}
