use crate::library::MusicLibrary;
use crate::player::MusicPlayer;
use crate::settings::{AudioSettings, SoundFlags, SONG_COUNT};
use crate::sink::{MidiSink, NullMidiSink, NullSampleSink, SampleSink};
use ffe_core::hmp::HmpError;
use std::fmt;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Errors from loading or playing audio. A missing song is reported explicitly;
/// the original game only logs it and continues, so callers may choose to ignore
/// `SongMissing` in the same way.
#[derive(Debug)]
pub enum AudioError {
    SongOutOfRange(usize),
    SongMissing {
        index: usize,
        path: Option<PathBuf>,
    },
    SongIo {
        path: PathBuf,
        source: std::io::Error,
    },
    Hmp {
        path: PathBuf,
        source: HmpError,
    },
    MidiUnavailable(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SongOutOfRange(index) => write!(formatter, "song index {index} is out of range"),
            Self::SongMissing { index, path } => {
                write!(formatter, "song {index} is missing")?;
                if let Some(path) = path {
                    write!(formatter, " at {}", path.display())?;
                }
                Ok(())
            }
            Self::SongIo { path, source } => {
                write!(formatter, "{}: {source}", path.display())
            }
            Self::Hmp { path, source } => write!(formatter, "{}: {source}", path.display()),
            Self::MidiUnavailable(reason) => write!(formatter, "MIDI output unavailable: {reason}"),
        }
    }
}

