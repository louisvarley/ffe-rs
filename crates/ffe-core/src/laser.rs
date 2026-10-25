//! Lasers as the original fires them: F931's gating, F930's heat and energy,
//! F953's damage, and F1100's target-range conversion.
//!
//! The original's lasers are hitscan, not moving objects: F931 takes the current
//! mount's laser byte (`ship+0xd2..+0xd5`), returns unless the refire delay
//! (`+0xe8`) has run out, the heat (`+0xea`) is above 0xf000 or the energy
//! (`+0x11e`) is empty, then tests every live object along the ray with F802/
//! F817 and applies F953's damage immediately. The visible beam is model 0x9c
//! (156) drawn at the gun by F1776 on the same frame. See `docs/notes/combat.md`.
//!
//! The values here are the original's tables (`DATA_007208`, `DATA_007209`) and
//! its word arithmetic. The cosmetic sound sample numbers are kept for the
//! caller, which does not yet play them.
use crate::ffp::{self, Ffp};
use crate::model_animation::{shift_vector, vector_msb};
use crate::model_math;

/// The original's laser equipment codes, in the order of `DATA_006265` (the
/// laser type/tonnage table): 1MW pulse, 5MW pulse, 1MW beam, 4MW beam, 20MW
/// beam, 100MW beam, 30MW mining laser and the Thargoid laser.
pub const PULSE_1MW: u8 = 0x88;
pub const PULSE_5MW: u8 = 0x91;
pub const BEAM_1MW: u8 = 0xa0;
pub const BEAM_4MW: u8 = 0xa9;
pub const BEAM_20MW: u8 = 0xb3;
pub const BEAM_100MW: u8 = 0xbc;
pub const MINING_30MW: u8 = 0xc6;
pub const THARGOID: u8 = 0xcf;

/// The object fields holding the front, rear, top and bottom lasers
/// (`ship+0xd2..+0xd5`), indexed like [`Combat::lasers`].
pub const MOUNT_OFFSETS: [usize; 4] = [0xd2, 0xd3, 0xd4, 0xd5];

/// F931: above this heat (`+0xea`) a laser does not fire.
pub const HEAT_LIMIT: u16 = 0xf000;
/// F930's pulse refire delay (`+0xe8`) for the 1MW and 5MW pulse lasers.
pub const PULSE_REFIRE: u16 = 0x795c;
/// F930's delay for the (unused) third pulse entry.
pub const THIRD_PULSE_REFIRE: u16 = 0xffff;

/// F930's energy cost per heat-unit, `DATA_007209` (11 dwords).
pub const HEAT_RATE: [u32; 11] = [
    0x0a, 0x32, 0xfa, 0x5dc, 0x1, 0x4, 0x14, 0x64, 0x12c, 0x258, 0x384,
];
/// F930's beam heat divisor, `DATA_007208` (8 dwords).
pub const BEAM_SHIFT: [u32; 8] = [0, 0, 0, 0, 2, 3, 7, 8];
/// F270's drive-to-energy scale, `DATA_005089` (16 dwords).
pub const DRIVE_ENERGY: [u32; 16] = [
    0, 0x7ff, 0x7ff, 0x3ff, 0x2aa, 0x1ff, 0x199, 0x155, 0x124, 0xff, 0x7ff, 0x3ff, 0x2aa, 0x1ff,
    0x7ff, 0, // the table has 14 used entries in ffedat; the last two are padding
];

/// What one laser firing costs and does, from F930.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaserSpec {
    /// The equipment code (`0x88`, `0xa0`, ...).
    pub code: u8,
    /// The `(code & 0x78) >> 3` index into the original's tables.
    pub index: u32,
    /// F930's return value: the damage F953 receives.
    pub damage: u32,
    /// Heat added to `+0xea`.
    pub heat: u16,
    /// Energy removed from `+0x11e` by F929.
    pub energy: u32,
    /// The refire delay written to `+0xe8` (0 for beams: F930 does not set it).
    pub refire: u16,
    /// True for the beam branch (index 4 and up).
    pub beam: bool,
    /// `SoundPlaySample` id: 0xa pulse, 0xb beam.
    pub sound: u8,
}

