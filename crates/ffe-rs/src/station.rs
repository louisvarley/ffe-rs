//! The original's docked station screens (the BBS module, F320-F362).
//!
//! The screens are the 40-byte entries of `DATA_005304` in `ffedat.asm`; the
//! strings are the `0x98xx` messages of `DATA_005299` (each `0x9800 + n`).
//! The layout below is taken from those tables and checked against a real
//! 320x200 frame of the DOS release (see `docs/notes/station.md`):
//!
//! | screen | index | header | entry x | pitch | buttons | back |
//! |---|---|---|---|---|---|---|
//! | services | 0 | - | 140 | 33 | one | no |
//! | comms | 1 | Suggested Responses | 140 | 10 | one | no |
//! | bulletin board | 2 | - | 140 | 33 | one | yes |
//! | stockmarket | 3 | Buy Sell Item ... | 152 | 10 | two | no |
//! | upgrades | 4 | Fit Remove Item ... | 152 | 10 | two | yes |
//! | ships | 5/6 | Buy View Ship ... | 152 | 10 | two | yes |
//! | repairs | 8 | Repair Item ... | 140 | 10 | one | yes |
//!
//! The row buttons are `F328` hot rectangles of 8x9 at x 124 and 136; the
//! up/down arrows are at (124, 147) and (136, 147), "More..." at (148, 148) and
//! "Go Back..." at (210, 147). The screen name is drawn at (4, 6), the column
//! header at y 2, entry text at the row's y + 1 and the second line at +10. The
//! cash/cargo/cabins line is at (1, 138) and (1, 148) (`F558`, message 0x98e8).

/// The docked screens. The variants are the original screen indices, except that
/// the services root and the shipyard share index 0 (F333 and F382).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    #[default]
    Services,
    Shipyard,
    Market,
    Upgrades,
    Repairs,
    Ships,
    Bulletin,
}

/// What activating a row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Launch,
    Open(Screen),
    Trade {
        buy: bool,
        row: usize,
    },
    Back,
    /// The service exists but its data or routine is not ported. The text is
    /// reported on the console, never drawn into the station screen.
    NotPorted(&'static str),
}

/// A row of a menu screen, in the original's order.
pub struct Entry {
    pub title: &'static str,
    pub detail: Option<&'static str>,
    pub action: Action,
}

/// The services root `F333` adds: Launch Request, then (unless the system's
/// allegiance is Thargoid, `w[D8996] == 4`) Shipyard, Bulletin Board and
/// Stockmarket. The port has no Thargoid systems, so the four rows always apply.
pub const SERVICES: [Entry; 4] = [
    Entry {
        title: "Launch Request",
        detail: Some("(Request launch permission from traffic control)"),
        action: Action::Launch,
    },
    Entry {
        title: "Shipyard",
        detail: Some("(Ships refitted, repaired, bought and sold)"),
        action: Action::Open(Screen::Shipyard),
    },
    Entry {
        title: "Bulletin Board",
        detail: Some("(Advertisements for goods and services)"),
        action: Action::Open(Screen::Bulletin),
    },
    Entry {
        title: "Stockmarket",
        detail: Some("(Direct trading on the local market)"),
        action: Action::Open(Screen::Market),
    },
];

/// The shipyard root `F382` (its title is the station's Shipyard, and it adds
/// these four entries to the services layout).
pub const SHIPYARD: [Entry; 4] = [
    Entry {
        title: "Upgrades",
        detail: Some("(Ship equipment fitted while you wait)"),
        action: Action::Open(Screen::Upgrades),
    },
    Entry {
        title: "Repairs and Servicing",
        detail: Some("(in our fully automated workshop)"),
        action: Action::Open(Screen::Repairs),
    },
    Entry {
        title: "New and Reconditioned Ships",
        detail: Some("(Ships part exchanged)"),
        action: Action::Open(Screen::Ships),
    },
    Entry {
        title: "Contact Local Police",
        detail: Some("(for paying fines, etc)"),
        action: Action::NotPorted("Contact Local Police (fines) is not ported."),
    },
];

