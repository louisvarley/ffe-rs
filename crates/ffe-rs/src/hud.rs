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
//! | Set/Act/Alt | `0x8620`/`0x8622` | (2,148) and (266,139) | 15 | `80712-80764` |
//! | turret Decn/Rotn | `0x8636` | (5,76) | 18 | `80700-80745` |
//!
//! Values: shields are `[ship+0xe0] * 100 / [ship+0xe2]` and are **not** drawn
//! when the two words are equal; hull is `[ship+0xe4] * 250 / descriptor[+6]`
//! (one decimal percentage) and only with `player+0x10e != 0`; Set is
//! `player+0x1b8`, Act `player+0xd8`, Alt `player+0xdc / 1000`, and the parent
//! object's name is `DATA_009124`. Speeds go through F1393:
//! `speed * 0x156 / 100` with one decimal and the ` km{` unit glyph below
//! `0x4705d`, else `speed / 0x69` with two decimals and ` km|`.
//!
//! The original's string 0x8622 is
//! `Set:<#>\rAct:<$>\xfe\x85\x8bAlt:<alt> m\r<parent>`: the Set/Act block at
//! (2,148), then the position code 0xfe 0x85 0x8b puts Alt at x=266, y=139
//! and the parent name on the next line. The port draws those columns at the
//! same spots. The port's flight model keeps metres per second, so `main`
//! converts to the original's ~0.1 m/s words before calling F1393.
//!
//! The turret angles are `player+0x204` and `+0x206` scaled by `900 / 65536`
//! (the shifts and multiplies at `80633-80647`); the second is negated.

use crate::indexed::BitmapFont;
use crate::messages::wrap;

pub const SHIELDS_POS: (usize, usize) = (2, 1);
pub const HULL_POS: (usize, usize) = (2, 11);
pub const AUTOPILOT_POS: (usize, usize) = (2, 139);
pub const SPEED_POS: (usize, usize) = (2, 148);
/// F1099's position code `0xfe 0x85 0x8b` for the Alt and parent columns.
pub const SPEED_RIGHT_POS: (usize, usize) = (266, 139);
pub const TURRET_POS: (usize, usize) = (5, 76);
pub const HUD_COLOUR: u8 = 15;
pub const TURRET_COLOUR: u8 = 18;
/// `F1393`'s low-speed limit (`0x4705d`): above it the two-decimal unit is used.
pub const SPEED_KMH_LIMIT: i32 = 0x4705d;
/// `F1393`'s low-speed factor (`0x156` over 100) and high-speed divisor (`0x69`).
const SPEED_FACTOR: i64 = 0x156;
const SPEED_HIGH_DIVISOR: i32 = 0x69;
/// `F1399`'s wrap width for the speed block (`0x94`/0x8b in the string).
const SPEED_WRAP: usize = 0x8b;

/// The values `F1099` prints for the player this frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayerHud {
    /// `ship+0xe0` current and `+0xe2` maximum shield words. Equal words mean
    /// the original draws no shield line.
    pub shields: [u16; 2],
    /// `ship+0xe4` and its full value (descriptor mass * 4). Zero means the port
    /// has no hull state and draws nothing.
    pub hull: [u16; 2],
    /// `player+0x1b8` set speed, in the original's speed units; `None` while the
    /// original would use the Act-only string (`0x8620`).
    pub set_speed: Option<i32>,
    /// `player+0xd8` actual speed, in the original's speed units.
    pub actual_speed: i32,
    /// `player+0xdc / 1000` altitude in metres, when the port has one.
    pub altitude: Option<i32>,
    /// `DATA_009124`, the parent object's name (`Hope` over the starport).
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

    /// F1377: `value` with `decimals` digits after a point, inserted from the
    /// right and zero-padded (`0`, one decimal -> `0.0`; `342` -> `34.2`).
    pub fn fraction(value: i32, decimals: usize) -> String {
        let sign = if value < 0 { "-" } else { "" };
        let mut digits = value.unsigned_abs().to_string();
        while digits.len() <= decimals {
            digits.insert(0, '0');
        }
        let split = digits.len() - decimals;
        format!("{sign}{}.{}", &digits[..split], &digits[split..])
    }

    /// F1393: `speed * 0x156 / 100` with one decimal and ` km{` below
    /// `0x4705d`, else `speed / 0x69` with two decimals and ` km|`. The glyphs
    /// 0x7b/0x7c are the font's `h⁻¹`-style unit marks.
    pub fn speed_text(value: i32) -> String {
        if value <= SPEED_KMH_LIMIT {
            let scaled = (i64::from(value) * SPEED_FACTOR / 100) as i32;
            format!("{} km{{", Self::fraction(scaled, 1))
        } else {
            format!("{} km|", Self::fraction(value / SPEED_HIGH_DIVISOR, 2))
        }
    }

    /// `0x8622` (or the Act-only `0x8621`): the left Set/Act block and the right
    /// Alt/parent column, both at F1099's spots. Wrapped at F1399's width.
    pub fn speed_columns(&self, font: &BitmapFont) -> (Vec<String>, Vec<String>) {
        let mut left = Vec::new();
        if let Some(set) = self.set_speed.filter(|_| !self.autopilot) {
            left.extend(wrap(
                font,
                &format!("Set:{}", Self::speed_text(set)),
                SPEED_WRAP,
            ));
        }
        left.extend(wrap(
            font,
            &format!("Act:{}", Self::speed_text(self.actual_speed)),
            SPEED_WRAP,
        ));
        let mut right = Vec::new();
        if let Some(altitude) = self.altitude.filter(|a| (0..=0xea60).contains(a)) {
            right.extend(wrap(font, &format!("Alt:{altitude} m"), SPEED_WRAP));
        }
        if !self.registration.is_empty() {
            right.extend(wrap(font, &self.registration, SPEED_WRAP));
        }
        (left, right)
    }

    /// The Act-only speed block, kept for callers that draw one column.
    pub fn speed_lines(&self, font: &BitmapFont) -> Vec<String> {
        self.speed_columns(font).0
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
    fn the_speed_block_uses_f1393s_units_and_the_two_columns() {
        // F1377's fraction and F1393's two branches.
        assert_eq!(PlayerHud::fraction(0, 1), "0.0");
        assert_eq!(PlayerHud::fraction(342, 1), "34.2");
        assert_eq!(PlayerHud::fraction(342, 2), "3.42");
        assert_eq!(PlayerHud::speed_text(0), "0.0 km{");
        assert_eq!(PlayerHud::speed_text(100), "34.2 km{");
        assert_eq!(
            PlayerHud::speed_text(crate::hud::SPEED_KMH_LIMIT + 1),
            "27.70 km|"
        );
        let hud = PlayerHud {
            set_speed: Some(250),
            actual_speed: 180,
            altitude: Some(1200),
            registration: "Hope".to_owned(),
            ..PlayerHud::default()
        };
        let (left, right) = hud.speed_columns(&font());
        assert_eq!(left, ["Set:85.5 km{", "Act:61.5 km{"]);
        assert_eq!(right, ["Alt:1200 m", "Hope"]);
        // Autopilot: the Act-only twin, no Set.
        let autopilot = PlayerHud {
            autopilot: true,
            ..hud
        };
        assert_eq!(autopilot.speed_columns(&font()).0, ["Act:61.5 km{"]);
    }

    #[test]
    fn speed_conversion_has_a_documented_constant_for_the_limit() {
        assert_eq!(crate::hud::SPEED_KMH_LIMIT, 0x4705d);
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