/// The laser equipment code's table index, or `None` for "no laser".
pub fn index(code: u8) -> Option<u32> {
    let index = u32::from(code & 0x78) >> 3;
    (code != 0).then_some(index)
}

/// F930's beam shift for an index. The original reads `DATA_007208[index]`
/// without a bound check; beyond the eight dwords it continues into
/// `DATA_007209`, so indices 8 and 9 use 0xa and 0x32. That is reproduced here
/// rather than clamped, because the mining and Thargoid lasers take that path.
fn beam_shift(index: u32) -> u32 {
    BEAM_SHIFT
        .get(index as usize)
        .copied()
        .unwrap_or_else(|| HEAT_RATE[index as usize - 8])
}

/// F930 for one firing at `frame_tics` (`DATA_009155`, the accelerated frame
/// time). The pulse branch uses fixed costs; the beam branch scales by
/// `tics >> 7`, as the original does.
pub fn spec(code: u8, frame_tics: u32) -> Option<LaserSpec> {
    let index = index(code)?;
    let rate = *HEAT_RATE.get(index as usize)?;
    if index < 4 {
        let (heat, energy, refire) = match index {
            3 => (
                rate.wrapping_mul(8),
                rate.wrapping_mul(0x10),
                THIRD_PULSE_REFIRE,
            ),
            _ => (rate.wrapping_mul(8), rate.wrapping_mul(0x10), PULSE_REFIRE),
        };
        Some(LaserSpec {
            code,
            index,
            damage: rate,
            heat: heat as u16,
            energy,
            refire,
            beam: false,
            sound: 0x0a,
        })
    } else {
        let scaled = ((frame_tics >> 7) & 0xffff)
            .saturating_mul(rate)
            .min(0xffff);
        let shift = beam_shift(index);
        Some(LaserSpec {
            code,
            index,
            damage: scaled,
            heat: ((scaled * 8) >> shift) as u16,
            energy: scaled << 4,
            refire: 0,
            beam: true,
            sound: 0x0b,
        })
    }
}

/// The mutable laser words of a ship object (`+0xe8` refire, `+0xea` heat,
/// `+0x11e` energy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct State {
    pub refire: u16,
    pub heat: u16,
    pub energy: u32,
}

/// Why F931 returned without firing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireError {
    /// The mount's laser byte is zero.
    NoLaser,
    /// `+0xe8` is still counting down.
    Refire,
    /// `+0xea` is above [`HEAT_LIMIT`].
    Overheated,
    /// `+0x11e` is empty.
    NoEnergy,
}

/// F931's gates and F930's costs for one firing. On success the state has the
/// heat added, the energy removed and the refire delay set.
pub fn fire(code: u8, state: &mut State, frame_tics: u32) -> Result<LaserSpec, FireError> {
    let spec = spec(code, frame_tics).ok_or(FireError::NoLaser)?;
    if state.refire != 0 {
        return Err(FireError::Refire);
    }
    if state.heat > HEAT_LIMIT {
        return Err(FireError::Overheated);
    }
    if state.energy == 0 {
        return Err(FireError::NoEnergy);
    }
    state.heat = state.heat.wrapping_add(spec.heat);
    state.energy = state.energy.saturating_sub(spec.energy);
    state.refire = spec.refire;
    Ok(spec)
}

/// F730's refire countdown for one object update. The original subtracts the
/// low word of `DATA_009155`; JJFFE changed the high-word guard (`jng` to
/// `jnz`) so sub-tic frames still count down. This follows JJFFE, the repo's
/// gameplay reference, and the difference is recorded in `docs/notes/combat.md`.
pub fn tick_refire(state: &mut State, frame_tics: u32) {
    if state.refire == 0 {
        return;
    }
    if frame_tics >> 16 != 0 {
        state.refire = 0;
    } else {
        state.refire = state.refire.saturating_sub(frame_tics as u16);
    }
}

