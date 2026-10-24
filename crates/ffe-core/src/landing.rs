//! F575's contact code for a pair of bodies that touch, and F574's use of it:
//! `DATA_008696 |= 2` and the test that decides a soft landing (`DATA_008699`
//! cleared, flag `0x40`) from a crash (`DATA_008699` = the crash mask, flag
//! `0x80`); the body is then put on the surface, settled when it landed, or
//! bounced when it crashed and it is the player.
//! Verified through the starports F879 places (`docs/notes/objects.md`) and the
//! collision oracles (`docs/notes/collision.md`).

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

/// What F575's contact tail resolved (`DATA_008699`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Touch {
    /// A soft landing: `DATA_008699` cleared, flags `0x40` set and the crash bit
    /// cleared; the body is left on the surface and turned to its up.
    Landing,
    /// A crash: `DATA_008699` set and flags `0x80`. `damage` is the amount passed
    /// to F953 (which itself raises a negative amount to `0x7fffffff`).
    Crash { damage: i32 },
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

/// The surface's up vector as F575 builds it (`JUMP_002411`/`JUMP_002410`): the
/// relative position cut to 0x1c bits and scaled to Q31 for the terrain, the
/// sphere's low dwords normalised (F1519) otherwise.
pub fn up(relpos: [i64; 3], surface: Surface) -> [i32; 3] {
    match surface {
        Surface::Terrain { .. } => {
            let (cut, _) = crate::frames::truncate(relpos, 0x1c);
            let mut up = [cut[0] as i32, cut[1] as i32, cut[2] as i32];
            set_magnitude(&mut up, 0x7fff_ffff);
            up
        }
        Surface::Sphere { .. } => {
            crate::physics::normalise([relpos[0] as i32, relpos[1] as i32, relpos[2] as i32])
        }
    }
}

/// F575 from `DATA_008696 |= 2` to the end for a pair of bodies that touch.
/// `relpos` is the body's position relative to the planet; `crash` is the crash
/// mask (`DATA_008700`: 0xff for the player's scan F569, zero for F574 and
/// F568); `player` is `DATA_008857`, the craft number whose body is bounced.
/// Returns the flags (`DATA_008696`) and the touch.
pub fn contact(
    pair: &mut Pair,
    relpos: [i64; 3],
    surface: Surface,
    mut flags: u8,
    crash: u8,
    player: u8,
) -> (u8, Touch) {
    flags |= 2;
    let speed = {
        let body = &*pair.body;
        (0..3).fold(0i32, |sum, k| {
            sum.wrapping_add(dword(body, 0x8c + 4 * k).wrapping_abs())
        })
    };
    let up = up(relpos, surface);
    let dot = {
        let body = &*pair.body;
        (0..3).fold(0i32, |sum, k| {
            sum.wrapping_add(multiply(dword(body, 0xc + 4 * k), up[k]))
        })
    };
    let word = u16::from_le_bytes([pair.body[0x9e], pair.body[0x9f]]);
    let landing = speed <= 0x258 && word == 0xffff && dot >= 0x7800_0000;
    let crash = if landing {
        flags |= 0x40;
        flags &= 0x7f;
        0
    } else {
        crash
    };
    // JUMP_002414: put the body on the surface (or inside the planet's radius).
    place(pair, relpos, &up, surface);
    let touch = if crash != 0 {
        flags |= 0x80;
        let body = &*pair.body;
        if body[0x86] == player {
            let damage = bounce(pair, up, speed);
            Touch::Crash { damage }
        } else if dword(body, 0x82) > 0xd && (body[0x87] == 0xb || body[0x87] == 0xe) {
            // JUMP_002426/002427: any other ship is turned to the surface, its
            // velocity cleared and - when the mask is set - F953 is given -1.
            realign(pair, up);
            Touch::Crash { damage: -1 }
        } else {
            Touch::Crash { damage: 0 }
        }
    } else {
        // JUMP_002428.
        realign(pair, up);
        Touch::Landing
    };
    // JUMP_002429: the planet's collision radius differs from its outer radius.
    if qword(pair.planet, 0x140) != qword(pair.planet, 0x138) {
        flags |= 1;
    }
    (flags, touch)
}

