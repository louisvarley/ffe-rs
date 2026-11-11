//! F688's ship creation as the original draws it: F635/F658 (the model), F772/
//! F773/F774 (the outfit), F689-F691 and F698-F699 (the roles), F775/F776 (a
//! trader's destination) and F694-F697 (pirate groups), call for call.
//!
//! The executed original (an F922 trace, `FFE_RNG_TRACE`) enters F688 at the
//! default start in the state `a3e751f9 1df3aaed` and makes 165 more draws:
//! each ship is `RND(model list)`, three F1530 steps, F774's `RND(0x33)` and the
//! outfit's shield, pylon and gadget draws. `ffe_core::bbs` reproduces the 1,711
//! draws before it; this module reproduces F688's own, so the shared pair leaves
//! the new game in the original's state and the counts, roles, models and
//! destinations are the original's.
//!
//! Positions are not drawn here: F702's placement hashes the pair with its own
//! explicit state (F1507), so only its F1530 step is taken.

use crate::bbs::neighbour;
use crate::galaxy;
use crate::hyperspace;
use crate::npc::{Random, SHIP_TYPES};
use crate::sector;

/// The statistics words F773 reads for a hull.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hull {
    /// `+0x6`, the mass word (F903's).
    pub mass: i32,
    /// The object's `+0x116`: the cargo statistic less an integral drive's mass
    /// (F1533).
    pub cargo: i32,
    /// `+0x12`, the missile mounts (read as a signed word).
    pub missiles: i32,
    /// `+0x14`, the drive word: negative is an integral drive of the low byte's
    /// class.
    pub drive_word: i16,
}

/// What F773 leaves in the new ship.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outfit {
    /// `+0x118`: F773's type argument.
    pub ai_type: u8,
    pub model: u32,
    /// `+0xa0` after F703: the object's seed (its name, its shared pair word).
    pub seed: u32,
    /// `+0xd0`: the drive class.
    pub drive: u8,
    /// `+0x119`: fuel tonnes.
    pub fuel: u8,
    /// `+0xd2`: the front laser byte.
    pub laser: u8,
    /// `+0xe2`: the shield maximum.
    pub shield: u16,
    /// `+0xd6..+0xdf`: the ten pylon words, 1-based as the original indexes them.
    pub pylons: [u8; 11],
    /// `+0x11e`: the energy word (`DATA_006266[drive] << 24`).
    pub energy: u32,
    /// `+0xc8`: the general equipment dword.
    pub equipment: u32,
    /// `+0x116` after the outfit: the hold left.
    pub hold: u16,
}

/// `DATA_006260`: the missile code by the draw's top nibble.
const MISSILES: [u8; 16] = [
    192, 193, 193, 130, 130, 130, 130, 130, 130, 130, 130, 130, 131, 131, 131, 132,
];
/// `DATA_006261`: the drive masses F773 takes from the budget.
const DRIVE_MASS: [i32; 16] = [
    0, 4, 10, 25, 45, 80, 150, 250, 400, 600, 6, 12, 24, 36, 0, 0,
];
/// `DATA_006262`: the fuel tonnes a drive class carries.
const DRIVE_FUEL: [u32; 16] = [0, 1, 1, 5, 10, 20, 40, 60, 80, 80, 8, 0, 0, 0, 0, 0];
/// `DATA_006263`: the share of what is left that becomes extra fuel, by type.
const FUEL_SHARE: [u32; 18] = [
    65535, 3276, 65535, 3276, 65535, 3276, 65535, 3276, 65535, 3276, 32767, 19660, 65535, 0, 65535,
    19660, 39321, 65535,
];
/// `DATA_006264`: F774's offset by type.
const LASER_OFFSET: [u32; 18] = [
    178, 12, 178, 12, 178, 12, 178, 12, 178, 12, 102, 153, 153, 0, 178, 7, 127, 178,
];
/// `DATA_006265`: F774's laser bytes and tonnages.
const LASERS: [(u8, u32); 9] = [
    (0, 0),
    (136, 1),
    (145, 3),
    (160, 5),
    (169, 20),
    (179, 75),
    (188, 200),
    (198, 500),
    (207, 900),
];
/// `DATA_006266`: the energy byte by drive class.
const ENERGY: [u32; 15] = [
    0, 32, 32, 64, 96, 128, 160, 192, 224, 255, 32, 64, 96, 128, 0,
];

