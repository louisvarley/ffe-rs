//! Flight input: the held-key state, the two control presets and how each
//! action is routed to them.
//!
//! The presets are the port's own choice, exposed on the Escape pages' `Port
//! Options` page and on F3 ([`crate::prefs`]). They are deliberately disjoint:
//!
//! | action | Modern | Classic (the original's scheme) |
//! |---|---|---|
//! | raise / lower thrust | W / S | Enter / Shift |
//! | pitch | Up / Down | A / Z |
//! | yaw | Left / Right | `.` / `,` |
//! | roll (original flight) | E / Q | `]` / `[` |
//! | steer by mouse | - | hold the right button and move |
//!
//! `docs/CONTROLS.md` has the full table, including the actions both presets
//! share (fire, missiles, targets, autopilot, views and so on). The routing is
//! pure so the presets can be tested without a window.
use minifb::Key;

use crate::camera::ViewMode;
use crate::flight::Controls as FlightControls;
use crate::original_flight::Keys as OriginalKeys;

/// The original's view keys (`DATA_007688` actions `0xf1`..`0xfa`). F1 cycles;
/// F6-F10 select a view directly. F11 is the combat computer, not a view the
/// port has, and F12 is the escape capsule, so neither is a debug key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ViewCommand {
    Cycle,
    Front,
    Rear,
    Turret,
    External,
    Missile,
}

/// Map one flight key to its source view action. The caller applies the
/// equipment/view gates for the missile view.
pub fn view_command(key: Key) -> Option<ViewCommand> {
    match key {
        Key::F1 => Some(ViewCommand::Cycle),
        Key::F6 => Some(ViewCommand::Front),
        Key::F7 => Some(ViewCommand::Rear),
        Key::F8 => Some(ViewCommand::Turret),
        Key::F9 => Some(ViewCommand::External),
        Key::F10 => Some(ViewCommand::Missile),
        _ => None,
    }
}

/// The source galaxy-map key. F3 is F1054's map action (`docs/CONTROLS.md`);
/// the port also accepts G as its long-standing shortcut. The control preset is
/// chosen only on `Escape` -> `Port Options`, never from a flight key.
pub fn galaxy_map_key(key: Key) -> bool {
    matches!(key, Key::F3 | Key::G)
}

/// The port's developer-only controls. None are bound in a player build: the
/// caller passes `--dev` or `FFE_DEV=1` to enable them, and every one uses a
/// Ctrl chord (or O) so no source key is taken.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DevCommand {
    Screenshot,
    ModelViewer,
    ModelLab,
    PrototypeFlight,
}

pub fn dev_command(enabled: bool, key: Key, ctrl: bool, shift: bool) -> Option<DevCommand> {
    if !enabled {
        return None;
    }
    match key {
        Key::F11 if ctrl && !shift => Some(DevCommand::Screenshot),
        Key::F12 if ctrl && shift => Some(DevCommand::ModelLab),
        Key::F12 if ctrl => Some(DevCommand::ModelViewer),
        Key::O if !ctrl && !shift => Some(DevCommand::PrototypeFlight),
        _ => None,
    }
}

#[derive(Default)]
pub struct HeldControls {
    held: Vec<Key>,
    blocked: Vec<Key>,
}

impl HeldControls {
    pub fn update(&mut self, down: &[Key], pressed: &[Key], enabled: bool) {
        self.held.retain(|key| down.contains(key));
        self.blocked.retain(|key| down.contains(key));
        if !enabled {
            self.disarm(down);
            return;
        }
        for key in pressed {
            if down.contains(key) && !self.blocked.contains(key) && !self.held.contains(key) {
                self.held.push(*key);
            }
        }
    }

    pub fn disarm(&mut self, down: &[Key]) {
        self.held.clear();
        self.blocked = down.to_vec();
    }

    pub fn contains(&self, key: &Key) -> bool {
        self.held.contains(key)
    }

    /// The keys latched as held this frame, for the preset routing.
    pub fn keys(&self) -> &[Key] {
        &self.held
    }
}

