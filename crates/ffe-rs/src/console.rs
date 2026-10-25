//! The original console: `DATA_005100`'s 82 buttons, their visibility and
//! current image (`F261`-`F264`), the time-acceleration meter, the screen and
//! view buttons, the status lamps and the engines-off equipment page.
//!
//! `Button` is one `ConButton` record; [`BUTTONS`] is the whole table decoded
//! byte-exactly from `ffedat.asm:4601` (`crate::console_table`). `Console` keeps
//! the mutable `visible`/`cur` state the original keeps in the table, including
//! `F262`'s radio reset, and [`Console::apply_flight`] derives the original's
//! page from the view, meter, labels and ship systems. The hot areas
//! (`DATA_007684`) stay in this module as the original's 22-row list.
//!
//! The bar at the lower left is the time-acceleration meter, not a throttle: six
//! steps rising from a pause mark, each a grey icon with a red lit twin, of
//! which one is lit (a radio group). Clicking a step, or the keys 6, 7, 8, 9 and
//! 0 (`FUNC_000208`: the lever's actions 0xc1-0xc5 are translated to those key
//! codes), sets the frame time scale (`DATA_009012`: 0x3e2 per 20 ms at 1x). The
//! four icons beside it open other screens (F1 views, F2 status, F3 map, F4
//! comms), which are not ported. Bitmap ids from 96 are the secondary table.
//! See `docs/notes/console.md`.

/// The original `DATA_005100` table, for tests and the integrator's audits.
pub fn buttons() -> &'static [Button; 82] {
    &crate::console_table::BUTTONS
}

/// One `ConButton` record of `DATA_005100`: position, ten image ids, the
/// visibility byte, the current image index and the radio group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Button {
    pub x: i32,
    pub y: i32,
    pub bitmaps: [u16; 10],
    pub visible: bool,
    pub cur: u8,
    pub radio: u16,
}

impl Button {
    /// A record with the table's unused `+0x38` word omitted (it is 1 in every
    /// record and read by no code in this build).
    pub const fn new(x: i32, y: i32, bitmaps: [u16; 10], visible: u8, cur: u8, radio: u16) -> Self {
        Self {
            x,
            y,
            bitmaps,
            visible: visible & 1 != 0,
            cur,
            radio,
        }
    }

    /// The bitmap id of the current image (`bmpindex[curindex]`), or `None` for
    /// an image the record does not have.
    pub fn image(&self) -> Option<u16> {
        self.bitmaps
            .get(usize::from(self.cur))
            .copied()
            .filter(|id| *id != 0 || self.cur != 0)
    }

    /// The record's bitmap for image `index`.
    pub fn bitmap(&self, index: u8) -> Option<u16> {
        self.bitmaps
            .get(usize::from(index))
            .copied()
            .filter(|id| *id != 0)
    }
}

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

/// The right-hand console buttons (`DATA_005100` records 0x0f-0x11): position,
/// idle and lit secondary bitmap ids, and the click. The table gives
/// hyperspace 0x99/0xa8 at 296,178, text labels 0x9a/0xa9 at 274,178 and the
/// navigation computer 0x9b/0xaa at 261,177. Their hot areas (`DATA_007684`)
/// are 15 pixels wide and skip the artwork's left edge: 300-315, 280-295 and
/// 265-280 respectively, at y 178-197 (177-196 for the navigation computer).
pub const RIGHT_BUTTONS: [(i32, i32, u16, u16, Click); 3] = [
    (296, 178, 0x99, 0xa8, Click::Hyperspace),
    (274, 178, 0x9a, 0xa9, Click::Labels),
    (261, 177, 0x9b, 0xaa, Click::Navigation),
];

/// The navigation computer's list marker (`Bitmap 0x31`, 11x10, masked zero).
pub const NAV_MARKER_BITMAP: &str = "base:bitmap/primary/049";

/// What the console shows besides the views: the time acceleration step,
/// whether the game is paused, the ship's speed in metres per second and, when
/// the original flight model is running, its set speed. The right-hand buttons'
/// on/off states are the original's `ConsoleSetButtonImage` images: the labels
/// button is lit while player flag `+0xe9` bit 2 is set (`l`), the navigation
/// computer while its list is open (`n`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConsoleState {
    pub acceleration: usize,
    pub paused: bool,
    pub speed: i32,
    pub set_speed: Option<i32>,
    pub labels: bool,
    pub navigation: bool,
    /// The `F2` status screen's engines-off/equipment page (`F1120`,
    /// `DATA_008871 & 8`).
    pub status_page: bool,
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
            // F2 is the status/equipment page (F1120), F3 the map, F1 the views
            // and F4 the station/comms action (F343): from `0x2a`/`0x30` it is
            // the services board, and `main` opens it at the pad.
            Click::Screen(2)
            | Click::Screen(3)
            | Click::Screen(4)
            | Click::Navigation
            | Click::Hyperspace => None,
            Click::Screen(_) => Some("F5 (comms) is not ported yet."),
            Click::Missile => Some("The missile view needs equipment (not ported)."),
            Click::Combat => Some("The combat computer needs equipment (not ported)."),
            // F1056 (78528) calls F1098; `main` runs the launch or reports that
            // no escape craft is fitted.
            Click::Capsule => None,
            // The labels toggle is the original's player flag +0xe9 bit 2.
            Click::Labels => None,
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

