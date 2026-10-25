use crate::AudioError;
use ffe_core::hmp::HmpSong;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub use crate::settings::SONG_COUNT;

/// The original `ppSoundSongNames` table from `win32/w32sound.cpp`, in index
/// order. The DOS game uses the same names; the HMP files are upper-case on the
/// shipped floppy and CD releases.
pub const SONG_NAMES: [&str; SONG_COUNT] = [
    "qqfront", "qqatmosp", "qqtravel", "qqdrama", "qqsuspen", "qqrock", "qqescape", "qqparadi",
    "qqfront2", "hallking", "intro", "babayaga", "barem", "jupalt", "valkries", "bluedan",
    "ggofkiev",
];

/// A scene in `ffe.asm` that names a song. Every direct start is listed in
/// [`SONGS`]; the other songs are only reachable through `F1902(-1)`'s random
/// selection or the "Choose Music From" table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongTrigger {
    /// `F919`: the front end plays song 0 and replays it when `F1904` says done.
    FrontEnd,
    /// `F306`: a page/row change in the FF-code message expansion starts song 7.
    MessagePage,
    /// `F307`: the message/video FF-code sequence starts song 10 once per bit.
    MessageVideo,
    /// `F750`/`F236`: battle start plays song 14 when music in battles is on.
    Battle,
    /// `F656`: a hyperspace exit plays song 15 when music after hyperspace is on.
    Hyperspace,
}

/// One row of the original's 17-song table and the code that can start it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SongInfo {
    pub index: usize,
    /// Lower-case file stem used by `MUSIC/<NAME>.HMP` and `base:music/<name>`.
    pub name: &'static str,
    pub title: &'static str,
    /// Direct scene starts. Empty means the song is only random/explicit.
    pub triggers: &'static [SongTrigger],
    /// True when `F1902(-1)` can choose it. Index 16 (`GGOFKIEV`) is explicit
    /// only: the original's `FUNC_000922(0x10)` returns `0..=15`.
    pub random: bool,
}

const NO_TRIGGERS: &[SongTrigger] = &[];

/// The complete song table: index, file, original menu title, direct triggers
/// and random-pool membership. See `docs/notes/music.md`.
pub const SONGS: [SongInfo; SONG_COUNT] = [
    SongInfo {
        index: 0,
        name: SONG_NAMES[0],
        title: SONG_TITLES[0],
        triggers: &[SongTrigger::FrontEnd],
        random: true,
    },
    SongInfo {
        index: 1,
        name: SONG_NAMES[1],
        title: SONG_TITLES[1],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 2,
        name: SONG_NAMES[2],
        title: SONG_TITLES[2],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 3,
        name: SONG_NAMES[3],
        title: SONG_TITLES[3],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 4,
        name: SONG_NAMES[4],
        title: SONG_TITLES[4],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 5,
        name: SONG_NAMES[5],
        title: SONG_TITLES[5],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 6,
        name: SONG_NAMES[6],
        title: SONG_TITLES[6],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 7,
        name: SONG_NAMES[7],
        title: SONG_TITLES[7],
        triggers: &[SongTrigger::MessagePage],
        random: true,
    },
    SongInfo {
        index: 8,
        name: SONG_NAMES[8],
        title: SONG_TITLES[8],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 9,
        name: SONG_NAMES[9],
        title: SONG_TITLES[9],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 10,
        name: SONG_NAMES[10],
        title: SONG_TITLES[10],
        triggers: &[SongTrigger::MessageVideo],
        random: true,
    },
    SongInfo {
        index: 11,
        name: SONG_NAMES[11],
        title: SONG_TITLES[11],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 12,
        name: SONG_NAMES[12],
        title: SONG_TITLES[12],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 13,
        name: SONG_NAMES[13],
        title: SONG_TITLES[13],
        triggers: NO_TRIGGERS,
        random: true,
    },
    SongInfo {
        index: 14,
        name: SONG_NAMES[14],
        title: SONG_TITLES[14],
        triggers: &[SongTrigger::Battle],
        random: true,
    },
    SongInfo {
        index: 15,
        name: SONG_NAMES[15],
        title: SONG_TITLES[15],
        triggers: &[SongTrigger::Hyperspace],
        random: true,
    },
    SongInfo {
        index: 16,
        name: SONG_NAMES[16],
        title: SONG_TITLES[16],
        triggers: NO_TRIGGERS,
        random: false,
    },
];

