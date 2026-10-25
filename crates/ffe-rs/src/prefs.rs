//! The "Preference pages available" screen the original shows on Escape.
//!
//! The pages, their order, labels, values and hot areas are the original's:
//! `ffedat.asm` string table `DATA_004894` (`DATA_004963`..`DATA_004967`), the
//! hot-area lists `DATA_004907` (`DATA_004898`..`DATA_004905`), the per-page
//! state offsets `DATA_004896` and the SysMenu handlers `FUNC_000216`,
//! `FUNC_000225` and `FUNC_000228` in `ffe.asm`. The page list is
//!
//! | page | title | source |
//! |---|---|---|
//! | 0 | `Preference pages available:` | `DATA_004963` |
//! | 1 | `Graphic Detail` | `DATA_004964` |
//! | 2 | `Sound Effects` | `DATA_004965` |
//! | 3 | `Others` | `DATA_004966` |
//! | 4 | `Maps` | `DATA_004967` |
//!
//! The original lays the menu out on the 320x158 upper view: a title at x 32,
//! the left column's icons at x 32 and labels at x 48, the right column's icons
//! at x 112 and labels at x 128 (the string's `0xfe` position codes are twice
//! their pixel x), rows 10 pixels apart. Only the 8x8 icon square is a hot
//! area, exactly as `DATA_004903`..`DATA_004905` list it; this port keeps the
//! same rectangles and adds keyboard navigation (a port addition).
//!
//! Port additions, clearly marked in the UI and in `docs/DEVIATIONS.md`:
//! the `Port Options` page (page 5, modern/classic controls) and the `Quit`
//! label where the original says `Quit to DOS`. The settings this port cannot
//! apply keep the original's labels and defaults but are drawn grey and do not
//! change (the page footer says so).

/// Pages, in the original's order, plus the port-only options page.
pub const MAIN: usize = 0;
pub const GRAPHIC: usize = 1;
pub const SOUND: usize = 2;
pub const OTHERS: usize = 3;
pub const MAPS: usize = 4;
pub const PORT: usize = 5;
pub const PAGES: usize = 6;

/// The original's "Level of Shape Detail" values (`DATA_004964`).
pub const SHAPE_DETAIL: [&str; 4] = ["Low", "Medium", "High", "Very High"];
/// The original's "Space Colour" values (`DATA_004964`). The value is the
/// blue of the player's 3-byte background colour `DATA_008815..817`, set as
/// `(0, 0, index)` with four-bit components by `FUNC_001575`.
pub const SPACE_COLOURS: [&str; 5] = [
    "Black",
    "Very Dark Blue",
    "Dark Blue",
    "Medium Blue",
    "Light Blue",
];

/// The original's "Other Options" rows (`DATA_004966`), in order.
pub const OTHER_LABELS: [&str; 10] = [
    "Space dust, clouds etc",
    "Background stars",
    "Lock external view to ship",
    "Reverse left-right controls",
    "Reverse up-down controls",
    "Elite control method in space",
    "Show targeting tunnels",
    "Allow targeting tunnels for ships",
    "Use filename extensions",
    "Video",
];

/// The original's "Map Options" rows (`DATA_004967`).
pub const MAP_LABELS: [&str; 5] = ["Stalks", "Grid", "Trade routes", "Names text", "Orbits"];

/// The original's "Sound Effects" rows (`DATA_004965`).
pub const SOUND_LABELS: [&str; 10] = [
    "Sound effects on",
    "Music on",
    "Engine sound",
    "Music after hyperspace",
    "Space station",
    "Docking music",
    "Wind noise",
    "Music in battles",
    "Icon beeps",
    "Continuous music",
];

