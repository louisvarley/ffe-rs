//! The end of F575 for a body that has touched its parent, in the branch where
//! the crash mask (`DATA_008700`, zero for F574 and F568) is clear: the body is
//! put on the surface, its orientation turned to the surface's up and its velocity
//! cleared. (The crash and bounce branch, for a non-zero mask, is not ported.)
//! Verified through the starports F879 places (`docs/notes/objects.md`).

use crate::model::multiply;
use crate::model_math::square_root;
use crate::objects::ModelInfo;

/// F1501: `x / divisor` as a signed Q31 fraction by restoring division.
fn fraction(x: i32, divisor: u32) -> i32 {
    let mut remainder = x.unsigned_abs();
    let mut quotient: u32 = 1;
    loop {
        remainder = remainder.wrapping_add(remainder);
        quotient = quotient.wrapping_add(quotient);
        if divisor <= remainder {
            remainder = remainder.wrapping_sub(divisor);
            quotient = quotient.wrapping_add(1);
        }
        if (quotient as i32) <= 0 {
            break;
        }
    }
    let magnitude = (quotient & 0x7fff_ffff) as i32;
    if x < 0 {
        -magnitude
    } else {
        magnitude
    }
}

/// F1504: a vector scaled to the length `magnitude`, given the sum `squares` of the
/// high dwords of its components' squares.
fn set_length(vector: &mut [i32; 3], magnitude: i32, squares: u32) {
    let root = square_root(squares.wrapping_add(squares));
    for c in vector.iter_mut() {
        *c = multiply(fraction(*c, root), magnitude);
    }
}

/// F1503: the same, with the squares summed here.
fn set_magnitude(vector: &mut [i32; 3], magnitude: i32) {
    let square = |c: i32| ((i64::from(c) * i64::from(c)) >> 32) as u32;
    let squares = vector
        .iter()
        .fold(0u32, |sum, c| sum.wrapping_add(square(*c)));
    set_length(vector, magnitude, squares);
}

/// What the touch is against: a terrain surface (`DATA_008707` 1), whose altitude
/// the terrain query left in `DATA_008705`, or a simple sphere (2), where F577 left
/// the sum of the squares of the scaled relative position.
#[derive(Debug, Clone, Copy)]
pub enum Surface {
    Terrain { altitude: i64 },
    Sphere { squares: u32 },
}

/// Both objects' bytes and models.
pub struct Pair<'a> {
    pub body: &'a mut [u8],
    pub planet: &'a [u8],
    pub body_model: ModelInfo,
    pub planet_model: ModelInfo,
}

fn dword(object: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(object[at..at + 4].try_into().unwrap())
}

