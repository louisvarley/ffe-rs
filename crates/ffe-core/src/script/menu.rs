//! The station menu engine as the scripts see it (F326-F360): the screen the
//! scripts open (F331/F332), the menu entries (`DATA_009048`, 45 records of
//! 48 bytes; F345 appends, F346 inserts at a random place), the header
//! (F347, `DATA_008688`), and the console lines F349/F352 post. Selecting an
//! entry (F360) writes its code and words to script globals 0-3, which is how
//! the scripts hear the player's answers.
//!
//! The game draws this state and reports selections; the scripts fill it.

use serde::{Deserialize, Serialize};

/// `DATA_009048`'s 45 entries (F345/F346 stop at 0x2d).
pub const ENTRIES: usize = 0x2d;

/// One 48-byte menu record (the fields the scripts and F350 use).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// `+0x00`: the message code.
    pub code: i32,
    /// `+0x04..+0x0c`: its words (globals 1-3 on selection).
    pub words: [i32; 3],
    /// `+0x18`: -1 for a console line in the log colour; otherwise the
    /// button that selected it (bit 0: the code's own text).
    pub flags: i32,
    /// `+0x1c`: the speaking ship's id (0: none).
    pub ship: i32,
    /// `+0x24` and `+0x2c`: two more words F350 expands.
    pub extra: [i32; 2],
}

impl Entry {
    /// F350's variables for the code: `[w1, w2, w3, +0x24, +0x2c]`.
    pub fn vars(&self) -> [i32; 5] {
        [
            self.words[0],
            self.words[1],
            self.words[2],
            self.extra[0],
            self.extra[1],
        ]
    }
}

/// A console line F350 builds from a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Line {
    pub entry: Entry,
    /// F349/F351's new entry (with its sound unless the flags are negative)
    /// rather than F352's replacement of the newest.
    pub new: bool,
}

/// What the scripts have set up.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Menu {
    /// `DATA_008640`: F331 put the station's comms screen up.
    pub active: bool,
    /// `DATA_009054`: the screen (F326's builder: 1 F523, 2 the bulletin
    /// board F453, 3 the market F538, 4 repairs F385, 5 F437, 6 F439, 7 the
    /// military F493, 8 F445, 11 F541; others F335).
    pub screen: i32,
    /// `DATA_009052`: the screen's title code.
    pub title: i32,
    /// `DATA_009056`, `9057`, `9061`.
    pub words: [i32; 3],
    /// `DATA_009049`: F332's time (tics).
    pub timer: i32,
    /// `DATA_008641` bit 0: F332 opened a timed screen.
    pub timed: bool,
    /// `DATA_008688`: the header record (F347); code 0 is none.
    pub header: Entry,
    /// `DATA_009048..9051`.
    pub entries: Vec<Entry>,
    /// Console lines waiting for the game.
    pub lines: Vec<Line>,
    /// F326 asked for the current screen to be rebuilt.
    pub rebuild: bool,
    /// The scripts selected an entry themselves (F360 from statements 25
    /// and 59); the game runs a native code's handler.
    pub selected: Vec<Entry>,
}

impl Menu {
    /// F341: no header, no entries.
    pub fn clear(&mut self) {
        self.header = Entry::default();
        self.entries.clear();
    }

    /// F345.
    pub fn append(&mut self, entry: Entry) {
        if self.entries.len() < ENTRIES {
            self.entries.push(entry);
        }
    }

    /// F346: put `entry` at index F921(count) seeded by the entry's code
    /// (`code << 16 | code` in both words), moving the entry there to the end.
    pub fn insert(&mut self, entry: Entry) {
        let count = self.entries.len();
        if count >= ENTRIES {
            return;
        }
        if count == 0 {
            self.entries.push(entry);
            return;
        }
        let seed = (entry.code as u32) << 16 | entry.code as u32;
        let at = (f921(count as u32, seed, seed) as usize).min(count - 1);
        let moved = self.entries[at];
        self.entries.push(moved);
        self.entries[at] = entry;
    }
}

/// F921 with its two seed words: a number below `range`.
fn f921(range: u32, mut a: u32, mut b: u32) -> u32 {
    a = a.wrapping_add(b);
    b = b.rotate_left(5);
    a = a.wrapping_add(b).rotate_left(16);
    (a & 0xffff).wrapping_mul(range) >> 16
}

/// F360's choice for `entry`: `None` for a code in the station's own range
/// (`0x9800..0x9874`, its handler table `DATA_005288`), else the global
/// writes in order: 1, 2, 3 the words, then 0 the code, then 0 back to 0.
pub fn selection(entry: &Entry) -> Option<[(usize, i32); 5]> {
    let native = entry.code.wrapping_sub(0x9800);
    if (0..0x74).contains(&native) {
        return None;
    }
    Some([
        (1, entry.words[0]),
        (2, entry.words[1]),
        (3, entry.words[2]),
        (0, entry.code),
        (0, 0),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f346_moves_the_entry_it_replaces_to_the_end() {
        let mut menu = Menu::default();
        for code in 1..=3 {
            menu.append(Entry {
                code,
                ..Entry::default()
            });
        }
        let entry = Entry {
            code: 9,
            ..Entry::default()
        };
        let seed = 9u32 << 16 | 9;
        let at = f921(3, seed, seed) as usize;
        menu.insert(entry);
        assert_eq!(menu.entries[at].code, 9);
        assert_eq!(menu.entries.len(), 4);
        assert_eq!(menu.entries[3].code, [1, 2, 3][at]);
    }

    #[test]
    fn f360_hands_script_codes_to_the_globals() {
        let station = Entry {
            code: 0x9819,
            ..Entry::default()
        };
        assert_eq!(selection(&station), None);
        let advert = Entry {
            code: 0xa800,
            words: [5, 6, 7],
            ..Entry::default()
        };
        let writes = selection(&advert).unwrap();
        assert_eq!(writes[0], (1, 5));
        assert_eq!(writes[3], (0, advert.code));
        assert_eq!(writes[4], (0, 0));
    }
}