/// F938's laser cooling: `+0xea -= seconds * 0x400`, or `* 0x1000` with the
/// laser cooling booster (`ship+0xc8 & 0x1`). The original applies whole
/// seconds; the fractional part is kept here so a 60 Hz frame still cools.
pub fn cool(state: &mut State, seconds: f64, cooling_booster: bool) {
    let scale = if cooling_booster { 4096.0 } else { 1024.0 };
    let cool = (seconds.max(0.0) * scale) as u32;
    state.heat = state
        .heat
        .saturating_sub(cool.min(u32::from(u16::MAX)) as u16);
}

/// F270's energy-bar height (0..=11) before the clip: `value * 12 >> 16`.
pub fn gauge_height(value: u32) -> i32 {
    ((u64::from(value) * 12) >> 16) as i32
}

/// F270's first gauge value from the drive byte and `+0x11e`.
pub fn energy_gauge(energy: u32, drive: u8) -> u32 {
    let scale = *DRIVE_ENERGY.get(usize::from(drive)).unwrap_or(&0);
    scale.wrapping_mul(energy >> 16) >> 8 & 0xffff
}

/// F1100's target range as the original keeps it: the F590 relative vector cut
/// to 13 bits, F1466's length, F1489's integer-to-ffp, then the cut's shift
/// added to the exponent and F1496's normalisation. The result is the mantissa
/// and exponent F1100 converts to km or AU.
pub fn range_words(from: [i64; 3], to: [i64; 3]) -> (i16, i16) {
    let delta: [i64; 3] = std::array::from_fn(|i| to[i].wrapping_sub(from[i]));
    let cut = vector_msb(delta) as i32 - 0xd;
    let (vector, shift) = if cut > 0 {
        (shift_vector(delta, cut as u32), cut as i16)
    } else {
        (delta, 0)
    };
    let length = model_math::length(vector.map(|v| v as i32));
    let ffp = ffp::from_int(length as i32);
    let exponent = ffp.exponent.wrapping_add(shift).wrapping_sub(0xf);
    let normal = ffp::normalise(Ffp::new(ffp.mantissa, exponent));
    (normal.mantissa, normal.exponent)
}

/// F1100's km/AU conversion: below the AU band the value is
/// `mantissa * 0xd1d0 >> (0x1d - exponent)`, otherwise
/// `mantissa * 0xeb54 >> (0x35 - exponent)`. Returns the number and whether it
/// is the AU form.
pub fn range_value(mantissa: i16, exponent: i16) -> (i32, bool) {
    if exponent <= 0x1d {
        let shift = (0x1d - i32::from(exponent)) as u32 & 31;
        (i32::from(mantissa).wrapping_mul(0xd1d0) >> shift, false)
    } else {
        let shift = (0x35 - i32::from(exponent)) as u32 & 31;
        (i32::from(mantissa).wrapping_mul(0xeb54) >> shift, true)
    }
}

/// A body the laser test can hit: its index in the system, its centre and its
/// interaction radius (`+0x138`, the word F819 tests against), in original
/// position units (millimetres).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub index: usize,
    pub centre: [i64; 3],
    pub radius: i64,
}

/// A laser hit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub index: usize,
    /// Where the ray enters the target's interaction box.
    pub point: [i64; 3],
    pub distance: f64,
}

/// F931's far bound for the hit test (`[D8733]`, 0x7fffff), the same length as
/// the beam model 0x9c (156). In original position units.
pub const RANGE: f64 = 0x7f_ffff as f64;

