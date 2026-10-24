//! The hyperdrive's range, fuel and jump time (F899-F903), ported from `ffe.asm`.
//!
//! A drive of class `c` has a power (`sector::Drive`); a ship of mass `m` reaches `power / m`
//! in the distance units of `sector::distance`. A jump of distance `d` needs
//! `d * m / 0x7531 + 1` tonnes of fuel, less the drive's modifier, and takes
//! `((d << 16) - 1) / range * 7` time units. The player's fuel is hydrogen in the hold
//! (military drives, class 10 and up, burn military fuel and leave radioactive waste).

use crate::model::multiply;
use crate::sector::Drive;

/// Hydrogen fuel's place in the cargo array (`DATA_008901`), military fuel's and the
/// radioactive waste's (`DATA_008902`, `DATA_008903`).
pub const HYDROGEN: usize = 19;
pub const MILITARY_FUEL: usize = 20;
pub const RADIOACTIVES: usize = 29;

/// The first class that burns military fuel.
pub const MILITARY_CLASS: i32 = 10;

/// What F903 reports. The outputs the original leaves untouched are `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    /// The return value: the distance, or -1 when the jump is not possible.
    pub result: i32,
    /// The drive's name code (`*drvname`).
    pub name: u32,
    /// The ship's mass first, then the fuel the jump needs (`*fuelneeded`; -1 when the
    /// distance is out of range or zero).
    pub fuel_needed: i32,
    /// `*maxrangecur`: the fuel on board, then (when the jump can be made) the range it gives.
    pub range_now: i32,
    /// `*jumptime`.
    pub jump_time: Option<i32>,
    /// `*maxrange`: the drive's range for this ship.
    pub max_range: i32,
    /// The player's "hyperspace possible" flag after the call (`None`: unchanged).
    pub flag: Option<bool>,
    /// Fuel tonnes taken from the ship (a jump, not a test).
    pub consumed: i32,
}

/// F903 for a ship of `mass` (the statistics' word at `+6`, signed) and drive `class`, with
/// `available` tonnes of its fuel. `distance` of zero reports the drive alone (F900). `jump`
/// removes the fuel (F901). `is_player` selects the flag the original keeps for the player.
pub fn plan(
    drives: &[Drive],
    mass: i32,
    class: i32,
    distance: i32,
    available: i32,
    jump: bool,
    is_player: bool,
) -> Result<Report, String> {
    let drive = usize::try_from(class)
        .ok()
        .and_then(|c| drives.get(c))
        .ok_or("no such hyperdrive class")?;
    if mass == 0 {
        return Err("a ship with no mass".to_owned());
    }
    let max_range = drive.power.wrapping_div(mass);
    let flag_off = (is_player && distance != 0).then_some(false);
    if max_range < distance || distance == 0 {
        return Ok(Report {
            result: -1,
            name: drive.name,
            fuel_needed: -1,
            range_now: 0,
            jump_time: None,
            max_range,
            flag: flag_off,
            consumed: 0,
        });
    }
    let modifier = drive.fuel_modifier.wrapping_shl(15);
    let mut fuel = distance
        .wrapping_mul(mass)
        .wrapping_div(0x7531)
        .wrapping_add(1);
    fuel = fuel.wrapping_sub(multiply(fuel, modifier));
    if fuel < 1 {
        fuel = 1;
    }
    let time = ((((distance as u32) << 16).wrapping_sub(1)) / (max_range as u32)).wrapping_mul(7);
    if fuel > available {
        return Ok(Report {
            result: -1,
            name: drive.name,
            fuel_needed: fuel,
            range_now: 0,
            jump_time: Some(time as i32),
            max_range,
            flag: is_player.then_some(false),
            consumed: 0,
        });
    }
    let mut left = available;
    let mut consumed = 0;
    let mut flag = is_player.then_some(true);
    if jump {
        left -= fuel;
        consumed = fuel;
        if is_player {
            flag = Some(false);
        }
    }
    let mut effective = mass.wrapping_sub(multiply(mass, modifier));
    if effective < 1 {
        effective = 1;
    }
    let mut range_now = (0x7531 / effective).wrapping_mul(left);
    if max_range < range_now {
        range_now = max_range;
    }
    Ok(Report {
        result: distance,
        name: drive.name,
        fuel_needed: fuel,
        range_now,
        jump_time: Some(time as i32),
        max_range,
        flag,
        consumed,
    })
}