/// What the generators read besides the pair.
pub struct World<'a> {
    /// `DATA_008885`.
    pub code: u32,
    /// `DATA_008996`: the allegiance word `>> 6`.
    pub allegiance: u32,
    /// `DATA_008991`.
    pub population: u32,
    /// `DATA_008995`: the starport tech level (F773's type 0xe word).
    pub tech: u32,
    /// `DATA_008997`: the number of places.
    pub places: u32,
    /// The orbital stations among them, as indexes into the place list
    /// (F701's candidates: the places whose `+0x131` bit 4 F622 set).
    pub orbital: &'a [u32],
    pub galaxy: &'a galaxy::Tables<'a>,
    pub sectors: &'a sector::Tables<'a>,
    /// The statistics words of a model.
    pub hull: &'a dyn Fn(u32) -> Option<Hull>,
    /// System codes the reference's galaxy table has as empty records.
    pub empty: &'a [u32],
}

fn population_of(world: &World<'_>, code: u32) -> u32 {
    if world.empty.contains(&code) {
        0
    } else {
        world.galaxy.market(code).population
    }
}

/// F635 for F772's `ai_type`: the model, drawing `RND(list)` (rows 0 and 1 are
/// shifted by the allegiance, and a small population scales the trader list).
pub fn pick_model(ai_type: usize, world: &World<'_>, rng: &mut Random) -> u32 {
    let row = if ai_type <= 1 {
        ai_type + 2 * world.allegiance as usize
    } else {
        ai_type
    };
    let list = SHIP_TYPES.get(row).copied().unwrap_or(&[]);
    let mut count = list.len() as u32;
    if ai_type == 1 && world.population < 4 {
        count = (world.population * count) >> 2;
    }
    let pick = rng.bounded(count) as usize;
    list.get(pick).copied().unwrap_or(0)
}

/// F774: the laser a ship of `ai_type` fits from its remaining budget.
fn laser(ai_type: u8, budget: &mut i32, rng: &mut Random) -> u8 {
    let offset = LASER_OFFSET.get(usize::from(ai_type)).copied().unwrap_or(0);
    let draw = rng.bounded(0x33).wrapping_add(offset);
    let value = draw.wrapping_mul(*budget as u32) >> 8;
    let mut index = 1;
    while index < 9 && value >= LASERS[index].1 {
        index += 1;
    }
    index -= 1;
    *budget = budget.wrapping_sub(LASERS[index].1 as i32);
    LASERS[index].0
}

/// F773: build and outfit a ship of `model`.
pub fn outfit(ai_type: u8, model: u32, world: &World<'_>, rng: &mut Random) -> Option<Outfit> {
    let hull = (world.hull)(model)?;
    // F703: one F1530 step and the object's seed.
    rng.advance();
    let seed = rng.a;
    // The equipment word and the tier draw.
    rng.advance();
    let mut word = (rng.a & 0xffff) as u16;
    let mut seed_out = seed;
    if ai_type == 0x0e {
        seed_out = 0;
        word = (world.tech << 8) as u16;
    }
    if ai_type == 0x10 {
        word = 0xffff;
    }
    let mut budget = hull.cargo & 0xffff;
    let mut flags = 0u32;
    rng.advance();
    let mut tier = rng.a & 0xffff;
    if ai_type == 0x0c {
        tier = 0xffff;
    }
    if tier > 0xf000 && ai_type != 1 {
        budget -= 1;
        flags |= 0x100;
    }
    let mut drive = hull.drive_word as u8;
    if hull.drive_word >= 0 {
        let mut value = i32::from(hull.drive_word);
        if tier > 0xc000 {
            value >>= 8;
        }
        value &= 0xf;
        drive = value as u8;
        budget -= DRIVE_MASS[value as usize];
    }
    let class = usize::from(drive).min(14);
    let energy = ENERGY[class] << 24;
    let mut fuel = DRIVE_FUEL[usize::from(drive).min(15)];
    if fuel >= budget as u32 {
        fuel = budget as u32;
    }
    budget = budget.wrapping_sub(fuel as i32);
    let mut fuel_byte = fuel as u8;
    let front = laser(ai_type, &mut budget, rng);

    let mut share = budget as u32;
    if share > 0x12c {
        share = 0x12c;
    }
    if ai_type == 1 {
        share /= 4;
    }
    share /= 4;
    share = (u64::from(word) * u64::from(share) / 0xffff) as u32;
    let first = rng.bounded(share);
    let second = rng.bounded(first);
    let shield_tonnes = (second * 2) as i32;
    budget -= shield_tonnes;
    let shield = (shield_tonnes << 4) as u16;

    let mut pylons = [0u8; 11];
    if budget >= 2 {
        let roll = rng.bounded(u32::from(word));
        if roll >= 0xc000 {
            flags |= 0x80;
            budget -= 2;
        } else if roll >= 0x4000 {
            flags |= 0x08;
            budget -= 2;
        }
    }
    if budget > 0 {
        let mounts = hull.missiles.max(0) as u32;
        let mut count = rng.bounded(mounts + 1) as usize;
        if count >= 1 {
            let code = MISSILES[usize::from(word >> 12)];
            while budget > 0 && count != 0 {
                pylons[count] = code;
                budget -= 1;
                count -= 1;
            }
        }
    }
    if budget > 0 {
        if rng.bounded(u32::from(word)) >= 0x800 {
            budget -= 1;
            flags |= 0x04;
        }
        if rng.bounded(u32::from(word)) >= 0xfc00 && budget >= 4 && ai_type != 0x0e {
            budget -= 4;
            flags |= 0x400;
        }
    }
    flags |= 0x4000;
    let share = FUEL_SHARE.get(usize::from(ai_type)).copied().unwrap_or(0);
    let mut extra = share.wrapping_mul(budget as u32);
    if extra > 0xaf {
        extra = 0xaf;
    }
    budget = budget.wrapping_sub(extra as i32);
    fuel_byte = fuel_byte.wrapping_add(extra as u8);
    Some(Outfit {
        ai_type,
        model,
        seed: seed_out,
        drive,
        fuel: fuel_byte,
        laser: front,
        shield,
        pylons,
        energy,
        equipment: flags.rotate_left(16),
        hold: budget as u16,
    })
}

