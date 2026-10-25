//! F1104/F917's game-over screen: the spinning tombstone.
//!
//! `F1104` (the player's destruction) sets `DATA_008870 = 0x2c`, clears the
//! screen and starts a 0x3cae6-tic wait; the state's second handler (`F1107`)
//! then calls `F918(1)` -> `F917`, which draws model 172 (or 175) rotating on
//! black for 0x3d090..-0xAAE60 tics and longjmps back to the front end.
//!
//! The screen is not a menu. The model's own opcode-10 text records are the
//! only text: `DATA_005129`/`DATA_005130` (FF9C00/01) engrave "GAME OVER" and
//! `COMMANDER <name> DIED <date>` on model 172; model 175 uses the underlined
//! FF9C02/03 variant. There are no cursor hot areas or options in this build;
//! the front end's start/load choices are what the player sees after the
//! longjmp. See `docs/notes/gameover.md`.

use crate::model_viewer::ModelViewer;
use crate::screen::Screen;
use crate::sound::{Cue, GameAudio};

/// `F1104`'s wait before `F1107` expires: `0x3cae6` tics (about 5.0 s at 1x).
pub const DEATH_VIEW_TICS: i64 = 0x3cae6;
/// `F917`'s start (0x3d090) and end (-0xAAE60) for the tombstone loop.
pub const TOMBSTONE_START_TICS: i64 = 0x3d090;
pub const TOMBSTONE_END_TICS: i64 = -0xAAE60;
/// The model the normal game-over screen draws; model 175 is the rare variant
/// `F917` selects when `DATA_008885 == 0xeb6973d`.
pub const TOMBSTONE_MODEL: u32 = 172;
pub const RARE_TOMBSTONE_MODEL: u32 = 175;
/// `DATA_008885`'s special value that selects model 175 (`ffe.asm:68069`).
pub const RARE_TOMBSTONE_TIME: u32 = 0x0eb6_973d;

/// Which of the two original tombstone models is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tombstone {
    /// Model 172, with "GAME OVER" on one face and the obituary on the other.
    Normal,
    /// Model 175, the underlined-name/date variant.
    Rare,
}

impl Tombstone {
    pub fn model(self) -> u32 {
        match self {
            Self::Normal => TOMBSTONE_MODEL,
            Self::Rare => RARE_TOMBSTONE_MODEL,
        }
    }
}

/// The original's two-stage game-over flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// `F1104`'s 0x3cae6-tic pause (the external-view wreck in the original).
    DeathView { remaining: i64 },
    /// `F917`'s spinning tombstone.
    Tombstone { remaining: i64 },
    /// The longjmp back to the front end has been reached.
    Finished,
}

/// The game-over screen's state and timing. Pure: drawing and audio are the
/// caller's, so the `death` stream can start it from its own destruction event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameOver {
    phase: Phase,
    tombstone: Tombstone,
    commander: String,
    date: String,
    announced: bool,
}

impl GameOver {
    /// `F1104`: the original pauses for 0x3cae6 tics before the tombstone.
    pub fn new(commander: impl Into<String>, date: impl Into<String>) -> Self {
        Self {
            phase: Phase::DeathView {
                remaining: DEATH_VIEW_TICS,
            },
            tombstone: Tombstone::Normal,
            commander: commander.into(),
            date: date.into(),
            announced: false,
        }
    }

    /// Start directly at `F917`, for a death stream that has already played the
    /// external-view hold and for scripted captures.
    pub fn tombstone(commander: impl Into<String>, date: impl Into<String>) -> Self {
        Self {
            phase: Phase::Tombstone {
                remaining: TOMBSTONE_START_TICS,
            },
            tombstone: Tombstone::Normal,
            commander: commander.into(),
            date: date.into(),
            announced: false,
        }
    }

    /// `F917`'s `DATA_008885 == 0xeb6973d` choice (model 175).
    pub fn with_variant(mut self, tombstone: Tombstone) -> Self {
        self.tombstone = tombstone;
        self
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn is_finished(&self) -> bool {
        self.phase == Phase::Finished
    }

    pub fn tombstone_kind(&self) -> Tombstone {
        self.tombstone
    }

    pub fn commander(&self) -> &str {
        &self.commander
    }

    pub fn date(&self) -> &str {
        &self.date
    }

    /// F917's spin angle: `(ebx >> 1)` as a binary angle. `None` outside the
    /// tombstone phase.
    pub fn angle(&self) -> Option<i32> {
        match self.phase {
            Phase::Tombstone { remaining } => Some((remaining >> 1) as i32),
            _ => None,
        }
    }

    /// Advance by one frame's tics. `F1107` subtracts the elapsed tics from the
    /// death-view timer; `F917` subtracts the same `DATA_009155` from its loop
    /// counter. At the end of the tombstone loop the original longjmps; this
    /// state reports [`Phase::Finished`] instead.
    pub fn tick(&mut self, tics: i64) -> Phase {
        match &mut self.phase {
            Phase::DeathView { remaining } => {
                *remaining -= tics;
                if *remaining <= 0 {
                    self.phase = Phase::Tombstone {
                        remaining: TOMBSTONE_START_TICS,
                    };
                }
            }
            Phase::Tombstone { remaining } => {
                *remaining -= tics;
                if *remaining <= TOMBSTONE_END_TICS {
                    self.phase = Phase::Finished;
                }
            }
            Phase::Finished => {}
        }
        self.phase
    }

    /// F1104's music/effect side: stop the song and play sample 0x1f (BOOM)
    /// before and after the escape-capsule test. Only the first call acts,
    /// mirroring the original's single transition.
    pub fn announce(&mut self, sound: &mut GameAudio) {
        if self.announced {
            return;
        }
        self.announced = true;
        sound.stop_for_sequence();
        sound.play(Cue::LargeExplosion);
        sound.play(Cue::LargeExplosion);
    }

    /// Draw the current phase. The death-view phase is black here: the original
    /// still renders the external wreck, which is the `death` stream's frame.
    pub fn draw(&self, screen: &mut Screen, viewer: &mut ModelViewer) -> Result<(), String> {
        match self.phase {
            Phase::Tombstone { .. } => {
                viewer.set_game_over_text(self.commander.as_bytes(), self.date.as_bytes());
                let angle = self.angle().unwrap_or(0);
                screen.draw_game_over(viewer, self.tombstone.model(), angle)
            }
            Phase::DeathView { .. } => {
                // F1104 clears/flips once and then the flight module keeps
                // drawing the external wreck; the death stream owns that frame.
                Ok(())
            }
            Phase::Finished => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioError;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Debug, Default)]
    struct Recorded {
        songs: Vec<i16>,
        stops: usize,
        samples: Vec<u8>,
        settings: crate::audio::AudioSettings,
        flags: crate::audio::SoundFlags,
    }

    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Recorded>>);

