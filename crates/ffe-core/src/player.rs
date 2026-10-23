//! The player's free-flight update, ported from `ffe.asm` and `cplayer.txt`:
//! F1041 (orientation, thrust request) with F1043 (engines-off thrust), and the
//! full free-flight tick F1021 makes of it: velocity (F601/F602) and position
//! (F1512), and the input layer F1037/F1038 (set speed, steering) with F1121. This
//! covers flight in free space: with no parent frame of reference (F606) and
//! far from any planet (F603's gravity, F609's auto-roll) those steps do
//! nothing, and anything that would need them is reported as an error rather
//! than guessed. Verified against the executed original
//! (`docs/notes/player.md`).
use crate::collision::Contact;
use crate::frames::Bodies;
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
    /// The ship module is active (`DATA_008630` negative); F1037 does nothing otherwise.
    pub active: bool,
    /// The input time (`DATA_009122`): the elapsed time key amounts scale with.
    pub input_time: u32,
    /// `DATA_007690`: when its low byte exceeds 0x10, F1121 scales the set-speed step by it / 16.
    pub speed_boost: u32,
    /// Key input is live (`DATA_007706` non-zero).
    pub keys_enabled: bool,
    /// The previous mouse accumulator (`+0xd0`): x in the low word, y in the high.
    pub mouse_previous: u32,
    /// The mouse axes are flipped when the original's bytes `+0x3f54` (x) and
    /// `+0x3f55` (y) are zero.
    pub mouse_flip: [bool; 2],
    /// The mouse's x motion rolls instead of yawing (`+0x3f56` non-zero).
    pub mouse_roll: bool,
    /// The view mode (`+0xea`): 2 and 3 are the turrets, which steer instead.
    pub view: u8,
    /// The turret angles (`+0x204` yaw, `+0x206` pitch).
    pub turret: [i16; 2],
    /// The truncated relative position F603 last left in
    /// `DATA_008708..DATA_008710`, which F609's auto-roll reads.
    pub gravity_vector: [i32; 3],
    /// The collision flags F1033 keeps at `+0x212` (`DATA_008934`).
    pub contact_flags: u8,
}

/// How long each steering key has been held, in 50 Hz ticks (zero: not held):
/// the key-accumulator entries F1038 reads, by scancode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SteerKeys {
    /// 0x2c: pitch down (lowers the pitch input).
    pub pitch_down: u32,
    /// 0x1e: pitch up.
    pub pitch_up: u32,
    /// 0x33: yaw left.
    pub left: u32,
    /// 0x34: yaw right.
    pub right: u32,
    /// 0x1a: roll one way, 0x1b: the other.
    pub roll_left: u32,
    pub roll_right: u32,
}

/// What the keyboard and mouse layer hands F1037/F1038 for one tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Controls {
    /// The mouse accumulator (`DATA_009188`): x in the low word, y in the high.
    pub mouse: u32,
    /// Return held (`DATA_007692` zero): raise the set speed.
    pub faster: bool,
    /// Shift held (`DATA_007694` zero): lower the set speed.
    pub slower: bool,
    pub steer: SteerKeys,
}

/// F1038's saturating 16-bit add of a steering delta to a rate.
fn wrap_add(rate: i16, delta: i16) -> i16 {
    rate.saturating_add(delta)
}

impl Player {
    /// FUNC_CalcKeyInput: the turn a key held for `held` ticks gives this
    /// frame: the input time times `min(held, 70) + 2`, over 64; nothing when
    /// the key is up or key input is off.
    fn key_amount(&self, held: u32) -> i16 {
        if !self.keys_enabled || held == 0 {
            return 0;
        }
        (self.input_time.wrapping_mul(held.min(70) + 2) >> 6) as i16
    }