/// The messages the screen's header/console uses. The DOS release prints the
/// currency as a font glyph the port's 116-glyph cache does not carry; `$` is
/// the closest ASCII stand-in and is called out in `docs/notes/station.md`.
pub const SERVICES_HINT: &str = "Which service do you require?";
pub const BULLETIN_EMPTY: &str = "Sorry, no advertisements have been placed.";
pub const MARKET_HINT: &str = "Choose Buy or Sell to trade one unit.";
pub const UPGRADES_HINT: &str = "Choose Fit or Remove for required item.";
pub const SHIPS_EMPTY: &str = "We have no ship hulls for sale, at the moment.";
pub const SHIPS_HINT: &str =
    "Select Buy to part exchange your ship for a ship with no extra equipment, or View to see the ship with suggested options fitted.";
pub const REPAIRS_HINT: &str = "Select desired repair or service. Remember servicing AT LEAST once a year is a wise precaution.";

/// The visible entry area ends at y 148 (`F327`: `height + y < 0x94`).
pub const LIST_BOTTOM: i32 = 148;
/// First entry's top row: the header height of 15 (`DATA_005306`).
pub const LIST_TOP: i32 = 15;

/// The original's per-screen geometry (the tables at `DATA_005304`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub pitch: i32,
    pub entry_x: i32,
    pub buttons: i32,
    pub back: bool,
}

impl Layout {
    /// The rows that fit between `LIST_TOP` and `LIST_BOTTOM`.
    pub fn visible(self) -> usize {
        let mut rows = 0;
        let mut y = LIST_TOP;
        while y + self.pitch < LIST_BOTTOM {
            rows += 1;
            y += self.pitch;
        }
        rows
    }
}

pub fn layout(screen: Screen) -> Layout {
    match screen {
        Screen::Services | Screen::Shipyard | Screen::Bulletin => Layout {
            pitch: 33,
            entry_x: 140,
            buttons: 1,
            back: screen == Screen::Bulletin,
        },
        Screen::Market => Layout {
            pitch: 10,
            entry_x: 152,
            buttons: 2,
            back: false,
        },
        Screen::Upgrades => Layout {
            pitch: 10,
            entry_x: 152,
            buttons: 2,
            back: true,
        },
        Screen::Ships => Layout {
            pitch: 10,
            entry_x: 152,
            buttons: 2,
            back: true,
        },
        Screen::Repairs => Layout {
            pitch: 10,
            entry_x: 140,
            buttons: 1,
            back: true,
        },
    }
}

/// The screen name: `0x98d2` services, `0x98d3` shipyard (also the title of the
/// shipyard's sub-screens), `0x98d5` bulletin board, `0x98d6` stockmarket.
pub fn title(screen: Screen, station: &str) -> String {
    let name = match screen {
        Screen::Services => "Services",
        Screen::Shipyard | Screen::Upgrades | Screen::Repairs | Screen::Ships => "Shipyard",
        Screen::Market => "Stockmarket",
        Screen::Bulletin => "Bulletin Board",
    };
    format!("{station} {name}")
}

/// The column header drawn at y 2 (`0x98e0`, `0x98f5`, `0x98f6`, `0x98f4`).
/// The x positions are the tab stops the original's `0xfd` glyph produces, read
/// from the captured frames.
pub fn header(screen: Screen) -> &'static [(i32, &'static str)] {
    match screen {
        Screen::Market => &[
            (120, "Buy Sell Item"),
            (213, "Price per t"),
            (255, "Stock"),
            (286, "Cargo"),
        ],
        Screen::Upgrades => &[
            (120, "Fit Remove"),
            (152, "Item"),
            (225, "$ to Fit"),
            (265, "$ for Removal"),
            (308, "Wt"),
        ],
        Screen::Ships => &[
            (120, "Buy View"),
            (142, "Ship"),
            (228, "Cost"),
            (258, "Pt.Exchg"),
            (306, "Cap."),
        ],
        Screen::Repairs => &[(120, "Repair"), (152, "Item"), (286, "Cost")],
        _ => &[],
    }
}

/// The market's column x positions for a row's price, stock and cargo.
pub const MARKET_PRICE_X: i32 = 214;
pub const MARKET_STOCK_X: i32 = 254;
pub const MARKET_CARGO_X: i32 = 290;

/// Position of the status line's labels and values (`F558`, message 0x98e8).
pub const STATUS_ROW_1: i32 = 138;
pub const STATUS_ROW_2: i32 = 148;
pub const STATUS_LABEL_X: i32 = 1;
pub const STATUS_VALUE_X: i32 = 44;
pub const STATUS_CASH_LABEL_X: i32 = 82;
pub const STATUS_CASH_RIGHT: i32 = 96;