/// The original's music list (`DATA_004965`), bottom right at x 160, 9 pixels
/// apart.
pub const MUSIC: [&str; 16] = [
    "First Encounters Theme - Quality Quartet",
    "Atmosphere - Quality Quartet",
    "Travel - Quality Quartet",
    "Drama - Quality Quartet",
    "Suspense - Quality Quartet",
    "Rock - Quality Quartet",
    "Escape - Quality Quartet",
    "Paradise - Quality Quartet",
    "Alien Encounters - Quality Quartet",
    "In the Hall of the Mountain King - Greig",
    "Frontier Main Theme - Lowe",
    "Baba Yaga - Mussorgsky",
    "Night on the Bare Mountain - Mussorgsky",
    "Frontier Second Theme - Lowe",
    "Ride of the Valkyries - Wagner",
    "Blue Danube - Strauss",
];

/// `DATA_008815..817`: the requested background colour, four-bit components
/// `(r, g, b)`. `FUNC_001575` copies it to `DATA_009223` and `DATA_009214`,
/// `FUNC_001578`/`FUNC_001579` look up the nearest palette entry for the
/// clear colour (`DATA_009222`), and the terrain code mixes the three bytes
/// into the sky. The five settings only change the blue component.
///
/// The original quantises the request to its runtime 256-colour palette (the
/// upper half is rebuilt by `F1330`; see [`crate::palette`]). The captured
/// reference frame's default ("Light Blue", requested index 4) is palette entry
/// 182, six-bit `(3, 3, 19)` = `#0c0c4c`. The owner decision (EXP-013) keeps
/// that captured entry for the default and derives the other four settings
/// linearly from it rather than from the port's own scene-dependent build:
/// each component is rounded to the nearest six-bit value and then expanded with
/// `v * 255 / 63`, which reproduces the capture's `#0c0c4c` and makes index 0
/// exactly `#000000`. The intro's own `F1575` value is resolved through the
/// runtime palette separately (`MODEL_SECOND_BACKGROUND`).
pub fn space_colour_rgb(index: usize) -> u32 {
    /// The captured default: palette entry 182, six-bit `(3, 3, 19)`.
    const LIGHT_BLUE: [u32; 3] = [3, 3, 19];
    let index = index.min(4) as u32;
    let component = |six: u32| {
        let six = (six * index + 2) / 4;
        six * 255 / 63
    };
    (component(LIGHT_BLUE[0]) << 16) | (component(LIGHT_BLUE[1]) << 8) | component(LIGHT_BLUE[2])
}

/// What a row or key does. `Other`, `Map` and `Sound` carry the row index.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Page(usize),
    ShapeDetail(usize),
    SpaceColour(usize),
    Other(usize),
    Map(usize),
    Sound(usize),
    Modern(bool),
    /// "Return to Game": close the pages.
    Close,
    /// The port's Quit (the original's "Quit to DOS").
    Quit,
    /// A row the original has but this port does not implement (it stays
    /// grey and inert), e.g. Load/Save/Delete/New Game.
    Ignored,
}

/// How the icon before a row is drawn: the original's 0x2c ("on") and 0x2d
/// ("off") bitmaps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mark {
    Action,
    Radio(bool),
    Toggle(bool),
}

/// One icon+label row. `x`, `y` are the original hot area's top-left (the
/// icon, not the label); the label is drawn at `x + 16`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Row {
    pub x: i32,
    pub y: i32,
    pub label: &'static str,
    pub action: Action,
    pub mark: Mark,
    /// False: the original has the row but this port cannot apply it; the
    /// label and icon are drawn grey.
    pub applied: bool,
    /// Original label colour (palette index): 15 white, 16 red, 18 yellow.
    pub colour: u8,
}

/// Extra text: page headings, the commander line and the page footnotes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Text {
    pub x: i32,
    pub y: i32,
    pub text: &'static str,
    pub colour: u8,
}

/// Keyboard actions (a port addition; the original pages are mouse-driven).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Backspace,
    Escape,
}

/// The result of a click or key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Activate {
    None,
    Close,
    Quit,
}