/// F817/F819's hit test against each object's interaction radius. The original
/// tests the laser line against a box of the object's radius in the object's
/// parent frame; the system frame is that frame here (all bodies are given in
/// it), so a ray against each axis-aligned box is the same test.
pub fn hit(origin: [i64; 3], direction: [f64; 3], targets: &[Target]) -> Option<Hit> {
    let mut best: Option<Hit> = None;
    for target in targets {
        if target.radius <= 0 {
            continue;
        }
        let radius = target.radius as f64;
        let (mut near, mut far) = (0.0_f64, RANGE);
        let mut miss = false;
        for axis in 0..3 {
            let min = target.centre[axis] as f64 - radius;
            let max = target.centre[axis] as f64 + radius;
            let origin = origin[axis] as f64;
            if direction[axis].abs() < 1e-12 {
                if origin < min || origin > max {
                    miss = true;
                    break;
                }
            } else {
                let mut t1 = (min - origin) / direction[axis];
                let mut t2 = (max - origin) / direction[axis];
                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                }
                near = near.max(t1);
                far = far.min(t2);
                if near > far || far < 0.0 {
                    miss = true;
                    break;
                }
            }
        }
        if miss || near > RANGE {
            continue;
        }
        let point = std::array::from_fn(|axis| {
            (origin[axis] as f64 + direction[axis] * near).round() as i64
        });
        if best.is_none_or(|hit| near < hit.distance) {
            best = Some(Hit {
                index: target.index,
                point,
                distance: near,
            });
        }
    }
    best
}

/// The shield/hull words F953 damages (`+0xe0/+0xe2` shields, `+0xe4` hull as
/// mass*4). Only ships take this path in the original; stations and planets
/// return without damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageState {
    /// Current shields (`+0xe0`).
    pub shields: u16,
    /// Full shields (`+0xe2`); zero means no shield generator.
    pub shields_max: u16,
    /// Remaining hull (`+0xe4`, mass * 4).
    pub hull: u16,
    /// The descriptor's mass word (`+0x6`), which `+0xe4` holds times four.
    pub mass: u16,
}

/// What F953 did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Damage {
    /// The hit was absorbed by the shields.
    Shields,
    /// The shields broke and the hull took the rest.
    Hull,
    /// The hull was used up: F942 destroys the object.
    Destroyed,
}

/// F953's damage order: shields first, then hull; a hit at least the remaining
/// hull destroys the object.
pub fn apply_damage(state: &mut DamageState, damage: u32) -> Damage {
    let damage = damage.min(i32::MAX as u32) as u16;
    if damage <= state.shields {
        state.shields -= damage;
        Damage::Shields
    } else {
        let rest = damage - state.shields;
        state.shields = 0;
        if rest >= state.hull {
            state.hull = 0;
            Damage::Destroyed
        } else {
            state.hull -= rest;
            Damage::Hull
        }
    }
}

/// The target readout's hull per-mille (`0..=1000`), F1099's
/// `hull * 250 / mass`. The original formats this as a percentage with one
/// decimal (`0x8625`/`0x8626`).
pub fn hull_per_mille(state: &DamageState) -> i32 {
    if state.mass == 0 {
        return 0;
    }
    i32::from(state.hull) * 250 / i32::from(state.mass)
}