fn set_dword(object: &mut [u8], at: usize, value: i32) {
    object[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn qword(object: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(object[at..at + 8].try_into().unwrap())
}

/// F575 from `DATA_008696 |= 2` to the end for the reorienting branch. `relpos` is
/// the body's position relative to the planet; `flags` is `DATA_008696`, updated and
/// returned.
pub fn settle(pair: &mut Pair, relpos: [i64; 3], surface: Surface, mut flags: u8) -> u8 {
    let Pair {
        body,
        planet,
        body_model,
        planet_model,
    } = pair;
    flags |= 2;
    let speed = (0..3).fold(0i32, |sum, k| {
        sum.wrapping_add(dword(body, 0x8c + 4 * k).wrapping_abs())
    });
    let low = |k: usize| relpos[k] as i32;
    let mut up;
    match surface {
        Surface::Terrain { .. } => {
            let (cut, _) = crate::frames::truncate(relpos, 0x1c);
            up = [cut[0] as i32, cut[1] as i32, cut[2] as i32];
            set_magnitude(&mut up, 0x7fff_ffff);
        }
        Surface::Sphere { .. } => {
            up = crate::physics::normalise([low(0), low(1), low(2)]);
        }
    }
    let dot = (0..3).fold(0i32, |sum, k| {
        sum.wrapping_add(multiply(dword(body, 0xc + 4 * k), up[k]))
    });
    let word_9e = u16::from_le_bytes([body[0x9e], body[0x9f]]);
    if speed <= 0x258 && word_9e == 0xffff && dot >= 0x7800_0000 {
        flags |= 0x40;
        flags &= 0x7f;
    }
    match surface {
        Surface::Terrain { altitude } => {
            let difference = qword(body, 0x140).wrapping_sub(altitude);
            let step = difference as i32;
            for (k, component) in up.iter().enumerate() {
                let moved = i64::from(multiply(*component, step));
                let at = 0x3e + 8 * k;
                let position = qword(body, at).wrapping_add(moved);
                body[at..at + 8].copy_from_slice(&position.to_le_bytes());
            }
        }
        Surface::Sphere { squares } => {
            let planet_zoom = planet_model.scale.wrapping_add(planet_model.scale_offset);
            let body_zoom = body_model.scale.wrapping_add(body_model.scale_offset);
            let mut length = (planet_model.radius_dword << 16) as i32;
            let mut extra = (u32::from(body_model.collision_radius as u16) << 16) as i32;
            let delta = planet_zoom.wrapping_sub(body_zoom);
            if delta != 0 {
                extra >>= delta & 31;
            }
            length = length.wrapping_add(extra);
            let mut squares = squares;
            if length < 0 {
                length = ((length as u32) >> 1) as i32;
                squares = squares.wrapping_add(1);
            }
            let mut direction = [low(0), low(1), low(2)];
            set_length(&mut direction, length, squares);
            let right = 0x38i32.wrapping_sub(planet_zoom);
            let left = 0x20 - right;
            for (k, v) in direction.iter().enumerate() {
                let at = 0x3e + 8 * k;
                let lo = (*v).wrapping_shl(left as u32 & 31) as u32;
                let hi = *v >> (right & 31);
                let position = (i64::from(hi) << 32) | i64::from(lo);
                body[at..at + 8].copy_from_slice(&position.to_le_bytes());
            }
        }
    }
    // Reorientate: the surface's up becomes the second row, then rows are swapped
    // (F571), re-orthogonalised (F1520) and swapped back.
    for (k, v) in up.iter().enumerate() {
        set_dword(body, 0xc + 4 * k, *v);
    }
    swap_rows(body);
    renormalise(body);
    swap_rows(body);
    for k in 0..3 {
        set_dword(body, 0x8c + 4 * k, 0);
    }
    if dword(planet, 0x140) != dword(planet, 0x138) {
        flags |= 1;
    }
    flags
}

/// F571: the second and third rows trade places and the first is negated.
fn swap_rows(object: &mut [u8]) {
    for k in 0..3 {
        let (second, third) = (dword(object, 0xc + 4 * k), dword(object, 0x18 + 4 * k));
        set_dword(object, 0xc + 4 * k, third);
        set_dword(object, 0x18 + 4 * k, second);
        let first = dword(object, 4 * k);
        set_dword(object, 4 * k, first.wrapping_neg());
    }
}

/// F1520 on an object's orientation: right and forward normalised, up the
/// normalised cross product of forward and right, right the cross product of up and
/// forward.
fn renormalise(object: &mut [u8]) {
    let row = |object: &[u8], r: usize| -> [i32; 3] {
        std::array::from_fn(|k| dword(object, 12 * r + 4 * k))
    };
    let right = crate::physics::normalise(row(object, 0));
    let forward = crate::physics::normalise(row(object, 2));
    let up = crate::physics::normalise(crate::physics::cross(forward, right));
    let right = crate::physics::cross(up, forward);
    for (r, v) in [(0, right), (1, up), (2, forward)] {
        for (k, c) in v.iter().enumerate() {
            set_dword(object, 12 * r + 4 * k, *c);
        }
    }
}
