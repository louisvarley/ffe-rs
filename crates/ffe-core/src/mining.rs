//! The MB4 mining machine: deployment (`FUNC_000246`), the daily dig
//! (`FUNC_000243`'s event 0x10), loading (`FUNC_000247`) and the F317 list.
//!
//! The source keeps up to ten 0x34-byte records at `DATA_009106`
//! (`DATA_008804+0x5122`):
//!
//! ```text
//! +0x00  system code where deployed; 0 = an MB4 in the hold, -1 = the
//!        Thargoid Vaccine machine (code 0x12) in the hold (FUNC_000248)
//! +0x04  hopper: tonnes dug and waiting (at most 10)
//! +0x05  reserve: tonnes left in the ground (0xff for water)
//! +0x06  the machine object's position (24 bytes)
//! +0x1e  the machine object's index in the current system
//! +0x1f  the find (0 unsuitable .. 8 precious metals)
//! +0x20  the planet's name (20 bytes)
//! ```
//!
//! F246 runs when the player presses `d` landed (`DATA_008870 == 0x34`) with
//! a machine in the hold: it drops the machine (model 0x6e, or 0x6f for the
//! Vaccine machine) beside the ship, frees its 30 t, assesses the surface
//! ([`assess`]) and writes the 0xa20c/0xa211 messages. Every game day F243
//! moves one tonne from the reserve to the hopper while the hopper holds fewer
//! than ten. F247 runs when `d` is pressed landed beside a machine: it loads
//! the hopper into the cargo bay (0xa20f, or 0xa210 when the bay fills), or,
//! with the hopper empty and 30 t free, takes the machine back aboard
//! (0xa20d), else 0x98e3.

use serde::{Deserialize, Serialize};

/// F246/F247's machine mass: `add [DATA_008804+0x118],0x1e` and the
/// `cmp eax,0x1e` free-space test for taking a machine back.
pub const MACHINE_TONNES: u16 = 0x1e;
/// F243's hopper limit (`cmp byte [eax-0x1],0xa`).
pub const HOPPER: u8 = 10;
/// The MB4's model (`0x6e`) and the Vaccine machine's (`0x6f`).
pub const MB4_MODEL: u32 = 0x6e;
pub const VACCINE_MODEL: u32 = 0x6f;
/// The Vaccine machine's equipment code (`FUNC_000248`'s `cmp ...,0x12`).
pub const VACCINE_CODE: u8 = 0x12;
/// F246's temperature limits on the planet's `+0xe0` word (kelvin).
pub const TOO_HOT: i16 = 0x1d9;
pub const TOO_COLD: i16 = 0x32;
/// Below this F246 may find water ice instead of rock (`cmp ...,0x107`).
pub const ICE_BELOW: i16 = 0x107;

/// `DATA_005053`'s strings 0xa200..0xa21d, in order.
const STRINGS: [&str; 30] = [
    "",
    "In Cargo Bay",
    "Unsuitable",
    "Mined Out",
    "Water Rich",
    "Low in Minerals",
    "Some Minerals",
    "Mineral Rich",
    "Metal Seam",
    "Metal Rich",
    "Precious Metals",
    "Target Set to Mining Machine",
    "Deploying MB4 Mining Machine",
    "Mining Machine Hopper Empty -Loading",
    "MB4 Mining Machine Loaded",
    "Loading %dt of %s",
    "Loading %dt of %s. Hopper not empty - Cargo Bay Full.",
    "MB4 Mining Assessment: Surface (at %d^C) is %s.",
    "too hot for mining with an MB4",
    "too cold for mining with an MB4",
    "high in sulphur and unstable. Drilling would damage the MB4",
    "mainly water ice, suitable for mining water. Starting mining",
    "high in easily obtained liquid water. Starting mining",
    "mined out. No substances worth mining detected",
    "very low in Minerals. Starting mining",
    "rocky but contains some minerals. Starting mining",
    "mineral rich. Starting mining",
    "rocky but there is a small metal seam. Starting mining",
    "Metal Rich. Starting mining",
    "rocky but contains some precious metals. Starting mining",
];

/// The text of mining string `code` (0xa200..0xa21d).
pub fn string(code: u16) -> &'static str {
    code.checked_sub(0xa200)
        .and_then(|index| STRINGS.get(usize::from(index)))
        .copied()
        .unwrap_or("")
}

