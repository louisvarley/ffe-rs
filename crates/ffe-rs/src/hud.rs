//! The player's own F1099 HUD text (`ffe.asm:80489-80855`, `80700-80760`).
//!
//! `F1099` writes the flight HUD straight into the low buffer at fixed spots,
//! with no panel art behind it:
//!
//! | text | string | position | colour | source |
//! |---|---|---|---|---|
//! | player shields | `0x8624` (`Shields: %i%%`) | (2,1) | 15 | `80780-80820` |
//! | player hull | `0x8638` (`Hull: %i.%i%%`) | (2,11) | 15 | `80821-80855` |
//! | Autopilot | `0x8618` | (2,139) | 15 | `80736-80748` |
//! | Set/Act/Alt | `0x8620`/`0x8622` | (2,148) | 15 | `80712-80764` |
//! | turret Decn/Rotn | `0x8636` | (5,76) | 18 | `80700-80745` |
//!
//! Values: shields are `[ship+0xe0] * 100 / [ship+0xe2]` and are **not** drawn
//! when the two words are equal; hull is `[ship+0xe4] * 250 / descriptor[+6]`
//! (one decimal percentage) and only with `player+0x10e != 0`; Set is
//! `player+0x1b8`, Act `player+0xd8`, Alt `player+0xdc / 1000`, and the
//! registration is `DATA_009124`. The original's speed conversion (`F1393`) and
//! its `km`/`m` suffixes are not modelled: the port shows the flight model's
//! speed in m/s. The *positions, conditions and value formulas* are the
//! original's.
//!
//! The turret angles are `player+0x204` and `+0x206` scaled by `900 / 65536`
//! (the shifts and multiplies at `80633-80647`); the second is negated.

use crate::indexed::BitmapFont;
use crate::messages::wrap;

pub const SHIELDS_POS: (usize, usize) = (2, 1);
pub const HULL_POS: (usize, usize) = (2, 11);
pub const AUTOPILOT_POS: (usize, usize) = (2, 139);
pub const SPEED_POS: (usize, usize) = (2, 148);
pub const TURRET_POS: (usize, usize) = (5, 76);
pub const HUD_COLOUR: u8 = 15;
pub const TURRET_COLOUR: u8 = 18;
/// `F1399`'s wrap width for the speed block (`0x94`/0x8b in the string).
const SPEED_WRAP: usize = 0x8b;

/// The values `F1099` prints for the player this frame.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerHud {
    /// `ship+0xe0` current and `+0xe2` maximum shield words. Equal words mean
    /// the original draws no shield line.
    pub shields: [u16; 2],
    /// `ship+0xe4` and its full value (descriptor mass * 4). Zero means the port
    /// has no hull state and draws nothing.
    pub hull: [u16; 2],
    /// `player+0x1b8` set speed, in m/s; `None` while the original would use the
    /// Act-only string (`0x8620`).
    pub set_speed: Option<i32>,
    /// `player+0xd8` actual speed, in m/s.
    pub actual_speed: i32,
    /// `player+0xdc / 1000` altitude in metres, when the port has one.
    pub altitude: Option<i32>,
    /// `DATA_009124`, the ship registration (the model name here).
    pub registration: String,
    /// `player+0xe8 == 2`: the Autopilot line and the Act-only speed string.
    pub autopilot: bool,
    /// The turret's `Decn`/`Rotn` angles in degrees (view 2/3 only).
    pub turret: Option<(i32, i32)>,
}

impl PlayerHud {
    /// `0x8624`: `Shields: n%`, or `None` when the two words are equal, as
    /// `F1099` skips the line at `80786-80787`.
    pub fn shield_text(&self) -> Option<String> {
        let [current, maximum] = self.shields;
        (current != maximum && maximum != 0).then(|| {
            format!(
                "Shields: {}%",
                u32::from(current) * 100 / u32::from(maximum)
            )
        })
    }

    /// `0x8638`: `Hull: n.n%` from `hull * 250 / mass`, one decimal place. The
    /// full word is mass * 4, so an undamaged hull reads 100.0%.
    pub fn hull_text(&self) -> Option<String> {
        let [current, full] = self.hull;
        (full != 0).then(|| {
            let per_mille = u32::from(current) * 1000 / u32::from(full);
            format!("Hull: {}.{}%", per_mille / 10, per_mille % 10)
        })
    }

