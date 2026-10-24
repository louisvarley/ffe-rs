//! Game-facing re-export of the `ffe-audio` API.
//!
//! The ESC/preferences page should use this module (or depend on `ffe-audio`
//! directly): `AudioSettings`/`SoundFlags` are the shared settings types, and
//! `Audio` is the player trait. See `docs/notes/sound.md` §4 for the exact
//! contract.

pub use ffe_audio::*;

use std::path::Path;

/// Open the player's `MUSIC` folder and prefer the system MIDI device, falling
/// back to silence when no device is available. This never fails.
pub fn open_game_audio(game_dir: impl AsRef<Path>) -> AudioEngine {
    AudioEngine::system(MusicLibrary::from_game_dir(game_dir))
}