/// The cash/cargo/cabins values of the docked screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Status {
    pub cargo_used: i32,
    pub cargo_capacity: i32,
    pub cash: i32,
    /// The original's `Cabins used : total`; the port has no passenger cabins
    /// yet, so both are zero as in a fresh commander's ship.
    pub cabins_used: i32,
    pub cabins_total: i32,
}

/// One drawn row: the entry's two lines (menus) or the item and its columns
/// (market). `columns` are absolute x positions and their colours.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DockRow {
    pub main: String,
    pub detail: Option<String>,
    pub columns: Vec<(i32, String, u8)>,
}

/// The rows and status the docked screen draws for one frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DockView {
    pub screen: Screen,
    pub title: String,
    pub rows: Vec<DockRow>,
    pub status: Status,
    pub scroll: usize,
    pub selected: usize,
    /// The number of entries in the whole list (not just this window).
    pub total: usize,
}

impl DockView {
    /// The down arrow is drawn while entries remain below the window; the up
    /// arrow while the window is scrolled.
    pub fn more(&self) -> bool {
        self.total > self.scroll + layout(self.screen).visible()
    }
}

/// The docked screen the player is on and the row the keyboard is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DockState {
    pub screen: Screen,
    pub selected: usize,
    pub scroll: usize,
}

impl DockState {
    pub fn open(&mut self, screen: Screen) {
        self.screen = screen;
        self.selected = 0;
        self.scroll = 0;
    }

    /// Go back one level, as the "Go Back..." arrow does. Screens without the
    /// arrow in the original (services, shipyard, market) return to the
    /// services root here; `Backspace` is the port's way out of them.
    pub fn back(&mut self) {
        self.open(match self.screen {
            Screen::Upgrades | Screen::Repairs | Screen::Ships => Screen::Shipyard,
            Screen::Services => Screen::Services,
            _ => Screen::Services,
        });
    }

    /// Move the keyboard row and keep it inside the visible window.
    pub fn step(&mut self, delta: i32, rows: usize) {
        if rows == 0 {
            self.selected = 0;
            self.scroll = 0;
            return;
        }
        let visible = layout(self.screen).visible();
        self.selected = (self.selected as i32 + delta).rem_euclid(rows as i32) as usize;
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + visible {
            self.scroll = self.selected + 1 - visible;
        }
        self.scroll = self.scroll.min(rows.saturating_sub(visible));
    }
}

/// What a hot area asks for. The rectangles are the original's `F328` hot
/// areas (8x9 at the button); clicks on the text do nothing in the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hot {
    Row(usize),
    /// Buy (left) or sell (right) the row.
    Market {
        buy: bool,
        row: usize,
    },
    More,
    Up,
    Down,
    Back,
}

/// A rectangle in logical pixels, inclusive edges (the mouse poll's test).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotArea {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
    pub hot: Hot,
}

impl HotArea {
    pub fn contains(self, x: i32, y: i32) -> bool {
        (self.x0..=self.x1).contains(&x) && (self.y0..=self.y1).contains(&y)
    }
}

/// The hot areas of `screen` for the rows starting at `scroll`. `rows` is the
/// total number of entries. The original adds them in row order, then the
/// arrows, then "Go Back"; a click on overlapping areas takes the last, as the
/// original's list does.
pub fn hot_areas(screen: Screen, scroll: usize, rows: usize) -> Vec<HotArea> {
    let layout = layout(screen);
    let mut areas = Vec::new();
    let last = (scroll + layout.visible()).min(rows);
    for row in scroll..last {
        let y = LIST_TOP + (row - scroll) as i32 * layout.pitch;
        let x0 = 124;
        let hot = if screen == Screen::Market {
            Hot::Market { buy: true, row }
        } else {
            Hot::Row(row)
        };
        areas.push(HotArea {
            x0,
            y0: y,
            x1: x0 + 7,
            y1: y + 8,
            hot,
        });
        if layout.buttons >= 2 {
            let hot = if screen == Screen::Market {
                Hot::Market { buy: false, row }
            } else {
                Hot::Row(row)
            };
            areas.push(HotArea {
                x0: 136,
                y0: y,
                x1: 143,
                y1: y + 8,
                hot,
            });
        }
    }
    if rows > scroll + layout.visible() {
        areas.push(HotArea {
            x0: 124,
            y0: 147,
            x1: 131,
            y1: 155,
            hot: Hot::More,
        });
    }
    if scroll > 0 {
        areas.push(HotArea {
            x0: 136,
            y0: 147,
            x1: 143,
            y1: 155,
            hot: Hot::Up,
        });
    }
    if layout.back {
        areas.push(HotArea {
            x0: 200,
            y0: 147,
            x1: 207,
            y1: 155,
            hot: Hot::Back,
        });
    }
    areas
}

