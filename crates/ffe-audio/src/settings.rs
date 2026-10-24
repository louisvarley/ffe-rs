use serde::{Deserialize, Serialize};

/// Number of music tracks in the original game (`SOUNDNUMSONGS`).
pub const SONG_COUNT: usize = 17;

/// Volume values are percentages, `0..=100`, so a preferences page can use a
/// slider without knowing MIDI's `0..=127` range.
pub const DEFAULT_VOLUME: u8 = 100;

/// The four settings the ESC/preferences page owns. The field names and meanings
/// are stable: `music_on` gates `Audio::play_song`, `effects_on` gates
/// `Audio::play_sample`, and the volumes scale the original's fixed levels.
///
/// This is deliberately independent of how preferences are stored or drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    /// The original's "Music on" flag (`DATA_009029`).
    pub music_on: bool,
    /// 0..=100; the port extension. The original game only had on/off.
    pub music_volume: u8,
    /// The original's "Sound effects on" flag (`DATA_009028`).
    pub effects_on: bool,
    /// 0..=100; the port extension. The original used fixed per-effect levels.
    pub effects_volume: u8,
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            music_on: true,
            music_volume: DEFAULT_VOLUME,
            effects_on: true,
            effects_volume: DEFAULT_VOLUME,
        }
    }
}

impl AudioSettings {
    /// Clamp both volumes into `0..=100`.
    pub fn clamped(self) -> Self {
        Self {
            music_on: self.music_on,
            music_volume: self.music_volume.min(100),
            effects_on: self.effects_on,
            effects_volume: self.effects_volume.min(100),
        }
    }

    /// MIDI channel volume (`CC7`) for the current music volume.
    pub fn music_midi_volume(&self) -> u8 {
        scale_volume(127, self.music_volume)
    }

    /// Apply the effects volume to an original linear level (`0..=63` for the
    /// routine's default, up to `0x7fff` after `F1908`'s shift).
    pub fn effect_level(&self, level: u8) -> u8 {
        scale_volume(level, self.effects_volume)
    }
}

/// Scale a MIDI/effect value by a percentage, rounding down and never above 127.
pub fn scale_volume(value: u8, percent: u8) -> u8 {
    let scaled = u32::from(value) * u32::from(percent.min(100)) / 100;
    scaled.min(127) as u8
}

/// The remaining sound switches from the original ESC "Sound Effects" page, and
/// the 17-entry "Choose Music From" table.
///
/// The names map to `DATA_009030..DATA_009037` and `DATA_009038[0..17]`. They are
/// separate from [`AudioSettings`] because they are scene/game-state flags rather
/// than mixer settings, but the preferences page can persist both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SoundFlags {
    /// `DATA_009030`: the station/engine ambience at sample index 16.
    pub space_station: bool,
    /// `DATA_009031`: docking/launch music.
    pub docking_music: bool,
    /// `DATA_009032`: engine noise (sample index 15).
    pub engine_sound: bool,
    /// `DATA_009033`: the Blue Danube after a hyperspace jump.
    pub music_after_hyperspace: bool,
    /// `DATA_009034`: wind samples 14/37.
    pub wind_noise: bool,
    /// `DATA_009035`: battle music (song 14).
    pub music_in_battles: bool,
    /// `DATA_009036`: icon/beep sample 20.
    pub icon_beeps: bool,
    /// `DATA_009037`: start another random song when one finishes.
    pub continuous_music: bool,
    /// `DATA_009038`: the "Choose Music From" tick boxes, indexed like the HMP
    /// file table (0 = qqfront, ..., 16 = ggofkiev).
    pub songs: [bool; SONG_COUNT],
}

impl Default for SoundFlags {
    fn default() -> Self {
        Self {
            space_station: true,
            docking_music: true,
            engine_sound: true,
            music_after_hyperspace: true,
            wind_noise: true,
            music_in_battles: true,
            icon_beeps: true,
            continuous_music: false,
            songs: [true; SONG_COUNT],
        }
    }
}

impl SoundFlags {
    pub fn song_enabled(&self, index: usize) -> bool {
        self.songs.get(index).copied().unwrap_or(false)
    }

    pub fn set_song_enabled(&mut self, index: usize, enabled: bool) {
        if let Some(song) = self.songs.get_mut(index) {
            *song = enabled;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_are_percentages_and_stay_in_midi_range() {
        let settings = AudioSettings {
            music_volume: 50,
            effects_volume: 150,
            ..AudioSettings::default()
        }
        .clamped();
        assert_eq!(settings.effects_volume, 100);
        assert_eq!(settings.music_midi_volume(), 63);
        assert_eq!(settings.effect_level(63), 63);
        assert_eq!(scale_volume(127, 0), 0);
        assert_eq!(scale_volume(127, 100), 127);
    }

    #[test]
    fn defaults_match_the_original_menu() {
        let flags = SoundFlags::default();
        assert!(flags.songs.iter().all(|enabled| *enabled));
        assert!(!flags.continuous_music);
        assert!(flags.music_after_hyperspace);
    }
}