/// Which settings the port applies. The original's `FUNC_000216` toggles the
/// player flags at `+0x3f51..5a` (`DATA_004904`); only these three move the
/// port's renderer.
pub fn other_applied(index: usize) -> bool {
    (1..=4).contains(&index)
}

/// The Escape preference state. The defaults are `FUNC_000219_SysMenuInit`'s
/// flags copy `DATA_004895`: shape detail High (index 2), space colour Light
/// Blue (index 4), background stars on, reverse controls off, and every
/// original option on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prefs {
    pub open: bool,
    pub page: usize,
    /// The row the port's keyboard navigation marks (not in the original).
    pub selected: usize,
    pub shape_detail: usize,
    pub space_colour: usize,
    pub others: [bool; 10],
    pub maps: [bool; 5],
    pub sound: [bool; 10],
    pub modern: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            open: false,
            page: MAIN,
            selected: 0,
            shape_detail: 2,
            space_colour: 4,
            others: [
                true, true, true, false, false, false, true, false, true, true,
            ],
            maps: [true; 5],
            sound: [true; 10],
            modern: true,
        }
    }
}

impl Prefs {
    /// Escape opens the pages; Escape closes them again (the original's
    /// `FUNC_000224` Escape flow, without its separate pause step).
    pub fn escape(&mut self) -> Activate {
        if self.open {
            self.open = false;
            Activate::Close
        } else {
            self.open = true;
            self.page = MAIN;
            self.selected = 0;
            Activate::None
        }
    }

    /// The F3 switch between the two flight presets; the Port Options rows set
    /// the same value through [`Prefs::apply`].
    pub fn toggle_modern(&mut self) {
        self.modern = !self.modern;
    }