/// The hot area under a point, last added first.
pub fn click(screen: Screen, scroll: usize, rows: usize, x: i32, y: i32) -> Option<Hot> {
    hot_areas(screen, scroll, rows)
        .into_iter()
        .rev()
        .find(|area| area.contains(x, y))
        .map(|area| area.hot)
}

/// The action of the row at `index`, for menus. Market rows are handled by the
/// caller (their goods are dynamic).
pub fn menu_entry(screen: Screen, index: usize, bulletin_rows: usize) -> Option<&'static Entry> {
    match screen {
        Screen::Services => SERVICES.get(index),
        Screen::Shipyard => SHIPYARD.get(index),
        Screen::Bulletin if index < bulletin_rows => None,
        _ => None,
    }
}

/// The number of rows a screen shows, given the dynamic BBS/market sizes.
pub fn row_count(screen: Screen, bulletin_rows: usize, market_rows: usize) -> usize {
    match screen {
        Screen::Services => SERVICES.len(),
        Screen::Shipyard => SHIPYARD.len(),
        Screen::Bulletin => bulletin_rows,
        Screen::Market => market_rows,
        // The equipment, ship and repair lists need data the port does not import.
        Screen::Upgrades | Screen::Repairs | Screen::Ships => 0,
    }
}

/// The title of the console line shown when a screen opens.
pub fn hint(screen: Screen, bulletin_rows: usize) -> &'static str {
    match screen {
        Screen::Services | Screen::Shipyard => SERVICES_HINT,
        Screen::Market => MARKET_HINT,
        Screen::Upgrades => UPGRADES_HINT,
        Screen::Ships => {
            if row_count(Screen::Ships, 0, 0) == 0 {
                SHIPS_EMPTY
            } else {
                SHIPS_HINT
            }
        }
        Screen::Repairs => REPAIRS_HINT,
        Screen::Bulletin => {
            if bulletin_rows == 0 {
                BULLETIN_EMPTY
            } else {
                "Please choose advertisement. Direct link will be made with advertiser."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screens_have_the_original_geometry() {
        for (screen, pitch, x, buttons, back) in [
            (Screen::Services, 33, 140, 1, false),
            (Screen::Shipyard, 33, 140, 1, false),
            (Screen::Bulletin, 33, 140, 1, true),
            (Screen::Market, 10, 152, 2, false),
            (Screen::Upgrades, 10, 152, 2, true),
            (Screen::Ships, 10, 152, 2, true),
            (Screen::Repairs, 10, 140, 1, true),
        ] {
            let layout = layout(screen);
            assert_eq!(layout.pitch, pitch, "{screen:?}");
            assert_eq!(layout.entry_x, x, "{screen:?}");
            assert_eq!(layout.buttons, buttons, "{screen:?}");
            assert_eq!(layout.back, back, "{screen:?}");
        }
        assert_eq!(layout(Screen::Services).visible(), 4);
        assert_eq!(layout(Screen::Market).visible(), 13);
    }

    #[test]
    fn titles_and_headers_are_the_original_strings() {
        assert_eq!(
            title(Screen::Services, "Sirocco Station"),
            "Sirocco Station Services"
        );
        assert_eq!(
            title(Screen::Shipyard, "Sirocco Station"),
            "Sirocco Station Shipyard"
        );
        assert_eq!(
            title(Screen::Market, "Sirocco Station"),
            "Sirocco Station Stockmarket"
        );
        assert_eq!(
            title(Screen::Bulletin, "Sirocco Station"),
            "Sirocco Station Bulletin Board"
        );
        assert_eq!(
            title(Screen::Upgrades, "Sirocco Station"),
            "Sirocco Station Shipyard"
        );
        assert_eq!(header(Screen::Market)[0], (120, "Buy Sell Item"));
        assert_eq!(header(Screen::Market)[1], (213, "Price per t"));
        assert_eq!(header(Screen::Repairs)[2], (286, "Cost"));
        assert_eq!(header(Screen::Services), &[]);
    }

    #[test]
    fn services_rows_are_the_original_four_in_order() {
        let titles: Vec<_> = SERVICES.iter().map(|entry| entry.title).collect();
        assert_eq!(
            titles,
            [
                "Launch Request",
                "Shipyard",
                "Bulletin Board",
                "Stockmarket"
            ]
        );
        assert_eq!(
            SERVICES[0].detail,
            Some("(Request launch permission from traffic control)")
        );
        assert_eq!(
            SERVICES[3].detail,
            Some("(Direct trading on the local market)")
        );
        let titles: Vec<_> = SHIPYARD.iter().map(|entry| entry.title).collect();
        assert_eq!(
            titles,
            [
                "Upgrades",
                "Repairs and Servicing",
                "New and Reconditioned Ships",
                "Contact Local Police"
            ]
        );
    }

    #[test]
    fn row_buttons_are_eight_by_nine_at_the_original_columns() {
        let areas = hot_areas(Screen::Services, 0, 4);
        assert_eq!(
            areas[0],
            HotArea {
                x0: 124,
                y0: 15,
                x1: 131,
                y1: 23,
                hot: Hot::Row(0)
            }
        );
        assert_eq!(areas[1].y0, 48);
        assert_eq!(areas[2].y0, 81);
        assert_eq!(areas[3].y0, 114);
        assert!(!areas.iter().any(|area| area.hot == Hot::Back));

        let areas = hot_areas(Screen::Market, 0, 33);
        assert_eq!(areas[0].hot, Hot::Market { buy: true, row: 0 });
        assert_eq!(
            areas[1],
            HotArea {
                x0: 136,
                y0: 15,
                x1: 143,
                y1: 23,
                hot: Hot::Market { buy: false, row: 0 }
            }
        );
        assert_eq!(areas[2].y0, 25);
        // 13 rows, the down arrow for the rest, no back arrow on the market.
        assert_eq!(
            areas
                .iter()
                .filter(|area| matches!(area.hot, Hot::Market { .. }))
                .count(),
            26
        );
        assert!(areas.iter().any(|area| area.hot == Hot::More));
        assert!(!areas.iter().any(|area| area.hot == Hot::Back));
    }

    #[test]
    fn back_arrows_and_scrolling_follow_the_screen_tables() {
        let areas = hot_areas(Screen::Upgrades, 0, 42);
        assert!(areas.iter().any(|area| area.hot == Hot::Back));
        assert!(areas.iter().any(|area| area.hot == Hot::More));
        assert!(!areas.iter().any(|area| area.hot == Hot::Up));
        let areas = hot_areas(Screen::Upgrades, 4, 42);
        assert!(areas.iter().any(|area| area.hot == Hot::Up));
        assert_eq!(click(Screen::Upgrades, 0, 42, 204, 150), Some(Hot::Back));
        assert_eq!(click(Screen::Upgrades, 0, 42, 127, 17), Some(Hot::Row(0)));
        assert_eq!(click(Screen::Upgrades, 0, 42, 139, 17), Some(Hot::Row(0)));
        assert_eq!(click(Screen::Upgrades, 0, 42, 150, 17), None);
    }

    #[test]
    fn selection_stays_inside_the_visible_window() {
        let mut dock = DockState::default();
        dock.open(Screen::Market);
        for _ in 0..20 {
            dock.step(1, 33);
        }
        assert_eq!(dock.selected, 20);
        assert_eq!(dock.scroll, 20 + 1 - 13);
        dock.step(-1, 33);
        assert_eq!(dock.selected, 19);
        dock.open(Screen::Shipyard);
        dock.step(1, 4);
        assert_eq!(dock.selected, 1);
        dock.back();
        assert_eq!(dock.screen, Screen::Services);
    }

    #[test]
    fn every_menu_row_has_an_action() {
        for screen in [Screen::Services, Screen::Shipyard] {
            for index in 0..row_count(screen, 0, 0) {
                assert!(menu_entry(screen, index, 0).is_some(), "{screen:?} {index}");
            }
        }
        assert!(menu_entry(Screen::Services, 4, 0).is_none());
        assert!(menu_entry(Screen::Market, 0, 0).is_none());
    }
}
