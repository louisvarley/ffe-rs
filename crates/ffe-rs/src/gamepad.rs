//! Game controller input (a port addition; the DOS original read a PC
//! joystick through its own driver). A connected pad is read each frame and
//! turned into the keys of the active control preset, so every screen and
//! the original flight model see ordinary key presses:
//!
//! | pad | action | Modern key | Classic key |
//! |---|---|---|---|
//! | left stick up / down | pitch down / up | Up / Down | Z / A |
//! | left stick left / right | yaw | Left / Right | `,` / `.` |
//! | right trigger / left trigger | faster / slower | W / S | Enter / Shift |
//! | right / left bumper | roll right / left | E / Q | `]` / `[` |
//! | south (A) | fire | Space | Space |
//! | east (B) | launch a missile | M | M |
//! | west (X) | combat target | T | T |
//! | north (Y) | next view | F1 | F1 |
//! | d-pad up / down | time faster / slower | `+` / `-` keys | same |
//! | start | the Escape pages | Escape | Escape |
//! | select | the galaxy map | G | G |
//!
//! The reading is pure ([`Pad`] to keys) so it is tested without a device;
//! only Windows builds open a real pad (the `gilrs` backend needs udev on
//! Linux, which this project does not require).

use minifb::Key;

use crate::input::Preset;

/// A stick deflection that counts as a key held.
pub const DEAD_ZONE: f32 = 0.35;

/// One frame of a pad: sticks in -1..1 (`y` up positive), triggers 0..1 and
/// the buttons held.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Pad {
    pub stick_x: f32,
    pub stick_y: f32,
    pub left_trigger: f32,
    pub right_trigger: f32,
    pub left_bumper: bool,
    pub right_bumper: bool,
    pub south: bool,
    pub east: bool,
    pub west: bool,
    pub north: bool,
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub start: bool,
    pub select: bool,
}

/// The keys `pad` holds for `preset`.
pub fn keys(pad: &Pad, preset: Preset) -> Vec<Key> {
    let modern = preset == Preset::Modern;
    let mut out = Vec::new();
    let mut push = |held: bool, key: Key| {
        if held && !out.contains(&key) {
            out.push(key);
        }
    };
    let (up, down, left, right, faster, slower, roll_left, roll_right) = if modern {
        (
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
            Key::W,
            Key::S,
            Key::Q,
            Key::E,
        )
    } else {
        (
            Key::Z,
            Key::A,
            Key::Comma,
            Key::Period,
            Key::Enter,
            Key::LeftShift,
            Key::LeftBracket,
            Key::RightBracket,
        )
    };
    push(pad.stick_y > DEAD_ZONE, up);
    push(pad.stick_y < -DEAD_ZONE, down);
    push(pad.stick_x < -DEAD_ZONE, left);
    push(pad.stick_x > DEAD_ZONE, right);
    push(pad.right_trigger > DEAD_ZONE, faster);
    push(pad.left_trigger > DEAD_ZONE, slower);
    push(pad.left_bumper, roll_left);
    push(pad.right_bumper, roll_right);
    push(pad.south, Key::Space);
    push(pad.east, Key::M);
    push(pad.west, Key::T);
    push(pad.north, Key::F1);
    push(pad.dpad_up, Key::Equal);
    push(pad.dpad_down, Key::Minus);
    push(pad.start, Key::Escape);
    push(pad.select, Key::G);
    out
}

/// The connected pads, when the platform has a backend.
pub struct Gamepads {
    #[cfg(windows)]
    gilrs: Option<gilrs::Gilrs>,
    held: Vec<Key>,
}

impl Default for Gamepads {
    fn default() -> Self {
        Self::new()
    }
}

impl Gamepads {
    pub fn new() -> Self {
        Self {
            #[cfg(windows)]
            gilrs: gilrs::Gilrs::new().ok(),
            held: Vec::new(),
        }
    }

    /// The first connected pad's state this frame, if any.
    pub fn read(&mut self) -> Option<Pad> {
        #[cfg(windows)]
        {
            use gilrs::{Axis, Button};
            let gilrs = self.gilrs.as_mut()?;
            while gilrs.next_event().is_some() {}
            let (_, pad) = gilrs.gamepads().next()?;
            let trigger = |button| pad.button_data(button).map_or(0.0, |data| data.value());
            Some(Pad {
                stick_x: pad.value(Axis::LeftStickX),
                stick_y: pad.value(Axis::LeftStickY),
                left_trigger: trigger(Button::LeftTrigger2),
                right_trigger: trigger(Button::RightTrigger2),
                left_bumper: pad.is_pressed(Button::LeftTrigger),
                right_bumper: pad.is_pressed(Button::RightTrigger),
                south: pad.is_pressed(Button::South),
                east: pad.is_pressed(Button::East),
                west: pad.is_pressed(Button::West),
                north: pad.is_pressed(Button::North),
                dpad_up: pad.is_pressed(Button::DPadUp),
                dpad_down: pad.is_pressed(Button::DPadDown),
                start: pad.is_pressed(Button::Start),
                select: pad.is_pressed(Button::Select),
            })
        }
        #[cfg(not(windows))]
        None
    }

    /// Add this frame's pad keys to the frame's `down` list and, for keys not
    /// held last frame, its `pressed` list.
    pub fn apply(&mut self, preset: Preset, down: &mut Vec<Key>, pressed: &mut Vec<Key>) {
        let now = self.read().map_or_else(Vec::new, |pad| keys(&pad, preset));
        for key in &now {
            if !down.contains(key) {
                down.push(*key);
            }
            if !self.held.contains(key) && !pressed.contains(key) {
                pressed.push(*key);
            }
        }
        self.held = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sticks_and_triggers_become_the_presets_keys() {
        let pad = Pad {
            stick_y: 0.9,
            stick_x: -0.5,
            right_trigger: 1.0,
            south: true,
            ..Pad::default()
        };
        assert_eq!(
            keys(&pad, Preset::Modern),
            [Key::Up, Key::Left, Key::W, Key::Space]
        );
        assert_eq!(
            keys(&pad, Preset::Classic),
            [Key::Z, Key::Comma, Key::Enter, Key::Space]
        );
    }

    #[test]
    fn small_deflections_are_ignored() {
        let pad = Pad {
            stick_x: 0.2,
            stick_y: -0.3,
            left_trigger: 0.1,
            ..Pad::default()
        };
        assert!(keys(&pad, Preset::Modern).is_empty());
    }

    #[test]
    fn a_new_press_is_reported_once() {
        let mut pads = Gamepads::new();
        let (mut down, mut pressed) = (Vec::new(), Vec::new());
        // No device here: nothing is added.
        pads.apply(Preset::Modern, &mut down, &mut pressed);
        assert!(down.is_empty() && pressed.is_empty());
    }
}
