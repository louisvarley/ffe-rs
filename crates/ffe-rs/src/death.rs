//! The player's destruction and escape-capsule sequence (F1104/F1098).
//!
//! Source: `jjffe-master/ffe.asm`.
//!
//! - **F953** (`ffe.asm:71190`) uses the hull up. Its lethal branch
//!   (`71279-71287`) calls **F942** (`70650`), which plays sample `0x05`
//!   (`70713`), creates model `0x9b` (155) at the ship with its debris
//!   (`F926`, `69099`) and plays `0x1f` for a hull of mass 10 or more
//!   (`70761`). It then calls **F924** (`69099`/`70837`) to remove the object
//!   from the AI lists.
//! - The destruction event reaches **F1104** (`81458`) through
//!   **F1103/F1102** (`81442`/`81394`, the object-event entries of
//!   `DATA_007254`, `ffedat.asm:18552-18564`). F1104 runs only for the player's
//!   object slot (`81464`) and only once (`81466`).
//! - F1104 plays `0x1f` (`81474`), calls **F1098** on the player
//!   (`81479`) and returns if a craft launched. Otherwise it sets the death
//!   state `DATA_008870 = 0x2c` (`81483`), switches to the external view
//!   (`DATA_008872 = 4`, `81507`), scans the objects for F942's model-0x9b
//!   explosion (`F1105`, `81527`), sets `DATA_008884 = 0x3cae6` (`81513`,
//!   248 550 tics = 5.001 s at 49 700 tics/s), clears the GUI hot areas, sets
//!   `DATA_008835 |= 0x21` and plays `0x1f` again (`81517`).
//! - **F1098** (`80355`) tests the general-equipment dword `ship+0xc8`:
//!   bit 3 (`0x8`, the "Stowmaster" fighter record, code `0x13`) first, then
//!   bit 27 (`0x8000000`, the Escape Capsule record, code `0x0b`). There is
//!   **no cabin check** and no cash/cargo test. It allocates a model-0x10 (16,
//!   Stowmaster) or model-0x0e (14, Escape Capsule) object (`80383`), makes it
//!   the player's ship (`80393`), turns the old ship into an AI object
//!   (`+0x87 = 0xb`, `+0xff = 0x16`), and resets the new ship:
//!   - general equipment `ship+0xc8 = 0x40200000` (autopilot bit 21 and
//!     atmospheric shielding bit 30, `80411`/`80418`);
//!   - the Stowmaster gets a front 1MW pulse laser (`0x88`), drive 2 and two
//!     cabins (`80412-80415`); the capsule gets no laser and inherits the old
//!     drive (`F1533` only replaces it when the model's drive word is
//!     negative) and the old cabin count (`+0x120` is not written);
//!   - all pylons, the rear/turret lasers, both shield words and the old
//!     general word are gone (`80421-80446`), and the energy is
//!     `0x20000000` (`80447`);
//!   - the new hull is model mass * 4 (`F1533`, `91084`), the position is the
//!     old one (`80448-80461`), and the launch sample is `0x20` (`80464`).
//! - The old ship is not exploded by F1098; it is handed to the AI as state
//!   `0x16`. The port shows F942's explosion at the old position and does not
//!   simulate the derelict's later AI states.
//! - The game-over screen is F917's spinning tombstone (model 172, or 175 when
//!   `DATA_008885 == 0xeb6973d`). Its model text records expand
//!   `DATA_005129` (`"\rGAME\rOVER"`) and `DATA_005130`
//!   (`"\r\r\r\r\r\r\r  COMMANDER\r   %s\r     DIED\r  %s"`) through the
//!   `ff9c` array expansion (`F277`, `21602`, and `DATA_007649`'s `ff9c`
//!   entry, `ffedat.asm:23186`). F286/F283 are the separate status/equipment
//!   pages, not the death screen. See `docs/notes/gameover.md`.
//! - **Music:** F770 stops the song when the player is killed
//!   (`50345`/`50460`, `F1903`). The port's [`GameAudio::end_battle`] is that
//!   stop. The escape launch does not stop the song (F1098 does not call it).
//! - **Manual launch:** `x` calls F1098 directly (`F1054`, `78206`) and the
//!   F12 console icon does the same (`F1056`, `78528`). The port wires the
//!   console icon; `x` is its brake key (a controls deviation, recorded in the
//!   report/note).
//!
//! The port's old substitute (recover at the nearest starport) is gone; a
//! player without a capsule stays dead through the external-view hold and then
//! enters [`crate::gameover::GameOver`]'s tombstone, which ends by returning to
//! the front end. There are no game-over options.

use serde::{Deserialize, Serialize};

use crate::combat::Combat;
use crate::flight::{Flight, Mode};
use crate::ships::ShipStats;