impl std::error::Error for AudioError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::SongIo { source, .. } => Some(source),
            Self::Hmp { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// The game-facing audio API. It is object-safe, so the preferences page can hold
/// `Box<dyn Audio>` or a concrete [`AudioEngine`].
pub trait Audio {
    /// Play a song by table index. A negative index is the original `F1902(-1)`
    /// random selection (0..=15, enabled, not the current song). `0..=16` is
    /// explicit. `music_on == false` makes this a no-op.
    fn play_song(&mut self, index: i16) -> Result<(), AudioError>;

    /// Stop the current song and silence all channels (`F1903`).
    fn stop_song(&mut self);

    /// True when no song is playing (`F1904`).
    fn song_done(&self) -> bool;

    /// Play an effect by its original sample-table index. The code is the index
    /// from `w32sound.cpp` (`0x14` = bip, `0x1e` = siren, ...), not a file name.
    fn play_sample(&mut self, code: u8, volume: u8, pitch: i32);

    /// Advance the player. Call once per frame with the elapsed wall time.
    fn tick(&mut self, elapsed: Duration);

    fn settings(&self) -> AudioSettings;
    fn set_settings(&mut self, settings: AudioSettings);

    fn sound_flags(&self) -> SoundFlags;
    fn set_sound_flags(&mut self, flags: SoundFlags);
}

/// The concrete player. It owns a [`MusicLibrary`], a MIDI sink, an effects sink,
/// and the current settings.
pub struct AudioEngine {
    library: MusicLibrary,
    midi: Box<dyn MidiSink>,
    samples: Box<dyn SampleSink>,
    player: MusicPlayer,
    settings: AudioSettings,
    flags: SoundFlags,
    rng: u32,
}

impl AudioEngine {
    /// Build an engine with explicit sinks. A game can pass a
    /// [`crate::WindowsMidiSink`] on Windows and a recording/null sink in tests.
    pub fn new(
        library: MusicLibrary,
        midi: impl MidiSink + 'static,
        samples: impl SampleSink + 'static,
    ) -> Self {
        Self::with_rng_seed(library, midi, samples, time_seed())
    }

    pub fn with_rng_seed(
        library: MusicLibrary,
        midi: impl MidiSink + 'static,
        samples: impl SampleSink + 'static,
        seed: u32,
    ) -> Self {
        Self {
            library,
            midi: Box::new(midi),
            samples: Box::new(samples),
            player: MusicPlayer::new(),
            settings: AudioSettings::default(),
            flags: SoundFlags::default(),
            rng: seed,
        }
    }

    /// No device: useful in CI and for `--no-audio` runs.
    pub fn silent(library: MusicLibrary) -> Self {
        Self::new(library, NullMidiSink, NullSampleSink)
    }

    /// Open the WinMM system-MIDI device. `None` uses `MIDI_MAPPER`.
    #[cfg(target_os = "windows")]
    pub fn windows(library: MusicLibrary, device: Option<u32>) -> Result<Self, AudioError> {
        let sink =
            crate::sink::WindowsMidiSink::open(device).map_err(AudioError::MidiUnavailable)?;
        Ok(Self::new(library, sink, NullSampleSink))
    }

    /// Prefer the system MIDI device, falling back to silence when none is
    /// available. This is the helper a game loop should normally use.
    pub fn system(library: MusicLibrary) -> Self {
        #[cfg(target_os = "windows")]
        if let Ok(engine) = Self::windows(library.clone(), None) {
            return engine;
        }
        Self::silent(library)
    }

    pub fn library(&self) -> &MusicLibrary {
        &self.library
    }

    pub fn player(&self) -> &MusicPlayer {
        &self.player
    }

    pub fn player_mut(&mut self) -> &mut MusicPlayer {
        &mut self.player
    }

    /// Play an explicit index without the `i16` negative-random convention.
    pub fn play_song_index(&mut self, index: usize) -> Result<(), AudioError> {
        let index = i16::try_from(index).map_err(|_| AudioError::SongOutOfRange(index))?;
        self.play_song(index)
    }

    /// The `F232` update: when continuous music is enabled and the current song
    /// has finished, choose another one. The game should call this from its
    /// event-10/song-update hook, not every frame.
    pub fn update_song(&mut self) -> Result<(), AudioError> {
        if self.settings.music_on && self.flags.continuous_music && self.player.song_done() {
            self.play_song(-1)?;
        }
        Ok(())
    }

    /// Pick the next random song exactly like `F1902(-1)`: ten attempts over
    /// indices 0..=15, skipping disabled songs and the current one. Index 16 is
    /// never chosen by the original's `push 0x10` random call.
    fn choose_random(&mut self) -> Option<usize> {
        let current = self.player.current_song();
        for _ in 0..10 {
            let index = usize::from(self.next_random(16));
            if self.flags.song_enabled(index) && Some(index) != current {
                return Some(index);
            }
        }
        None
    }

    /// The original `FUNC_000922(n)` scales a 16-bit random value by `n >> 16`.
    fn next_random(&mut self, limit: u16) -> u16 {
        self.rng = self.rng.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (((self.rng >> 16) * u32::from(limit)) >> 16) as u16
    }

    fn play_selected(&mut self, index: usize) -> Result<(), AudioError> {
        let song = self.library.load(index)?;
        self.player
            .play(song, index, &mut *self.midi, self.settings);
        Ok(())
    }
}

impl Audio for AudioEngine {
    fn play_song(&mut self, index: i16) -> Result<(), AudioError> {
        if !self.settings.music_on {
            return Ok(());
        }
        let index = if index < 0 {
            match self.choose_random() {
                Some(index) => index,
                None => return Ok(()),
            }
        } else {
            let index = usize::try_from(index).map_err(|_| AudioError::SongOutOfRange(0))?;
            if index >= SONG_COUNT {
                return Err(AudioError::SongOutOfRange(index));
            }
            index
        };
        if !self.flags.song_enabled(index) {
            return Ok(());
        }
        if self.player.current_song() == Some(index) && self.player.is_playing() {
            return Ok(());
        }
        self.play_selected(index)
    }

    fn stop_song(&mut self) {
        self.player.stop(&mut *self.midi);
    }

    fn song_done(&self) -> bool {
        self.player.song_done()
    }

    fn play_sample(&mut self, code: u8, volume: u8, pitch: i32) {
        if !self.settings.effects_on {
            return;
        }
        self.samples
            .play_sample(code, self.settings.effect_level(volume), pitch);
    }

    fn tick(&mut self, elapsed: Duration) {
        self.player.tick(elapsed, &mut *self.midi, self.settings);
    }

    fn settings(&self) -> AudioSettings {
        self.settings
    }

    fn set_settings(&mut self, settings: AudioSettings) {
        let settings = settings.clamped();
        let effects_were_on = self.settings.effects_on;
        self.settings = settings;
        if !settings.music_on {
            self.player.stop(&mut *self.midi);
        } else {
            self.player.set_settings(settings, &mut *self.midi);
        }
        if effects_were_on && !settings.effects_on {
            self.samples.stop_all();
        }
    }

    fn sound_flags(&self) -> SoundFlags {
        self.flags
    }

    fn set_sound_flags(&mut self, flags: SoundFlags) {
        self.flags = flags;
    }
}

fn time_seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0x1234_5678, |duration| duration.as_nanos() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::{RecordingMidiSink, RecordingSampleSink};

    #[test]
    fn random_selection_skips_disabled_and_sixteen() {
        let midi = RecordingMidiSink::new();
        let mut engine =
            AudioEngine::with_rng_seed(MusicLibrary::new("missing"), midi, NullSampleSink, 7);
        let mut only_sixteen = SoundFlags::default();
        only_sixteen
            .songs
            .iter_mut()
            .enumerate()
            .for_each(|(index, enabled)| *enabled = index == 16);
        engine.set_sound_flags(only_sixteen);
        assert_eq!(engine.choose_random(), None);

        let mut only_two = SoundFlags::default();
        only_two
            .songs
            .iter_mut()
            .enumerate()
            .for_each(|(index, enabled)| *enabled = index == 2);
        engine.set_sound_flags(only_two);
        let chosen = (0..100).find_map(|_| engine.choose_random());
        assert_eq!(chosen, Some(2));
    }

    #[test]
    fn settings_gate_music_and_scale_effects() {
        let midi = RecordingMidiSink::new();
        let samples = RecordingSampleSink::new();
        let mut engine = AudioEngine::new(MusicLibrary::new("missing"), midi, samples.clone());
        engine.set_settings(AudioSettings {
            effects_on: true,
            effects_volume: 50,
            ..AudioSettings::default()
        });
        engine.play_sample(20, 63, -1);
        assert_eq!(
            samples.calls(),
            vec![crate::SampleCall {
                code: 20,
                volume: 31,
                pitch: -1
            }]
        );

        engine.set_settings(AudioSettings {
            effects_on: false,
            effects_volume: 100,
            music_on: false,
            music_volume: 100,
        });
        engine.play_sample(20, 63, -1);
        assert_eq!(samples.calls().len(), 1);
        assert!(engine.play_song(0).is_ok());
        assert!(engine.song_done());
    }
}
