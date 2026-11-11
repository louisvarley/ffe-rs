//! Which CD video a station screen shows (JJFFE 2.8a7, `ffe.asm`).
//!
//! Four screens set the current clip list (`DATA_002321`) and its length
//! (`DATA_002322`); F120 then plays `list[F922(length)]` from the CD's `DATA`
//! folder when videos are enabled (`DATA_009027`), and forgets the list:
//!
//! * F333, the services root, once per arrival (`DATA_005315`, set by F320
//!   and F1031): by the system's allegiance (`player+0x43c`), with the hull,
//!   the jurisdiction's fine, the starport tech level and the military
//!   ranking points choosing other lists, and a 3-in-20 chance of the
//!   general `g` list. A Thargoid system or station shows none.
//! * F382, the shipyard root: the shipyard staff by allegiance, a 3-in-20
//!   chance of the independent list, and a damage remark below 75% hull.
//! * F384, the upgrades screen: always the `isy1`.. list.
//! * F442, a ship bought: the `isy25`.. list.
//!
//! The lists (`ffedat.asm` `DATA_005258..5273`) carry more names than some
//! counts reach (`i16`..`i18`, for one); the counts are the routines'.
//! Every F922 draw comes from the shared stream in the original's order,
//! including F333's and F382's draws when no video is played.

use crate::npc::Random;

/// A clip list: the names F120 picks from, by the routine's count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct List {
    names: &'static [&'static str],
    count: u32,
}

impl List {
    const fn new(names: &'static [&'static str], count: u32) -> Self {
        List { names, count }
    }

    /// The names F120 can pick (`count` of them).
    pub fn names(&self) -> &'static [&'static str] {
        &self.names[..self.count as usize]
    }

    /// F120: `list[F922(count)]`.
    pub fn pick(&self, random: &mut Random) -> &'static str {
        self.names[random.bounded(self.count) as usize]
    }
}

const D5258: [&str; 6] = ["f25", "f26", "f27", "f28", "f29", "f30"];
const D5259: [&str; 21] = [
    "f31", "f32", "f33", "f34", "f35", "f36", "f37", "f38", "f39", "f40", "f41", "f42", "f43",
    "f44", "f45", "f46", "f47", "f48", "f49", "f50", "f51",
];
const D5260: [&str; 7] = ["f1", "f2", "f3", "f4", "f5", "f6", "f7"];
const D5261: [&str; 12] = [
    "i1", "i2", "i3", "i4", "i5", "i6", "i7", "i8", "i9", "i16", "i17", "i18",
];
const D5262: [&str; 27] = [
    "i10", "i11", "i12", "i13", "i14", "i15", "i19", "i20", "i21", "i22", "i23", "i24", "i25",
    "i26", "i27", "i28", "i29", "i30", "i31", "i32", "i33", "i34", "c9", "c10", "c11", "c12",
    "c13",
];
const D5263: [&str; 13] = [
    "c14", "c15", "c16", "c17", "c18", "c19", "c20", "c21", "c22", "c23", "c24", "g9", "g10",
];
const D5264: [&str; 15] = [
    "c25", "c26", "c27", "c28", "c29", "c30", "c31", "c32", "c39", "c40", "c43", "c44", "c45",
    "c46", "c47",
];
const D5265: [&str; 9] = ["c1", "c2", "c3", "c4", "c5", "c6", "c7", "c8", "fr1"];
const D5266: [&str; 24] = [
    "fr2", "fr3", "fr4", "fr5", "fr6", "fr7", "fr8", "fr9", "fr10", "fr11", "m1", "m2", "m3", "m4",
    "r1", "r2", "r3", "r4", "c33", "c34", "c35", "c36", "c37", "c38",
];
const D5267: [&str; 9] = ["g1", "g2", "g3", "g4", "g5", "g6", "g7", "g8", "g11"];
const D5268: [&str; 9] = [
    "sh1", "sh2", "sh3", "sh4", "sh5", "sh6", "sh7", "sh8", "isy11",
];
const D5269: [&str; 9] = [
    "fsh1", "fsh2", "fsh3", "fsh4", "fsh5", "fsh6", "fsh7", "fsh8", "fsh9",
];
const D5270: [&str; 9] = [
    "ish1", "ish2", "ish3", "ish4", "ish5", "ish6", "ish7", "ish8", "ish9",
];
const D5271: [&str; 18] = [
    "isy1", "isy2", "isy3", "isy4", "isy5", "isy6", "isy7", "isy8", "isy9", "isy10", "isy13",
    "isy14", "isy15", "isy16", "isy17", "isy18", "isy19", "isy20",
];
const D5272: [&str; 4] = ["isy21", "isy22", "isy23", "isy24"];
const D5273: [&str; 6] = ["isy25", "isy26", "isy27", "isy28", "isy29", "isy30"];