    /// `0x8622`/`0x8623` (or the Act-only `0x8620`/`0x8621`): the two-line speed
    /// block, then the registration on the third line. Returned as lines so the
    /// caller draws them at `F1586`'s 8-pixel spacing.
    pub fn speed_lines(&self, font: &BitmapFont) -> Vec<String> {
        let speed = |value: i32| format!("{value} m/s");
        let mut text = match self.set_speed {
            Some(set) if !self.autopilot => {
                format!("Set: {} Act: {}", speed(set), speed(self.actual_speed))
            }
            _ => format!("Act: {}", speed(self.actual_speed)),
        };
        if let Some(altitude) = self.altitude {
            text.push_str(&format!("\rAlt: {altitude} m"));
        }
        if !self.registration.is_empty() {
            text.push('\r');
            text.push_str(&self.registration);
        }
        let mut lines = Vec::new();
        for part in text.split('\r') {
            lines.extend(wrap(font, part, SPEED_WRAP));
        }
        lines
    }

    /// `0x8636`: `Decn.: n^` then `Rotn.: n^`. `F1099` prints `+0x204` as
    /// Declination and `-+0x206` as Rotation.
    pub fn turret_lines(&self) -> Option<[String; 2]> {
        let tenths = |value: i32| {
            let sign = if value < 0 { "-" } else { "" };
            let value = value.abs();
            format!("{sign}{}.{}", value / 10, value % 10)
        };
        self.turret.map(|(declination, rotation)| {
            [
                format!("Decn.: {}^", tenths(declination)),
                format!("Rotn.: {}^", tenths(rotation)),
            ]
        })
    }

    /// The ten-times-degrees conversion at `ffe.asm:80633-80647`: 16, ×5, ×5,
    /// ×9 then `sar 16`, i.e. `raw * 3600 / 65536`. `0x4000` is 90.0 degrees,
    /// printed through the one-decimal string (`0x8636`).
    pub fn angle(raw: i16) -> i32 {
        let scaled = i32::from(raw) * 0xe10;
        scaled >> 16
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> BitmapFont {
        let mut bytes = vec![0; 1160];
        for record in bytes.chunks_exact_mut(10) {
            record[0] = 0x81;
            record[1] = 0x40;
            record[8] = 0x20;
            record[9] = 4;
        }
        BitmapFont::new(&bytes).unwrap()
    }

    #[test]
    fn shields_are_skipped_only_when_the_words_are_equal() {
        let mut hud = PlayerHud {
            shields: [128, 256],
            ..PlayerHud::default()
        };
        assert_eq!(hud.shield_text().as_deref(), Some("Shields: 50%"));
        hud.shields = [0, 256];
        assert_eq!(hud.shield_text().as_deref(), Some("Shields: 0%"));
        hud.shields = [256, 256];
        assert_eq!(hud.shield_text(), None);
    }

    #[test]
    fn hull_is_hull_times_250_over_mass_with_one_decimal() {
        let hud = PlayerHud {
            // 1152 * 4 = 4608 full; 2304 is half.
            hull: [2304, 4608],
            ..PlayerHud::default()
        };
        assert_eq!(hud.hull_text().as_deref(), Some("Hull: 50.0%"));
        let full = PlayerHud {
            hull: [4608, 4608],
            ..PlayerHud::default()
        };
        assert_eq!(full.hull_text().as_deref(), Some("Hull: 100.0%"));
        assert_eq!(PlayerHud::default().hull_text(), None);
    }

    #[test]
    fn the_speed_block_uses_the_set_act_alt_and_registration_strings() {
        let hud = PlayerHud {
            set_speed: Some(250),
            actual_speed: 180,
            altitude: Some(1200),
            registration: "EAGLE".to_owned(),
            ..PlayerHud::default()
        };
        assert_eq!(
            hud.speed_lines(&font()),
            ["Set: 250 m/s Act: 180 m/s", "Alt: 1200 m", "EAGLE"]
        );
        // Autopilot: the Act-only twin, no Set.
        let autopilot = PlayerHud {
            autopilot: true,
            ..hud
        };
        assert_eq!(
            autopilot.speed_lines(&font()),
            ["Act: 180 m/s", "Alt: 1200 m", "EAGLE"]
        );
    }

    #[test]
    fn turret_angles_are_scaled_to_tenths_of_a_degree() {
        // A quarter turn is 90.0 degrees (900 tenths).
        assert_eq!(PlayerHud::angle(0x4000), 900);
        assert_eq!(PlayerHud::angle(-0x4000), -900);
        let hud = PlayerHud {
            turret: Some((PlayerHud::angle(0x2000), -PlayerHud::angle(0x1000))),
            ..PlayerHud::default()
        };
        assert_eq!(
            hud.turret_lines().unwrap(),
            ["Decn.: 45.0^".to_owned(), "Rotn.: -22.5^".to_owned()]
        );
        assert!(PlayerHud::default().turret_lines().is_none());
    }
}