/// The player ship's equipment/status bytes the console routines read. The
/// equipment words now come from [`ffe_core::equipment::Loadout`] through
/// [`ShipSystems::with_loadout`]; the engine, target and GUI bytes stay
/// explicit runtime inputs with new-game defaults. `docs/notes/dashboard.md`
/// §6.10 lists every condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShipSystems {
    /// `player+0xe8`: the engine/autopilot status byte (F1116 compares it with
    /// 0x2e, 0x28, 0x34, 6 and 2).
    pub e8: u8,
    /// `player+0xe9`: bit 0 the labels flag, bit 1 the 0x38/0x46 image, bit 3
    /// the engines-off page (`DATA_008871`).
    pub e9: u8,
    /// `player+0xec`: the target-present byte for the 0x49 lamp.
    pub ec: u8,
    /// `player+0xf0`: the dword that suppresses the 0x47 lamp.
    pub f0: u32,
    /// `player+0xc6`'s word: the 0x45 lamp reads bit 0x40 here and bit 0 in the
    /// high byte (`player+0xc7`). These are status bits, not equipment.
    pub status: u16,
    /// `ship+0xc8`: the general equipment bitfield (`Category::General`), whose
    /// four bytes are `ship+0xc8..+0xcb`. The bit from a record is
    /// `(record[0x12] ^ 0x10) & 0x1f` (F393).
    pub general: u32,
    /// `ship+0xca`: F272's scanner gate (`& 4`). The new-game Eagle carries the
    /// standard scanner, as every flyable ship does.
    pub ca: u8,
    /// `player+0xef`: the missile-view equipment byte.
    pub ef: u8,
    /// `ship+0xd6..+0xdf`: ten missile pylons; the low nibble selects the icon
    /// image (F1120) and the byte is the missile record's code (F418).
    pub slots: [u8; 10],
    /// `player+0x174`: the dword F1120's 0x51 condition tests.
    pub p174: u16,
    /// `player+0x511e`.
    pub p511e: u32,
    /// `player+0x268`.
    pub p268: u8,
    /// `DATA_007246`/`DATA_007247`: the targeters' state.
    pub data_246: bool,
    pub data_247: bool,
    /// `DATA_003fb6`: bit 1 in the 0x48 image condition.
    pub b6: u8,
    /// `ship+0xd1`, gun mounts (the turret button and view).
    pub mounts: i8,
}

impl Default for ShipSystems {
    fn default() -> Self {
        // An empty ship: one gun mount and no equipment. `main` calls
        // `with_loadout` with the saved loadout; the new game's F998 loadout is
        // `Loadout::new_game`.
        Self {
            e8: 0,
            e9: 0,
            ec: 0,
            f0: 0,
            status: 0,
            general: 0,
            ca: 0x04,
            ef: 0,
            slots: [0; 10],
            p174: 0,
            p511e: 0,
            p268: 0,
            data_246: false,
            data_247: false,
            b6: 0,
            mounts: 1,
        }
    }
}

impl ShipSystems {
    /// Fill the words that come from the fitted loadout: the general-equipment
    /// dword and the ten missile pylons. The rest (engine/target/status bytes)
    /// are runtime state and stay as the caller set them.
    pub fn with_loadout(mut self, loadout: &ffe_core::equipment::Loadout) -> Self {
        self.general = loadout.general;
        self.slots = loadout.missiles;
        self
    }

    /// `player+0xc6` and its high byte `player+0xc7`.
    fn status_bytes(&self) -> [u8; 2] {
        self.status.to_le_bytes()
    }

    /// `ship+0xc8..+0xcb`: the general-equipment dword's four bytes.
    fn general_bytes(&self) -> [u8; 4] {
        self.general.to_le_bytes()
    }
}

/// The console's mutable `DATA_005100` state. Each record keeps the original's
/// `visible` and `curindex`; `F261` draws the visible records in index order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Console {
    pub buttons: [Button; 82],
}

impl Default for Console {
    fn default() -> Self {
        Self::new()
    }
}

impl Console {
    /// The table at its initial state (every record hidden; the two records the
    /// data gives a non-zero image keep it).
    pub fn new() -> Self {
        Self {
            buttons: crate::console_table::BUTTONS,
        }
    }