/// F772: pick a model of `ai_type` and outfit it.
pub fn create(ai_type: u8, world: &World<'_>, rng: &mut Random) -> Option<Outfit> {
    let model = pick_model(usize::from(ai_type), world, rng);
    outfit(ai_type, model, world, rng)
}

/// F700: a random place, or `None` when the system has none.
fn place(world: &World<'_>, rng: &mut Random) -> Option<u32> {
    if world.places == 0 {
        None
    } else {
        Some(rng.bounded(world.places))
    }
}

/// F776: a destination of population four or more that the ship's drive and
/// fuel can reach; the fuel is spent.
fn destination(world: &World<'_>, ship: &mut Outfit, rng: &mut Random, mode: u32) -> Option<u32> {
    let system = neighbour(world.sectors, rng, world.code, mode);
    if system == 0 || population_of(world, system) <= 3 {
        return None;
    }
    let from = world.sectors.system_position(world.code)?;
    let to = world.sectors.system_position(system)?;
    let distance = sector::distance(from, to);
    let hull = (world.hull)(ship.model)?;
    let drives: Vec<sector::Drive> = (0..16)
        .filter_map(|class| world.sectors.drive(class))
        .collect();
    let report = hyperspace::plan(
        &drives,
        hull.mass,
        i32::from(ship.drive as i8),
        distance,
        i32::from(ship.fuel),
        true,
        false,
    )
    .ok()?;
    if report.result < 0 {
        return None;
    }
    ship.fuel = ship.fuel.wrapping_sub(report.consumed as u8);
    Some(system)
}

/// F775: four tries, two jumps of one sector then two within the sector.
fn find_destination(world: &World<'_>, ship: &mut Outfit, rng: &mut Random) -> Option<u32> {
    [1, 1, 0, 0]
        .into_iter()
        .find_map(|mode| destination(world, ship, rng, mode))
}

/// The roles F688 creates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// F689: a trader flying to a destination system.
    Trader,
    /// F690: the same with type 0x11.
    Thargoid,
    /// F691: a ship parked in a station bay.
    Bay,
    /// F699: the auxiliary carrier at an orbital station.
    Aux,
    /// F694/F696: a pirate leader.
    PirateLeader,
    /// F695/F696: an escort of the group before it.
    PirateEscort,
}

/// One ship F688 made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spawned {
    pub role: Role,
    pub outfit: Outfit,
    /// The place (an index into the place list) for traders, bay ships and the
    /// aux carrier.
    pub place: Option<u32>,
    /// A trader's destination system.
    pub destination: Option<u32>,
    /// F697's bounty row for pirates (the group's offset into `DATA_006268`).
    pub bounty_group: Option<u8>,
}

/// F689 (type 1) or F690 (type 0x11).
pub fn trader(ai_type: u8, world: &World<'_>, rng: &mut Random) -> Option<Spawned> {
    let mut ship = create(ai_type, world, rng)?;
    // F702.
    rng.advance();
    let spot = place(world, rng)?;
    rng.advance();
    let destination = find_destination(world, &mut ship, rng)?;
    Some(Spawned {
        role: if ai_type == 1 {
            Role::Trader
        } else {
            Role::Thargoid
        },
        outfit: ship,
        place: Some(spot),
        destination: Some(destination),
        bounty_group: None,
    })
}

/// F691: a ship in a bay of a random place. `bay_free` is F650's allocation.
pub fn bay_ship(
    world: &World<'_>,
    rng: &mut Random,
    bay_free: &mut dyn FnMut(u32) -> bool,
) -> Option<Spawned> {
    let ship = create(1, world, rng)?;
    let spot = place(world, rng)?;
    if !bay_free(spot) {
        return None;
    }
    rng.advance();
    Some(Spawned {
        role: Role::Bay,
        outfit: ship,
        place: Some(spot),
        destination: None,
        bounty_group: None,
    })
}