/// The original's death hold, `DATA_008884 = 0x3cae6` (`F1104`, `81513`).
pub const GAME_OVER_TICS: u32 = 0x0003_cae6;
/// The game clock's tics per second (`F1900`/`GameTics`).
pub const TICS_PER_SECOND: f64 = 49_700.0;
/// The external-view hold before the game-over page, seconds.
pub const GAME_OVER_SECONDS: f64 = GAME_OVER_TICS as f64 / TICS_PER_SECOND;

/// `ship+0xc8` after F1098: bit 21 (autopilot) and bit 30 (atmospheric
/// shielding), the two general-equipment bits every escape craft carries.
pub const ESCAPE_GENERAL: u32 = 0x4020_0000;

/// The source's default commander name (`DATA_004963`); the port has no name
/// entry, so the death page uses it.
pub const COMMANDER_NAME: &str = "Jameson";

/// F1098's two escape craft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EscapeCraft {
    /// Model `0x0e` (14), the Escape Capsule (general bit 27).
    Capsule,
    /// Model `0x10` (16), the "Stowmaster" fighter (general bit 3).
    Stowmaster,
}

impl EscapeCraft {
    /// The original model table slot (`shipidx.txt`).
    pub fn model(self) -> u32 {
        match self {
            Self::Capsule => 0x0e,
            Self::Stowmaster => 0x10,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Capsule => "Escape Capsule",
            Self::Stowmaster => "\"Stowmaster\" fighter",
        }
    }

    /// `80411`/`80418`: both craft get autopilot and atmospheric shielding.
    pub fn general(self) -> u32 {
        ESCAPE_GENERAL
    }

    /// `80412`/`80419`: the Stowmaster keeps a front 1MW pulse; the capsule has
    /// no gun.
    pub fn laser(self) -> Option<u8> {
        match self {
            Self::Capsule => None,
            Self::Stowmaster => Some(ffe_core::laser::PULSE_1MW),
        }
    }

    /// `80413`: the Stowmaster is a class-2 drive. The capsule inherits the old
    /// drive unless model 14's statistics carry a drive word (`F1533`, `91085`).
    pub fn drive(self, stats: Option<&ShipStats>, old: u8) -> u8 {
        match self {
            Self::Stowmaster => 2,
            Self::Capsule => stats.map_or(old, |stats| {
                if stats.drive < 0 {
                    stats.drive as u16 as u8
                } else {
                    old
                }
            }),
        }
    }

    /// `80415`: the Stowmaster carries two cabins. F1098 does not write
    /// `player+0x120` on the capsule branch, so the capsule inherits the old
    /// count (a source quirk the port keeps).
    pub fn cabins(self, old: u16) -> u16 {
        match self {
            Self::Stowmaster => 2,
            Self::Capsule => old,
        }
    }
}

/// The local bit for the Escape Capsule record (`code 0x0b`): `F393`'s
/// `(code ^ 0x10) & 0x1f`.
pub const ESCAPE_CAPSULE_BIT: u32 = 1 << 27;
/// The local bit for the "Stowmaster" fighter record (`code 0x13`).
pub const STOWMASTER_BIT: u32 = 1 << 3;

/// F1098's equipment test, in the source's order: Stowmaster first, then the
/// Escape Capsule. `None` when neither is fitted.
pub fn escape_craft(loadout: &ffe_core::equipment::Loadout) -> Option<EscapeCraft> {
    if loadout.general & STOWMASTER_BIT != 0 {
        Some(EscapeCraft::Stowmaster)
    } else if loadout.general & ESCAPE_CAPSULE_BIT != 0 {
        Some(EscapeCraft::Capsule)
    } else {
        None
    }
}

/// The death sequence's phases. `Exploding` is F1104's external view for the
/// `0x3cae6` tics; `GameOver` is F917's spinning tombstone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeathPhase {
    Exploding,
    GameOver,
}

/// The player's death state (`DATA_008870 = 0x2c` and F1104's timer).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Death {
    pub phase: DeathPhase,
    /// Seconds left in the external-view hold (`DATA_008884`).
    pub remaining: f64,
}

impl Death {
    /// F1104's `0x3cae6` hold, starting in the external view.
    pub fn exploding() -> Self {
        Self {
            phase: DeathPhase::Exploding,
            remaining: GAME_OVER_SECONDS,
        }
    }

    /// The same state with a chosen hold, for tests and scripted frames.
    pub fn exploding_for(seconds: f64) -> Self {
        Self {
            phase: DeathPhase::Exploding,
            remaining: seconds.max(0.0),
        }
    }

    /// Advance the hold; the game-over page follows at zero.
    pub fn tick(&mut self, seconds: f64) {
        if self.phase != DeathPhase::Exploding || !seconds.is_finite() || seconds <= 0.0 {
            return;
        }
        self.remaining = (self.remaining - seconds).max(0.0);
        if self.remaining == 0.0 {
            self.phase = DeathPhase::GameOver;
        }
    }