/// The allegiances F333 and F382 test (`player+0x43c`, F869's `>> 6`).
pub const INDEPENDENT: u16 = 0;
pub const IMPERIAL: u16 = 1;
pub const FEDERAL: u16 = 2;
pub const THARGOID: u16 = 4;
/// The Thargoid station model F333 and F564 refuse (`+0x82 == 0x45`).
pub const THARGOID_STATION: u32 = 0x45;

/// What the services root reads about the player and the place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Arrival {
    /// `player+0x43c`.
    pub allegiance: u16,
    /// The hull as F333 computes it: `hull * 100 / (mass * 4)`.
    pub hull_percent: i32,
    /// `player+0x50f8 + 4 * allegiance`: the fine with this jurisdiction.
    pub fine: u32,
    /// `player+0x43a`: the starport's tech level (`DATA_008995`).
    pub tech_level: u16,
    /// `player+0x1a0` and `+0x1a4`: the military ranking points.
    pub points: [u32; 2],
    /// The docked station's model (`player+0xcc` -> `+0x82`).
    pub station_model: u32,
}

/// F333's choice on arrival, or `None` where it shows no video.
pub fn services_root(arrival: &Arrival, random: &mut Random) -> Option<List> {
    let damaged = arrival.hull_percent < 0x4b;
    let mut list = match arrival.allegiance {
        INDEPENDENT => {
            let mut list = List::new(&D5265, 8);
            if damaged && random.bounded(2) != 0 {
                list = List::new(&D5263, 0xd);
            }
            if arrival.fine > 0x32 && random.bounded(2) != 0 {
                list = List::new(&D5264, 8);
            }
            if arrival.tech_level < 0xb4 && random.bounded(2) != 0 {
                list = List::new(&D5266, 0xa);
            }
            list
        }
        IMPERIAL => {
            let mut list = List::new(&D5261, 9);
            if arrival.points[0] >= 0x2710 && random.bounded(2) != 0 {
                list = List::new(&D5262, 6);
            }
            list
        }
        FEDERAL => {
            let mut list = List::new(&D5260, 7);
            if arrival.points[1] >= 0x2710 && random.bounded(2) != 0 {
                list = List::new(&D5259, 6);
            }
            if damaged && random.bounded(2) != 0 {
                list = List::new(&D5258, 4);
            }
            list
        }
        _ => List::new(&D5267, 9),
    };
    if random.bounded(0x14) < 3 {
        list = List::new(&D5267, 9);
    }
    (arrival.allegiance != THARGOID && arrival.station_model != THARGOID_STATION).then_some(list)
}

/// F382's choice when the shipyard opens, or `None` in a Thargoid system.
pub fn shipyard(allegiance: u16, hull_percent: i32, random: &mut Random) -> Option<List> {
    if allegiance == THARGOID {
        return None;
    }
    let mut list = match allegiance {
        IMPERIAL => List::new(&D5270, 9),
        FEDERAL => List::new(&D5269, 9),
        _ => List::new(&D5268, 9),
    };
    if random.bounded(0x14) < 3 {
        list = List::new(&D5268, 9);
    }
    if hull_percent < 0x4b && random.bounded(2) != 0 {
        list = List::new(&D5272, 4);
    }
    Some(list)
}

