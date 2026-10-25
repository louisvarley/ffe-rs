//! HMP music playback and the interface the preferences page uses.
//!
//! `ffe_core::hmp` parses the 17 original HMP songs. This crate turns them into
//! timed MIDI messages, scales the user's volume, and sends them to a system MIDI
//! device (WinMM on Windows) or to a recording/null backend for tests.
//!
//! The crate deliberately does not depend on the game's screen or preferences
//! code. See `docs/notes/sound.md` for the original routine references and the
//! exact integration contract.

mod audio;
pub mod library;
pub mod mixer;
pub mod player;
pub mod settings;
pub mod sink;
#[cfg(target_os = "windows")]
mod waveout;

pub use audio::{Audio, AudioEngine, AudioError};
pub use library::{
    random_pool, song_for_trigger, MusicLibrary, SongInfo, SongTrigger, SONGS, SONG_NAMES,
    SONG_TITLES,
};
pub use mixer::{
    decode_wav, level_centibels, level_gain, sample_desc, EffectLibrary, EffectSample, Mixer,
    SampleDesc, WavError, EFFECT_NAMES, SAMPLE_RATE, SAMPLE_TABLE, VOICE_COUNT,
};
pub use player::{LoopMode, MusicPlayer};
pub use settings::{AudioSettings, SoundFlags, SONG_COUNT};
pub use sink::{
    MidiSink, MixerSampleSink, NullMidiSink, NullSampleSink, RecordingMidiSink, RecordingMixSink,
    RecordingSampleSink, SampleCall, SampleSink,
};

#[cfg(target_os = "windows")]
pub use sink::WindowsMidiSink;

pub use ffe_core::hmp::{HmpError, HmpEvent, HmpEventKind, HmpSong, HmpTrack};