    /// F1121: the set-speed step. The speed is doubled up to five times while
    /// its top bit is clear (0x7ffeffff if it gets there), taken from the high
    /// word plus one, scaled by `speed_boost / 16` when that byte exceeds 0x10
    /// (capped at 0x7fff), times the input time over 64.
    pub fn set_speed_step(&self, speed: i32) -> i32 {
        let mut value = speed as u32;
        for _ in 0..5 {
            if value & 0x8000_0000 != 0 {
                value = 0x7ffe_ffff;
                break;
            }
            value = value.wrapping_add(value);
        }
        let mut step = (value >> 16).wrapping_add(1);
        if (self.speed_boost & 0xff) > 0x10 {
            step = self.speed_boost.wrapping_mul(step) >> 4;
            if step > 0x7fff {
                step = 0x7fff;
            }
        }
        (step.wrapping_mul(self.input_time) >> 6) as i32
    }

    /// F1038: the heading input. The mouse motion since last tick (x and y in
    /// the accumulator's words, times 64, flipped by the options) and the held
    /// keys give pitch, yaw and roll deltas; in a turret view they turn the
    /// turret (clamped as the view's rules say), otherwise they are added to the
    /// rates with saturation.
    pub fn steer(&mut self, controls: &Controls) {
        let previous = self.mouse_previous;
        self.mouse_previous = controls.mouse;
        let delta = |new: u32, old: u32| ((new as u16).wrapping_sub(old as u16) << 6) as i16;
        let mut pitch = delta(controls.mouse >> 16, previous >> 16);
        if self.mouse_flip[1] {
            pitch = pitch.wrapping_neg();
        }
        let mut yaw = delta(controls.mouse, previous);
        if self.mouse_flip[0] {
            yaw = yaw.wrapping_neg();
        }
        let mut roll = 0i16;
        if self.mouse_roll && self.view != 2 && self.view != 3 {
            // The original's `mov al,[+0xea]` overwrites the delta's low byte with
            // the view mode before storing it as the roll: kept as it is.
            roll = ((yaw as u16 & 0xff00) | u16::from(self.view)) as i16;
            yaw = 0;
        }
        let keys = &controls.steer;
        pitch = pitch.wrapping_sub(self.key_amount(keys.pitch_down));
        pitch = pitch.wrapping_add(self.key_amount(keys.pitch_up));
        yaw = yaw.wrapping_add(self.key_amount(keys.left));
        yaw = yaw.wrapping_sub(self.key_amount(keys.right));
        roll = roll.wrapping_add(self.key_amount(keys.roll_left));
        roll = roll.wrapping_sub(self.key_amount(keys.roll_right));
        if self.view != 2 && self.view != 3 {
            self.motion.rate[0] = wrap_add(self.motion.rate[0], pitch);
            self.motion.rate[2] = wrap_add(self.motion.rate[2], yaw.wrapping_neg());
            self.motion.rate[1] = wrap_add(self.motion.rate[1], roll);
            return;
        }
        self.turret[0] = self.turret[0].wrapping_sub(yaw);
        self.turret[1] = self.turret[1].wrapping_add(pitch);
        if self.turret[1] < 0 {
            self.view = 2;
            if self.turret[1] <= -0x4000 {
                self.turret[1] = -0x3fff;
            }
        } else if self.motion.gun_mounts >= 4 {
            self.view = 3;
            if self.turret[1] >= 0x4000 {
                self.turret[1] = 0x3fff;
            }
        } else {
            self.turret[1] = -1;
        }
    }

    /// F1037: the set-speed and heading update. Nothing unless the ship module
    /// is active. With the engines on, Return raises the set speed (a negative
    /// one is first brought to zero, and then only the heading is updated) and
    /// Shift lowers it (a non-positive speed falls at a steady rate unless the
    /// slow-down flag 0x80 is set; a positive one by F1121's step, to zero at
    /// most); releasing Shift clears the flag. The heading input runs last.
    pub fn update_set_speed(&mut self, controls: &Controls) {
        if !self.active {
            return;
        }
        if self.flags & 1 == 0 {
            if controls.faster {
                if self.set_speed < 0 {
                    self.set_speed = 0;
                    self.steer(controls);
                    return;
                }
                self.set_speed = self
                    .set_speed
                    .wrapping_add(self.set_speed_step(self.set_speed));
                if self.set_speed < 0 {
                    self.set_speed = 0x7fff_ffff;
                }
            }
            if controls.slower {
                let before = self.set_speed;
                if before > 0 {
                    self.flags |= 0x80;
                    self.set_speed = before.wrapping_sub(self.set_speed_step(before));
                    if before < self.set_speed || self.set_speed < 0 {
                        self.set_speed = 0;
                    }
                } else if self.flags & 0x80 == 0 {
                    self.set_speed = before.wrapping_sub((self.input_time >> 8) as i32);
                    if self.set_speed > 0 {
                        self.set_speed = i32::MIN;
                    }
                }
            } else {
                self.flags &= 0x7f;
            }
        }
        self.steer(controls);
    }

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