    impl Recorder {
        fn recorded(&self) -> std::sync::MutexGuard<'_, Recorded> {
            self.0.lock().unwrap_or_else(|poison| poison.into_inner())
        }
    }

    impl crate::audio::Audio for Recorder {
        fn play_song(&mut self, index: i16) -> Result<(), AudioError> {
            self.recorded().songs.push(index);
            Ok(())
        }

        fn stop_song(&mut self) {
            self.recorded().stops += 1;
        }

        fn song_done(&self) -> bool {
            true
        }

        fn play_sample(&mut self, code: u8, _volume: u8, _pitch: i32) {
            self.recorded().samples.push(code);
        }

        fn tick(&mut self, _elapsed: Duration) {}

        fn settings(&self) -> crate::audio::AudioSettings {
            self.recorded().settings
        }

        fn set_settings(&mut self, settings: crate::audio::AudioSettings) {
            self.recorded().settings = settings;
        }

        fn sound_flags(&self) -> crate::audio::SoundFlags {
            self.recorded().flags
        }

        fn set_sound_flags(&mut self, flags: crate::audio::SoundFlags) {
            self.recorded().flags = flags;
        }
    }

    #[test]
    fn scripted_death_screen_then_front_end() {
        let mut game = GameOver::new("JAMESON", "1-JAN-3250");
        // F1104's external-view hold.
        assert!(matches!(game.phase(), Phase::DeathView { .. }));
        game.tick(DEATH_VIEW_TICS);
        assert!(matches!(game.phase(), Phase::Tombstone { .. }));
        assert_eq!(game.tombstone_kind().model(), TOMBSTONE_MODEL);
        // F917's loop: the angle is its remaining tics >> 1.
        assert_eq!(game.angle(), Some((TOMBSTONE_START_TICS >> 1) as i32));
        game.tick(TOMBSTONE_START_TICS - TOMBSTONE_END_TICS);
        // F918(1)/F917's longjmp is the front end.
        assert_eq!(game.phase(), Phase::Finished);
    }

    #[test]
    fn death_view_runs_for_its_original_ticks_then_the_tombstone() {
        let mut game = GameOver::new("JAMESON", "1-JAN-3250");
        assert!(matches!(
            game.phase(),
            Phase::DeathView {
                remaining: DEATH_VIEW_TICS
            }
        ));
        game.tick(DEATH_VIEW_TICS - 1);
        assert!(matches!(game.phase(), Phase::DeathView { .. }));
        game.tick(1);
        assert!(matches!(
            game.phase(),
            Phase::Tombstone {
                remaining: TOMBSTONE_START_TICS
            }
        ));
    }

    #[test]
    fn tombstone_angle_matches_f917_and_finishes_at_its_end() {
        let mut game = GameOver::tombstone("JAMESON", "1-JAN-3250");
        assert_eq!(game.angle(), Some((TOMBSTONE_START_TICS >> 1) as i32));
        game.tick(1);
        assert_eq!(game.angle(), Some(((TOMBSTONE_START_TICS - 1) >> 1) as i32));
        game.tick(TOMBSTONE_START_TICS - TOMBSTONE_END_TICS);
        assert_eq!(game.phase(), Phase::Finished);
        assert!(game.is_finished());
        assert_eq!(game.angle(), None);
    }

    #[test]
    fn rare_variant_is_model_175() {
        let game = GameOver::tombstone("JAMESON", "1-JAN-3250").with_variant(Tombstone::Rare);
        assert_eq!(game.tombstone_kind().model(), RARE_TOMBSTONE_MODEL);
        assert_eq!(Tombstone::Normal.model(), TOMBSTONE_MODEL);
    }

    #[test]
    fn announce_stops_music_and_plays_the_death_boom_once() {
        let recorder = Recorder::default();
        let mut sound = GameAudio::new(recorder.clone());
        let mut game = GameOver::new("JAMESON", "1-JAN-3250");
        game.announce(&mut sound);
        game.announce(&mut sound);
        let recorded = recorder.recorded();
        assert_eq!(recorded.stops, 1);
        assert_eq!(recorded.samples, vec![0x1f, 0x1f]);
    }

    #[test]
    fn rare_time_selects_the_rare_model() {
        assert_eq!(RARE_TOMBSTONE_TIME, 0x0eb6_973d);
    }
}