/// F701: a random orbital station.
fn orbital(world: &World<'_>, rng: &mut Random) -> Option<u32> {
    if world.orbital.is_empty() {
        return None;
    }
    let pick = rng.bounded(world.orbital.len() as u32) as usize;
    world.orbital.get(pick).copied()
}

/// F699: the auxiliary carrier at a random orbital station.
pub fn aux(world: &World<'_>, rng: &mut Random) -> Option<Spawned> {
    let station = orbital(world, rng)?;
    let ship = create(0x0f, world, rng)?;
    rng.advance();
    Some(Spawned {
        role: Role::Aux,
        outfit: ship,
        place: Some(station),
        destination: None,
        bounty_group: None,
    })
}

/// F697: the bounty row (one F1530 step).
fn bounty(rng: &mut Random, offset: u8) -> u8 {
    rng.advance();
    ((rng.a & 0xf) as u8).wrapping_add(offset)
}

/// F698: `RND(4)` picks F694 (a lone pirate), F695 (a group of the leader's
/// model), F696 (a group of fresh models) or F693 (nothing).
pub fn pirates(world: &World<'_>, rng: &mut Random) -> Vec<Spawned> {
    let mut ships = Vec::new();
    match rng.bounded(4) {
        0 => {
            if let Some(leader) = create(0x0a, world, rng) {
                rng.advance();
                let group = bounty(rng, 4);
                ships.push(Spawned {
                    role: Role::PirateLeader,
                    outfit: leader,
                    place: None,
                    destination: None,
                    bounty_group: Some(group),
                });
            }
        }
        which @ (1 | 2) => {
            let fresh = which == 2;
            let leader_type = if fresh { 0x0a } else { 0x0b };
            let Some(leader) = create(leader_type, world, rng) else {
                return ships;
            };
            rng.advance();
            let group = bounty(rng, if fresh { 4 } else { 2 });
            let model = leader.model;
            ships.push(Spawned {
                role: Role::PirateLeader,
                outfit: leader,
                place: None,
                destination: None,
                bounty_group: Some(group),
            });
            for escort in 0..3 {
                let made = if fresh {
                    create(0x0b, world, rng)
                } else {
                    outfit(0x0b, model, world, rng)
                };
                let Some(escort_ship) = made else {
                    return ships;
                };
                let group = bounty(rng, 0);
                ships.push(Spawned {
                    role: Role::PirateEscort,
                    outfit: escort_ship,
                    place: None,
                    destination: None,
                    bounty_group: Some(group),
                });
                if escort < 2 {
                    rng.advance();
                    if rng.a & 0x4000 != 0 {
                        return ships;
                    }
                }
            }
        }
        _ => {}
    }
    ships
}

/// F688's counts for a system record (the draws that decide how many of each
/// F689-F699 call runs). `start` is F686's first-pass variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub traders: u32,
    pub thargoids: u32,
    pub bay: u32,
    pub aux: u32,
    pub pirates: u32,
}

/// F688 in full: the counts and every ship, in the original's order. `patrol`
/// is the record's `+6` byte, `trade` its `+8` byte, `limit` F35(0x16).
#[allow(clippy::too_many_arguments)]
pub fn run(
    world: &World<'_>,
    patrol: u32,
    trade: u32,
    limit: u32,
    start: bool,
    rng: &mut Random,
    bay_free: &mut dyn FnMut(u32) -> bool,
) -> (Counts, Vec<Spawned>) {
    let mut ships = Vec::new();
    let traders = rng.bounded(trade);
    for _ in 0..traders {
        ships.extend(trader(1, world, rng));
    }
    let thargoids = rng.bounded(limit);
    for _ in 0..thargoids {
        ships.extend(trader(0x11, world, rng));
    }
    let (mut bay, mut aux_count) = (0, 0);
    if !start {
        let mut none = false;
        if world.places != 0 {
            bay = rng.bounded(world.places * 4).min(0x19);
            none = bay == 0;
            for _ in 0..bay {
                ships.extend(bay_ship(world, rng, bay_free));
            }
        }
        if !none {
            aux_count = rng.bounded(trade);
            for _ in 0..aux_count {
                ships.extend(aux(world, rng));
            }
        }
    }
    let patrol_limit = if start { patrol } else { patrol << 2 };
    let pirate_count = rng.bounded(patrol_limit);
    for _ in 0..pirate_count {
        ships.extend(pirates(world, rng));
    }
    // F692: a stub, but its count is drawn.
    let _ = rng.bounded(patrol);
    (
        Counts {
            traders,
            thargoids,
            bay,
            aux: aux_count,
            pirates: pirate_count,
        },
        ships,
    )
}
