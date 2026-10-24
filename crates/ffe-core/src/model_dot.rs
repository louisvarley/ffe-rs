//! The dots a point command draws (F1699, F1700, F1701 and the particle rasteriser F1610/F1611).
//!
//! A point (opcode 1) is a sphere seen from afar: its projected radius decides whether it is a
//! single pixel, a small two-tone dot (F1801: a halo particle and a core particle) or one larger
//! filled circle (F1800). Only the unlit materials (`0x2000`, which stars use) are ported here;
//! a lit point is drawn as a shaded sphere by polygons, which is not.

use crate::model_commands::material;
use crate::model_math::table_root;
use crate::model_thruster::{base_colour, special_divide};

/// A horizontal run of pixels of one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub x: i32,
    pub y: i32,
    pub width: i32,
}

/// The view's size: the particle code clips to 320 by 158 (`0x9e`).
const WIDTH: i32 = 320;
const HEIGHT: i32 = 0x9e;

/// F1600/F1601: a run at row `y`, clipped to the width (F1600) or not (F1601).
fn run(spans: &mut Vec<Span>, x: i32, y: i32, width: i32, clip: bool) {
    let (mut x, mut width) = (x, width);
    if clip {
        if x < 0 {
            if width + x <= 0 {
                return;
            }
            width += x;
            x = 0;
        }
        if x + width > WIDTH {
            width = WIDTH - x;
            if width <= 0 {
                return;
            }
        }
    }
    if width > 0 {
        spans.push(Span { x, y, width });
    }
}

/// F1611: a roughly circular particle of `size` (a diameter) centred on (x, y), as the runs it
/// draws in order. Sizes of 948 or more, and centres outside x 2..=317, y 2..=155 for the six
/// hand-drawn sizes, draw nothing.
pub fn particle(x: i32, y: i32, size: i32) -> Vec<Span> {
    let mut spans = Vec::new();
    if !(0..0x3b4).contains(&size) {
        return spans;
    }
    if size < 6 {
        if !(2..=0x13d).contains(&x) || !(2..=0x9b).contains(&y) {
            return spans;
        }
        let rows: &[(i32, i32, i32)] = match size {
            0 => &[(0, 0, 1)],
            1 => &[(-1, -1, 2), (-1, 0, 2)],
            2 => &[(0, -1, 1), (-1, 0, 3), (0, 1, 1)],
            3 => &[(-1, -1, 3), (-1, 0, 3), (-1, 1, 3)],
            4 => &[(-1, -2, 2), (-2, -1, 4), (-2, 0, 4), (-1, 1, 2)],
            _ => &[(0, -2, 1), (-1, -1, 3), (-2, 0, 5), (-1, 1, 3), (0, 2, 1)],
        };
        for (dx, dy, width) in rows {
            run(&mut spans, x + dx, y + dy, *width, false);
        }
        return spans;
    }
    let r = size / 2;
    let mut rows = vec![(0, 0); 2 * r as usize + 2];
    let mut d = size - 2;
    if d == 5 {
        d += 3;
    }
    let mut d = -d + 6;
    let mut b = r;
    let (mut top, mut p18, mut p1c, mut p14) = (r, r, 2 * r, 0);
    let mut i = 1;
    if b > i {
        loop {
            top -= 1;
            rows[top as usize] = (x - b, 2 * b);
            rows[p18 as usize] = (x - b, 2 * b);
            p18 += 1;
            if d < 0 {
                d += i * 4 + 6;
            } else {
                p1c -= 1;
                rows[p1c as usize] = (x - i, 2 * i);
                rows[p14 as usize] = (x - i, 2 * i);
                p14 += 1;
                d += (i - b) * 4 + 10;
                b -= 1;
            }
            i += 1;
            if b <= i {
                break;
            }
        }
    }
    if b == i {
        top -= 1;
        rows[top as usize] = (x - i, 2 * i);
        rows[p18 as usize] = (x - i, 2 * i);
    }
    let mut at = 0usize;
    let mut row = y - r;
    if row < 0 {
        at = (-row) as usize;
        row = 0;
    }
    let end = (y + r).min(HEIGHT);
    let clip = x - r < 0 || x + r >= WIDTH;
    while row < end {
        let (left, width) = rows.get(at).copied().unwrap_or((0, 0));
        run(&mut spans, left, row, width, clip);
        at += 1;
        row += 1;
    }
    spans
}

