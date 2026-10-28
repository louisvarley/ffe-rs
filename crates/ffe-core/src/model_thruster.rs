//! Thruster glows (opcode 9, F1738) and the line queueing they share with F1704.
//!
//! F1738 resolves vertex A (F1471, projected) and, only when A lies deeper than
//! z = 0x200, vertex B; nothing is queued unless both are deeper than 0x200. The
//! record's signed width word is shifted by the instance scale (+0x148, `<<` or
//! arithmetic `>>` by `scale - 6`, count masked to 5 bits as x86 does) and divided
//! by A's depth (F1666: `(width << 8) / z`, both halved while z > 0x7fff). A zero
//! width queues nothing; a width of at most 4 (negative included) is a line queued
//! by F1704; a wider one queues one or two textured quads drawn by F1805:
//!
//! - **core** (texture word 0x123): when the screen length `L` (F1467 of
//!   `dx² + dy²`, A minus B) is below the width, a square of half-size
//!   `width >> 2` around the point one third of the way from A to B;
//! - **shaft** (texture word 0x65): unless `8 L` is below the width, a quad
//!   along A-B, `width >> 1` wide in total, both ends offset by the normal
//!   `(ny, -nx) * width >> 17` with `n = (B - A) << 15 / L` truncated to 16 bits.
//!
//! Both are keyed by F1777 of A's view and carry six colours in slots 1..6 of the
//! F1805 slot table: the material colour (F1692/F1694, *without* F1691's texture
//! addition) scaled by `(i + 1) / 7`. Slot 0 is never written by F1738.
//! All arithmetic follows the original's 16- and 32-bit wrap-around.
use crate::model_commands::material;
use crate::model_depth;
use crate::model_instance::Vertex;
use crate::model_math::table_root;

/// F1805's texture word for the core square (bitmap 0x123, orientation 0).
pub const CORE_TEXTURE: u32 = 0x123;
/// F1805's texture word for the shaft quad (bitmap 0x65, orientation 0).
pub const SHAFT_TEXTURE: u32 = 0x65;
/// Both vertices must be deeper than this for F1738 to queue anything.
pub const MIN_DEPTH: i32 = 0x200;

/// One queued thruster entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    /// An F1803 line entry queued by F1704: the stored x (`x - 1` when positive,
    /// else 0) and y of each end, after F1705's clipping, and the F1691 colour
    /// before F1326 (see [`line_colour`] for textured materials).
    Line {
        points: [[i32; 2]; 2],
        colour: [u8; 3],
    },
    /// An F1805 textured quad: four screen points (words 1..4 of the entry), the
    /// texture word ([`CORE_TEXTURE`] or [`SHAFT_TEXTURE`]) and colour slots 1..6.
    Glow {
        points: [[i16; 2]; 4],
        texture: u32,
        colours: [[u8; 3]; 6],
    },
}

/// A queued thruster primitive and the F1777 key it was queued with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The opcode 9 command's offset.
    pub offset: usize,
    /// The record's material word (for F1691's texture addition to a line).
    pub material: u16,
    pub key: u32,
    pub shape: Shape,
    /// Not the original's: A's and B's view positions, for a renderer that draws the entry
    /// itself (the port's modern engine).
    pub views: [[i32; 3]; 2],
}

/// F1666 (`SpecialDiv`): `(value << 8) / divisor`, with both halved (arithmetic)
/// while a positive divisor exceeds 0x7fff; a zero divisor returns `value`.
pub fn special_divide(value: i32, divisor: i32) -> Result<i32, String> {
    let (mut value, mut divisor) = (value, divisor);
    if divisor > 0 {
        while divisor > 0x7fff {
            divisor >>= 1;
            value >>= 1;
        }
    }
    if divisor == 0 {
        return Ok(value);
    }
    value
        .wrapping_shl(8)
        .checked_div(divisor)
        .ok_or_else(|| "F1666 division overflows (the original faults).".to_owned())
}

/// The width F1738 uses: the record's word shifted by `scale - 6` (x86 count
/// masking) and divided by A's depth through F1666.
pub fn scaled_width(width: i16, scale: i32, depth: i32) -> Result<i32, String> {
    let shift = scale.wrapping_sub(6);
    let shifted = if shift < 0 {
        i32::from(width) >> (shift.wrapping_neg() as u32 & 31)
    } else {
        i32::from(width).wrapping_shl(shift as u32 & 31)
    };
    special_divide(shifted, depth)
}

/// F1692/F1694: the colour F1738 starts from: material nibbles (zero when
/// textured) plus the instance colour (+0xf4) for local materials, bytes wrapping.
pub fn base_colour(value: u16, instance: [u8; 3]) -> [u8; 3] {
    let decoded = material(value);
    if decoded.local_colour {
        std::array::from_fn(|i| decoded.rgb[i].wrapping_add(instance[i]))
    } else {
        decoded.rgb
    }
}