    /// The rows of the current page, in the original's order.
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        match self.page {
            MAIN => {
                rows.push(row(
                    32,
                    28,
                    "Graphic Detail",
                    Action::Page(GRAPHIC),
                    Mark::Action,
                    true,
                    15,
                ));
                rows.push(row(
                    32,
                    38,
                    "Sound Effects",
                    Action::Page(SOUND),
                    Mark::Action,
                    true,
                    15,
                ));
                rows.push(row(
                    32,
                    48,
                    "Others",
                    Action::Page(OTHERS),
                    Mark::Action,
                    true,
                    15,
                ));
                rows.push(row(
                    32,
                    58,
                    "Maps",
                    Action::Page(MAPS),
                    Mark::Action,
                    true,
                    15,
                ));
                // Port addition: a page for the port's own options, in the
                // empty row the original leaves at y 68.
                rows.push(row(
                    32,
                    68,
                    "Port Options",
                    Action::Page(PORT),
                    Mark::Action,
                    true,
                    18,
                ));
                rows.push(row(32, 78, "Quit", Action::Quit, Mark::Action, true, 16));
                rows.push(row(
                    112,
                    28,
                    "Load",
                    Action::Ignored,
                    Mark::Action,
                    false,
                    15,
                ));
                rows.push(row(
                    112,
                    38,
                    "Save",
                    Action::Ignored,
                    Mark::Action,
                    false,
                    15,
                ));
                rows.push(row(
                    112,
                    48,
                    "Delete",
                    Action::Ignored,
                    Mark::Action,
                    false,
                    15,
                ));
                rows.push(row(
                    112,
                    58,
                    "Return to Game",
                    Action::Close,
                    Mark::Action,
                    true,
                    15,
                ));
                rows.push(row(
                    112,
                    78,
                    "New Game",
                    Action::Ignored,
                    Mark::Action,
                    false,
                    16,
                ));
            }
            GRAPHIC => {
                for (index, label) in SHAPE_DETAIL.iter().enumerate() {
                    rows.push(row(
                        32,
                        24 + 10 * index as i32,
                        label,
                        Action::ShapeDetail(index),
                        Mark::Radio(self.shape_detail == index),
                        // The original stores the level; this port always
                        // draws full detail (see the footnote).
                        false,
                        15,
                    ));
                }
                for (index, label) in SPACE_COLOURS.iter().enumerate() {
                    rows.push(row(
                        32,
                        80 + 10 * index as i32,
                        label,
                        Action::SpaceColour(index),
                        Mark::Radio(self.space_colour == index),
                        true,
                        15,
                    ));
                }
                rows.push(main_menu(32, 136));
            }
            SOUND => {
                for (index, label) in SOUND_LABELS.iter().enumerate() {
                    rows.push(row(
                        32,
                        28 + 10 * index as i32,
                        label,
                        Action::Sound(index),
                        Mark::Toggle(self.sound[index]),
                        // The port applies all ten switches to its audio player.
                        true,
                        15,
                    ));
                }
                rows.push(main_menu(32, 138));
            }
            OTHERS => {
                for (index, label) in OTHER_LABELS.iter().enumerate() {
                    rows.push(row(
                        32,
                        28 + 10 * index as i32,
                        label,
                        Action::Other(index),
                        Mark::Toggle(self.others[index]),
                        other_applied(index),
                        15,
                    ));
                }
                rows.push(main_menu(32, 136));
            }
            MAPS => {
                for (index, label) in MAP_LABELS.iter().enumerate() {
                    rows.push(row(
                        32,
                        28 + 10 * index as i32,
                        label,
                        Action::Map(index),
                        Mark::Toggle(self.maps[index]),
                        // The port's galaxy map has its own rules.
                        false,
                        15,
                    ));
                }
                rows.push(main_menu(32, 136));
            }
            PORT => {
                rows.push(row(
                    32,
                    38,
                    "Modern",
                    Action::Modern(true),
                    Mark::Radio(self.modern),
                    true,
                    15,
                ));
                rows.push(row(
                    32,
                    48,
                    "Classic",
                    Action::Modern(false),
                    Mark::Radio(!self.modern),
                    true,
                    15,
                ));
                rows.push(main_menu(32, 136));
            }
            _ => {}
        }
        rows
    }

    /// Page headings, the commander line, the music list and the footnotes.
    pub fn texts(&self) -> Vec<Text> {
        let mut texts = Vec::new();
        match self.page {
            MAIN => {
                // `DATA_004963`: the original's default commander name.
                texts.push(Text {
                    x: 32,
                    y: 8,
                    text: "Commander Jameson",
                    colour: 17,
                });
                texts.push(Text {
                    x: 32,
                    y: 18,
                    text: "Preference pages available:",
                    colour: 16,
                });
                texts.push(Text {
                    x: 32,
                    y: 146,
                    text: "Yellow: port additions. Grey: not implemented.",
                    colour: 18,
                });
            }
            GRAPHIC => {
                texts.push(Text {
                    x: 32,
                    y: 14,
                    text: "Level of Shape Detail:",
                    colour: 16,
                });
                texts.push(Text {
                    x: 32,
                    y: 70,
                    text: "Space Colour:",
                    colour: 16,
                });
                texts.push(Text {
                    x: 32,
                    y: 146,
                    text: "This port always draws full shape detail.",
                    colour: 18,
                });
            }
            SOUND => {
                texts.push(Text {
                    x: 32,
                    y: 18,
                    text: "Sound Effects:",
                    colour: 16,
                });
                texts.push(Text {
                    x: 60,
                    y: 0,
                    text: "Choose Music From:",
                    colour: 16,
                });
                for (index, label) in MUSIC.iter().enumerate() {
                    texts.push(Text {
                        x: 160,
                        y: 9 * index as i32,
                        text: label,
                        colour: 8,
                    });
                }
                texts.push(Text {
                    x: 32,
                    y: 146,
                    text: "Sound and music use the system MIDI device.",
                    colour: 18,
                });
            }
            OTHERS => {
                texts.push(Text {
                    x: 32,
                    y: 18,
                    text: "Other Options:",
                    colour: 16,
                });
                texts.push(Text {
                    x: 32,
                    y: 146,
                    text: "Grey options are not implemented in this port.",
                    colour: 18,
                });
            }
            MAPS => {
                texts.push(Text {
                    x: 32,
                    y: 18,
                    text: "Map Options:",
                    colour: 16,
                });
                texts.push(Text {
                    x: 32,
                    y: 146,
                    text: "The port's galaxy map uses its own rules.",
                    colour: 18,
                });
            }
            PORT => {
                texts.push(Text {
                    x: 32,
                    y: 18,
                    text: "Port options:",
                    colour: 18,
                });
                texts.push(Text {
                    x: 32,
                    y: 28,
                    text: "Controls:",
                    colour: 15,
                });
                texts.push(Text {
                    x: 48,
                    y: 62,
                    text: "Modern: W/S thrust, arrows steer, Q/E roll.",
                    colour: 15,
                });
                texts.push(Text {
                    x: 48,
                    y: 72,
                    text: "Classic: hold right mouse and move to steer.",
                    colour: 15,
                });
                texts.push(Text {
                    x: 48,
                    y: 82,
                    text: "Enter/Shift thrust; ,/. yaw; A/Z pitch; [ ] roll.",
                    colour: 15,
                });
                texts.push(Text {
                    x: 48,
                    y: 92,
                    text: "F3 or these rows switch preset; docs/CONTROLS.md.",
                    colour: 15,
                });
                texts.push(Text {
                    x: 32,
                    y: 146,
                    text: "Port additions, not in the original DOS release.",
                    colour: 18,
                });
            }
            _ => {}
        }
        texts
    }

    /// Apply an action and report whether the page state should close.
    pub fn apply(&mut self, action: Action) -> Activate {
        match action {
            Action::Page(page) if page < PAGES => {
                self.page = page;
                self.selected = 0;
                Activate::None
            }
            Action::ShapeDetail(index) if index < SHAPE_DETAIL.len() => {
                self.shape_detail = index;
                Activate::None
            }
            Action::SpaceColour(index) if index < SPACE_COLOURS.len() => {
                self.space_colour = index;
                Activate::None
            }
            Action::Other(index) if index < OTHER_LABELS.len() && other_applied(index) => {
                self.others[index] = !self.others[index];
                Activate::None
            }
            Action::Sound(index) if index < SOUND_LABELS.len() => {
                self.sound[index] = !self.sound[index];
                Activate::None
            }
            Action::Modern(modern) => {
                self.modern = modern;
                Activate::None
            }
            Action::Close => {
                self.open = false;
                Activate::Close
            }
            Action::Quit => Activate::Quit,
            Action::Page(_)
            | Action::ShapeDetail(_)
            | Action::SpaceColour(_)
            | Action::Other(_)
            | Action::Map(_)
            | Action::Sound(_)
            | Action::Ignored => Activate::None,
        }
    }

    /// A click at logical (x, y): the original's icon rectangles only, first
    /// match wins (as `FUNC_001440` scans the list).
    pub fn click(&mut self, x: i32, y: i32) -> Activate {
        let rows = self.rows();
        let Some((index, row)) = rows
            .iter()
            .enumerate()
            .find(|(_, row)| x >= row.x && x < row.x + 8 && y >= row.y && y < row.y + 8)
        else {
            return Activate::None;
        };
        self.selected = index;
        self.apply(row.action)
    }

    /// A key (a port addition). Escape toggles the pages through
    /// [`Prefs::escape`]; the caller should handle it there.
    pub fn key(&mut self, key: Key) -> Activate {
        if !self.open {
            return Activate::None;
        }
        let rows = self.rows();
        match key {
            Key::Up | Key::Left => {
                if !rows.is_empty() {
                    self.selected = self.selected.saturating_sub(1);
                }
            }
            Key::Down | Key::Right => {
                if !rows.is_empty() {
                    self.selected = (self.selected + 1).min(rows.len() - 1);
                }
            }
            Key::Backspace => {
                self.page = MAIN;
                self.selected = 0;
            }
            Key::Enter => {
                if let Some(row) = rows.get(self.selected) {
                    return self.apply(row.action);
                }
            }
            Key::Escape => return self.escape(),
        }
        Activate::None
    }
}

