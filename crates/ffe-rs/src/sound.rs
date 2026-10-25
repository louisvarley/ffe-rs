//! Game-event sound cues and music, the port's side of the original's
//! `F1906`/`F1907`/`F1908` effects and `F1902`/`F1903`/`F1904` music.
//!
//! The original never calls a sample by name. A game routine pushes a sample
//! table index and calls `F1906` (default level 63) or `F1907` (log volume from
//! the object's `+0x88` byte); `F1908` gates the special cases and forwards the
//! index to the platform mixer. This module keeps that shape: the game posts a
//! [`Cue`], the cue resolves to the original's sample-table index, and the
//! [`Audio`](crate::audio::Audio) player receives `play_sample(index, level,
//! pitch)`.
//!
//! Music follows `F1902`: [`GameAudio`] holds the scene rules and calls
//! `play_song(index)` or `play_song(-1)` for the source's random selector. Cache
//! v29 supplies all 17 songs (v28 supplies song 0 only), and the numbered table
//! lives in `ffe_audio::SONGS`/`docs/notes/music.md`.
//!
//! Two implementation notes:
//!
//! - The effect mixer (`ffe_audio::Mixer`) decodes the cache WAVs and mixes 16
//!   voices. The system engine opens a WinMM `waveOut` device and installs the
//!   cache's `base:effect/*` payloads; a missing device (and every test/capture)
//!   falls back to the same mixer headless. The cue mapping and event wiring are
//!   independent of the device.
//! - The continuous engine/station/wind beds are one-shot cue calls here. The
//!   original re-sends `0x0f`/`0x10` with `F1910`/volume envelopes and loops a
//!   50-copy sample; the mixer implements the loop count, but the port does not
//!   yet drive the envelope from the flight state.
//!
//! Citations are to `jjffe-master/ffe.asm`; sample names are the `FX/*.RAW`
//! table in `win32/w32sound.cpp` (`pSoundSampleDescs`).

use crate::audio::{
    Audio, AudioEngine, AudioSettings, EffectLibrary, MusicLibrary, SoundFlags, EFFECT_NAMES,
    SONG_NAMES,
};
use crate::prefs::Prefs;
use ffe_content::VerifiedCache;
use std::path::Path;
use std::time::Duration;

/// `F1906`'s fixed level (`push dword 0x3f`): the default for a cue that the
/// original plays with `F1906`.
pub const DEFAULT_LEVEL: u8 = 0x3f;

/// The original sample-table indices (`pSoundSampleDescs`, 40 entries). The
/// files are `FX/<name>.RAW`; the cache stores them as `audio/fx/<name>.wav`.
pub mod sample {
    /// `shipexp`: ship destruction (`F942`, `F937`).
    pub const SHIP_EXPLOSION: u8 = 0x05;
    /// `missexpl`: missile/mine explosion (`F719`, `F720`).
    pub const MISSILE_EXPLOSION: u8 = 0x06;
    /// `laser1`: player laser bank 0..3 (`F930`).
    pub const LASER_1: u8 = 0x0a;
    /// `laser2`: player laser bank 4..15 (`F930`).
    pub const LASER_2: u8 = 0x0b;
    /// `airopen`: airlock/door/collision (`F745`, `F1056`).
    pub const AIRLOCK: u8 = 0x0c;
    /// `wind`: atmosphere wind (`F1014`).
    pub const WIND: u8 = 0x0e;
    /// `station`: engine noise bed, voice 1 (`F1092`).
    pub const ENGINE: u8 = 0x0f;
    /// `station`: space-station ambience, voice 2 (`F1910`).
    pub const STATION: u8 = 0x10;
    /// `airopen`: door/airlock (`F1026`).
    pub const AIRLOCK_INNER: u8 = 0x12;
    /// `airopen`: docking/airlock approach (`F1019`, `F1024`, `F1029`).
    pub const DOCKING: u8 = 0x13;
    /// `bip`: icon/UI beep (`F150`, `F209`, `F216`, `F361`, `F1056`).
    pub const BIP: u8 = 0x14;
    /// `bip`: the second selection beep (`F380`, `F983`, `F1059`..`F1065`).
    pub const BIP_2: u8 = 0x15;
    /// `bong`: message/notification (`F351`).
    pub const BONG: u8 = 0x16;
    /// `ecm`: ECM (`F935`).
    pub const ECM: u8 = 0x19;
    /// `necm`: naval ECM (`F935`).
    pub const NAVAL_ECM: u8 = 0x1a;
    /// `shipexp`: ship destroyed by energy damage (`F937`).
    pub const SHIP_EXPLOSION_ENERGY: u8 = 0x1b;
    /// `missexpl`: shields absorb a missile/laser hit (`F953`).
    pub const MISSILE_EXPLOSION_SHIELD: u8 = 0x1c;
    /// `damage`: hull/energy damage (`F953`).
    pub const DAMAGE: u8 = 0x1d;
    /// `siren`: battle alert/player hit (`F750`, `F941`).
    pub const SIREN: u8 = 0x1e;
    /// `boom`: large explosion (`F942`, mass >= 0xa).
    pub const BOOM: u8 = 0x1f;
    /// `launch`: launch/escape pod (`F291`, `F941`, `F1098`).
    pub const LAUNCH: u8 = 0x20;
    /// `hype`: hyperspace (`F908`).
    pub const HYPERSPACE: u8 = 0x21;
    /// `wind`: the second wind voice (`F1908`'s random second call).
    pub const WIND_2: u8 = 0x25;
    /// `camera`: view change (`F1125`).
    pub const CAMERA: u8 = 0x27;
}