/// F575's tail for F574's scan, whose crash mask is zero: the body is settled on
/// the surface whether or not the landing test passed, and the flags are
/// returned. `relpos` is the body's position relative to the planet.
pub fn settle(pair: &mut Pair, relpos: [i64; 3], surface: Surface, flags: u8) -> u8 {
    contact(pair, relpos, surface, flags, 0, 0).0
}

/// JUMP_002414: put the touched body on the surface. For the terrain the
/// body's `+0x140` less the altitude (both 64-bit, the low dword used) is added
/// along the up vector; for the sphere the position is rebuilt from the
/// relative position, scaled to the planet's radius plus the body's collision
/// radius and to `squares`.
fn place(pair: &mut Pair, relpos: [i64; 3], up: &[i32; 3], surface: Surface) {
    let Pair {
        body,
        planet_model,
        body_model,
        ..
    } = pair;
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
            let mut direction = [relpos[0] as i32, relpos[1] as i32, relpos[2] as i32];
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
}

/// JUMP_002428: the surface's up becomes the second row, then rows are swapped
/// (F571), re-orthogonalised (F1520) and swapped back; the velocity is cleared.
fn realign(pair: &mut Pair, up: [i32; 3]) {
    let body = &mut *pair.body;
    for (k, v) in up.iter().enumerate() {
        set_dword(body, 0xc + 4 * k, *v);
    }
    swap_rows(body);
    renormalise(body);
    swap_rows(body);
    for k in 0..3 {
        set_dword(body, 0x8c + 4 * k, 0);
    }
}

/// F575's player bounce (`JUMP_002421` after the craft comparison): the
/// surface's up becomes the second row, the rows are swapped (F571),
/// re-orthogonalised (F1520) and swapped back, and the velocity is reflected
/// about the surface plane as `v[k] = 2 v[k] / 3 - 10 (up[k] . v) / 12` with the
/// dot products F1521. The reflected direction is flipped to keep the body's
/// forward sense (the saved row 2), or replaced by it when it cancels; it then
/// becomes row 1 and F1520 runs again. Returns the damage F953 is called with:
/// the model's mass word (`statistics + 6`) times four, times the speed lost,
/// over 0x708, but at least the mass over four.
pub fn bounce(pair: &mut Pair, up: [i32; 3], speed: i32) -> i32 {
    let body = &mut *pair.body;
    let saved: [i32; 9] = std::array::from_fn(|k| dword(body, 4 * k));
    for (k, v) in up.iter().enumerate() {
        set_dword(body, 0xc + 4 * k, *v);
    }
    swap_rows(body);
    renormalise(body);
    swap_rows(body);
    let dot = (0..3).fold(0i32, |sum, k| {
        sum.wrapping_add(multiply(up[k], dword(body, 0x8c + 4 * k)))
    });
    let mut direction = [0i32; 3];
    for (k, component) in direction.iter_mut().enumerate() {
        let v = dword(body, 0x8c + 4 * k);
        let reflected = multiply(up[k], dot).wrapping_mul(10) / 12;
        *component = v.wrapping_mul(2) / 3 - reflected;
        set_dword(body, 0x8c + 4 * k, *component);
    }
    let forward = [saved[6], saved[7], saved[8]];
    let along = (0..3).fold(0i32, |sum, k| {
        sum.wrapping_add(multiply(direction[k], forward[k]))
    });
    if along < 0 {
        for v in &mut direction {
            *v = v.wrapping_neg();
        }
    }
    if direction == [0; 3] {
        direction = forward;
    }
    set_magnitude(&mut direction, 0x7fff_ffff);
    for (k, v) in saved.iter().enumerate() {
        set_dword(body, 4 * k, *v);
    }
    for (k, v) in direction.iter().enumerate() {
        set_dword(body, 0x18 + 4 * k, *v);
    }
    renormalise(body);
    let new_speed = (0..3).fold(0i32, |sum, k| {
        sum.wrapping_add(dword(body, 0x8c + 4 * k).wrapping_abs())
    });
    let loss = speed.wrapping_sub(new_speed);
    let mass = pair.body_model.stats.map_or(0, |stats| {
        i32::from(i16::from_le_bytes([stats[6], stats[7]]))
    });
    let mass = mass.wrapping_mul(4);
    let product = i64::from(mass) * i64::from(loss);
    // The original's `idiv` faults where the quotient does not fit 32 bits; game
    // speeds keep it far below that.
    let share = (product / 0x708) as i32;
    let floor = mass / 4;
    share.max(floor)
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