/// F384's list for the upgrades screen.
pub fn upgrades() -> List {
    List::new(&D5271, 0x12)
}

/// F442's list after a ship is bought.
pub fn ship_bought() -> List {
    List::new(&D5273, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arrival(allegiance: u16) -> Arrival {
        Arrival {
            allegiance,
            hull_percent: 100,
            fine: 0,
            tech_level: 0xff,
            points: [0, 0],
            station_model: 0x4a,
        }
    }

    /// The lists' first names and counts are the routines' (`ffedat.asm`).
    #[test]
    fn the_lists_and_counts_are_the_originals() {
        assert_eq!(upgrades().names().len(), 18);
        assert_eq!(upgrades().names()[10], "isy13");
        assert_eq!(ship_bought().names(), D5273);
        // F333's Imperial list stops at i9 (count 9 of 12 names).
        assert_eq!(List::new(&D5261, 9).names().last(), Some(&"i9"));
    }

    /// The draws follow F333's order: a quiet independent arrival draws only
    /// the 3-in-20 roll; each condition that holds adds one coin flip first.
    #[test]
    fn f333_draws_in_the_originals_order() {
        let mut quiet = Random::new(7, 9);
        let mut copy = quiet;
        services_root(&arrival(INDEPENDENT), &mut quiet).unwrap();
        copy.bounded(0x14);
        assert_eq!(quiet, copy, "one draw");
        let busy = Arrival {
            hull_percent: 50,
            fine: 100,
            tech_level: 10,
            ..arrival(INDEPENDENT)
        };
        let mut random = Random::new(7, 9);
        let mut copy = random;
        services_root(&busy, &mut random).unwrap();
        for limit in [2, 2, 2, 0x14] {
            copy.bounded(limit);
        }
        assert_eq!(random, copy, "three flips and the roll");
    }

    /// Every list F333 can choose is reachable and holds real clip names;
    /// Thargoid systems and stations show none (the draws still happen).
    #[test]
    fn f333_covers_every_allegiance() {
        let mut seen = std::collections::BTreeSet::new();
        for seed in 0..400u32 {
            let mut random = Random::new(seed, seed.wrapping_mul(0x9e37_79b9) | 1);
            for allegiance in [INDEPENDENT, IMPERIAL, FEDERAL, 3] {
                let busy = Arrival {
                    hull_percent: 10,
                    fine: 100,
                    tech_level: 10,
                    points: [0x2710, 0x2710],
                    ..arrival(allegiance)
                };
                let list = services_root(&busy, &mut random).unwrap();
                seen.insert(list.names()[0]);
            }
        }
        for first in [
            "c1", "c14", "c25", "fr2", "i1", "i10", "f1", "f31", "f25", "g1",
        ] {
            assert!(seen.contains(first), "{first} never chosen");
        }
        let mut random = Random::new(1, 2);
        assert_eq!(services_root(&arrival(THARGOID), &mut random), None);
        let thargoid_station = Arrival {
            station_model: THARGOID_STATION,
            ..arrival(FEDERAL)
        };
        assert_eq!(services_root(&thargoid_station, &mut random), None);
    }

    #[test]
    fn f382_chooses_the_shipyard_staff_by_allegiance() {
        let mut firsts = std::collections::BTreeSet::new();
        for seed in 0..200u32 {
            let mut random = Random::new(seed, seed ^ 0x55aa);
            for allegiance in [INDEPENDENT, IMPERIAL, FEDERAL] {
                let list = shipyard(allegiance, 50, &mut random).unwrap();
                firsts.insert(list.names()[0]);
            }
        }
        assert_eq!(
            firsts.into_iter().collect::<Vec<_>>(),
            ["fsh1", "ish1", "isy21", "sh1"]
        );
        assert_eq!(shipyard(THARGOID, 100, &mut Random::new(1, 2)), None);
    }
}
