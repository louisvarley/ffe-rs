use crate::AudioError;
use ffe_core::hmp::HmpSong;
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
/// The port reads the player's files at runtime, as the original does. The 17
/// HMPs are not in `data.bin`: they are player-supplied content and are never
/// committed. `build-data` may add converted SMF entries later, but playback
/// does not depend on a cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MusicLibrary {
    root: PathBuf,
}

impl MusicLibrary {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The default layout: `<game>/MUSIC`.
    pub fn from_game_dir(game_dir: impl AsRef<Path>) -> Self {
        Self::new(game_dir.as_ref().join("MUSIC"))
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

    /// Read and parse one song.
    pub fn load(&self, index: usize) -> Result<HmpSong, AudioError> {
        if index >= SONG_COUNT {
            return Err(AudioError::SongOutOfRange(index));
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
}