/// The F1805 colour ramp: slot `i` (1..=6) is `base * (i + 1) / 7` per component.
pub fn ramp(base: [u8; 3]) -> [[u8; 3]; 6] {
    std::array::from_fn(|slot| base.map(|c| (u32::from(c) * (slot as u32 + 2) / 7) as u8))
}

/// F1691 for the narrow-line case: [`base_colour`] plus, for a textured material,
/// the first colour of its texture descriptor (record +8..+10), bytes wrapping.
/// F1738 hands the line this colour; the queued [`Shape::Line`] holds the base
/// colour because the executor has no descriptor table.
pub fn line_colour(
    value: u16,
    base: [u8; 3],
    descriptors: &crate::model_texture::Descriptors,
) -> Result<[u8; 3], String> {
    let texture = material(value).texture;
    if texture == 0 {
        return Ok(base);
    }
    let first = descriptors
        .get(texture)?
        .colours
        .first()
        .copied()
        .unwrap_or([0; 3]);
    Ok(std::array::from_fn(|i| base[i].wrapping_add(first[i])))
}

const SCREEN_WIDTH: i32 = 0x140;
const SCREEN_HEIGHT: i32 = 0x9e;
const NEAR: i32 = 0x40;

fn on_screen([x, y]: [i16; 2]) -> bool {
    (i32::from(x) as u32) < SCREEN_WIDTH as u32 && (i32::from(y) as u32) < SCREEN_HEIGHT as u32
}

/// F1562 (axis 0, limit 0x13f) / F1563 (axis 1, limit 0x9d): move `point` along
/// the line to `other` onto the screen edge it lies beyond on that axis.
pub(crate) fn clip_axis(
    point: &mut [i16; 2],
    other: [i16; 2],
    axis: usize,
    limit: i32,
) -> Result<(), String> {
    let across = 1 - axis;
    let value = i32::from(point[axis]);
    if value as u32 <= limit as u32 {
        return Ok(());
    }
    let target = if point[axis] < 0 {
        if other[axis] < 0 {
            i32::from(other[axis])
        } else {
            0
        }
    } else if limit < i32::from(other[axis]) {
        i32::from(other[axis])
    } else {
        limit
    };
    if point[axis] != other[axis] {
        let product = i32::from(point[across])
            .wrapping_sub(i32::from(other[across]))
            .wrapping_mul(target.wrapping_sub(i32::from(other[axis])));
        let quotient = product
            .checked_div(value.wrapping_sub(i32::from(other[axis])))
            .ok_or_else(|| "F1562/F1563 division overflows (the original faults).".to_owned())?;
        point[across] = (quotient as i16).wrapping_add(other[across]);
    }
    point[axis] = target as i16;
    Ok(())
}

/// F1561: F1562 then F1563 (x onto 0..=0x13f, then y onto 0..=0x9d), each
/// moving `point` along the line to `other`. Not a rejection test: the point
/// can still be off screen afterwards.
pub fn clip_toward(point: &mut [i16; 2], other: [i16; 2]) -> Result<(), String> {
    clip_axis(point, other, 0, 0x13f)?;
    clip_axis(point, other, 1, 0x9d)
}

/// F1705: bring `point` (the screen of `this`) onto the screen along the line to
/// `other`. Behind the near plane (z < 0x40) it first takes F1564's crossing
/// (both behind: rejected). Then F1561 clips x, then y; rejected (`true`) unless
/// the result is on screen.
fn clip_end(point: &mut [i16; 2], this: &Vertex, other: &Vertex) -> Result<bool, String> {
    if this.view[2] < NEAR {
        if other.view[2] < NEAR {
            return Ok(true);
        }
        *point = crate::model_polygon::near_point(this.view, other.view);
    }
    clip_axis(point, other.screen, 0, 0x13f)?;
    clip_axis(point, other.screen, 1, 0x9d)?;
    Ok(!on_screen(*point))
}

/// F1715 (x, limit 0x140) / F1716 (y, limit 0x9e) outcodes.
fn outcode(value: i16, limit: i32) -> u32 {
    let value = i32::from(value);
    if value == -32766 {
        8
    } else if (value as u32) < limit as u32 {
        2
    } else if value < 0 {
        1
    } else {
        4
    }
}

/// F1704: the screen points of the F1803 line entry for `a`-`b` (stored x, y),
/// or `None` when the original queues nothing. Ends off screen are clipped
/// (F1705); two off-screen ends are first rejected when their x or y outcodes
/// combine to all-left, all-right or all-behind (`DATA_007818`).
pub fn line(a: &Vertex, b: &Vertex) -> Result<Option<[[i32; 2]; 2]>, String> {
    let (mut pa, mut pb) = (a.screen, b.screen);
    let queued = if on_screen(pa) {
        on_screen(pb) || !clip_end(&mut pb, b, a)?
    } else if on_screen(pb) {
        !clip_end(&mut pa, a, b)?
    } else {
        let rejected = |combined: u32| matches!(combined, 1 | 4 | 8);
        !(rejected(outcode(pa[0], SCREEN_WIDTH) | outcode(pb[0], SCREEN_WIDTH))
            || rejected(outcode(pa[1], SCREEN_HEIGHT) | outcode(pb[1], SCREEN_HEIGHT))
            || clip_end(&mut pa, a, b)?
            || clip_end(&mut pb, b, a)?)
    };
    let stored = |[x, y]: [i16; 2]| [if x > 0 { i32::from(x) - 1 } else { 0 }, i32::from(y)];
    Ok(queued.then(|| [stored(pa), stored(pb)]))
}