    /// F1041 in free space (no parent frame, an empty world): see
    /// [`Player::orient_and_thrust_in`]. Errors for an object with a parent.
    pub fn orient_and_thrust(&mut self, keys: &ThrusterKeys) -> Result<(), String> {
        if self.motion.parent != 0 {
            return Err("A parent frame needs a world: use orient_and_thrust_in.".to_owned());
        }
        self.orient_and_thrust_in(keys, &mut Bodies::default(), 1)
    }

    /// F1041 in a world of bodies, where the player is body `index`: the
    /// orientation twice (F585), the parent's gravity (F603) and, at or below
    /// [`AUTO_ROLL_ALTITUDE`], the auto-roll (F609) into the roll rate; then the
    /// frame countdown, which runs F606 on reaching below zero (the player is
    /// copied into the world for it and back), and the thrust request.
    pub fn orient_and_thrust_in(
        &mut self,
        keys: &ThrusterKeys,
        world: &mut Bodies,
        index: usize,
    ) -> Result<(), String> {
        let half = (self.frame_tics >> 1) as i32;
        for _ in 0..2 {
            self.motion.orient(half, false, half, &mut self.scanner);
        }
        let mass = world.body[usize::from(self.motion.parent)].mass;
        if let Some(vector) = self.motion.gravity(mass, self.frame_tics as i32) {
            self.gravity_vector = vector;
        }
        if self.altitude <= AUTO_ROLL_ALTITUDE {
            self.motion.auto_roll(self.gravity_vector);
            self.motion.rate[1] = self.motion.autopilot[1];
        }
        self.reference_counter = self.reference_counter.wrapping_sub(1);
        if self.reference_counter < 0 {
            world.body[index].take_motion(&self.motion);
            if world.select_frame(index).is_none() {
                return Err("A station's velocity is not ported.".to_owned());
            }
            world.body[index].give_motion(&mut self.motion);
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

    /// [`Player::step`] in a world of bodies where the player is body `index`.
    pub fn step_in(
        &mut self,
        keys: &ThrusterKeys,
        time: i64,
        world: &mut Bodies,
        index: usize,
    ) -> Result<(), String> {
        self.orient_and_thrust_in(keys, world, index)?;
        if self.flags & 1 != 0 {
            self.motion.accelerate_uncapped(time as u32);
        } else {
            self.motion.accelerate(time as u32);
        }
        self.motion.advance(time);
        Ok(())
    }

    /// One whole tick in a world, as F1021 runs it: [`Player::step_in`], then
    /// the collision scan (F1033 calls F569): the player is copied into its
    /// body, [`Bodies::collide`] may switch it between the linear and rotating
    /// frames, and the altitude is set from the distance. Errors where the
    /// original reaches the contact code, which is not ported.
    pub fn tick_in(
        &mut self,
        keys: &ThrusterKeys,
        time: i64,
        world: &mut Bodies,
        index: usize,
    ) -> Result<Contact, String> {
        self.step_in(keys, time, world, index)?;
        world.body[index].take_motion(&self.motion);
        world.frame_tics = self.frame_tics;
        let Some(contact) = world.collide(index) else {
            return Err("Contact with a body is not ported.".to_owned());
        };
        world.body[index].give_motion(&mut self.motion);
        self.altitude = contact.altitude();
        self.contact_flags = contact.flags;
        Ok(contact)
    }

    /// One free-flight tick over the 64-bit `time` (`DATA_008869:DATA_008868`):
    /// F1041, then the velocity (F602 with the engines off, F601 otherwise, over
    /// the low dword) and the position (F1512), in free space.
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