    pub fn is_game_over(&self) -> bool {
        self.phase == DeathPhase::GameOver
    }
}

/// What F1104/F1098 did to the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// F1098 found and launched an escape craft; the player flies it.
    Escaped(EscapeCraft),
    /// No craft; F1104's death sequence started.
    Destroyed,
}

/// F1104's check and F1098's launch. `craft_stats` is the statistics block of
/// the craft the loadout selects (the caller reads it from the cache); it fills
/// the new hull from the model's mass (`F1533`).
///
/// The player is already at zero hull when this is called. With a craft the
/// state is moved into it and `None` is left in `state.death`; without one the
/// external-view hold begins.
pub fn begin(state: &mut Flight, craft_stats: Option<&ShipStats>) -> Outcome {
    // F1104 returns at once when `DATA_008870` is already 0x2c (81466).
    if state.death.is_some() {
        return Outcome::Destroyed;
    }
    let Some(craft) = escape_craft(&state.loadout) else {
        state.death = Some(Death::exploding());
        return Outcome::Destroyed;
    };
    board(state, craft, craft_stats);
    Outcome::Escaped(craft)
}

/// F1098's new-ship setup, without the equipment check. Used by the automatic
/// death path and the F12 console icon.
pub fn board(state: &mut Flight, craft: EscapeCraft, stats: Option<&ShipStats>) {
    let old_drive = state.loadout.drive.max(state.combat.drive);
    let old_cabins = state.loadout.cabins;
    let mass = stats.map_or(0, |stats| stats.mass);

    state.ship = craft.model();
    state.loadout.general = craft.general();
    state.loadout.missiles = [0; ffe_core::missile::PYLONS];
    state.loadout.shield_generators = 0;
    state.loadout.cabins = craft.cabins(old_cabins);
    // F1098's new ship has no mining machines, and the capsule's inherited
    // cabins cannot hold more passengers than it has cabins.
    state.loadout.mining = [0; ffe_core::equipment::MINING_SLOTS];
    state.loadout.occupied_cabins = state.loadout.occupied_cabins.min(state.loadout.cabins);

    state.combat = Combat {
        lasers: [craft.laser().unwrap_or(0), 0, 0, 0],
        missiles: [0; ffe_core::missile::PYLONS],
        energy: 0x2000_0000,
        heat: 0,
        refire: 0,
        drive: craft.drive(stats, old_drive),
        cooling_booster: false,
        cool_carry: 0.0,
    };
    state.loadout.drive = state.combat.drive;
    // F1533 clears both shield words; the new hull is model mass * 4.
    state.shields = [0; 2];
    state.hull = if mass == 0 {
        [0; 2]
    } else {
        [mass.saturating_mul(4); 2]
    };
    state.death = None;
    state.mode = Mode::Flight;
    state.docking_requested = false;
}

/// F942/F926's effects for the player: the model-0x9b explosion and debris at
/// the old position, with the ship's velocity and axes. `clock` seeds the
/// original's bounded random (the caller passes the game clock or ticks).
pub fn player_explosion(
    position: [i64; 3],
    velocity: [f64; 3],
    cargo: u16,
    axes: [[f64; 3]; 3],
    clock: f64,
) -> Vec<ffe_core::effects::Effect> {
    let seed = (clock * 1000.0) as u32 ^ (position[0] as u32).rotate_left(7);
    let mut rng = ffe_core::npc::Random::new(seed, seed.rotate_left(13) ^ 0x4567_89ab);
    ffe_core::effects::explosion(position, velocity, cargo, axes, &mut rng)
}

/// The no-craft branch's audible sequence: F942's `0x05` and `0x1f`, F1104's
/// `0x1f` before F1098 (`81474`), the failed F1098, and F1104's second `0x1f`
/// (`81517`). F770's song stop is
/// [`crate::sound::GameAudio::end_battle`].
pub fn play_destroyed_audio(audio: &mut crate::sound::GameAudio) {
    audio.end_battle();
    audio.play(crate::sound::Cue::ShipExplosion);
    audio.play(crate::sound::Cue::LargeExplosion);
    audio.play(crate::sound::Cue::LargeExplosion);
    audio.play(crate::sound::Cue::LargeExplosion);
}

/// F1098's launch sample (`0x20`) for the manual F12 path, where no destruction
/// preceded it.
pub fn play_escape_audio(audio: &mut crate::sound::GameAudio) {
    audio.play(crate::sound::Cue::Launch);
}

