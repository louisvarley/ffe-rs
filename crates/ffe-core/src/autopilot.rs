//! Source autopilot arithmetic: F587, F589, F592/F612 and F597.
//! Positions are system millimetres; velocities retain F1511's native units.
use crate::model::multiply;
use crate::model_animation::vector_msb;
use crate::model_math::table_root;
use crate::physics::Motion;
use crate::thrust::{clamp, Limits};

/// F1020: target present, state below 0x28, and object +0xca bit 0x20.
pub fn can_engage(mode: u8, has_target: bool, equipment: u32) -> bool {
    mode < 0x28 && has_target && equipment & (1 << 21) != 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Guidance {
    pub thrust: [i16; 3],
    pub cap: [i32; 3],
    pub turns: [i16; 3],
    pub distance_shift: i32,
}

/// F612: normalise by the OR of the absolute components, not Euclidean length.
pub fn normalise_velocity(vector: &mut [i32; 3]) -> i32 {
    let bits = vector.iter().fold(0u32, |n, v| n | v.wrapping_abs() as u32);
    let shift = 31u32.saturating_sub(bits.leading_zeros()) as i32 - 14;
    for value in vector {
        *value = if shift < 0 { value.wrapping_shl((-shift) as u32) } else { *value >> shift };
    }
    shift
}

/// F587, including its half-turn escape when the target is behind the ship.
pub fn face_target(matrix: [[i32; 3]; 3], target: [i16; 3]) -> [i16; 3] {
    let dot = |row: [i32; 3]| row.into_iter().zip(target).fold(0i32, |sum, (a,b)|
        sum.wrapping_add((a >> 16).wrapping_mul(i32::from(b))));
    let pitch = (dot(matrix[1]).wrapping_mul(2).wrapping_neg() >> 16) as i16;
    let yaw = (dot(matrix[0]).wrapping_mul(2) >> 16) as i16;
    if dot(matrix[2]) < 0 {
        [if pitch < 0 { -0x4000 } else { 0x4000 }, 0,
         if yaw < 0 { -0x4000 } else { 0x4000 }]
    } else { [pitch, 0, yaw] }
}

/// F589: equalise roll, or all three axes. Both matrices must share a frame.
pub fn match_orientation(matrix: [[i32; 3]; 3], target: [[i32; 3]; 3], all: bool) -> [i16; 3] {
    let dot = |a: [i32; 3], b: [i32; 3]| a.into_iter().zip(b).fold(0i32, |sum, (a,b)|
        sum.wrapping_add((a >> 16).wrapping_mul(b >> 16)));
    let roll = (dot(matrix[0], target[1]).wrapping_neg() >> 16) as i16;
    if all { [(dot(matrix[1], target[2]).wrapping_neg() >> 16) as i16,
        roll, (dot(matrix[2], target[0]).wrapping_neg() >> 16) as i16] }
    else { [0, roll, 0] }
}

/// F597's ordinary guidance branch. Returns local thrust request and world caps.
/// The accelerated near-offset frame-match branch belongs to the world adapter.
pub fn request(matrix: [[i32; 3]; 3], mut velocity: [i32; 3], mut velocity_shift: i32,
    mut position: [i32; 3], distance_shift: i32, modifier: i32) -> ([i32; 3], [i32; 3]) {
    let mut scale = distance_shift.wrapping_add(modifier).wrapping_sub(8);
    if scale >= 0 {
        if scale & 1 != 0 {
            scale = (scale >> 1).wrapping_add(1);
            position = position.map(|v| v >> 1);
        } else { scale >>= 1; }
        let sum = position.iter().fold(0i32, |s,v| s.wrapping_add(v.wrapping_mul(*v)));
        let length = table_root(sum as u32);
        let root = table_root(length.wrapping_shl(16));
        if root != 0 { position = position.map(|v| v.wrapping_shl(15) / root as i32); }
    }
    let shift = velocity_shift.wrapping_sub(scale.wrapping_sub(2));
    if shift >= 0 {
        velocity = std::array::from_fn(|k| velocity[k].wrapping_add(position[k] >> (shift & 31)));
    } else {
        velocity = std::array::from_fn(|k| (velocity[k] >> ((-shift) & 31)).wrapping_add(position[k]));
        velocity_shift = velocity_shift.wrapping_sub(shift);
    }
    if velocity_shift > 12 { velocity_shift = 12; }
    let cap = if velocity_shift > 0 { velocity.map(|v| v.wrapping_shl(velocity_shift as u32)) } else { velocity };
    let local: [i32; 3] = matrix.map(|row| row.into_iter().zip(velocity).fold(0i32, |sum,(a,b)|
        sum.wrapping_add(multiply(a,b))));
    let scale = velocity_shift.wrapping_add(2);
    let local = if scale < 0 { local.map(|v| v >> ((-scale) & 31)) }
        else { local.map(|v| v.wrapping_shl(scale.min(2) as u32)) };
    (local, cap)
}

/// F592/F596 followed by F597/F599 and F587. No direct orientation changes,
/// keyboard-speed emulation or long-distance time/position transfers.
pub fn guide(motion: &Motion, position: [i64; 3], target: [i64; 3],
    target_velocity: [i32; 3], limits: &Limits, modifier: i32) -> Guidance {
    guide_to_radius(motion, position, target, target_velocity, limits, modifier, 0)
}

/// F610's large-body radius subtraction before the ordinary F597 branch.
pub fn guide_to_radius(motion: &Motion, position: [i64; 3], target: [i64; 3],
    target_velocity: [i32; 3], limits: &Limits, modifier: i32, radius: i64) -> Guidance {
    let relative: [i64; 3] = std::array::from_fn(|k| target[k].wrapping_sub(position[k]));
    let distance_shift = (vector_msb(relative) as i32 - 13).max(0);
    let mut direction = relative.map(|v| (v >> distance_shift) as i32);
    if radius >= 100_000_000 {
        let sum = direction.iter().fold(0i32, |s,v| s.wrapping_add(v.wrapping_mul(*v)));
        let length = table_root(sum as u32) as i32;
        if length != 0 {
            let normal = direction.map(|v| v.wrapping_shl(15) / length);
            let radius = (radius >> distance_shift) as i32;
            direction = std::array::from_fn(|k| direction[k].wrapping_sub(multiply(normal[k].wrapping_shl(16), radius)));
        }
    }
    let mut velocity = std::array::from_fn(|k| target_velocity[k].wrapping_sub(motion.velocity[k]));
    let velocity_shift = normalise_velocity(&mut velocity);
    let (request, cap) = request(motion.orientation, velocity, velocity_shift, direction, distance_shift, modifier);
    Guidance { thrust: clamp(limits, request).words.map(|w| w as i16), cap,
        turns: face_target(motion.orientation, direction.map(|v| v as i16)), distance_shift }
}