/// F1610: [`particle`] unless the whole particle is off the 320 by 158 view (a half-size
/// margin, rounded towards zero, is allowed).
pub fn particle_clipped(x: i16, y: i16, size: i32) -> Vec<Span> {
    let half = size / 2;
    let outside = |value: i16, limit: i32| {
        let value = i32::from(value);
        if (0..limit).contains(&value) {
            false
        } else if value < 0 {
            value + half <= 0
        } else {
            value - half >= limit
        }
    };
    if outside(x, WIDTH) || outside(y, HEIGHT) {
        return Vec::new();
    }
    particle(i32::from(x), i32::from(y), size)
}

/// The size tables F1801 reads: `DATA_007835` (the core's size) and `DATA_007836` (the halo's),
/// by the dot's size minus one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DotTables {
    pub core: [u8; 9],
    pub halo: [u8; 9],
}

pub const TABLE_BYTES: usize = 18;

impl DotTables {
    pub fn new(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != TABLE_BYTES {
            return Err(format!(
                "The dot tables must be {TABLE_BYTES} bytes, not {}.",
                bytes.len()
            ));
        }
        let mut core = [0; 9];
        let mut halo = [0; 9];
        core.copy_from_slice(&bytes[..9]);
        halo.copy_from_slice(&bytes[9..]);
        if core.iter().chain(&halo).any(|size| *size > 16) {
            return Err("A dot size is out of range.".to_owned());
        }
        Ok(Self { core, halo })
    }
}

/// One dot as F1700 queues it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dot {
    /// The screen centre.
    pub x: i16,
    pub y: i16,
    pub shape: DotShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DotShape {
    /// F1800: one particle of `size`.
    One { size: i32, colour: [u8; 3] },
    /// F1801: a halo particle, then a core particle over it.
    Two {
        halo_size: i32,
        halo: [u8; 3],
        core_size: i32,
        core: [u8; 3],
    },
}

impl Dot {
    /// The runs the dot draws, in order, each with the colour it takes.
    pub fn spans(&self) -> Vec<(Span, [u8; 3])> {
        let (x, y) = (self.x, self.y);
        match self.shape {
            DotShape::One { size, colour } => particle_clipped(x, y, size)
                .into_iter()
                .map(|span| (span, colour))
                .collect(),
            DotShape::Two {
                halo_size,
                halo,
                core_size,
                core,
            } => particle_clipped(x, y, halo_size)
                .into_iter()
                .map(|span| (span, halo))
                .chain(
                    particle_clipped(x, y, core_size)
                        .into_iter()
                        .map(|span| (span, core)),
                )
                .collect(),
        }
    }
}

/// What F1700 needs about a point to draw it as a dot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointView {
    /// The projected centre (the slot's screen words).
    pub screen: [i16; 2],
    /// The point's view-space x and y (`+4` and `+8` of the slot).
    pub view: [i32; 2],
    /// The point's view-space depth (`+0xc` of the slot).
    pub depth: i32,
    /// The point's radius in view units (F1474's word shifted by the scale less 8).
    pub radius: i32,
    /// F1691's colour.
    pub colour: [u8; 3],
}

