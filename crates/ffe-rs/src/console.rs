//! The original console's time-acceleration meter and screen buttons
//! (`DATA_005100` buttons 0x0b-0x0e and 0x26-0x2b, hot areas `DATA_007684`).
//!
//! The bar at the lower left is the time-acceleration meter, not a throttle: six
//! steps rising from a pause mark, each a grey icon with a red lit twin, of
//! which one is lit (a radio group). Clicking a step, or the keys 6, 7, 8, 9 and
//! 0 (`FUNC_000208`: the lever's actions 0xc1-0xc5 are translated to those key
//! codes), sets the frame time scale (`DATA_009012`: 0x3e2 per 20 ms at 1x). The
//! four icons beside it open other screens (F1 views, F2 status, F3 map, F4
//! comms), which are not ported. Bitmap ids from 96 are the secondary table.
//! See `docs/notes/console.md`.

/// Time acceleration (`DATA_009012`) for 1x, 10x, 100x, 1000x and 10000x.
pub const TIME_ACCELERATION: [u32; 5] = [0x3e2, 0x26d6, 0x1_845c, 0xf_2b98, 0x97_b3f0];
pub const ACCELERATION_NAMES: [&str; 5] = ["1X", "10X", "100X", "1000X", "10000X"];

/// The meter's six steps (buttons 0x26-0x2b): position, grey and lit bitmap ids.
/// The first is the pause mark.
pub const BAR: [(i32, i32, u16, u16); 6] = [
    (40, 192, 243, 252),
    (41, 187, 242, 251),
    (43, 180, 241, 250),
    (44, 173, 240, 249),
    (45, 165, 239, 248),
    (48, 158, 238, 247),
];

/// The screen buttons (0x0b-0x0e), left to right in the original's order from F4
/// to F1: position, idle and lit bitmap ids, and the function key.
pub const SCREEN_BUTTONS: [(i32, i32, u16, u16, u8); 4] = [
    (107, 164, 127, 164, 4),
    (91, 165, 128, 165, 3),
    (70, 166, 129, 166, 2),
    (60, 172, 130, 167, 1),
];

/// What the console shows besides the views: the time acceleration step,
/// whether the game is paused, the ship's speed in metres per second and, when
/// the original flight model is running, its set speed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConsoleState {
    pub acceleration: usize,
    pub paused: bool,
    pub speed: i32,
    pub set_speed: Option<i32>,
}

impl ConsoleState {
    /// The meter step that is lit: the pause mark, or the acceleration's step.
    pub fn lit_step(&self) -> usize {
        if self.paused {
            0
        } else {
            1 + self.acceleration.min(4)
        }
    }
}

/// What a console click asks for (the action codes of `DATA_007684`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    /// A meter step (actions 0xc1-0xc5): the time acceleration index 0..=4.
    Acceleration(usize),
    /// The meter's pause mark (action 7).
    Pause,
    /// F1: cycle the views.
    CycleView,
    /// F2-F5: a screen the port does not have yet.
    Screen(u8),
    /// F6-F9: front, rear, turret and external views.
    Front,
    Rear,
    Turret,
    External,
    /// F10 and F11: the missile and combat-computer views (they need equipment).
    Missile,
    Combat,
    /// F12: the escape capsule.
    Capsule,
    /// The right-hand buttons: `l` text labels, `n` navigation computer, `H`
    /// hyperspace, and the `t` target button.
    Labels,
    Navigation,
    Hyperspace,
    /// The target button: lock the next body by range.
    Target,
    /// The autopilot buttons below the view: fly to the locked body (docking at a
    /// station), and stop.
    Autopilot,
    AutopilotOff,
}

impl Click {
    /// The note for a click whose behaviour is not ported, or `None` when it is.
    pub fn unported(self) -> Option<&'static str> {
        match self {
            Click::Screen(2) => Some("F2 (status) is not ported yet."),
            // The map button lists the systems and the navigation computer lists this
            // system (ffe-rs navigation, not the original's screens).
            Click::Screen(3) | Click::Navigation | Click::Hyperspace => None,
            Click::Screen(_) => Some("F4/F5 (comms) are not ported yet."),
            Click::Missile => Some("The missile view needs equipment (not ported)."),
            Click::Combat => Some("The combat computer needs equipment (not ported)."),
            Click::Capsule => Some("The escape capsule is not ported yet."),
            Click::Labels => Some("Text labels are not ported yet."),
            _ => None,
        }
    }
}

/// The console hot areas in the original's list order (`DATA_007684`):
/// `(x0, y0, x1, y1)` and the click. A click inside several takes the last of
/// them, and the edges are inclusive, as the mouse poll tests them.
const HOT_AREAS: [((i32, i32, i32, i32), Click); 22] = [
    ((40, 194, 52, 200), Click::Pause),
    ((40, 188, 52, 194), Click::Acceleration(0)),
    ((41, 182, 53, 188), Click::Acceleration(1)),
    ((43, 174, 55, 182), Click::Acceleration(2)),
    ((44, 166, 56, 174), Click::Acceleration(3)),
    ((45, 158, 57, 166), Click::Acceleration(4)),
    ((64, 172, 79, 191), Click::CycleView),
    ((74, 166, 89, 185), Click::Screen(2)),
    ((95, 165, 110, 184), Click::Screen(3)),
    ((111, 163, 126, 182), Click::Screen(4)),
    ((129, 163, 144, 182), Click::Screen(5)),
    ((146, 163, 161, 182), Click::Front),
    ((166, 163, 181, 182), Click::Rear),
    ((185, 163, 200, 182), Click::Turret),
    ((202, 165, 217, 184), Click::External),
    ((215, 168, 230, 187), Click::Missile),
    ((228, 169, 243, 188), Click::Combat),
    ((241, 174, 256, 193), Click::Capsule),
    ((265, 177, 280, 196), Click::Navigation),
    ((280, 178, 295, 197), Click::Labels),
    ((300, 178, 315, 197), Click::Hyperspace),
    ((24, 158, 41, 166), Click::Target),
];

/// The console click under logical pixel (x, y), if any.
pub fn click(x: i32, y: i32) -> Option<Click> {
    HOT_AREAS
        .iter()
        .rev()
        .find(|((x0, y0, x1, y1), _)| (*x0..=*x1).contains(&x) && (*y0..=*y1).contains(&y))
        .map(|(_, click)| *click)
}

/// The acceleration step for a key (6, 7, 8, 9, 0), as `FUNC_000208` maps them.
pub fn acceleration_for_digit(digit: char) -> Option<usize> {
    match digit {
        '6' => Some(0),
        '7' => Some(1),
        '8' => Some(2),
        '9' => Some(3),
        '0' => Some(4),
        _ => None,
    }
}