/// A game event that has an original sample cue. The variants are named after
/// the situation, not the file, because several indices share `airopen` and
/// `shipexp`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cue {
    /// A GUI button/hot-area click (`F150`, `F209`, `F216`, `F361`, `F1056`).
    UiBeep,
    /// A selection/icon change (`F380`, `F983`, `F1059`..`F1065`).
    UiBeep2,
    /// A message/notification on an object event (`F351`).
    Message,
    /// The player's laser. `bank` is the original's
    /// `(DATA_008836 & 0x78) >> 3`: 0..3 is `laser1`, 4..15 `laser2`.
    Laser { bank: u8 },
    /// ECM activation (`F935`).
    Ecm,
    /// Naval ECM activation (`F935`).
    NavalEcm,
    /// A missile/object launch (`F291`).
    MissileLaunch,
    /// F1054's chaff dispenser (`0x14`, the `bip` sample).
    Chaff,
    /// A missile/mine explosion (`F719`, `F720`, `F953`).
    MissileExplosion,
    /// A ship destroyed by hull damage (`F937`, `F942`).
    ShipExplosion,
    /// The larger `boom` a destroyed hull of mass >= 0xa gets (`F942`).
    LargeExplosion,
    /// Hull/energy damage (`F953`).
    HullDamage,
    /// Shields absorb a hit (`F953`).
    ShieldHit,
    /// Battle alert or the player being hit (`F750`, `F941`).
    Siren,
    /// Undock/launch (`F1098`); `F941`'s escape launch uses the same sample.
    Launch,
    /// Hyperspace (`F908`).
    Hyperspace,
    /// A camera/view change (`F1125`).
    Camera,
    /// An airlock/door (`F745`, `F1019`, `F1024`, `F1026`, `F1029`).
    Door,
    /// A docking/landing transition (`F1088`, `F1019`).
    Docking,
    /// The engine-noise bed (`F1092`).
    Engine,
    /// The station-ambience bed (`F1910`).
    Station,
    /// Atmospheric wind (`F1014`).
    Wind,
}

/// One resolved effect call: the original table index, the linear level and
/// the pitch argument (`-1` is the 22,050 Hz default, as `F1908` passes).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CueSample {
    pub code: u8,
    pub level: u8,
    pub pitch: i32,
}