impl SongInfo {
    /// `MUSIC/<NAME>.HMP` on the shipped releases.
    pub fn file_name(&self) -> String {
        format!("{}.HMP", self.name.to_ascii_uppercase())
    }

    pub fn has_trigger(&self, trigger: SongTrigger) -> bool {
        self.triggers.contains(&trigger)
    }
}

/// Find the song index for a scene trigger. `None` for a trigger that no song
/// names directly (the docking/launch and continuous-music paths use the random
/// selection instead).
pub fn song_for_trigger(trigger: SongTrigger) -> Option<usize> {
    SONGS
        .iter()
        .find(|song| song.has_trigger(trigger))
        .map(|song| song.index)
}

/// All indices `F1902(-1)` can choose, before its enabled/current filtering.
pub fn random_pool() -> impl Iterator<Item = usize> {
    SONGS
        .iter()
        .filter(|song| song.random)
        .map(|song| song.index)
}

/// The titles printed by the original "Choose Music From" menu (`DATA_004965`).
pub const SONG_TITLES: [&str; SONG_COUNT] = [
    "First Encounters Theme - Quality Quartet",
    "Atmosphere - Quality Quartet",
    "Travel - Quality Quartet",
    "Drama - Quality Quartet",
    "Suspense - Quality Quartet",
    "Rock - Quality Quartet",
    "Escape - Quality Quartet",
    "Paradise - Quality Quartet",
    "Alien Encounters - Quality Quartet",
    "In the Hall of the Mountain King - Grieg",
    "Frontier Main Theme - Lowe",
    "Baba Yaga - Mussorgsky",
    "Night on the Bare Mountain - Mussorgsky",
    "Frontier Second Theme - Lowe",
    "Ride of the Valkyries - Wagner",
    "Blue Danube - Strauss",
    "Great Gates of Kiev - Mussorgsky",
];

/// A directory of HMP tracks, normally the game's `MUSIC` folder (`[DIR]
/// SongPath`, default `MUSIC`).
///
/// The port reads the player's files at runtime, as the original does. Cache
/// v29 supplies all seventeen as `base:music/<name>`; v28 supplies only song 0.
/// A cache payload is preferred, so the intro works without the folder and a
/// stale folder cannot shadow a cached song.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicLibrary {
    root: PathBuf,
    /// Cache-backed HMP payloads, by song index.
    cached: BTreeMap<usize, Vec<u8>>,
}

