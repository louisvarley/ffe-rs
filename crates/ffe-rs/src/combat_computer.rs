//! F11's combat computer view (view 6): F1056 enters it when the Combat
//! Computer is fitted (`ship+0xc8 & 0x40`), F1054 places a chase camera that
//! backs off to keep the nearest ship in frame, and F1099 runs F1109 over the
//! objects to print each ship's range, hull, shields and type beside it.
//!
//! - **F1056 (`0xfb`).** View 6, `DATA_007270 = DATA_007269 = 0x320`,
//!   `DATA_007271 = 0`, console button 0x13 lit.
//! - **F1099 / F1109.** `DATA_007270 = 0x320`, then for every ship (control
//!   `+0x87 == 0xb`, distance class `+0x88 <= 0x14`, model 0x0f..=0x3f) the
//!   range in hundredths of a km lowers `DATA_007270`, and when the ship's
//!   screen point is left of x 320 and above y 128 the amber (0x5f) text
//!   0x863a (or 0x8639 without shields) goes at (x - 8, y + 9):
//!   `{range}km`, `{hull}%` (`,{shields}%`), the type name.
//! - **F1054 (view 6).** `DATA_007270` at least 0x12c; `DATA_007269 =
//!   (3 * DATA_007269 + DATA_007270) / 4`; `DATA_007271 = (DATA_007271 +
//!   (ship+0xb4 >> 1)) >> 1`. The camera rows turn by 0x9f5 about right
//!   (rot(up, forward)) and by `DATA_007271` about up (rot(forward, right)),
//!   and F1053 backs the eye off along forward by `-(DATA_007269 * 40)` (a
//!   16-bit word, `>> 12` on the Q15 rows: the external view's distance word
//!   `<< 11`).

use ffe_core::npc::NpcShip;

/// F1056/F1099's starting nearest range, `0x320` (8.00 km).
pub const NEAREST_START: i32 = 0x320;
/// F1054's floor on the nearest range, `0x12c` (3.00 km).
pub const NEAREST_MIN: i32 = 0x12c;
/// F1054's fixed pitch of the camera rows.
pub const PITCH: u16 = 0x9f5;
/// F1109's text colour (amber).
pub const COLOUR: u8 = 0x5f;
/// F1109's distance-class limit (`+0x88 <= 0x14`): class `c` is `2^(c+13)` mm.
pub const RANGE_LIMIT: f64 = (1u64 << (0x14 + 13)) as f64;

/// `DATA_007269..7271`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CombatCamera {
    /// `DATA_007270`: this frame's nearest range (hundredths of a km).
    pub nearest: i32,
    /// `DATA_007269`: the smoothed range the camera backs off by.
    pub distance: i32,
    /// `DATA_007271`: the smoothed yaw turn.
    pub yaw: i32,
}

impl Default for CombatCamera {
    fn default() -> Self {
        Self::enter()
    }
}

impl CombatCamera {
    /// F1056's `0xfb`.
    pub fn enter() -> Self {
        Self {
            nearest: NEAREST_START,
            distance: NEAREST_START,
            yaw: 0,
        }
    }

    /// One frame: F1099/F1109's nearest range (`None` with no ship), then
    /// F1054's smoothing with the player's yaw rate (`ship+0xb4`).
    pub fn step(&mut self, nearest: Option<i32>, yaw_rate: i16) {
        self.nearest = nearest
            .map_or(NEAREST_START, |range| range.min(NEAREST_START))
            .max(NEAREST_MIN);
        self.distance = (self.distance * 3 + self.nearest) >> 2;
        self.yaw = (self.yaw + (i32::from(yaw_rate) >> 1)) >> 1;
    }

    /// The camera's offset along its turned forward row, in the external
    /// camera's distance units (model units * 256, negative behind).
    pub fn offset(&self) -> f64 {
        let word = (-(self.distance * 40)) as i16;
        f64::from(word) * 2048.0
    }
}

/// One ship's F1109 readout.
#[derive(Debug, Clone, PartialEq)]
pub struct Readout {
    /// The ship's centre in flight units.
    pub centre: [f64; 3],
    /// Range in hundredths of a km (F1109's `[ebp-0x20]`).
    pub range: i32,
    /// 0x8639/0x863a's three lines.
    pub lines: [String; 3],
}

/// F1109 for `ship` seen from `player` (system mm), or `None` when the ship
/// does not qualify. `unit` converts mm to flight units.
pub fn readout(ship: &NpcShip, player: [i64; 3], unit: f64) -> Option<Readout> {
    if !ship.alive() || !ship.visible() || !(0x0f..=0x3f).contains(&ship.model) {
        return None;
    }
    let metres: f64 = (0..3)
        .map(|k| (ship.position[k] - player[k]) as f64)
        .map(|d| d * d)
        .sum::<f64>()
        .sqrt();
    if metres > RANGE_LIMIT {
        return None;
    }
    let range = (metres / 10_000.0) as i32;
    let damage = ship.damage;
    let mass = i32::from(damage.mass).max(1);
    // F1109: `+0xe4 * 25 / mass` (the hull is mass * 4), shields
    // `+0xe0 * 100 / +0xe2`.
    let hull = i32::from(damage.hull) * 25 / mass;
    let second = if damage.shields_max != 0 {
        let shields = i32::from(damage.shields) * 100 / i32::from(damage.shields_max);
        format!("{hull}%,{shields}%")
    } else {
        format!("{hull}%")
    };
    Some(Readout {
        centre: ship.position.map(|v| v as f64 / unit),
        range,
        lines: [
            format!("{}.{:02}km", range / 100, range % 100),
            second,
            if ship.type_name.is_empty() {
                ship.name.clone()
            } else {
                ship.type_name.clone()
            },
        ],
    })
}

/// F1109's screen gate and placement: text at (x - 8, y + 9) when x < 320
/// and y + 30 < 158.
pub fn text_origin(x: i32, y: i32) -> Option<(i32, i32)> {
    ((0..0x140).contains(&x) && y + 0x1e < 0x9e).then_some((x - 8, y + 9))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_follows_f1054s_smoothing() {
        let mut camera = CombatCamera::enter();
        assert_eq!(
            (camera.nearest, camera.distance, camera.yaw),
            (0x320, 0x320, 0)
        );
        // No ship: the range stays at 8 km.
        camera.step(None, 0);
        assert_eq!(camera.distance, 0x320);
        // A ship 1 km away is floored at 3 km, and the distance eases in.
        camera.step(Some(100), 0x400);
        assert_eq!(camera.nearest, NEAREST_MIN);
        assert_eq!(camera.distance, (0x320 * 3 + 0x12c) >> 2);
        assert_eq!(camera.yaw, 0x100);
        // A far ship never pushes the range beyond F1099's 8 km start.
        camera.step(Some(5000), 0);
        assert_eq!(camera.nearest, NEAREST_START);
        // F1053's word: -(distance * 40), then << 11 in external units.
        let camera = CombatCamera::enter();
        assert_eq!(camera.offset(), -32_000.0 * 2048.0);
    }

    #[test]
    fn readout_text_is_f1109s() {
        assert_eq!(text_origin(100, 50), Some((92, 59)));
        assert_eq!(text_origin(320, 50), None);
        assert_eq!(text_origin(100, 128), None);
    }
}