/// `DATA_005055/56/57`: F246's yield table, `(find, message, reserve)` per
/// index 0..=26.
const YIELDS: [(u8, u16, u8); 27] = [
    (1, 0xa217, 0),
    (3, 0xa218, 3),
    (3, 0xa218, 5),
    (3, 0xa218, 7),
    (4, 0xa219, 0xa),
    (4, 0xa219, 0xc),
    (4, 0xa219, 0x11),
    (5, 0xa21a, 0x19),
    (5, 0xa21a, 0x1e),
    (5, 0xa21a, 0x25),
    (6, 0xa21b, 4),
    (6, 0xa21b, 5),
    (6, 0xa21b, 6),
    (6, 0xa21b, 7),
    (7, 0xa21c, 0xf),
    (7, 0xa21c, 0x17),
    (7, 0xa21c, 0x23),
    (8, 0xa21d, 1),
    (8, 0xa21d, 1),
    (8, 0xa21d, 2),
    (8, 0xa21d, 2),
    (8, 0xa21d, 2),
    (8, 0xa21d, 3),
    (8, 0xa21d, 4),
    (8, 0xa21d, 5),
    (8, 0xa21d, 6),
    (8, 0xa21d, 7),
];

/// `DATA_005058`: the commodity each find digs (market index).
pub const COMMODITY: [usize; 9] = [0, 0, 0, 0x12, 0x12, 0x12, 0xf, 0xf, 0x10];

/// What F246 found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Assessment {
    /// `+0x1f`: 0 unsuitable, 1 mined out, 2 water, 3..8 minerals to precious
    /// metals.
    pub find: u8,
    /// The 0xa212..0xa21d description.
    pub message: u16,
    /// `+0x05`: tonnes in the ground.
    pub reserve: u8,
}

/// F246's survey of the planet the machine stands on. `temperature` is the
/// planet's `+0xe0` word (kelvin), `model` its `+0x82`, `size` its `+0xa6`
/// word and `tech` the system record's starport tech level (F869's `+0x1b5`
/// byte, `Details::tech_level`). `random(n)` is F922 (`0..n`) and `coin` one
/// F1530 bit.
pub fn assess(
    temperature: i16,
    model: u32,
    size: i16,
    tech: u32,
    coin: bool,
    random: impl FnOnce(u32) -> u32,
) -> Assessment {
    let none = |message| Assessment {
        find: 0,
        message,
        reserve: 0,
    };
    if temperature > TOO_HOT {
        return none(0xa212);
    }
    if temperature < TOO_COLD {
        return none(0xa213);
    }
    match model {
        0x7a => return none(0xa214),
        0x7f => {
            return Assessment {
                find: 2,
                message: 0xa215,
                reserve: 0xff,
            }
        }
        0x7d | 0x7e | 0x80 | 0x81 => {
            return Assessment {
                find: 0,
                message: 0xa216,
                reserve: 0xff,
            }
        }
        _ => {}
    }
    if temperature < ICE_BELOW && coin {
        return Assessment {
            find: 2,
            message: 0xa215,
            reserve: 0,
        };
    }
    // An unexplored system digs best; a developed one is mostly worked out.
    let richness: u32 = match tech {
        0 => 10,
        1..=2 => 9,
        3..=13 => 8,
        14..=26 => 6,
        _ => 4,
    };
    let scale = if size >= 0x1d { 3 } else { 2 };
    let roll = random(richness * scale);
    let index = ((roll * roll) >> 5).min(26) as usize;
    let (find, message, reserve) = YIELDS[index];
    Assessment {
        find,
        message,
        reserve,
    }
}

/// One deployed machine (a `DATA_009106` record with a system code).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    /// The equipment code (an MB4, or [`VACCINE_CODE`]).
    pub code: u8,
    /// `+0x00`: the system it stands in.
    pub system: u32,
    pub system_name: String,
    /// `+0x20`: the planet's name.
    pub planet: String,
    /// The position in the planet's own frame (millimetres), so it stays put
    /// as the planet turns and moves.
    pub offset: [i64; 3],
    /// `+0x04` and `+0x05`.
    pub hopper: u8,
    pub reserve: u8,
    /// `+0x1f`.
    pub find: u8,
    /// The last game day F243 has dug for.
    pub day: u32,
}

impl Machine {
    /// The model F243/F246 create for it.
    pub fn model(&self) -> u32 {
        if self.code == VACCINE_CODE {
            VACCINE_MODEL
        } else {
            MB4_MODEL
        }
    }

    /// F243's event 0x10, once per game day since the last: a tonne from the
    /// reserve into the hopper while the hopper is not full.
    pub fn dig(&mut self, day: u32) {
        while self.day < day {
            self.day += 1;
            if self.reserve > 0 && self.hopper < HOPPER {
                self.reserve -= 1;
                self.hopper += 1;
            }
        }
    }

