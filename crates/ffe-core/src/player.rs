//! The player's free-flight update, ported from `ffe.asm` and `cplayer.txt`:
//! F1041 (orientation, thrust request) with F1043 (engines-off thrust), and the
//! full free-flight tick F1021 makes of it: velocity (F601/F602) and position
//! (F1512). This
//! covers flight in free space: with no parent frame of reference (F606) and
//! far from any planet (F603's gravity, F609's auto-roll) those steps do
//! nothing, and anything that would need them is reported as an error rather
//! than guessed. Verified against the executed original
//! (`docs/notes/player.md`).
use crate::model::multiply;
use crate::physics::{Motion, Scanner};
use crate::thrust::{clamp, Limits};

/// Altitude (`DATA_008867`) at and below which F1041 rolls toward the planet (F609).
pub const AUTO_ROLL_ALTITUDE: i32 = 0x0262_5a00;

/// The thruster keys F1043 reads, true while pressed (the original's key bytes
/// are zero when down). Names are the key-table offsets; see the notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThrusterKeys {
    /// `DATA_007692`: forward thrust.
    pub forward: bool,
    /// `DATA_007694`: reverse thrust.
    pub reverse: bool,
    /// `DATA_007691 + 0x48` and `+0x4c`: forward and reverse again.
    pub numpad_forward: bool,
    pub numpad_reverse: bool,
    /// `+0x4b` and `+0x4d`: both vertical thrust up (the original maps both so).
    pub up_a: bool,
    pub up_b: bool,
    /// `+0x4f` and `+0x51`: vertical thrust down.
    pub down_a: bool,
    pub down_b: bool,
    /// `+0x49`: lateral thrust right; `+0x47`: left.
    pub right: bool,
    pub left: bool,
}

/// The player's state F1041 reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Player {
    pub motion: Motion,
    /// The thrust limits (`+0xbc..+0xc6`), see [`crate::thrust`].
    pub limits: Limits,
    /// The set speed (`DATA_008918`).
    pub set_speed: i32,
    /// The flight mode (`DATA_008870`); 0x24 is take-off.
    pub mode: u8,
    /// The flight flags (`DATA_008871`); bit 0 is engines off.
    pub flags: u8,
    /// The altitude above the nearest planet (`DATA_008867`).
    pub altitude: i32,
    /// The frame's time (`DATA_009155`).
    pub frame_tics: u32,
    /// The frame-of-reference countdown (`+0x14d`).
    pub reference_counter: i8,
    /// Combat mode (`DATA_009046` bit 1) and its reference velocity
    /// (`DATA_008839..8841`), which engines-off thrust damps toward.
    pub combat: bool,
    pub combat_velocity: [i32; 3],
    pub scanner: Scanner,
}

impl Player {
    /// F1043: the engines-off thrust request (x lateral, y vertical, z forward).
    /// In combat mode the velocity relative to the reference, projected on the
    /// right and up axes, beyond +-256 is opposed (times four); then each
    /// pressed key sets its axis to full thrust, later keys over earlier ones.
    pub fn engines_off_request(&self, keys: &ThrusterKeys) -> [i32; 3] {
        let mut request = [0i32; 3];
        if self.combat {
            let relative: [i32; 3] = std::array::from_fn(|axis| {
                self.motion.velocity[axis].wrapping_sub(self.combat_velocity[axis])
            });
            let project = |row: [i32; 3]| {
                row.iter()
                    .zip(relative)
                    .fold(0i32, |sum, (r, v)| sum.wrapping_add(multiply(*r, v)))
            };
            let up = project(self.motion.orientation[1]);
            let right = project(self.motion.orientation[0]);
            let oppose =
                |v: i32| (!(-0x100..=0x100).contains(&v)).then(|| v.wrapping_neg().wrapping_shl(2));
            if let Some(x) = oppose(right) {
                request[0] = x;
            }
            if let Some(y) = oppose(up) {
                request[1] = y;
            }
        }
        const FULL: i32 = 0x7fff;
        if keys.forward {
            request[2] = FULL;
        }
        if keys.reverse {
            request[2] = -FULL;
        }
        if keys.numpad_forward {
            request[2] = FULL;
        }
        if keys.numpad_reverse {
            request[2] = -FULL;
        }
        if keys.up_a || keys.up_b {
            request[1] = FULL;
        }
        if keys.down_a || keys.down_b {
            request[1] = -FULL;
        }
        if keys.right {
            request[0] = FULL;
        }
        if keys.left {
            request[0] = -FULL;
        }
        request
    }

    /// F1041 in free space: the orientation is updated twice over half the
    /// frame's time (F585, autopilot off), the frame-of-reference counter ticks
    /// (F606 has nothing to do without a parent), and the thrust request - take-off
    /// thrust in mode 0x24, the engines-off keys, or the set speed - is clamped
    /// to the limits (F599) into the thrust words. Errors where the original would
    /// run steps this port does not have: a parent frame (F603/F606) or an altitude
    /// at or below [`AUTO_ROLL_ALTITUDE`] (F609).
    pub fn orient_and_thrust(&mut self, keys: &ThrusterKeys) -> Result<(), String> {
        if self.motion.parent != 0 {
            return Err(
                "Free flight needs F603/F606 for an object with a parent frame.".to_owned(),
            );
        }
        if self.altitude <= AUTO_ROLL_ALTITUDE {
            return Err("Free flight near a planet needs F609's auto-roll.".to_owned());
        }
        let half = (self.frame_tics >> 1) as i32;
        for _ in 0..2 {
            self.motion.orient(half, false, half, &mut self.scanner);
        }
        self.reference_counter = self.reference_counter.wrapping_sub(1);
        if self.reference_counter < 0 {
            self.reference_counter = 0xd;
        }
        let request = if self.mode == 0x24 {
            [0, 0xcc6, 0]
        } else if self.flags & 1 != 0 {
            self.engines_off_request(keys)
        } else {
            self.motion.set_speed_request(self.set_speed)
        };
        self.motion.thrust = clamp(&self.limits, request).words.map(|w| w as i16);
        Ok(())
    }

    /// One free-flight tick over the 64-bit `time` (`DATA_008869:DATA_008868`):
    /// F1041, then the velocity (F602 with the engines off, F601 otherwise, over
    /// the low dword) and the position (F1512). The same limits as
    /// [`Player::orient_and_thrust`] apply.
    pub fn step(&mut self, keys: &ThrusterKeys, time: i64) -> Result<(), String> {
        self.orient_and_thrust(keys)?;
        if self.flags & 1 != 0 {
            self.motion.accelerate_uncapped(time as u32);
        } else {
            self.motion.accelerate(time as u32);
        }
        self.motion.advance(time);
        Ok(())
    }
}