/// Which control preset is active. `Prefs::modern` stores the choice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Preset {
    Modern,
    Classic,
}

impl Preset {
    pub fn from_modern(modern: bool) -> Self {
        if modern {
            Self::Modern
        } else {
            Self::Classic
        }
    }

    pub fn modern(self) -> bool {
        self == Self::Modern
    }
}

/// The part of the view that decides who owns the steering input. Turrets aim
/// with the steering keys; the external camera owns the arrow keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum View {
    #[default]
    Ship,
    Turret,
    External,
}

impl View {
    pub fn of(mode: ViewMode) -> Self {
        if mode.is_turret() {
            Self::Turret
        } else if mode == ViewMode::External {
            Self::External
        } else {
            Self::Ship
        }
    }
}

/// Mouse movement since the previous frame, in logical 320x200 pixels, with
/// `dx` right and `dy` down. `held` is the right button: the original flies only
/// while it is down.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MouseSteer {
    pub held: bool,
    pub dx: i32,
    pub dy: i32,
}

/// Pixels of movement for a full-prototype-flight deflection.
const MOUSE_FULL_DEFLECTION: f64 = 50.0;
/// The original F1038 scale: one mouse count is one pixel here, and F1038
/// shifts each count left six.
const MOUSE_UNITS_PER_PIXEL: i32 = 64;

/// A relative mouse counter in the original's `DATA_009188` form: x in the low
/// word and y in the high word. It only moves while the right button is held,
/// and is cleared as soon as it is released so a later press starts at zero.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct MouseAccumulator {
    x: i32,
    y: i32,
}

impl MouseAccumulator {
    pub fn update(&mut self, mouse: MouseSteer) {
        if mouse.held {
            self.x = self.x.wrapping_add(mouse.dx);
            self.y = self.y.wrapping_add(mouse.dy);
        } else {
            self.reset();
        }
    }

    pub fn reset(&mut self) {
        self.x = 0;
        self.y = 0;
    }

    /// `DATA_009188`: x in the low word, y in the high word.
    pub fn packed(self) -> u32 {
        (self.x as u16 as u32) | ((self.y as u16 as u32) << 16)
    }
}

fn down(held: &[Key], key: Key) -> bool {
    held.contains(&key)
}

fn axis(held: &[Key], positive: Key, negative: Key) -> f64 {
    f64::from(u8::from(down(held, positive))) - f64::from(u8::from(down(held, negative)))
}

fn any_shift(held: &[Key]) -> bool {
    down(held, Key::LeftShift) || down(held, Key::RightShift)
}

/// A held-key axis as -1, 0 or 1, and a mouse deflection in the same range.
fn axis_with_mouse(held: &[Key], positive: Key, negative: Key, delta: i32) -> f64 {
    (axis(held, positive, negative) + (f64::from(delta) / MOUSE_FULL_DEFLECTION).clamp(-1.0, 1.0))
        .clamp(-1.0, 1.0)
}

fn flip(value: f64, reverse: bool) -> f64 {
    if reverse {
        -value
    } else {
        value
    }
}

/// The prototype flight's stick for this frame. `reverse` is the original's
/// "Reverse left-right controls" and "Reverse up-down controls"
/// (`prefs.others[3]` and `[4]`).
pub fn flight_controls(
    preset: Preset,
    view: View,
    held: &[Key],
    mouse: MouseSteer,
    reverse: [bool; 2],
) -> FlightControls {
    let steering = view == View::Ship || view == View::External;
    let mouse = if preset == Preset::Classic && steering && mouse.held {
        mouse
    } else {
        MouseSteer::default()
    };
    let (thrust, yaw, pitch) = match preset {
        Preset::Modern => (
            axis(held, Key::W, Key::S),
            if steering {
                axis(held, Key::Right, Key::Left)
            } else {
                0.0
            },
            if steering {
                axis(held, Key::Up, Key::Down)
            } else {
                0.0
            },
        ),
        Preset::Classic => (
            f64::from(u8::from(down(held, Key::Enter))) - f64::from(u8::from(any_shift(held))),
            if steering {
                axis_with_mouse(held, Key::Period, Key::Comma, mouse.dx)
            } else {
                0.0
            },
            if steering {
                axis_with_mouse(held, Key::A, Key::Z, -mouse.dy)
            } else {
                0.0
            },
        ),
    };
    FlightControls {
        thrust,
        yaw: flip(yaw, reverse[0]),
        pitch: flip(pitch, reverse[1]),
        brake: down(held, Key::X),
    }
}