    /// The F317 status label (0xa202 + find).
    pub fn label(&self) -> &'static str {
        string(0xa202 + u16::from(self.find))
    }

    /// The commodity it digs.
    pub fn commodity(&self) -> usize {
        COMMODITY[usize::from(self.find.min(8))]
    }
}

/// What F247 did when `d` was pressed beside a machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Load {
    /// The hopper's tonnes went into the bay (`full`: the bay filled first,
    /// 0xa210).
    Cargo {
        commodity: usize,
        tonnes: u16,
        full: bool,
    },
    /// The empty machine came back aboard (0xa20d).
    Machine,
    /// The hopper was empty and there was no room for the machine (0x98e3).
    NoRoom,
}

/// F247: load from `machine` with `free` tonnes in the bay.
pub fn load(machine: &mut Machine, free: u16) -> Load {
    if machine.hopper > 0 {
        let tonnes = u16::from(machine.hopper).min(free);
        machine.hopper -= tonnes as u8;
        return Load::Cargo {
            commodity: machine.commodity(),
            tonnes,
            full: tonnes < u16::from(machine.hopper) + tonnes,
        };
    }
    if free >= MACHINE_TONNES {
        Load::Machine
    } else {
        Load::NoRoom
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_follow_data_005053() {
        assert_eq!(string(0xa201), "In Cargo Bay");
        assert_eq!(string(0xa20c), "Deploying MB4 Mining Machine");
        assert_eq!(
            string(0xa21d),
            "rocky but contains some precious metals. Starting mining"
        );
        assert_eq!(string(0xa21e), "");
    }

    #[test]
    fn hot_cold_sulphur_and_water_worlds() {
        let never = |_| panic!("no roll");
        assert_eq!(assess(0x1da, 0x70, 0, 0, false, never).message, 0xa212);
        assert_eq!(assess(0x31, 0x70, 0, 0, false, never).message, 0xa213);
        assert_eq!(assess(0x120, 0x7a, 0, 0, false, never).message, 0xa214);
        let ice = assess(0x120, 0x7f, 0, 0, false, never);
        assert_eq!((ice.find, ice.message, ice.reserve), (2, 0xa215, 0xff));
        assert_eq!(assess(0x120, 0x80, 0, 0, false, never).message, 0xa216);
        assert_eq!(assess(0x100, 0x70, 0, 0, true, never).message, 0xa215);
    }

    #[test]
    fn rock_rolls_the_yield_table_by_tech_and_size() {
        // Tech 0, a large world: RND(30); 29*29>>5 = 26, the last row.
        let rich = assess(0x120, 0x70, 0x1d, 0, false, |limit| {
            assert_eq!(limit, 30);
            29
        });
        assert_eq!((rich.find, rich.message, rich.reserve), (8, 0xa21d, 7));
        // A developed system, a small world: RND(8); 0 is mined out.
        let poor = assess(0x120, 0x70, 0, 40, false, |limit| {
            assert_eq!(limit, 8);
            0
        });
        assert_eq!((poor.find, poor.message, poor.reserve), (1, 0xa217, 0));
    }

    fn machine(hopper: u8, reserve: u8, find: u8) -> Machine {
        Machine {
            code: 0x11,
            system: 1,
            system_name: String::new(),
            planet: String::new(),
            offset: [0; 3],
            hopper,
            reserve,
            find,
            day: 0,
        }
    }

    #[test]
    fn the_hopper_fills_a_tonne_a_day_to_ten() {
        let mut m = machine(0, 12, 5);
        m.dig(3);
        assert_eq!((m.hopper, m.reserve), (3, 9));
        m.dig(20);
        assert_eq!((m.hopper, m.reserve), (10, 2));
        assert_eq!(m.commodity(), 0x12);
        assert_eq!(m.label(), "Mineral Rich");
    }

    #[test]
    fn loading_empties_the_hopper_then_takes_the_machine() {
        let mut m = machine(7, 0, 8);
        assert_eq!(
            load(&mut m, 5),
            Load::Cargo {
                commodity: 0x10,
                tonnes: 5,
                full: true
            }
        );
        assert_eq!(
            load(&mut m, 50),
            Load::Cargo {
                commodity: 0x10,
                tonnes: 2,
                full: false
            }
        );
        assert_eq!(load(&mut m, 29), Load::NoRoom);
        assert_eq!(load(&mut m, 30), Load::Machine);
    }
}