    /// `F263`: make a record visible.
    pub fn show(&mut self, id: usize) {
        if let Some(button) = self.buttons.get_mut(id) {
            button.visible = true;
        }
    }

    /// `F264`: hide a record.
    pub fn hide(&mut self, id: usize) {
        if let Some(button) = self.buttons.get_mut(id) {
            button.visible = false;
        }
    }

    pub fn is_visible(&self, id: usize) -> bool {
        self.buttons.get(id).is_some_and(|button| button.visible)
    }

    pub fn image(&self, id: usize) -> Option<u16> {
        self.buttons.get(id).and_then(Button::image)
    }

    /// `F262_ConsoleSetButtonImage`: set a record's image and, when the new image
    /// is non-zero, reset every record in the same radio group to image 0 first.
    /// Setting the image a record already has is a no-op.
    pub fn set_image(&mut self, id: usize, image: u8) {
        let Some(button) = self.buttons.get(id) else {
            return;
        };
        if button.cur == image {
            return;
        }
        let radio = button.radio;
        if image != 0 {
            for other in &mut self.buttons {
                if other.radio == radio {
                    other.cur = 0;
                }
            }
        }
        self.buttons[id].cur = image;
    }

    /// The visible records in table order, with their current bitmap ids.
    pub fn drawable(&self) -> impl Iterator<Item = (usize, &Button)> {
        self.buttons
            .iter()
            .enumerate()
            .filter(|(_, button)| button.visible)
    }

    /// `F991`: the view-row buttons 0x10-0x19. 0x10 is the labels toggle;
    /// 0x11-0x14 are the equipment icons, conditioned on the general-equipment
    /// dword (bits 5, 27 or 3, 6, and 9 with `player+0xef`) exactly as F991's
    /// byte/dword tests read it; 0x15-0x19 are the views and the status page.
    pub fn apply_view_row(
        &mut self,
        view: crate::camera::ViewMode,
        systems: &ShipSystems,
        console: &ConsoleState,
    ) {
        use crate::camera::ViewMode;
        self.show(0x10);
        self.show_if(0x11, systems.general & (1 << 5) != 0);
        self.show_if(
            0x12,
            systems.general & (1 << 27) != 0 || systems.general & (1 << 3) != 0,
        );
        self.show_if(0x13, systems.general & (1 << 6) != 0);
        self.show_if(0x14, systems.ef != 0 && systems.general & (1 << 9) != 0);
        for id in [0x15usize, 0x17, 0x18, 0x19] {
            self.show(id);
        }
        if systems.mounts > 2 {
            self.show(0x16);
        } else {
            self.hide(0x16);
        }
        // The views are radio group 1: one image 1, the rest 0. 0x14 is the
        // missile view's button (F1054's F5/F10).
        for id in [0x14usize, 0x15, 0x16, 0x17, 0x18] {
            self.set_image(id, 0);
        }
        let selected = match view {
            ViewMode::Front => 0x18,
            ViewMode::Rear => 0x17,
            ViewMode::TopTurret | ViewMode::BottomTurret => 0x16,
            ViewMode::External => 0x15,
            ViewMode::Missile => 0x14,
        };
        if self.is_visible(selected) {
            self.set_image(selected, 1);
        }
        // F1117 clears the 0x19 image; F1120 lights it on the equipment page.
        self.set_image(0x19, u8::from(console.status_page));
    }

    /// `F1117`'s normal page: F1116's five lamps plus the three ship lamps.
    pub fn apply_normal_page(&mut self, systems: &ShipSystems) {
        let status = systems.status_bytes();
        let general = systems.general_bytes();
        for id in [0x37usize, 0x38, 0x39] {
            self.show(id);
        }
        if systems.e8 >= 0x2e && systems.f0 == 0 {
            self.show(0x47);
        } else {
            self.hide(0x47);
        }
        if systems.p268 != 0 && systems.e8 <= 6 && status[0] & 0x40 != 0 && status[1] & 1 != 0 {
            self.show(0x45);
        } else {
            self.hide(0x45);
        }
        self.show(0x46);
        let image = if systems.e9 & 2 != 0 { 0 } else { 1 };
        self.set_image(0x38, image);
        self.set_image(0x46, image);
        self.show(0x48);
        if !systems.data_246 {
            self.show(0x49);
        } else {
            self.hide(0x49);
        }
        if systems.e8 == 2 {
            self.set_image(0x39, 1);
        } else {
            self.set_image(0x39, 0);
            let image = u8::from(systems.ec != 0 && general[2] & 0x20 != 0);
            self.set_image(0x49, image);
        }
        let image = if systems.e8 < 0x28 && (systems.e9 & 1 == 0 || systems.b6 & 2 != 0) {
            1
        } else {
            0
        };
        self.set_image(0x48, image);
    }