impl Cue {
    /// Resolve this cue to the original's sample call. `None` means the
    /// original's user switch suppresses it (`F1908`'s `DATA_009028`,
    /// `DATA_009030`, `DATA_009032`, `DATA_009034`, `DATA_009036`) or the cue
    /// is a deliberately silent scene marker.
    ///
    /// `AudioSettings::effects_on` and the effects volume are applied by the
    /// player, not here, exactly as `F1908` checks `DATA_009028` before the
    /// per-sample switches.
    pub fn sample(self, flags: SoundFlags) -> Option<CueSample> {
        let (code, pitch) = match self {
            Self::UiBeep => {
                if !flags.icon_beeps {
                    return None;
                }
                (sample::BIP, -1)
            }
            Self::UiBeep2 => (sample::BIP_2, -1),
            Self::Message => (sample::BONG, -1),
            Self::Laser { bank } => {
                if bank < 4 {
                    (sample::LASER_1, -1)
                } else {
                    (sample::LASER_2, -1)
                }
            }
            Self::Ecm => (sample::ECM, -1),
            Self::NavalEcm => (sample::NAVAL_ECM, -1),
            Self::MissileLaunch | Self::Launch => (sample::LAUNCH, -1),
            Self::Chaff => (sample::BIP, -1),
            Self::MissileExplosion => (sample::MISSILE_EXPLOSION, -1),
            Self::ShipExplosion => (sample::SHIP_EXPLOSION, -1),
            Self::LargeExplosion => (sample::BOOM, -1),
            Self::HullDamage => (sample::DAMAGE, -1),
            Self::ShieldHit => (sample::MISSILE_EXPLOSION_SHIELD, -1),
            Self::Siren => (sample::SIREN, -1),
            Self::Hyperspace => (sample::HYPERSPACE, -1),
            Self::Camera => (sample::CAMERA, -1),
            Self::Door => (sample::DOCKING, -1),
            Self::Docking => (sample::DOCKING, -1),
            Self::Engine => {
                if !flags.engine_sound {
                    return None;
                }
                (sample::ENGINE, -1)
            }
            Self::Station => {
                if !flags.space_station {
                    return None;
                }
                (sample::STATION, -1)
            }
            Self::Wind => {
                if !flags.wind_noise {
                    return None;
                }
                (sample::WIND, -1)
            }
        };
        Some(CueSample {
            code,
            level: DEFAULT_LEVEL,
            pitch,
        })
    }
}

/// `F1907`'s logarithmic volume: clamp negative to 0, `>> 2`, then
/// `0x3f >> shift`. `0 -> 63`, `4 -> 31`, `8 -> 15`, `12 -> 7`, `16 -> 3`,
/// `20 -> 1`, `>= 24 -> 0`.
pub fn log_volume(logvol: i16) -> u8 {
    let logvol = logvol.max(0) as u32;
    let shift = logvol >> 2;
    if shift >= 8 {
        0
    } else {
        DEFAULT_LEVEL >> shift
    }
}

/// The game-facing cue and music director. It owns the [`Audio`] player so the
/// game loop has one object, and keeps the original's scene rules (F232, F750,
/// F908/F656, F1048, F1094, F919) in one testable place.
pub struct GameAudio {
    audio: Box<dyn Audio>,
    /// F919: the front end restarts song 0 whenever it finishes.
    front_end: bool,
    /// F750/F770: a battle is in progress (song 14 / silence when it ends).
    battle: bool,
}

impl GameAudio {
    /// Wrap any player. Tests pass a recording implementation; the game passes
    /// [`AudioEngine::system`] or [`AudioEngine::silent`].
    pub fn new(audio: impl Audio + 'static) -> Self {
        Self {
            audio: Box::new(audio),
            front_end: false,
            battle: false,
        }
    }

    /// The normal game engine: the WinMM system synth when a device opens,
    /// silence otherwise. Never fails. It has no effect samples; the game uses
    /// [`Self::from_cache`], so the cache's WAVs are installed.
    pub fn from_game_dir(game_dir: impl AsRef<Path>) -> Self {
        Self::new(AudioEngine::system(MusicLibrary::from_game_dir(game_dir)))
    }

    /// The normal game engine with the cache's decoded effects and every cached
    /// song. `cache` is the verified `data.bin`; its `base:effect/*` WAVs are
    /// turned into the 16-voice mixer's sample set, and `base:music/<name>`
    /// seeds the matching song index. A v29 cache carries all 17; a v28 cache
    /// carries only song 0 (`qqfront`) and the player's `MUSIC/` folder supplies
    /// the rest; a v27 cache has no songs.
    pub fn from_cache(game_dir: impl AsRef<Path>, cache: &VerifiedCache) -> Self {
        Self::new(AudioEngine::system_with_effects(
            cached_music(game_dir, cache),
            cached_effects(cache),
        ))
    }

    /// No device, for captures and the smoke test. Cues are no-ops; music is
    /// silent; nothing in the game or its tests depends on a device.
    pub fn silent(game_dir: impl AsRef<Path>) -> Self {
        Self::new(AudioEngine::silent(MusicLibrary::from_game_dir(game_dir)))
    }

    pub fn audio(&self) -> &dyn Audio {
        &*self.audio
    }

    pub fn audio_mut(&mut self) -> &mut dyn Audio {
        &mut *self.audio
    }