/// The four points of the core square: `(2A + B) / 3` plus or minus `width >> 2`
/// (and `-width >> 2`), in the original's 16-bit arithmetic.
fn core_points(a: [i16; 2], b: [i16; 2], width: i32) -> [[i16; 2]; 4] {
    let third = |axis: usize| ((i32::from(b[axis]) + 2 * i32::from(a[axis])) / 3) as i16;
    let (mx, my) = (third(0), third(1));
    let quarter = (width >> 2) as i16;
    let negative = (width.wrapping_neg() >> 2) as i16;
    [
        [mx.wrapping_add(negative), my.wrapping_add(quarter)],
        [mx.wrapping_add(quarter), my.wrapping_add(quarter)],
        [mx.wrapping_sub(negative), my.wrapping_sub(quarter)],
        [mx.wrapping_sub(quarter), my.wrapping_sub(quarter)],
    ]
}

/// The four points of the shaft quad: A + p, B + p, B - p, A - p.
fn shaft_points(
    a: [i16; 2],
    b: [i16; 2],
    dx: i32,
    dy: i32,
    length: i32,
    width: i32,
) -> [[i16; 2]; 4] {
    // `length` is at least width / 8 > 0 here, and |dx << 15| < 2^31.
    let nx = (dx.wrapping_shl(15) / length) as i16;
    let ny = (dy.wrapping_shl(15) / length) as i16;
    let px = (i32::from(ny).wrapping_mul(width) >> 17) as i16;
    let py = (i32::from(nx).wrapping_mul(width).wrapping_neg() >> 17) as i16;
    let add = |p: [i16; 2]| [p[0].wrapping_add(px), p[1].wrapping_add(py)];
    let sub = |p: [i16; 2]| [p[0].wrapping_sub(px), p[1].wrapping_sub(py)];
    [add(a), add(b), sub(b), sub(a)]
}

/// The inputs F1738 reads besides its record.
#[derive(Debug, Clone, Copy)]
pub struct Instance {
    /// +0x148, the scale the width is shifted by.
    pub scale: i32,
    /// +0x130, F1777's key scale.
    pub key_scale: i32,
    /// +0xf4..+0xf6, added for local materials.
    pub colour: [u8; 3],
}

/// What F1738 did with one thruster.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Queued {
    /// The scaled width, when both vertices were deep enough to compute it. A
    /// width in `..=4` other than zero took the F1704 line path, queued or not.
    pub width: Option<i32>,
    /// The entries queued, in queue order.
    pub entries: Vec<Entry>,
}

/// F1738 after its vertices are resolved: the scaled width and the entries it
/// queues. `b` is `None` when A is not deeper than 0x200 (the original does not
/// resolve B then).
pub fn entries(
    thruster: &crate::model_commands::Thruster,
    a: &Vertex,
    b: Option<&Vertex>,
    instance: Instance,
) -> Result<Queued, String> {
    let Some(b) = b.filter(|b| a.view[2] > MIN_DEPTH && b.view[2] > MIN_DEPTH) else {
        return Ok(Queued::default());
    };
    let width = scaled_width(thruster.width, instance.scale, a.view[2])?;
    let queued = |entries| Queued {
        width: Some(width),
        entries,
    };
    if width == 0 {
        return Ok(queued(Vec::new()));
    }
    let base = base_colour(thruster.material, instance.colour);
    let key = model_depth::key(a.view, instance.key_scale);
    let entry = |shape| Entry {
        offset: thruster.offset,
        material: thruster.material,
        key,
        shape,
        views: [a.view, b.view],
    };
    if width <= 4 {
        return Ok(queued(
            line(a, b)?
                .map(|points| {
                    entry(Shape::Line {
                        points,
                        colour: base,
                    })
                })
                .into_iter()
                .collect(),
        ));
    }
    let dx = i32::from(a.screen[0]) - i32::from(b.screen[0]);
    let dy = i32::from(a.screen[1]) - i32::from(b.screen[1]);
    let length = table_root(dx.wrapping_mul(dx).wrapping_add(dy.wrapping_mul(dy)) as u32) as i32;
    let colours = ramp(base);
    let mut glows = Vec::new();
    if length < width {
        glows.push(entry(Shape::Glow {
            points: core_points(a.screen, b.screen, width),
            texture: CORE_TEXTURE,
            colours,
        }));
        if length.wrapping_shl(3) < width {
            return Ok(queued(glows));
        }
    }
    glows.push(entry(Shape::Glow {
        points: shaft_points(a.screen, b.screen, dx, dy, length, width),
        texture: SHAFT_TEXTURE,
        colours,
    }));
    Ok(queued(glows))
}