impl MusicLibrary {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            cached: BTreeMap::new(),
        }
    }

    /// The default layout: `<game>/MUSIC`.
    pub fn from_game_dir(game_dir: impl AsRef<Path>) -> Self {
        Self::new(game_dir.as_ref().join("MUSIC"))
    }

    /// Use an HMP payload from the content cache instead of `MUSIC/<name>.HMP`.
    /// The cache path is tried first, so a cache asset works without the
    /// player's folder and a stale folder cannot shadow it.
    pub fn insert_hmp(&mut self, index: usize, bytes: Vec<u8>) {
        if index < SONG_COUNT {
            self.cached.insert(index, bytes);
        }
    }

    /// The song-table row for a numbered index, or `None` past index 16.
    pub fn song(index: usize) -> Option<&'static SongInfo> {
        SONGS.get(index)
    }

    /// The numeric indices that were supplied from the cache. A v28 cache has
    /// `[0]`; a v29 cache has all seventeen.
    pub fn cached_indices(&self) -> impl Iterator<Item = usize> + '_ {
        self.cached.keys().copied()
    }

    pub fn cached(&self, index: usize) -> bool {
        self.cached.contains_key(&index)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn name(index: usize) -> Option<&'static str> {
        SONG_NAMES.get(index).copied()
    }

    pub fn title(index: usize) -> Option<&'static str> {
        SONG_TITLES.get(index).copied()
    }

    /// Locate a song without opening it. The shipped files are upper-case, but
    /// the original table is lower-case and a user's `SongPath` may hold either.
    pub fn path_for(&self, index: usize) -> Option<PathBuf> {
        let name = Self::name(index)?;
        let upper = self.root.join(format!("{}.HMP", name.to_ascii_uppercase()));
        if upper.exists() {
            return Some(upper);
        }
        let lower = self.root.join(format!("{name}.hmp"));
        if lower.exists() {
            return Some(lower);
        }
        let mixed = self.root.join(format!("{name}.HMP"));
        mixed.exists().then_some(mixed)
    }

    /// True when the song exists on disk. A missing file is not a structural
    /// error: the original logs it and plays nothing.
    pub fn contains(&self, index: usize) -> bool {
        self.path_for(index).is_some()
    }

    /// Read and parse one song, preferring the cache payload when present.
    pub fn load(&self, index: usize) -> Result<HmpSong, AudioError> {
        if index >= SONG_COUNT {
            return Err(AudioError::SongOutOfRange(index));
        }
        if let Some(bytes) = self.cached.get(&index) {
            return HmpSong::parse(bytes).map_err(|source| AudioError::Hmp {
                path: PathBuf::from(format!(
                    "<cache:base:music/{}>",
                    Self::name(index).unwrap_or("song")
                )),
                source,
            });
        }
        let path = self
            .path_for(index)
            .ok_or(AudioError::SongMissing { index, path: None })?;
        let bytes = std::fs::read(&path).map_err(|source| AudioError::SongIo {
            path: path.clone(),
            source,
        })?;
        HmpSong::parse(&bytes).map_err(|source| AudioError::Hmp {
            path: path.clone(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_titles_are_complete() {
        assert_eq!(SONG_NAMES.len(), SONG_COUNT);
        assert_eq!(SONG_TITLES.len(), SONG_COUNT);
        assert_eq!(SONG_NAMES[0], "qqfront");
        assert_eq!(SONG_NAMES[10], "intro");
        assert_eq!(SONG_NAMES[16], "ggofkiev");
        assert!(MusicLibrary::title(10)
            .unwrap()
            .contains("Frontier Main Theme"));
    }

    #[test]
    fn the_song_table_is_indexable_and_names_every_file() {
        assert_eq!(SONGS.len(), SONG_COUNT);
        for (index, song) in SONGS.iter().enumerate() {
            assert_eq!(song.index, index);
            assert_eq!(song.name, SONG_NAMES[index]);
            assert_eq!(song.title, SONG_TITLES[index]);
            assert_eq!(
                song.file_name(),
                format!("{}.HMP", SONG_NAMES[index].to_uppercase())
            );
            assert_eq!(MusicLibrary::song(index), Some(song));
        }
        assert_eq!(MusicLibrary::song(SONG_COUNT), None);
    }

    #[test]
    fn direct_scene_starts_match_the_original_call_sites() {
        assert_eq!(song_for_trigger(SongTrigger::FrontEnd), Some(0));
        assert_eq!(song_for_trigger(SongTrigger::MessagePage), Some(7));
        assert_eq!(song_for_trigger(SongTrigger::MessageVideo), Some(10));
        assert_eq!(song_for_trigger(SongTrigger::Battle), Some(14));
        assert_eq!(song_for_trigger(SongTrigger::Hyperspace), Some(15));
        // F1094 and F232 use the random selector, not a fixed song.
        assert_eq!(
            SONGS
                .iter()
                .filter(|song| !song.triggers.is_empty())
                .count(),
            5
        );
    }

    #[test]
    fn only_indices_zero_to_fifteen_enter_the_random_pool() {
        assert_eq!(
            random_pool().collect::<Vec<_>>(),
            (0..=15).collect::<Vec<_>>()
        );
        assert!(!SONGS[16].random);
        assert!(SONGS[..16].iter().all(|song| song.random));
    }

    #[test]
    fn out_of_range_is_an_explicit_error() {
        let library = MusicLibrary::new("does-not-exist");
        assert!(matches!(
            library.load(SONG_COUNT),
            Err(AudioError::SongOutOfRange(index)) if index == SONG_COUNT
        ));
        assert!(matches!(
            library.load(0),
            Err(AudioError::SongMissing { index: 0, .. })
        ));
    }

    #[test]
    fn a_cached_payload_is_preferred_over_the_music_folder() {
        let mut library = MusicLibrary::new("also-missing");
        library.insert_hmp(0, b"not an HMP".to_vec());
        assert!(library.cached(0));
        assert_eq!(library.cached_indices().collect::<Vec<_>>(), vec![0]);
        // The folder is not consulted: a cache parse error is reported as Hmp,
        // not SongMissing.
        assert!(matches!(library.load(0), Err(AudioError::Hmp { .. })));
        assert!(!library.cached(1));
        assert!(matches!(
            library.load(1),
            Err(AudioError::SongMissing { index: 1, .. })
        ));
        library.insert_hmp(SONG_COUNT, b"ignored".to_vec());
        assert!(!library.cached(SONG_COUNT));
    }
}