    pub fn settings(&self) -> AudioSettings {
        self.audio.settings()
    }

    pub fn set_settings(&mut self, settings: AudioSettings) {
        self.audio.set_settings(settings);
    }

    pub fn flags(&self) -> SoundFlags {
        self.audio.sound_flags()
    }

    pub fn set_flags(&mut self, flags: SoundFlags) {
        self.audio.set_sound_flags(flags);
    }

    /// Set the port-extension volumes (`0..=100`). The original only has the
    /// on/off switches, so this is for a preferences page that adds sliders.
    pub fn set_volumes(&mut self, music_volume: u8, effects_volume: u8) {
        let mut settings = self.audio.settings();
        settings.music_volume = music_volume;
        settings.effects_volume = effects_volume;
        self.audio.set_settings(settings.clamped());
    }

    /// Apply the ESC "Sound Effects" page to the player. The ten rows map
    /// element-for-element to `DATA_009028..009037` (the original's order in
    /// `DATA_004965`).
    pub fn apply_prefs(&mut self, prefs: &Prefs) {
        let previous = self.audio.sound_flags();
        let mut flags = previous;
        flags.engine_sound = prefs.sound[2];
        flags.music_after_hyperspace = prefs.sound[3];
        flags.space_station = prefs.sound[4];
        flags.docking_music = prefs.sound[5];
        flags.wind_noise = prefs.sound[6];
        flags.music_in_battles = prefs.sound[7];
        flags.icon_beeps = prefs.sound[8];
        flags.continuous_music = prefs.sound[9];

        let mut settings = self.audio.settings();
        settings.effects_on = prefs.sound[0];
        settings.music_on = prefs.sound[1];
        self.audio.set_sound_flags(flags);
        self.audio.set_settings(settings.clamped());

        // F216's action 0x8a: turning continuous music on chooses a song.
        if !previous.continuous_music && flags.continuous_music {
            let _ = self.audio.play_song(-1);
        }
    }

    /// Post a cue at `F1906`'s default level.
    pub fn play(&mut self, cue: Cue) -> bool {
        self.play_level(cue, DEFAULT_LEVEL)
    }

    /// Post a cue at an explicit linear level (0..=63, as `F1908` receives).
    pub fn play_level(&mut self, cue: Cue, level: u8) -> bool {
        let Some(sample) = cue.sample(self.audio.sound_flags()) else {
            return false;
        };
        self.audio.play_sample(sample.code, level, sample.pitch);
        true
    }

    /// Post a cue using `F1907`'s log volume, e.g. an object's `+0x88` byte.
    pub fn play_log(&mut self, cue: Cue, logvol: i16) -> bool {
        self.play_level(cue, log_volume(logvol))
    }

    pub fn song_done(&self) -> bool {
        self.audio.song_done()
    }

    /// The current song index (`player+0x5334`), or `None` when stopped.
    pub fn current_song(&self) -> Option<usize> {
        self.audio.current_song()
    }

    pub fn play_song(&mut self, index: i16) -> bool {
        self.audio.play_song(index).is_ok()
    }

    pub fn stop_song(&mut self) {
        self.audio.stop_song();
    }

    /// `F919`: the front end plays song 0 and restarts it when it finishes.
    pub fn front_end_music(&mut self) {
        self.front_end = true;
        let _ = self.audio.play_song(0);
    }

    /// `F919` cleanup: leaving the front end stops its theme.
    pub fn leave_front_end(&mut self) {
        self.front_end = false;
        self.audio.stop_song();
    }

    /// `F306`: an FF-code page/row change plays song 7 (`qqparadi`). The current
    /// port has no FF-code message renderer, so this is the source rule exposed
    /// for the future message path rather than a trigger invented by the UI.
    pub fn message_page_music(&mut self) {
        let _ = self.audio.play_song(7);
    }

    /// `F307`: the message/video sequence plays song 10 (`intro`) once per
    /// message bit. Like [`Self::message_page_music`], the feed itself is not
    /// ported; the rule is kept here so a future FF-code path cannot drift.
    pub fn message_video_music(&mut self) {
        let _ = self.audio.play_song(10);
    }

    /// `F750`/`F236`: battle start. Song 14 plays only when "Music in battles"
    /// (`DATA_009035`) and "Music on" (`DATA_009029`) are set; otherwise the
    /// original stops the song. The siren is `F750`'s `0x1e`.
    pub fn start_battle(&mut self) {
        if self.battle {
            return;
        }
        self.battle = true;
        self.play(Cue::Siren);
        self.battle_music();
    }