/// The original flight model's keys for this frame, including the mouse word
/// while the Classic scheme holds the right button.
pub fn original_keys(
    preset: Preset,
    view: View,
    held: &[Key],
    mouse: MouseAccumulator,
    mouse_held: bool,
    reverse: [bool; 2],
) -> OriginalKeys {
    let mut keys = match preset {
        Preset::Modern => OriginalKeys {
            faster: down(held, Key::W),
            slower: down(held, Key::S),
            pitch_down: down(held, Key::Up),
            pitch_up: down(held, Key::Down),
            left: down(held, Key::Left),
            right: down(held, Key::Right),
            roll_left: down(held, Key::Q),
            roll_right: down(held, Key::E),
            ..OriginalKeys::default()
        },
        Preset::Classic => OriginalKeys {
            faster: down(held, Key::Enter),
            slower: any_shift(held),
            pitch_down: down(held, Key::Z),
            pitch_up: down(held, Key::A),
            left: down(held, Key::Comma),
            right: down(held, Key::Period),
            roll_left: down(held, Key::LeftBracket),
            roll_right: down(held, Key::RightBracket),
            mouse: mouse.packed(),
            mouse_enabled: mouse_held,
            mouse_flip: reverse,
        },
    };
    if reverse[0] {
        std::mem::swap(&mut keys.left, &mut keys.right);
    }
    if reverse[1] {
        std::mem::swap(&mut keys.pitch_down, &mut keys.pitch_up);
    }
    // In the external view the arrows are the camera's, so Modern does not also
    // steer the ship with them. Classic keeps its own steering keys there.
    if preset == Preset::Modern && view == View::External {
        keys.left = false;
        keys.right = false;
        keys.pitch_down = false;
        keys.pitch_up = false;
        keys.roll_left = false;
        keys.roll_right = false;
    }
    keys
}

/// The prototype turret's steering keys, in `(pitch up, pitch down, yaw left,
/// yaw right)` order. The original model routes them through
/// [`original_keys`]; this is for the synthetic turret view.
pub fn turret_keys(preset: Preset) -> [Key; 4] {
    match preset {
        Preset::Modern => [Key::Up, Key::Down, Key::Left, Key::Right],
        Preset::Classic => [Key::A, Key::Z, Key::Comma, Key::Period],
    }
}