/// The automatic escape path: F942's `0x05`/`0x1f`, F1104's `0x1f` (played
/// before it calls F1098, `81474`) and F1098's `0x20`. The song is not stopped:
/// F1104 returns before the game-over branch and F1098 does not call F1903.
pub fn play_destroyed_escape_audio(audio: &mut crate::sound::GameAudio) {
    audio.play(crate::sound::Cue::ShipExplosion);
    audio.play(crate::sound::Cue::LargeExplosion);
    audio.play(crate::sound::Cue::LargeExplosion);
    audio.play(crate::sound::Cue::Launch);
}

/// The `DATA_005130` template split into lines, retained for the source check;
/// the tombstone expands the same strings through its model text records.
pub fn died_lines(name: &str) -> [String; 3] {
    [
        "COMMANDER".to_owned(),
        format!("   {name}"),
        "     DIED".to_owned(),
    ]
}

/// The port's log line for F1104's no-craft branch. The original's death path
/// does not print a message (F351's bong is for object events); the port names
/// the outcome so the log explains why the game-over page appeared.
pub fn destroyed_message(killer: &str) -> String {
    format!("{killer} destroys your ship. No escape craft fitted: GAME OVER.")
}

/// The log line for F1098's launch, with the source craft name.
pub fn escaped_message(killer: &str, craft: EscapeCraft) -> String {
    format!("{killer} destroys your ship. {} launched.", craft.name())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffe_core::equipment::Loadout;

    #[test]
    fn escape_craft_reads_the_source_bits_in_f1098s_order() {
        let mut loadout = Loadout::default();
        assert_eq!(escape_craft(&loadout), None);
        loadout.general = ESCAPE_CAPSULE_BIT;
        assert_eq!(escape_craft(&loadout), Some(EscapeCraft::Capsule));
        // F1098 tests bit 3 first, so a ship with both launches the Stowmaster.
        loadout.general |= STOWMASTER_BIT;
        assert_eq!(escape_craft(&loadout), Some(EscapeCraft::Stowmaster));
        // The equipment table's records really produce these bits.
        let capsule = ffe_core::equipment::equipment()
            .iter()
            .position(|item| item.name == "Escape Capsule")
            .unwrap();
        let stowmaster = ffe_core::equipment::equipment()
            .iter()
            .position(|item| item.name == "\"Stowmaster\" fighter")
            .unwrap();
        assert!(Loadout {
            general: 1 << ((ffe_core::equipment::get(capsule).unwrap().code ^ 0x10) & 0x1f),
            ..Loadout::default()
        }
        .has_general(capsule));
        assert!(Loadout {
            general: 1 << ((ffe_core::equipment::get(stowmaster).unwrap().code ^ 0x10) & 0x1f),
            ..Loadout::default()
        }
        .has_general(stowmaster));
    }

    #[test]
    fn the_game_over_hold_is_the_sources_0x3cae6_tics() {
        assert_eq!(GAME_OVER_TICS, 248_550);
        assert!((GAME_OVER_SECONDS - 5.001_006).abs() < 1e-6);
        let mut death = Death::exploding();
        death.tick(5.0);
        assert_eq!(death.phase, DeathPhase::Exploding);
        death.tick(0.02);
        assert_eq!(death.phase, DeathPhase::GameOver);
        assert_eq!(death.remaining, 0.0);
        // A game-over page does not tick back.
        death.tick(10.0);
        assert_eq!(death.phase, DeathPhase::GameOver);
    }

    #[test]
    fn f1098s_craft_fields_match_the_source() {
        assert_eq!(EscapeCraft::Capsule.model(), 14);
        assert_eq!(EscapeCraft::Stowmaster.model(), 16);
        assert_eq!(ESCAPE_GENERAL, 0x4020_0000);
        // Bits 21 (autopilot) and 30 (atmospheric shielding).
        assert_eq!(ESCAPE_GENERAL & (1 << 21), 1 << 21);
        assert_eq!(ESCAPE_GENERAL & (1 << 30), 1 << 30);
        assert_eq!(EscapeCraft::Capsule.laser(), None);
        assert_eq!(
            EscapeCraft::Stowmaster.laser(),
            Some(ffe_core::laser::PULSE_1MW)
        );
        assert_eq!(EscapeCraft::Stowmaster.drive(None, 9), 2);
        assert_eq!(EscapeCraft::Capsule.drive(None, 9), 9);
        assert_eq!(EscapeCraft::Capsule.cabins(4), 4);
        assert_eq!(EscapeCraft::Stowmaster.cabins(4), 2);
        // F1533 takes a negative model drive word's low byte.
        let mut block = [0u8; 48];
        block[0x14..0x16].copy_from_slice(&(-3i16).to_le_bytes());
        let stats = ShipStats::from_block(&block).unwrap();
        assert_eq!(EscapeCraft::Capsule.drive(Some(&stats), 9), 0xfd);
    }
}