fn row(
    x: i32,
    y: i32,
    label: &'static str,
    action: Action,
    mark: Mark,
    applied: bool,
    colour: u8,
) -> Row {
    Row {
        x,
        y,
        label,
        action,
        mark,
        applied,
        colour,
    }
}

fn main_menu(x: i32, y: i32) -> Row {
    row(
        x,
        y,
        "Main Menu",
        Action::Page(MAIN),
        Mark::Action,
        true,
        15,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_list_is_the_originals_plus_the_marked_port_page() {
        let prefs = Prefs::default();
        let labels: Vec<&str> = prefs.rows().iter().map(|row| row.label).collect();
        assert_eq!(
            labels,
            [
                "Graphic Detail",
                "Sound Effects",
                "Others",
                "Maps",
                "Port Options",
                "Quit",
                "Load",
                "Save",
                "Delete",
                "Return to Game",
                "New Game",
            ]
        );
    }

    #[test]
    fn the_originals_defaults_are_shape_detail_high_and_light_blue() {
        let prefs = Prefs::default();
        assert_eq!(SHAPE_DETAIL[prefs.shape_detail], "High");
        assert_eq!(SPACE_COLOURS[prefs.space_colour], "Light Blue");
        assert_eq!(
            prefs.others,
            [true, true, true, false, false, false, true, false, true, true]
        );
        assert_eq!(prefs.maps, [true; 5]);
        assert_eq!(prefs.sound, [true; 10]);
    }

    #[test]
    fn space_colours_are_the_captured_original_ramp() {
        // The captured default is palette entry 182, six-bit (3,3,19);
        // the rest scale that entry with the requested index.
        assert_eq!(space_colour_rgb(0), 0x00_0000);
        assert_eq!(space_colour_rgb(1), 0x04_0414);
        assert_eq!(space_colour_rgb(2), 0x08_0828);
        assert_eq!(space_colour_rgb(3), 0x08_0838);
        assert_eq!(space_colour_rgb(4), 0x0c_0c4c);
        assert_eq!(space_colour_rgb(99), 0x0c_0c4c);
    }

    #[test]
    fn escape_opens_and_closes_the_pages() {
        let mut prefs = Prefs::default();
        assert!(!prefs.open);
        assert_eq!(prefs.escape(), Activate::None);
        assert!(prefs.open);
        assert_eq!(prefs.page, MAIN);
        assert_eq!(prefs.escape(), Activate::Close);
        assert!(!prefs.open);
    }

    #[test]
    fn return_to_game_closes_and_quit_reports_quit() {
        let mut prefs = Prefs::default();
        prefs.escape();
        assert_eq!(prefs.apply(Action::Close), Activate::Close);
        assert!(!prefs.open);
        assert_eq!(prefs.apply(Action::Quit), Activate::Quit);
    }

    #[test]
    fn the_sound_rows_toggle_their_switches() {
        let mut prefs = Prefs::default();
        prefs.escape();
        prefs.apply(Action::Page(SOUND));
        assert!(prefs.rows().iter().all(|row| row.applied));
        assert_eq!(prefs.apply(Action::Sound(0)), Activate::None);
        assert!(!prefs.sound[0]);
        prefs.apply(Action::Sound(0));
        assert!(prefs.sound[0]);
        // An out-of-range row does nothing.
        prefs.apply(Action::Sound(99));
        assert_eq!(prefs.sound, [true; 10]);
    }
}
