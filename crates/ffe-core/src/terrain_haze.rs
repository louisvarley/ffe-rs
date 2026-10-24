//! The planet atmosphere: F1877 (the haze set-up and the ring around the planet), F1878
//! (the light on a direction) and F1879 (one ring segment). F1876 calls F1877 before it
//! draws the surface; it leaves the haze state the emitter reads (`DATA_009262..9267`),
//! the sky colour (`DATA_008814..8817`) and, in a group of its own, the ring's quads in
//! the order queue. The original's floating-point solve runs in x87 extended precision
//! ([`crate::x87`]); the rest is wrapping 32-bit integer arithmetic.

use crate::model::multiply;
use crate::model_instance::project;
use crate::model_math::{table_root, SineTable};
use crate::terrain::{extent, tables, Fault};
use crate::terrain_draw::length;
use crate::terrain_emit::{Atmosphere, Queue, RING_QUAD, TEXTURE_QUAD};
use crate::x87::Ext;

/// `DATA_007895`: the six orders the axes are tried in.
const ORDERS: [[usize; 3]; 6] = [
    [0, 1, 2],
    [0, 2, 1],
    [1, 0, 2],
    [1, 2, 0],
    [2, 0, 1],
    [2, 1, 0],
];

/// What F1877 reads of the instance and the record.
#[derive(Debug, Clone, Copy)]
pub struct Haze {
    /// The planet centre in view space.
    pub origin: [i32; 3],
    /// The three axis vectors.
    pub axes: [i32; 9],
    /// `DATA_009259`, the surface colour of the record's material.
    pub material: [u8; 3],
    /// The record's word `+8` shifted left sixteen bits.
    pub strength: i32,
    /// The record's word `+10`.
    pub scale: i32,
    /// The instance's `+0x134`.
    pub light: [i32; 3],
    /// The instance's `+0x84`.
    pub shades: [[u8; 3]; 8],
}

/// The state F1877 leaves behind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Left {
    /// `DATA_009262`.
    pub flag: i32,
    /// `DATA_009263`.
    pub distance: i32,
    /// `DATA_009265` (three bytes).
    pub colour: u32,
    /// `DATA_009267`.
    pub scale: i32,
}

impl Left {
    /// What the emitter reads, when there is a haze.
    pub fn atmosphere(&self) -> Option<Atmosphere> {
        (self.flag != 0).then_some(Atmosphere {
            offset: self.distance,
            colour: self.colour,
            divisor: self.scale,
        })
    }
}

/// `DATA_008814` and `DATA_008815..8817`: the sky colour and whether F1877 set it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Sky {
    pub colour: [u8; 3],
    pub flag: i32,
}

fn divide(a: i32, b: i32) -> Result<i32, Fault> {
    if b == 0 || (a == i32::MIN && b == -1) {
        return Err(Fault::Division);
    }
    Ok(a / b)
}

/// F1878: the shade the light gives a direction (the vector is normalised in
/// place); zero when the light is not on it.
fn shade(light: [i32; 3], shades: &[[u8; 3]; 8], v: &mut [i32; 3]) -> Result<[u8; 3], Fault> {
    let shift = extent(*v) - 14;
    if shift > 0 {
        *v = v.map(|c| c >> shift);
    }
    let bottom = length(*v).wrapping_add(1);
    for c in v.iter_mut() {
        *c = divide(c.wrapping_shl(15), bottom)?;
    }
    let dot = light[0]
        .wrapping_mul(v[0])
        .wrapping_add(light[1].wrapping_mul(v[1]))
        .wrapping_add(light[2].wrapping_mul(v[2]));
    if dot >= 0 {
        return Ok([0; 3]);
    }
    let mut index = (dot as u32) >> 27;
    if index < 0x18 {
        index = 0;
    }
    Ok(shades[(index & 7) as usize])
}