/// The turret's mouse aim for one frame, as F1038's counts (left six).
pub fn turret_mouse(preset: Preset, mouse: MouseSteer) -> (i32, i32) {
    if preset != Preset::Classic || !mouse.held {
        return (0, 0);
    }
    (
        -mouse.dy * MOUSE_UNITS_PER_PIXEL,
        -mouse.dx * MOUSE_UNITS_PER_PIXEL,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prefs::Prefs;

    const MODERN: [Key; 4] = [Key::W, Key::Left, Key::Up, Key::E];
    const CLASSIC: [Key; 9] = [
        Key::Enter,
        Key::LeftShift,
        Key::RightShift,
        Key::Comma,
        Key::A,
        Key::LeftBracket,
        Key::Z,
        Key::Period,
        Key::RightBracket,
    ];

    fn controls(preset: Preset, held: &[Key]) -> FlightControls {
        flight_controls(preset, View::Ship, held, MouseSteer::default(), [false; 2])
    }

    #[test]
    fn intro_space_must_be_released_before_a_new_press_can_fire() {
        let mut held = HeldControls::default();
        held.disarm(&[Key::Space]);
        held.update(&[Key::Space], &[Key::Space], true);
        assert!(!held.contains(&Key::Space));
        held.update(&[], &[], true);
        held.update(&[Key::Space], &[Key::Space], true);
        assert!(held.contains(&Key::Space));
        held.update(&[Key::Space], &[], true);
        assert!(held.contains(&Key::Space));
    }

    #[test]
    fn held_control_requires_a_real_press_and_clears_on_release() {
        let mut input = HeldControls::default();
        input.update(&[Key::W], &[], true);
        assert!(!input.contains(&Key::W));
        input.update(&[Key::W], &[Key::W], true);
        assert!(input.contains(&Key::W));
        input.update(&[Key::W], &[], true);
        assert!(input.contains(&Key::W));
        input.update(&[], &[], true);
        assert!(!input.contains(&Key::W));
    }

    #[test]
    fn disabled_controls_stay_blocked_until_released_even_if_reported_as_pressed() {
        let mut input = HeldControls::default();
        input.update(&[Key::W], &[Key::W], true);
        input.update(&[Key::W, Key::Right], &[Key::Right], false);
        input.update(&[Key::W, Key::Right], &[Key::W, Key::Right], true);
        assert!(!input.contains(&Key::W));
        assert!(!input.contains(&Key::Right));
        input.update(&[Key::Right], &[], true);
        input.update(&[Key::W, Key::Right], &[Key::W], true);
        assert!(input.contains(&Key::W));
        assert!(!input.contains(&Key::Right));
    }

    #[test]
    fn mode_transition_disarms_all_current_keys_without_blocking_new_keys() {
        let mut input = HeldControls::default();
        input.update(&[Key::W], &[Key::W], true);
        input.disarm(&[Key::W]);
        input.update(&[Key::W, Key::S], &[Key::W, Key::S], true);
        assert!(!input.contains(&Key::W));
        assert!(input.contains(&Key::S));
        input.update(&[], &[Key::W], true);
        assert!(!input.contains(&Key::W));
        input.update(&[Key::W], &[Key::W], true);
        assert!(input.contains(&Key::W));
    }

    #[test]
    fn modern_thrust_and_steering_are_w_s_arrows_and_q_e() {
        assert_eq!(controls(Preset::Modern, &[Key::W]).thrust, 1.0);
        assert_eq!(controls(Preset::Modern, &[Key::S]).thrust, -1.0);
        assert_eq!(controls(Preset::Modern, &[Key::Right]).yaw, 1.0);
        assert_eq!(controls(Preset::Modern, &[Key::Left]).yaw, -1.0);
        assert_eq!(controls(Preset::Modern, &[Key::Up]).pitch, 1.0);
        assert_eq!(controls(Preset::Modern, &[Key::Down]).pitch, -1.0);
        let keys = original_keys(
            Preset::Modern,
            View::Ship,
            &[Key::W, Key::Right, Key::Down, Key::E],
            MouseAccumulator::default(),
            false,
            [false; 2],
        );
        assert!(keys.faster && keys.right && keys.pitch_up && keys.roll_right);
        assert!(!keys.slower && !keys.left && !keys.pitch_down && !keys.roll_left);
        // The external view's arrows are the camera's in Modern.
        let external = original_keys(
            Preset::Modern,
            View::External,
            &[Key::Right, Key::Up],
            MouseAccumulator::default(),
            false,
            [false; 2],
        );
        assert!(!external.left && !external.right);
        assert!(!external.pitch_down && !external.pitch_up);
        // Classic still flies from its own keys in the external view.
        let classic = original_keys(
            Preset::Classic,
            View::External,
            &[Key::Period, Key::A],
            MouseAccumulator::default(),
            false,
            [false; 2],
        );
        assert!(classic.right && classic.pitch_up);
    }

    #[test]
    fn classic_enter_shift_and_the_original_keys_do_not_leak_into_modern() {
        let thrust = controls(Preset::Classic, &[Key::Enter]);
        assert_eq!(thrust.thrust, 1.0);
        let reverse = controls(Preset::Classic, &[Key::LeftShift]);
        assert_eq!(reverse.thrust, -1.0);
        // Right Shift lowers the set speed as the original does.
        assert_eq!(controls(Preset::Classic, &[Key::RightShift]).thrust, -1.0);
        for key in CLASSIC {
            let classic = controls(Preset::Classic, &[key]);
            let modern = controls(Preset::Modern, &[key]);
            assert_eq!(
                modern,
                FlightControls::default(),
                "{key:?} must do nothing in Modern"
            );
            if key == Key::Enter || key == Key::LeftShift {
                assert_ne!(classic.thrust, 0.0, "{key:?} thrusts in Classic");
            }
        }
        for key in MODERN {
            assert_eq!(
                controls(Preset::Classic, &[key]),
                FlightControls::default(),
                "{key:?} must do nothing in Classic"
            );
        }
        // Modern's original-flight keys do not include any of the Classic set.
        let keys = original_keys(
            Preset::Modern,
            View::Ship,
            &CLASSIC,
            MouseAccumulator::default(),
            false,
            [false; 2],
        );
        assert!(!keys.faster && !keys.slower && !keys.left && !keys.right);
        assert!(!keys.pitch_down && !keys.pitch_up);
        assert!(!keys.roll_left && !keys.roll_right);
    }

    #[test]
    fn the_right_mouse_button_steers_only_in_classic() {
        let mouse = MouseSteer {
            held: true,
            dx: 25,
            dy: -25,
        };
        let classic = flight_controls(Preset::Classic, View::Ship, &[], mouse, [false; 2]);
        assert!((classic.yaw - 0.5).abs() < 1e-9, "{classic:?}");
        assert!((classic.pitch - 0.5).abs() < 1e-9, "{classic:?}");
        let modern = flight_controls(Preset::Modern, View::Ship, &[], mouse, [false; 2]);
        assert_eq!(modern, FlightControls::default());
        // Released, the button does nothing in Classic either.
        let released = MouseSteer {
            held: false,
            ..mouse
        };
        assert_eq!(
            flight_controls(Preset::Classic, View::Ship, &[], released, [false; 2]),
            FlightControls::default()
        );
        // It steers the ship only: the turret has its own aim.
        assert_eq!(
            flight_controls(Preset::Classic, View::Turret, &[], mouse, [false; 2]),
            FlightControls::default()
        );
        let (pitch, yaw) = turret_mouse(Preset::Classic, mouse);
        assert_eq!((pitch, yaw), (25 * 64, -25 * 64));
        assert_eq!(turret_mouse(Preset::Modern, mouse), (0, 0));
    }

    #[test]
    fn reverse_options_flip_the_keys_and_the_mouse() {
        let mouse = MouseSteer {
            held: true,
            dx: 50,
            dy: 50,
        };
        let normal = flight_controls(Preset::Classic, View::Ship, &[], mouse, [false; 2]);
        let reversed = flight_controls(Preset::Classic, View::Ship, &[], mouse, [true, true]);
        assert_eq!((normal.yaw, normal.pitch), (1.0, -1.0));
        assert_eq!((reversed.yaw, reversed.pitch), (-1.0, 1.0));
        let keys = original_keys(
            Preset::Classic,
            View::Ship,
            &[Key::Comma, Key::Z],
            MouseAccumulator::default(),
            false,
            [true, true],
        );
        // "Reverse left-right" swaps the yaw keys and "up-down" the pitch keys.
        assert!(keys.right && !keys.left);
        assert!(keys.pitch_up && !keys.pitch_down);
        assert_eq!(keys.mouse_flip, [true, true]);
    }

    #[test]
    fn the_mouse_accumulator_moves_only_while_held_and_starts_at_zero() {
        let mut accumulator = MouseAccumulator::default();
        let step = |held, dx, dy| MouseSteer { held, dx, dy };
        accumulator.update(step(true, 3, -2));
        assert_eq!(accumulator.packed(), 0xfffe_0003);
        accumulator.update(step(true, 2, 1));
        assert_eq!(accumulator.packed(), 0xffff_0005);
        // Releasing starts a fresh count so the next press cannot jump.
        accumulator.update(step(false, 100, 100));
        assert_eq!(accumulator.packed(), 0);
        accumulator.update(step(true, 1, 0));
        assert_eq!(accumulator.packed(), 1);
    }

    #[test]
    fn f3_is_the_source_map_key_and_not_a_preset_switch() {
        assert!(galaxy_map_key(Key::F3));
        assert!(galaxy_map_key(Key::G));
        // The source's other screen and view keys keep their own routes.
        for key in [Key::F1, Key::F2, Key::F4, Key::F6, Key::F12] {
            assert!(!galaxy_map_key(key), "{key:?} is not the map key");
        }
    }

    #[test]
    fn the_source_view_keys_are_unchanged_and_help_is_not_a_binding() {
        assert_eq!(view_command(Key::F1), Some(ViewCommand::Cycle));
        assert_eq!(view_command(Key::F6), Some(ViewCommand::Front));
        assert_eq!(view_command(Key::F7), Some(ViewCommand::Rear));
        assert_eq!(view_command(Key::F8), Some(ViewCommand::Turret));
        assert_eq!(view_command(Key::F9), Some(ViewCommand::External));
        assert_eq!(view_command(Key::F10), Some(ViewCommand::Missile));
        // The retired port help key and the debug keys are not view actions.
        assert_eq!(view_command(Key::Slash), None);
        assert_eq!(view_command(Key::F11), None);
        assert_eq!(view_command(Key::F12), None);
    }

    #[test]
    fn developer_commands_require_the_dev_switch_and_never_take_a_source_key() {
        // A player build has no debug bindings at all.
        for (key, ctrl, shift) in [
            (Key::F11, true, false),
            (Key::F12, true, false),
            (Key::F12, true, true),
            (Key::O, false, false),
        ] {
            assert_eq!(dev_command(false, key, ctrl, shift), None);
        }
        // Enabled, the debug chord is Ctrl+F11 / Ctrl+F12 / Ctrl+Shift+F12.
        assert_eq!(
            dev_command(true, Key::F11, true, false),
            Some(DevCommand::Screenshot)
        );
        assert_eq!(
            dev_command(true, Key::F12, true, false),
            Some(DevCommand::ModelViewer)
        );
        assert_eq!(
            dev_command(true, Key::F12, true, true),
            Some(DevCommand::ModelLab)
        );
        assert_eq!(
            dev_command(true, Key::O, false, false),
            Some(DevCommand::PrototypeFlight)
        );
        // Plain F11/F12 stay free for their source actions.
        assert_eq!(dev_command(true, Key::F11, false, false), None);
        assert_eq!(dev_command(true, Key::F12, false, false), None);
        assert_eq!(dev_command(true, Key::F12, false, true), None);
    }

    #[test]
    fn switching_the_prefs_preset_changes_the_route_immediately() {
        let mut prefs = Prefs::default();
        assert_eq!(Preset::from_modern(prefs.modern), Preset::Classic);
        prefs.toggle_modern();
        assert_eq!(Preset::from_modern(prefs.modern), Preset::Modern);
        assert_eq!(
            controls(Preset::from_modern(prefs.modern), &[Key::W]).thrust,
            1.0
        );
        assert_eq!(
            controls(Preset::from_modern(prefs.modern), &[Key::Enter]).thrust,
            0.0
        );
        prefs.toggle_modern();
        assert!(!prefs.modern);
        assert_eq!(
            controls(Preset::from_modern(prefs.modern), &[Key::W]).thrust,
            0.0
        );
        assert_eq!(
            controls(Preset::from_modern(prefs.modern), &[Key::Enter]).thrust,
            1.0
        );
    }
}