    /// `F770`: combat ends (or the player dies); stop the song.
    pub fn end_battle(&mut self) {
        self.battle = false;
        self.audio.stop_song();
    }

    /// `F770`/`F941`: the player's ship was destroyed. The original stops the
    /// song and clears the scene music; the port's recovery at the starport must
    /// not leave battle music playing.
    pub fn player_death(&mut self) {
        self.front_end = false;
        self.battle = false;
        self.audio.stop_song();
    }

    fn battle_music(&mut self) {
        if self.audio.sound_flags().music_in_battles && self.audio.settings().music_on {
            let _ = self.audio.play_song(14);
        } else {
            self.audio.stop_song();
        }
    }

    /// `F908` + `F656`: after a jump, song 15 (`bluedan`) if "Music after
    /// hyperspace" (`DATA_009033`) is set.
    pub fn hyperspace_music(&mut self) {
        if self.audio.sound_flags().music_after_hyperspace && self.audio.settings().music_on {
            let _ = self.audio.play_song(15);
        }
    }

    /// `F1094`: the end of a launch sequence chooses a random enabled song when
    /// "Docking music" (`player+0x3f5f`) is set. A station *docked* state is
    /// F1903's stop, not this call (`GameAudio::stop_song`).
    pub fn sequence_end_music(&mut self) {
        if self.audio.sound_flags().docking_music && self.audio.settings().music_on {
            let _ = self.audio.play_song(-1);
        }
    }

    /// `F1047`/`F1048`: a rough landing changes no music; an open-air starport's
    /// F1048 stops the song (F1903) before F1049/F1050 draw the landed view.
    pub fn landed(&mut self, starport: bool) {
        if starport {
            self.audio.stop_song();
        }
    }

    /// `F1048`/`F1091`: a docking or launch transition stops the song.
    pub fn stop_for_sequence(&mut self) {
        self.front_end = false;
        self.battle = false;
        self.audio.stop_song();
    }

    /// `F919`'s song-done restart and `F232`'s continuous-music event. Called
    /// from [`GameAudio::tick`]; the original's F232 is the SysMenu event 10,
    /// but checking once per frame is equivalent because `F1904` is a state
    /// query.
    pub fn update_music(&mut self) {
        if self.front_end {
            if self.audio.song_done() {
                let _ = self.audio.play_song(0);
            }
            return;
        }
        let flags = self.audio.sound_flags();
        if flags.continuous_music && self.audio.settings().music_on && self.audio.song_done() {
            let _ = self.audio.play_song(-1);
        }
    }

    /// Advance the player and the scene music. Call once per frame with the
    /// wall time; the MIDI player is tick-driven and does not spin a thread.
    pub fn tick(&mut self, elapsed: Duration) {
        self.audio.tick(elapsed);
        self.update_music();
    }
}

/// Seed a [`MusicLibrary`] from every `base:music/<name>` payload the cache has.
/// A v29 cache supplies all 17; a v28 cache supplies only `qqfront`; older
/// caches supply none. The caller's `MUSIC/` folder remains the fallback.
fn cached_music(game_dir: impl AsRef<Path>, cache: &VerifiedCache) -> MusicLibrary {
    let mut music = MusicLibrary::from_game_dir(game_dir);
    for (index, name) in SONG_NAMES.iter().enumerate() {
        let id = format!("base:music/{name}");
        if let Some(bytes) = cache.asset(&id) {
            music.insert_hmp(index, bytes.to_vec());
        }
    }
    music
}