fn pack(bytes: [u8; 3]) -> u32 {
    (u32::from(bytes[0]) << 8)
        .wrapping_add(u32::from(bytes[1]) << 4)
        .wrapping_add(u32::from(bytes[2]))
}

fn code(table: &[u8], mask: u32) -> bool {
    let at = tables::OUTCODES + 4 * (mask as usize & 0xf);
    u32::from_le_bytes([table[at], table[at + 1], table[at + 2], table[at + 3]]) != 0
}

/// The region code of a screen word (F1715 for x, F1716 for y).
fn region(value: i16, limit: u32) -> u32 {
    if value == -0x7ffe {
        8
    } else if (i32::from(value) as u32) < limit {
        2
    } else if value >= 0 {
        4
    } else {
        1
    }
}

type Point = [i16; 2];

const OFF: Point = [-0x7ffe; 2];

/// F1879: one segment of the ring between the previous step's three points and this
/// step's, queued as a textured quad and a ring quad when they are on the view.
#[allow(clippy::too_many_arguments)]
fn segment(
    table: &[u8],
    queue: &mut Queue,
    light: [i32; 3],
    shades: &[[u8; 3]; 8],
    previous: [Point; 3],
    current: [Point; 3],
    colours: &[[u8; 3]; 7],
    vector: &mut [i32; 3],
) -> Result<(), Fault> {
    // `previous` and `current` are (third, first, second): the points at 7/8, 1 and
    // the outer radius.
    let [p3, p1, p2] = previous;
    let [c3, c1, c2] = current;
    if p1 == OFF || c1 == OFF {
        return Ok(());
    }
    let shaded = if colours[1].iter().any(|b| *b > 8) {
        [0u8; 3]
    } else {
        shade(light, shades, vector)?
    };
    let tint = |a: [u8; 3], b: [u8; 3]| [0, 1, 2].map(|k| a[k].wrapping_add(b[k]));
    let base = pack(tint(colours[0], shaded));
    let word = |p: Point| u32::from(p[0] as u16) | u32::from(p[1] as u16) << 16;
    let outside = |points: [Point; 4]| {
        let x = points.iter().fold(0, |m, p| m | region(p[0], 0x140));
        let y = points.iter().fold(0, |m, p| m | region(p[1], 0x9e));
        code(table, x) || code(table, y)
    };
    if !outside([p1, p2, c2, c1]) {
        let at = queue.entry(0, 13)?;
        queue.words[at] = TEXTURE_QUAD;
        queue.words[at + 1] = word(p2);
        queue.words[at + 2] = word(p1);
        queue.words[at + 3] = word(c1);
        queue.words[at + 4] = word(c2);
        queue.words[at + 5] = 0x43;
        queue.words[at + 6] = base;
        for (k, colour) in colours.iter().enumerate().skip(1) {
            let step = |s: u8, c: u8| ((i32::from(s) * (6 - k as i32)) / 6) as u8 + c;
            let mixed = [
                step(shaded[0], colour[0]),
                step(shaded[1], colour[1]),
                step(shaded[2], colour[2]),
            ];
            queue.words[at + 6 + k] = pack(mixed);
        }
    }
    if !outside([p1, p3, c3, c1]) {
        let at = queue.entry(0, 6)?;
        queue.words[at] = RING_QUAD;
        queue.words[at + 1] = word(p1);
        queue.words[at + 2] = word(p3);
        queue.words[at + 3] = word(c3);
        queue.words[at + 4] = word(c1);
        queue.words[at + 5] = base;
    }
    Ok(())
}