/// F1699's far path then F1700 for an unlit point: the dot, or `None` when it is off the view or
/// too near to draw (a depth under 0x40). A point whose radius is more than an eighth of its
/// depth is drawn by F1699's near path (an ellipse), which is not ported: `Err`.
/// `sky` is the ambient colour the halo is mixed with (`DATA_008815`).
pub fn unlit_dot(
    point: PointView,
    tables: &DotTables,
    sky: [u8; 3],
) -> Result<Option<Dot>, String> {
    let PointView {
        mut screen,
        view,
        depth,
        radius,
        colour,
    } = point;
    if depth < 0x40 {
        return Ok(None);
    }
    let size = if (radius as u32).wrapping_shl(3) > depth as u32 {
        // F1699's near path: the point's extent is a good part of its depth, so it is placed
        // and sized as an ellipse's centre from the exact projection. Nothing is drawn when the
        // radius reaches the depth.
        if radius as u32 >= depth as u32 {
            return Ok(None);
        }
        let mut bits = depth | radius | view[0].wrapping_abs() | view[1].wrapping_abs();
        let mut k = 0u32;
        for (limit, step) in [(0x400_0000, 16), (0x4_0000, 8), (0x4000, 4), (0x1000, 2)] {
            if bits >= limit {
                k += step;
                bits >>= step;
            }
        }
        if bits >= 0x800 {
            k += 1;
        }
        let reduced = (radius as u32 >> k) as i32;
        let depth_k = depth >> k;
        let gap = depth_k
            .wrapping_mul(depth_k)
            .wrapping_sub(reduced.wrapping_mul(reduced));
        if gap == 0 {
            return Ok(None);
        }
        let (x, y) = (view[0] >> k, view[1] >> k);
        let across = |value: i32| depth_k.wrapping_mul(value).wrapping_shl(8) / gap;
        screen = [
            (across(x) as i16).wrapping_add(0xa0),
            0x4f_i16.wrapping_sub(across(y) as i16),
        ];
        let root = table_root(
            x.wrapping_mul(x)
                .wrapping_add(y.wrapping_mul(y))
                .wrapping_add(gap) as u32,
        );
        let product = (root as i32).wrapping_mul(reduced.wrapping_shl(8)) as u32;
        let size = (product / gap as u32) as i32;
        if size < 0 {
            return Err("A negative dot size is not ported.".to_owned());
        }
        size
    } else {
        special_divide(radius, depth)?
    };
    // F1700's clip: the dot must reach the view.
    let [x, y] = screen.map(i32::from);
    let half = size / 2;
    let reaches = |value: i32, limit: i32| {
        if (0..limit).contains(&value) {
            true
        } else if value < 0 {
            let moved = value + half;
            (0..limit).contains(&moved)
        } else {
            let moved = value - half;
            (0..limit).contains(&moved)
        }
    };
    if !reaches(x, 0x140) || !reaches(y, HEIGHT) {
        return Ok(None);
    }
    let (x, y) = (screen[0], screen[1]);
    if size == 0 || size > 9 {
        // F1701: one particle, in the colour F1691 gave.
        return Ok(Some(Dot {
            x,
            y,
            shape: DotShape::One { size, colour },
        }));
    }
    // F1801: a halo and a core; the halo is the colour mixed a third towards the sky.
    if !(0..0x140).contains(&i32::from(x)) || !(0..HEIGHT).contains(&i32::from(y)) {
        return Ok(None);
    }
    let index = (size - 1) as usize;
    let halo =
        std::array::from_fn(|k| ((u32::from(colour[k]) * 2 + u32::from(sky[k]) + 1) / 3) as u8);
    Ok(Some(Dot {
        x,
        y,
        shape: DotShape::Two {
            halo_size: i32::from(tables.halo[index]),
            halo,
            core_size: i32::from(tables.core[index]),
            core: colour,
        },
    }))
}

/// F1699's radius: the point's angle word (F1474) shifted by the instance's scale less 8
/// (logically right for a negative shift, as the original's `shr`).
pub fn point_radius(angle: i32, scale: i32) -> i32 {
    let shift = scale.wrapping_sub(8);
    if shift < 0 {
        ((angle as u32) >> (shift.wrapping_neg() as u32 & 31)) as i32
    } else {
        angle.wrapping_shl(shift as u32 & 31)
    }
}

/// A point command as a dot: its material (an unlit one), the F1474 angle value, the instance's
/// scale and colour, and where the point projects. Lit points are shaded spheres drawn by
/// polygons, which are not ported (`Err`).
#[allow(clippy::too_many_arguments)]
pub fn point_dot(
    material_word: u16,
    angle: i32,
    scale: i32,
    instance: [u8; 3],
    screen: [i16; 2],
    view: [i32; 2],
    depth: i32,
    tables: &DotTables,
    sky: [u8; 3],
) -> Result<Option<Dot>, String> {
    if !material(material_word).unlit {
        return Err("A lit point is a shaded sphere, which is not ported.".to_owned());
    }
    unlit_dot(
        PointView {
            screen,
            view,
            depth,
            radius: point_radius(angle, scale),
            colour: base_colour(material_word, instance),
        },
        tables,
        sky,
    )
}