    /// `F1120`: the engines-off/equipment page. Hides the five F1116 lamps,
    /// shows the 60x45 ship diagram (0x3a), ten missile icons (0x3b-0x44), the
    /// system lamps (0x4a-0x4e, 0x4f, 0x51) and lights 0x19. Every lamp and icon
    /// reads the general-equipment bytes or the missile pylons, as the original
    /// does.
    pub fn apply_engines_off_page(&mut self, systems: &ShipSystems) {
        let general = systems.general_bytes();
        let status = systems.status_bytes();
        for id in [0x45usize, 0x46, 0x47, 0x48, 0x49] {
            self.hide(id);
        }
        self.show(0x3a);
        if systems.data_247 {
            self.hide(0x4f);
        } else {
            self.show(0x4f);
        }
        for (index, value) in systems.slots.iter().enumerate() {
            let id = 0x3b + index;
            if *value == 0 {
                self.hide(id);
            } else {
                self.show(id);
                self.set_image(id, value & 0xf);
            }
        }
        // 0x51: the chaff dispenser (general bit 11) and player+0x174's count.
        if general[1] & 8 != 0 && systems.p174 != 0 {
            self.show(0x51);
        } else {
            self.hide(0x51);
        }
        // 0x4a: Naval ECM (bit 23) is image 1, plain ECM (bit 19) image 0.
        if general[2] & 0x80 != 0 {
            self.set_image(0x4a, 1);
            self.show(0x4a);
        } else if general[2] & 8 != 0 {
            self.set_image(0x4a, 0);
            self.show(0x4a);
        } else {
            self.hide(0x4a);
        }
        // 0x4b: energy bomb (bit 26); 0x4c: hyperspace cloud analyser (bit 24);
        // 0x4d: military cameras (bit 2).
        self.show_if(0x4b, general[3] & 4 != 0);
        self.show_if(0x4c, general[3] & 1 != 0);
        self.show_if(0x4d, general[0] & 4 != 0);
        // 0x4e: a missile in flight (player+0x511e) or the c6 status bit 0x10.
        self.show_if(
            0x4e,
            (systems.p511e != 0 && systems.e8 == 0x34) || status[0] & 0x10 != 0,
        );
        // The three ship lamps are not hidden by F1120.
        for id in [0x37usize, 0x38, 0x39] {
            self.show(id);
        }
        self.set_image(0x19, 1);
    }

    fn show_if(&mut self, id: usize, condition: bool) {
        if condition {
            self.show(id);
        } else {
            self.hide(id);
        }
    }

    /// The flight console's whole page: the screen buttons, the meter, the
    /// right-hand buttons, `F991`'s view row and either `F1116`'s lamps or
    /// `F1120`'s equipment page. This is the state `F261` then draws in index
    /// order; nothing outside the table is needed.
    pub fn apply_flight(
        &mut self,
        view: crate::camera::ViewMode,
        console: &ConsoleState,
        systems: &ShipSystems,
    ) {
        *self = Self::new();
        // F1439/F1003: the four screen buttons, with F1's image lit.
        for id in 0x0bu8..=0x0e {
            self.show(usize::from(id));
        }
        self.set_image(0x0e, 1);
        // The time-acceleration meter (0x26-0x2b), radio group 3.
        for id in 0x26u8..=0x2b {
            self.show(usize::from(id));
        }
        for id in 0x26usize..=0x2b {
            self.set_image(id, 0);
        }
        self.set_image(0x26 + console.lit_step(), 1);
        // The right-hand buttons 0x0f-0x11 (their images are the original's
        // labels/navigation on states; hyperspace has no lit twin).
        for id in 0x0fusize..=0x11 {
            self.show(id);
        }
        self.set_image(0x10, u8::from(console.labels));
        self.set_image(0x11, u8::from(console.navigation));
        self.apply_view_row(view, systems, console);
        if console.status_page {
            self.apply_engines_off_page(systems);
        } else {
            self.apply_normal_page(systems);
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f1120_draws_the_missile_pylons_from_the_d6_words() {
        let mut systems = ShipSystems {
            slots: [0x82, 0x82, 0, 0, 0, 0, 0, 0, 0, 0],
            ..ShipSystems::default()
        };
        let mut console = Console::new();
        console.apply_engines_off_page(&systems);
        // The two 0x82 words show icons 0x3b/0x3c with image 2 (& 0xf).
        assert!(console.is_visible(0x3b));
        assert!(console.is_visible(0x3c));
        assert_eq!(console.buttons[0x3b].cur, 2);
        assert_eq!(console.buttons[0x3c].cur, 2);
        assert!(!console.is_visible(0x3d));
        // The 0x4f lamp is hidden while DATA_007247 (the combat targetter is
        // armed) is set.
        assert!(console.is_visible(0x4f));
        systems.data_247 = true;
        console.apply_engines_off_page(&systems);
        assert!(!console.is_visible(0x4f));
    }
}