/// F1877. `sky` is the sky colour on entry and on return.
pub fn haze(
    input: &Haze,
    table: &[u8],
    sine: &SineTable,
    sky: &mut Sky,
    queue: &mut Queue,
) -> Result<Left, Fault> {
    let mut left = Left {
        flag: input.strength,
        scale: input.scale,
        ..Left::default()
    };
    if input.strength == 0 {
        return Ok(left);
    }
    left.colour = u32::from(input.material[0])
        | u32::from(input.material[1]) << 8
        | u32::from(input.material[2]) << 16;
    // The axes that dominate each coordinate.
    let mut best = [[0i32; 3]; 3];
    for order in ORDERS {
        let row = |i: usize| {
            [
                input.axes[3 * i],
                input.axes[3 * i + 1],
                input.axes[3 * i + 2],
            ]
        };
        if row(order[0])[0].wrapping_abs() > best[0][0].wrapping_abs()
            && row(order[1])[1].wrapping_abs() > best[1][1].wrapping_abs()
            && row(order[2])[2].wrapping_abs() > best[2][2].wrapping_abs()
        {
            best = [row(order[0]), row(order[1]), row(order[2])];
        }
    }
    let origin = input.origin;
    if best[0][0] == 0 {
        return Ok(left);
    }
    // Solve for the origin in the axes' frame, in the original's single-precision
    // steps through the x87 stack.
    let load = Ext::from_f32;
    let store = |e: Ext| e.to_f32();
    let fi = |v: i32| Ext::from_i32(v).to_f32();
    let (mut a1, mut a2, mut a3) = (fi(best[0][0]), fi(best[0][1]), fi(best[0][2]));
    let (mut b1, mut b2, mut b3) = (fi(best[1][0]), fi(best[1][1]), fi(best[1][2]));
    let (c1, c2, c3) = (fi(best[2][0]), fi(best[2][1]), fi(best[2][2]));
    let (mut r1, mut r2, mut r3) = (
        fi(origin[0].wrapping_neg()),
        fi(origin[1].wrapping_neg()),
        fi(origin[2].wrapping_neg()),
    );
    a3 = store(load(a3).div(load(c3)));
    b3 = store(load(b3).div(load(c3)));
    r3 = store(load(r3).div(load(c3)));
    a2 = store(load(a2).sub(load(a3).mul(load(c2))));
    a1 = store(load(a1).sub(load(a3).mul(load(c1))));
    b2 = store(load(b2).sub(load(b3).mul(load(c2))));
    b1 = store(load(b1).sub(load(b3).mul(load(c1))));
    r2 = store(load(r2).sub(load(r3).mul(load(c2))));
    r1 = store(load(r1).sub(load(r3).mul(load(c1))));
    a2 = store(load(a2).div(load(b2)));
    r2 = store(load(r2).div(load(b2)));
    a1 = store(load(a1).sub(load(a2).mul(load(b1))));
    r1 = store(load(r1).sub(load(r2).mul(load(b1))));
    r1 = store(load(r1).div(load(a1)));
    r2 = store(load(r2).sub(load(r1).mul(load(a2))));
    r3 = store(load(r3).sub(load(r1).mul(load(a3))));
    r3 = store(load(r3).sub(load(r2).mul(load(b3))));
    let scale = Ext::from_f32(32768.0);
    let mut v = [r1, r2, r3].map(|r| load(r).mul(scale).truncate_low());
    let sum = v.iter().fold(0i32, |s, c| s.wrapping_add(c.wrapping_abs()));
    if sum >= 0x80_0000 {
        left.flag = 0;
        return Ok(left);
    }
    let shift = extent(v) - 13;
    if shift <= 0 {
        left.flag = 0;
        return Ok(left);
    }
    v = v.map(|c| c >> shift);
    let mut square = v
        .iter()
        .fold(0i32, |s, c| s.wrapping_add(c.wrapping_mul(*c)));
    let mut root = table_root(square as u32) as i32;
    if (root.wrapping_shl(shift as u32 & 31)) < 0x8800 {
        root = root.wrapping_shl(shift as u32 & 31);
        for c in v.iter_mut() {
            *c = divide(c.wrapping_shl(4).wrapping_add(*c).wrapping_shl(11), root)?;
        }
        square = 0x4840_0000i32 >> ((2 * shift) & 31);
        root = 0x8800 >> (shift & 31);
    }
    // The direction in the plane across the axis, and the axis it turns about.
    let mut across = [0i32; 3];
    let mut turn: [i32; 3];
    if v[0].wrapping_abs().wrapping_add(v[1].wrapping_abs()) > 0x1f4 {
        let flat = table_root(
            v[1].wrapping_mul(v[1])
                .wrapping_add(v[0].wrapping_mul(v[0])) as u32,
        ) as i32;
        turn = [
            divide(v[1].wrapping_shl(15), flat)?,
            divide(v[0].wrapping_neg().wrapping_shl(15), flat)?,
            0,
        ];
    } else {
        turn = [0x8000, 0, 0];
    }
    across[0] = turn[1]
        .wrapping_mul(v[2])
        .wrapping_sub(turn[2].wrapping_mul(v[1]))
        >> 15;
    across[1] = turn[2]
        .wrapping_mul(v[0])
        .wrapping_sub(turn[0].wrapping_mul(v[2]))
        >> 15;
    across[2] = turn[0]
        .wrapping_mul(v[1])
        .wrapping_sub(turn[1].wrapping_mul(v[0]))
        >> 15;
    let rest = square.wrapping_sub(0x4000_0000i32 >> ((2 * shift) & 31));
    if rest < 0 {
        return Ok(left);
    }
    let tangent = table_root(rest as u32) as i32;
    let bottom = root.wrapping_add(1);
    for c in turn.iter_mut() {
        *c = divide(c.wrapping_mul(tangent), bottom)?;
    }
    let lower = (square >> 15).wrapping_add(1);
    for c in across.iter_mut() {
        *c = divide(c.wrapping_mul(tangent), lower)?;
    }
    // The axes as rows: component k of `M . w` is the sum over the three vectors.
    let apply = |w: [i32; 3]| -> [i32; 3] {
        std::array::from_fn(|k| {
            multiply(best[0][k], w[0])
                .wrapping_add(multiply(best[1][k], w[1]))
                .wrapping_add(multiply(best[2][k], w[2]))
                .wrapping_mul(2)
        })
    };
    let reduced = |mut w: [i32; 3]| -> (i32, i32) {
        let shift = (extent(w) - 13).max(0);
        if shift > 0 {
            w = w.map(|c| c >> shift);
        }
        let sum = w
            .iter()
            .fold(0i32, |s, c| s.wrapping_add(c.wrapping_mul(*c)));
        (
            (table_root(sum as u32) as i32).wrapping_shl(shift as u32 & 31),
            shift,
        )
    };
    let outer = root.wrapping_shl(shift as u32 & 31);
    let mut local = sky.colour;
    // `ebx` after the branch: the sky distance, or the fraction between the radii.
    let carried;
    if outer > (input.strength >> 16).wrapping_add(0x8000) {
        let half = (input.strength >> 1).wrapping_add(0x4000_0000);
        let mut w = [0i32; 3];
        for k in 0..3 {
            let quotient = divide(v[k].wrapping_shl(15), root)?;
            w[k] = multiply(quotient, half).wrapping_shl(16);
        }
        let moved = apply(w);
        let position: [i32; 3] = std::array::from_fn(|k| moved[k].wrapping_add(origin[k]));
        left.distance = reduced(position).0;
        let sky_distance = reduced(origin).0;
        carried = sky_distance;
        let top = Ext::from_i32(left.scale).mul(Ext::from_i32(sky_distance));
        let bottom = Ext::from_i32(sky_distance.wrapping_sub(left.distance));
        left.scale = top.div(bottom).truncate_low();
        if left.scale < 1 {
            left.scale = 1;
        }
    } else {
        left.distance = 0;
        let fraction = divide(
            outer.wrapping_sub(0x8000).wrapping_shl(15),
            input.strength >> 16,
        )?;
        carried = fraction;
        let keep = 0x8000i32.wrapping_sub(fraction);
        let m = input.material;
        local = [
            (keep.wrapping_mul(i32::from(m[0])).wrapping_mul(3) >> 15) as u8,
            (keep.wrapping_mul(i32::from(m[1])).wrapping_mul(3) >> 15) as u8,
            (keep
                .wrapping_mul(i32::from(m[2]))
                .wrapping_mul(3)
                .wrapping_add(fraction.wrapping_shl(2))
                >> 15) as u8,
        ];
    }
    // The direction to the origin, in 15 bits, for the sky.
    let mut unit = [0i32; 3];
    let bottom_shift = (15 - shift) & 31;
    let denominator = (square >> bottom_shift).wrapping_add(1);
    for k in 0..3 {
        unit[k] = divide(v[k].wrapping_shl(15), denominator)?;
    }
    if left.distance == 0 {
        let w = unit.map(|c| c.wrapping_shl(15));
        let mut direction = apply(w);
        let lit = shade(input.light, &input.shades, &mut direction)?;
        if lit == [0; 3] {
            local = sky.colour;
        } else {
            let keep = 0x8000i32.wrapping_sub(carried);
            for k in 0..3 {
                let mut value = i32::from(lit[k]).wrapping_mul(keep);
                if value < 0 {
                    value = value.wrapping_add(0x3fff);
                }
                local[k] = local[k].wrapping_add((value >> 14) as u8);
                if local[k] > 0xf {
                    local[k] = 0xf;
                }
            }
            sky.flag = -1;
            sky.colour = local;
        }
    }
    // The ring: seven colours from the material to the sky, then 32 steps round.
    queue.open(0xffff_ffff)?;
    let colours: [[u8; 3]; 7] = std::array::from_fn(|k| {
        std::array::from_fn(|c| {
            let m = i32::from(input.material[c]);
            let l = i32::from(local[c]);
            ((m * (6 - k as i32) + l * k as i32) / 6) as u8
        })
    });
    let mut first: Option<[Point; 3]> = None;
    let mut previous = [OFF; 3];
    let mut vector = [0i32; 3];
    // F1913: the table's value shifted down sixteen bits, negated after the shift in
    // the lower half-turn.
    let sin16 = |angle: i32| -> i32 {
        let value = sine.sin32(angle);
        if (angle as u32 >> 14) & 3 >= 2 {
            -(value.wrapping_neg() >> 16)
        } else {
            value >> 16
        }
    };
    for step in 0..32i32 {
        let angle = step * 0x800;
        let (cos, sin) = (sin16(angle + 0x4000), sin16(angle));
        let w: [i32; 3] = std::array::from_fn(|k| {
            cos.wrapping_mul(turn[k])
                .wrapping_add(sin.wrapping_mul(across[k]))
                .wrapping_add(unit[k].wrapping_shl(15))
        });
        vector = apply(w);
        let place = |p: [i32; 3]| project(p).unwrap_or(OFF);
        let at =
            |p: [i32; 3]| -> [i32; 3] { std::array::from_fn(|k| p[k].wrapping_add(origin[k])) };
        let inner = place(at(vector));
        let outer_point = place(at(
            vector.map(|c| multiply(c, input.strength).wrapping_add(c))
        ));
        let third = place(at(vector.map(|c| c.wrapping_sub(c >> 3))));
        let current = [third, inner, outer_point];
        if step == 0 {
            first = Some(current);
        } else {
            segment(
                table,
                queue,
                input.light,
                &input.shades,
                previous,
                current,
                &colours,
                &mut vector,
            )?;
        }
        previous = current;
    }
    if let Some(first) = first {
        segment(
            table,
            queue,
            input.light,
            &input.shades,
            first,
            previous,
            &colours,
            &mut vector,
        )?;
    }
    queue.close()?;
    Ok(left)
}