/// Decode every cached `base:effect/<name>` WAV. A missing asset is not an
/// error (a v27 cache has them all; a hand-built cache may not), and a malformed
/// one is reported once and skipped rather than disabling the rest.
fn cached_effects(cache: &VerifiedCache) -> EffectLibrary {
    let mut effects = EffectLibrary::new();
    for name in EFFECT_NAMES {
        let id = format!("base:effect/{name}");
        let Some(bytes) = cache.asset(&id) else {
            continue;
        };
        if let Err(error) = effects.insert_wav(name, bytes) {
            eprintln!("Effect {id}: {error}");
        }
    }
    effects
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioError, SONG_COUNT};
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Default)]
    struct Recorded {
        samples: Vec<(u8, u8, i32)>,
        songs: Vec<i16>,
        stops: usize,
        done: bool,
        current: Option<usize>,
        settings: AudioSettings,
        flags: SoundFlags,
    }

    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Recorded>>);

    impl Recorder {
        fn recorded(&self) -> std::sync::MutexGuard<'_, Recorded> {
            self.0.lock().unwrap_or_else(|poison| poison.into_inner())
        }

        fn codes(&self) -> Vec<u8> {
            self.recorded().samples.iter().map(|call| call.0).collect()
        }
    }

    impl Audio for Recorder {
        fn play_song(&mut self, index: i16) -> Result<(), AudioError> {
            let mut recorded = self.recorded();
            if !recorded.settings.music_on {
                return Ok(());
            }
            if let Ok(index) = usize::try_from(index) {
                if !recorded.flags.song_enabled(index) {
                    return Ok(());
                }
            }
            recorded.songs.push(index);
            recorded.done = false;
            recorded.current = usize::try_from(index).ok();
            Ok(())
        }

        fn stop_song(&mut self) {
            let mut recorded = self.recorded();
            recorded.stops += 1;
            recorded.done = true;
            recorded.current = None;
        }

        fn song_done(&self) -> bool {
            self.recorded().done
        }

        fn current_song(&self) -> Option<usize> {
            self.recorded().current
        }

        fn play_sample(&mut self, code: u8, volume: u8, pitch: i32) {
            self.recorded().samples.push((code, volume, pitch));
        }

        fn tick(&mut self, _elapsed: Duration) {}

        fn settings(&self) -> AudioSettings {
            self.recorded().settings
        }

        fn set_settings(&mut self, settings: AudioSettings) {
            self.recorded().settings = settings;
        }

        fn sound_flags(&self) -> SoundFlags {
            self.recorded().flags
        }

        fn set_sound_flags(&mut self, flags: SoundFlags) {
            self.recorded().flags = flags;
        }
    }

    #[test]
    fn cue_table_is_the_original_sample_indices() {
        let flags = SoundFlags::default();
        let cases = [
            (Cue::UiBeep, sample::BIP),
            (Cue::UiBeep2, sample::BIP_2),
            (Cue::Message, sample::BONG),
            (Cue::Ecm, sample::ECM),
            (Cue::NavalEcm, sample::NAVAL_ECM),
            (Cue::MissileLaunch, sample::LAUNCH),
            (Cue::MissileExplosion, sample::MISSILE_EXPLOSION),
            (Cue::ShipExplosion, sample::SHIP_EXPLOSION),
            (Cue::LargeExplosion, sample::BOOM),
            (Cue::HullDamage, sample::DAMAGE),
            (Cue::ShieldHit, sample::MISSILE_EXPLOSION_SHIELD),
            (Cue::Siren, sample::SIREN),
            (Cue::Launch, sample::LAUNCH),
            (Cue::Hyperspace, sample::HYPERSPACE),
            (Cue::Camera, sample::CAMERA),
            (Cue::Door, sample::DOCKING),
            (Cue::Docking, sample::DOCKING),
            (Cue::Engine, sample::ENGINE),
            (Cue::Station, sample::STATION),
            (Cue::Wind, sample::WIND),
        ];
        for (cue, code) in cases {
            let resolved = cue.sample(flags).unwrap();
            assert_eq!(resolved.code, code, "{cue:?}");
            assert_eq!(resolved.level, DEFAULT_LEVEL);
            assert_eq!(resolved.pitch, -1);
        }
    }

    #[test]
    fn laser_bank_selects_f930s_two_samples() {
        let flags = SoundFlags::default();
        for bank in 0..4 {
            assert_eq!(
                Cue::Laser { bank }.sample(flags).unwrap().code,
                sample::LASER_1
            );
        }
        for bank in 4..16 {
            assert_eq!(
                Cue::Laser { bank }.sample(flags).unwrap().code,
                sample::LASER_2
            );
        }
    }

    #[test]
    fn the_preference_switches_gate_the_special_cases() {
        let mut flags = SoundFlags {
            icon_beeps: false,
            ..SoundFlags::default()
        };
        assert!(Cue::UiBeep.sample(flags).is_none());
        assert!(Cue::UiBeep2.sample(flags).is_some());
        flags.engine_sound = false;
        assert!(Cue::Engine.sample(flags).is_none());
        flags.space_station = false;
        assert!(Cue::Station.sample(flags).is_none());
        flags.wind_noise = false;
        assert!(Cue::Wind.sample(flags).is_none());
        // Everything else is only gated by the master effects switch, which
        // the player applies.
        assert!(Cue::Laser { bank: 0 }.sample(flags).is_some());
    }

    #[test]
    fn log_volume_is_f1907s_shift() {
        assert_eq!(log_volume(-1), 0x3f);
        assert_eq!(log_volume(0), 63);
        assert_eq!(log_volume(3), 63);
        assert_eq!(log_volume(4), 31);
        assert_eq!(log_volume(8), 15);
        assert_eq!(log_volume(12), 7);
        assert_eq!(log_volume(16), 3);
        assert_eq!(log_volume(20), 1);
        assert_eq!(log_volume(24), 0);
        assert_eq!(log_volume(0x7fff), 0);
    }

    #[test]
    fn game_audio_records_cues_at_the_default_level() {
        let recorder = Recorder::default();
        let mut game = GameAudio::new(recorder.clone());
        assert!(game.play(Cue::Laser { bank: 0 }));
        assert!(game.play(Cue::ShieldHit));
        assert!(game.play(Cue::ShipExplosion));
        assert!(game.play(Cue::Launch));
        assert_eq!(recorder.recorded().samples[0], (sample::LASER_1, 63, -1));
        assert_eq!(
            recorder.codes(),
            vec![
                sample::LASER_1,
                sample::MISSILE_EXPLOSION_SHIELD,
                sample::SHIP_EXPLOSION,
                sample::LAUNCH
            ]
        );

        assert!(game.play_log(Cue::Laser { bank: 0 }, 8));
        assert_eq!(
            recorder.recorded().samples.last(),
            Some(&(sample::LASER_1, 15, -1))
        );
    }

    #[test]
    fn battle_hyperspace_and_sequence_music_follow_the_original_rules() {
        let recorder = Recorder::default();
        let mut game = GameAudio::new(recorder.clone());
        game.start_battle();
        game.start_battle();
        assert_eq!(recorder.recorded().songs, vec![14]);
        assert_eq!(recorder.codes(), vec![sample::SIREN]);

        game.end_battle();
        assert_eq!(recorder.recorded().stops, 1);

        game.hyperspace_music();
        game.sequence_end_music();
        assert_eq!(recorder.recorded().songs, vec![14, 15, -1]);

        let mut flags = game.flags();
        flags.music_after_hyperspace = false;
        flags.docking_music = false;
        game.set_flags(flags);
        let before = recorder.recorded().songs.len();
        game.hyperspace_music();
        game.sequence_end_music();
        assert_eq!(recorder.recorded().songs.len(), before);
    }

    #[test]
    fn a_starport_landing_stops_the_song_and_a_rough_landing_leaves_it() {
        let recorder = Recorder::default();
        let mut game = GameAudio::new(recorder.clone());
        game.landed(false);
        assert_eq!(recorder.recorded().stops, 0);
        game.landed(true);
        assert_eq!(recorder.recorded().stops, 1);
    }

    #[test]
    fn front_end_theme_loops_and_continuous_music_chooses_again() {
        let recorder = Recorder::default();
        let mut game = GameAudio::new(recorder.clone());
        game.front_end_music();
        assert_eq!(recorder.recorded().songs, vec![0]);
        // F919: song_done -> play song 0 again.
        recorder.recorded().done = true;
        game.update_music();
        assert_eq!(recorder.recorded().songs, vec![0, 0]);

        let mut flags = game.flags();
        flags.continuous_music = true;
        game.set_flags(flags);
        game.leave_front_end();
        recorder.recorded().done = true;
        game.update_music();
        assert_eq!(recorder.recorded().songs, vec![0, 0, -1]);
    }

    #[test]
    fn the_scene_music_sequence_never_leaves_a_stale_song() {
        let recorder = Recorder::default();
        let mut game = GameAudio::new(recorder.clone());
        game.front_end_music();
        assert_eq!(game.current_song(), Some(0));

        game.leave_front_end();
        assert_eq!(game.current_song(), None);
        assert_eq!(recorder.recorded().stops, 1);

        game.start_battle();
        assert_eq!(game.current_song(), Some(14));
        game.player_death();
        assert_eq!(game.current_song(), None);
        assert_eq!(recorder.recorded().stops, 2);

        game.hyperspace_music();
        assert_eq!(game.current_song(), Some(15));
        game.stop_for_sequence();
        assert_eq!(game.current_song(), None);

        game.sequence_end_music();
        assert_eq!(recorder.recorded().songs.last(), Some(&-1));
    }

    #[test]
    fn music_off_suppresses_every_scene_start_and_message_songs_use_their_indices() {
        let recorder = Recorder::default();
        let mut game = GameAudio::new(recorder.clone());
        game.set_settings(AudioSettings {
            music_on: false,
            ..AudioSettings::default()
        });
        game.front_end_music();
        game.start_battle();
        game.hyperspace_music();
        game.sequence_end_music();
        game.message_page_music();
        game.message_video_music();
        // The recorder accepts the calls (the engine gates them), so check the
        // source indices directly rather than the recorded song list.
        game.set_settings(AudioSettings::default());
        game.message_page_music();
        game.message_video_music();
        assert_eq!(recorder.recorded().songs, vec![7, 10]);
    }

    #[test]
    fn cache_music_seeds_all_v29_songs_and_only_song_zero_from_v28() {
        fn hmp() -> Vec<u8> {
            fn delta(value: u32) -> Vec<u8> {
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
            let mut track = Vec::new();
            track.extend_from_slice(&delta(0));
            track.extend_from_slice(&[0x90, 60, 64]);
            track.extend_from_slice(&delta(192));
            track.extend_from_slice(&[0xff, 0x2f, 0x00]);
            let mut image = vec![0u8; 0x40];
            image[..14].copy_from_slice(b"HMIMIDIP013195");
            image[0x30] = 2;
            image.extend_from_slice(&[0xff, 0x2f, 0x00]);
            image.extend_from_slice(&1u32.to_le_bytes());
            image.extend_from_slice(&((track.len() as u32) + 12).to_le_bytes());
            image.extend_from_slice(&[0, 0, 0, 0]);
            image.extend_from_slice(&track);
            image.extend_from_slice(&[0, 0, 0, 0]);
            image
        }

        fn cache(version: u32, indices: std::ops::Range<usize>) -> VerifiedCache {
            let manifest = ffe_content::Manifest {
                format: "ffe-content-cache".to_owned(),
                format_version: version,
                tool_version: "test".to_owned(),
                source_profile: "synthetic".to_owned(),
                source_files: vec![ffe_content::SourceFile {
                    path: "FIRSTENC.EXE".to_owned(),
                    size: 1,
                    sha256: "a".repeat(64),
                }],
                complete_game_assets: false,
                unresolved: vec!["synthetic".to_owned()],
                files: Vec::new(),
            };
            let assets = indices
                .map(|index| ffe_content::Asset {
                    id: format!("base:music/{}", SONG_NAMES[index]),
                    path: format!("music/{}.hmp", SONG_NAMES[index]),
                    kind: ffe_content::AssetKind::MusicHmp {
                        index: index as u32,
                    },
                    bytes: hmp(),
                })
                .collect();
            let bytes = ffe_content::build(manifest, assets).unwrap();
            ffe_content::read(&bytes).unwrap()
        }

        let v29 = cache(29, 0..SONG_COUNT);
        let music = cached_music("missing-music", &v29);
        assert_eq!(
            music.cached_indices().collect::<Vec<_>>(),
            (0..SONG_COUNT).collect::<Vec<_>>()
        );

        let v28 = cache(28, 0..1);
        let music = cached_music("missing-music", &v28);
        assert_eq!(music.cached_indices().collect::<Vec<_>>(), vec![0]);
    }

    #[test]
    fn the_sound_page_maps_to_settings_and_flags() {
        let recorder = Recorder::default();
        let mut game = GameAudio::new(recorder.clone());
        let mut prefs = Prefs {
            sound: [
                true, true, false, false, true, false, true, false, true, true,
            ],
            ..Prefs::default()
        };
        game.apply_prefs(&prefs);
        let settings = game.audio().settings();
        assert!(settings.effects_on);
        assert!(settings.music_on);
        let flags = game.audio().sound_flags();
        assert!(!flags.engine_sound);
        assert!(!flags.music_after_hyperspace);
        assert!(flags.space_station);
        assert!(!flags.docking_music);
        assert!(flags.wind_noise);
        assert!(!flags.music_in_battles);
        assert!(flags.icon_beeps);
        assert!(flags.continuous_music);

        prefs.sound[0] = false;
        prefs.sound[1] = false;
        game.apply_prefs(&prefs);
        assert!(!game.audio().settings().effects_on);
        assert!(!game.audio().settings().music_on);
    }
}