/// The target readout's shield percentage, `current * 100 / max` (`0x8625`).
pub fn shield_percent(state: &DamageState) -> i32 {
    if state.shields_max == 0 {
        return 0;
    }
    i32::from(state.shields) * 100 / i32::from(state.shields_max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pulse_costs_are_f930s_values() {
        let pulse = spec(PULSE_1MW, 994).unwrap();
        assert_eq!(pulse.index, 1);
        assert_eq!(pulse.damage, 50);
        assert_eq!(pulse.heat, 400);
        assert_eq!(pulse.energy, 800);
        assert_eq!(pulse.refire, PULSE_REFIRE);
        assert!(!pulse.beam);
        // 5MW pulse: index 2, five times the 1MW costs.
        let strong = spec(PULSE_5MW, 994).unwrap();
        assert_eq!(
            (strong.damage, strong.heat, strong.energy),
            (250, 2000, 4000)
        );
        // The unused index 3 still has F930's own values.
        let third = spec(0x98, 994).unwrap();
        assert_eq!(
            (third.damage, third.heat, third.energy),
            (1500, 12000, 24000)
        );
        assert_eq!(third.refire, THIRD_PULSE_REFIRE);
    }

    #[test]
    fn beam_costs_scale_with_the_frame_time() {
        // 1MW beam, index 4: rate 1, shift 2; 994 tics is 7 in the high test.
        let beam = spec(BEAM_1MW, 994).unwrap();
        assert_eq!((beam.damage, beam.heat, beam.energy), (7, 14, 112));
        assert_eq!(beam.refire, 0);
        assert!(beam.beam);
        // 4MW beam: index 5, rate 4, shift 3.
        let strong = spec(BEAM_4MW, 994).unwrap();
        assert_eq!((strong.damage, strong.heat, strong.energy), (28, 28, 448));
        // 99400 tics (100x) scales the pulse-free beam damage.
        assert_eq!(spec(BEAM_1MW, 99_400).unwrap().damage, 776);
        // The out-of-table indices read DATA_007209 as the original does.
        assert_eq!(beam_shift(8), 0x0a);
        assert_eq!(beam_shift(9), 0x32);
    }

    #[test]
    fn fire_applies_heat_energy_and_refire_and_obeys_the_gates() {
        let mut state = State {
            energy: 0x2000_0000,
            ..State::default()
        };
        let first = fire(PULSE_1MW, &mut state, 994).unwrap();
        assert_eq!(first.damage, 50);
        assert_eq!(state.energy, 0x2000_0000 - 800);
        assert_eq!(state.heat, 400);
        assert_eq!(state.refire, PULSE_REFIRE);
        assert_eq!(fire(PULSE_1MW, &mut state, 994), Err(FireError::Refire));
        // F730: 0x795c tics clears the delay.
        tick_refire(&mut state, PULSE_REFIRE as u32);
        assert_eq!(state.refire, 0);
        assert_eq!(fire(PULSE_1MW, &mut state, 994).unwrap().damage, 50);
        // A high frame time clears it in one step (JJFFE's fix).
        tick_refire(&mut state, 0x1_0000);
        assert_eq!(state.refire, 0);
        // Heat at 0xf001 and above blocks; 0xf000 still fires.
        state.heat = HEAT_LIMIT;
        state.refire = 0;
        assert!(fire(PULSE_1MW, &mut state, 994).is_ok());
        state.heat = HEAT_LIMIT + 1;
        state.refire = 0;
        assert_eq!(fire(PULSE_1MW, &mut state, 994), Err(FireError::Overheated));
        // No laser on the mount, and an empty energy pool.
        assert_eq!(fire(0, &mut state, 994), Err(FireError::NoLaser));
        state.heat = 0;
        state.energy = 0;
        assert_eq!(fire(PULSE_1MW, &mut state, 994), Err(FireError::NoEnergy));
    }

    #[test]
    fn cooling_matches_f938() {
        let mut state = State {
            heat: 10_000,
            ..State::default()
        };
        cool(&mut state, 1.0, false);
        assert_eq!(state.heat, 10_000 - 0x400);
        state.heat = 10_000;
        cool(&mut state, 1.0, true);
        assert_eq!(state.heat, 10_000 - 0x1000);
        state.heat = 10;
        cool(&mut state, 1.0, false);
        assert_eq!(state.heat, 0);
    }

    #[test]
    fn range_words_are_f1100s_km_and_au_forms() {
        // 1e6 original units: shift 6, mantissa 0x7a12, exponent 5, and the
        // 0xd1d0 factor gives 100 in the km form.
        let (mantissa, exponent) = range_words([0; 3], [1_000_000, 0, 0]);
        assert_eq!((mantissa, exponent), (0x7a12, 5));
        assert_eq!(range_value(mantissa, exponent), (100, false));
        // A zero range is zero, not garbage.
        assert_eq!(range_words([0; 3], [0; 3]), (0, -22));
        assert_eq!(range_value(0, -22), (0, false));
        // Far enough out the AU branch is selected and the value stays positive.
        let (mantissa, exponent) = range_words([0; 3], [4_000_000_000_000, 0, 0]);
        assert_eq!(exponent, 27);
        assert!(!range_value(mantissa, exponent).1);
        let (mantissa, exponent) = range_words([0; 3], [40_000_000_000_000, 0, 0]);
        assert!(exponent > 0x1d, "{exponent}");
        assert!(range_value(mantissa, exponent).1);
        assert!(range_value(mantissa, exponent).0 > 0);
        // F1100's threshold is `exponent <= 0x1d` for km, else AU
        // (`ffe.asm:81263-81281`); the branch ignores the mantissa.
        assert!(!range_value(0x4000, 0x1d).1);
        assert!(range_value(0x4000, 0x1e).1);
    }

    #[test]
    fn hit_tests_the_interaction_radius_along_the_ray() {
        let targets = [
            Target {
                index: 7,
                centre: [10_000, 0, 0],
                radius: 1_000,
            },
            Target {
                index: 8,
                centre: [20_000, 0, 0],
                radius: 1_000,
            },
        ];
        let shot = hit([0; 3], [1.0, 0.0, 0.0], &targets).unwrap();
        assert_eq!(shot.index, 7);
        assert_eq!(shot.point, [9_000, 0, 0]);
        // Off to the side, and behind the origin, miss.
        assert!(hit([0; 3], [0.0, 1.0, 0.0], &targets).is_none());
        assert!(hit([0; 3], [-1.0, 0.0, 0.0], &targets).is_none());
        // The first box wins even when a farther one is also crossed.
        assert_eq!(hit([0; 3], [1.0, 0.0, 0.0], &targets).unwrap().index, 7);
    }

    #[test]
    fn damage_follows_f953_shields_then_hull() {
        let mut state = DamageState {
            shields: 100,
            shields_max: 100,
            hull: 100, // 25 t * 4
            mass: 25,
        };
        assert_eq!(apply_damage(&mut state, 50), Damage::Shields);
        assert_eq!((state.shields, state.hull), (50, 100));
        assert_eq!(apply_damage(&mut state, 80), Damage::Hull);
        assert_eq!((state.shields, state.hull), (0, 70));
        assert_eq!(apply_damage(&mut state, 70), Damage::Destroyed);
        assert_eq!(state.hull, 0);
        // F1099's readouts: full hull is 1000 per-mille, shields are percent.
        let mut full = DamageState {
            shields: 100,
            shields_max: 100,
            hull: 100,
            mass: 25,
        };
        assert_eq!(hull_per_mille(&full), 1000);
        assert_eq!(shield_percent(&full), 100);
        full.shields = 25;
        full.hull = 50;
        assert_eq!(shield_percent(&full), 25);
        assert_eq!(hull_per_mille(&full), 500);
        let no_shields = DamageState {
            shields: 0,
            shields_max: 0,
            hull: 100,
            mass: 25,
        };
        assert_eq!(shield_percent(&no_shields), 0);
    }

    #[test]
    fn gauge_height_and_energy_bar_match_f270() {
        assert_eq!(gauge_height(0), 0);
        assert_eq!(gauge_height(0xffff), 11);
        assert_eq!(gauge_height(0x8000), 6);
        // The new game's 1 t energy at drive type 2 is a full bar.
        assert_eq!(energy_gauge(0x2000_0000, 2), 0xffe0);
        assert_eq!(gauge_height(energy_gauge(0x2000_0000, 2)), 11);
        assert_eq!(energy_gauge(0, 2), 0);
    }
}
