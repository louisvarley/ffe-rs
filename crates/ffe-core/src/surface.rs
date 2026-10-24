//! The collision tests against a complex shape (a planet, or a ship made of
//! several spheres), ported from `ccoll.txt` and `ffe.asm`: F576 (the object's
//! own spheres), F579 (move the position into the shape's frame and scale), F580
//! (a child model at a vertex), F581 (the terrain record), F582 (the collision
//! stream) and F583 (a sphere against one of a model's surfaces). The shapes come
//! from the cache: each model's vertices, normals and collision stream.
//! Verified against the executed original (`docs/notes/surface.md`).

use crate::model::multiply;
use crate::model_math::{fraction, length, transform_rows};
use crate::objects::shift64;
use crate::terrain::{self, Terrain};

/// The registers of the collision code the shape tests set (`DATA_008696` and
/// the others around it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Registers {
    /// `DATA_008696`: bit 1 different radii, 2 distance valid, 0x40 landing, 0x80
    /// crash.
    pub flags: u8,
    /// `DATA_008697`: the flags the stream entries collect.
    pub more: u8,
    /// `DATA_008698`: the collision object's index.
    pub object: u8,
    /// `DATA_008701`: the stream entry that was hit (or'd over child models).
    pub chunk: u8,
    /// `DATA_008703:DATA_008704`: the distance.
    pub distance: i64,
    /// `DATA_008705:DATA_008706`: the altitude.
    pub altitude: i64,
    /// `DATA_008707`: 0 out of the planet's radius, 1 inside, 0xff in between.
    pub planet: u8,
}

/// A model's mesh as the collision code reads it.
#[derive(Debug, Clone, Copy)]
pub struct Mesh<'a> {
    /// Six-byte vertex records (`+4`).
    pub vertices: &'a [u8],
    /// Six-byte normal records (`+0xc`), the first being surface 1.
    pub normals: &'a [u8],
    /// The scale (`+0x14`) and the scale offset (`+0x18`).
    pub scale: i32,
    pub offset: i32,
}

impl Mesh<'_> {
    fn zoom(&self) -> i32 {
        self.scale + self.offset
    }

    /// A vertex as bytes: x negated for an odd index.
    fn vertex(&self, index: u8) -> Result<[i32; 3], Fault> {
        let at = usize::from(index >> 1) * 6;
        let record = self.vertices.get(at..at + 6).ok_or(Fault::OutOfRange)?;
        let x = i32::from(record[2] as i8);
        Ok([
            if index & 1 != 0 { -x } else { x },
            i32::from(record[3] as i8),
            i32::from(record[4] as i8),
        ])
    }
}

/// A model for the collision tests: its mesh, its collision stream (`+0x34`) and
/// the region words before the stream (F576 reads them backwards).
#[derive(Debug, Clone, Copy)]
pub struct Shape<'a> {
    pub mesh: Mesh<'a>,
    pub stream: &'a [u8],
    pub back: &'a [u8],
}

/// Where the shapes of child models come from.
pub trait Shapes {
    fn shape(&self, model: u32) -> Option<Shape<'_>>;

    /// F1538's model record (`+0x14` scale, `+0x18` scale offset, `+0x2c` radius
    /// and `+0x30` collision radius words, `+0x38` statistics), which F575's
    /// landing, crash and bounce code reads. `None` where the cache has no
    /// record, which leaves a touch declined.
    fn model(&self, _model: u32) -> Option<crate::objects::ModelInfo> {
        None
    }
}

/// What the tests read of a physics object.
#[derive(Debug, Clone)]
pub struct Hull {
    /// `+0x86`
    pub index: u8,
    /// `+0x56` and `+0x57`
    pub parent: u8,
    pub rotating: u8,
    /// `+0x14c` negative
    pub complex: bool,
    /// The orientation rows (`+0`).
    pub orientation: [i32; 9],
    /// The radii `+0x138` and `+0x140`.
    pub outer: i64,
    pub inner: i64,
    /// `+0xa0`, the terrain seed.
    pub seed: u32,
    /// The animation words from `+0x9c`.
    pub words: Vec<u16>,
}

/// What the complex shapes need: where the models' collision data comes from, the
/// terrain tables and height maps, and the game's detail setting.
pub struct SurfaceTools<'a> {
    pub shapes: &'a dyn Shapes,
    pub terrain_table: &'a [u8],
    pub terrain_maps: &'a [u8],
    pub detail: i32,
}

impl Hull {
    /// A body's fields.
    pub fn from_body(body: &crate::frames::Body) -> Self {
        Self {
            index: body.index,
            parent: body.parent,
            rotating: body.rotating,
            complex: body.shape & 0x80 != 0,
            orientation: body.orientation,
            outer: body.outer_radius,
            inner: body.inner_radius,
            seed: body.seed,
            words: body.words.0.to_vec(),
        }
    }

    /// A physics object's fields (its `0x152` bytes).
    pub fn from_object(object: &[u8]) -> Self {
        let dword = |at: usize| u32::from_le_bytes(object[at..at + 4].try_into().unwrap());
        let qword = |at: usize| i64::from_le_bytes(object[at..at + 8].try_into().unwrap());
        Self {
            index: object[0x86],
            parent: object[0x56],
            rotating: object[0x57],
            complex: object[0x14c] & 0x80 != 0,
            orientation: std::array::from_fn(|k| dword(4 * k) as i32),
            outer: qword(0x138),
            inner: qword(0x140),
            seed: dword(0xa0),
            words: (0x9c..0x152)
                .step_by(2)
                .map(|at| u16::from_le_bytes([object[at], object[at + 1]]))
                .collect(),
        }
    }
}

/// Why a test could not follow the original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    Terrain(terrain::Fault),
    /// The original reads a stack variable it never set.
    Uninitialised,
    /// A zero surface index, which the original reads from before the normals.
    ZeroSurface,
    /// A child model with no shape.
    Missing,
    /// The stream names a surface or vertex the model does not have (the original
    /// reads whatever lies beyond its data).
    OutOfRange,
}

impl From<terrain::Fault> for Fault {
    fn from(fault: terrain::Fault) -> Self {
        Fault::Terrain(fault)
    }
}

/// The mesh with the scaled position the stream is tested against (F579's
/// `CollInfo`).
struct Frame<'a> {
    relpos: [i32; 3],
    mesh: Mesh<'a>,
    scale: i32,
}

/// The scene the tests run in: the terrain engine, the shapes of child models and
/// the game's detail setting.
pub struct Scene<'a, 'b> {
    pub terrain: &'a mut Terrain<'b>,
    pub shapes: &'a dyn Shapes,
    pub detail: i32,
}

fn unit(value: i32, length: i32) -> i32 {
    let magnitude = fraction(value.unsigned_abs(), length as u32);
    let clipped = if (magnitude as i32) < 0 {
        0x7fff_ffff
    } else {
        magnitude as i32
    };
    if value < 0 {
        -clipped
    } else {
        clipped
    }
}

/// F583: whether the sphere of `radius` at the position is outside surface `index`.
fn outside(radius: i32, index: u8, frame: &Frame) -> Result<bool, Fault> {
    let at = (usize::from(index >> 1) * 6)
        .checked_sub(6)
        .ok_or(Fault::OutOfRange)?;
    let normal = frame
        .mesh
        .normals
        .get(at..at + 6)
        .ok_or(Fault::OutOfRange)?;
    let mesh = &frame.mesh;
    let vertex_at = usize::from(normal[0] >> 1) * 6;
    let record = mesh
        .vertices
        .get(vertex_at..vertex_at + 6)
        .ok_or(Fault::OutOfRange)?;
    let odd = index & 1 != 0;
    let side = |byte: u8| i32::from(byte as i8) << 8;
    let direction = [
        if odd {
            -i32::from(normal[2] as i8) << 8
        } else {
            side(normal[2])
        },
        side(normal[3]),
        side(normal[4]),
    ];
    // The vertex's x is negated when the surface's index parity and its vertex
    // reference's parity differ.
    let flip = odd != (normal[0] & 1 != 0);
    let x = i32::from(record[2] as i8);
    let vertex = [
        if flip { -x } else { x },
        i32::from(record[3] as i8),
        i32::from(record[4] as i8),
    ];
    let shift = mesh.zoom() - frame.scale;
    let scaled = vertex.map(|v| {
        if shift < 0 {
            v >> (-shift & 31)
        } else {
            v.wrapping_shl(shift as u32 & 31)
        }
    });
    let offset: [i32; 3] = std::array::from_fn(|k| scaled[k].wrapping_sub(frame.relpos[k]));
    let bits = offset.iter().fold(0u32, |a, v| a | v.wrapping_abs() as u32);
    let msb = 31 - bits.max(1).leading_zeros() as i32;
    let cut = (msb - 13).max(0) as u32;
    let sum = (0..3).fold(0i32, |s, k| {
        s.wrapping_add((offset[k] >> cut).wrapping_mul(direction[k]))
    });
    let plane = sum.wrapping_neg().wrapping_add(sum.wrapping_neg());
    Ok(plane >= radius >> cut)
}

/// F1822: the end of a terrain record's feature list.
fn skip_features(stream: &[u8], mut at: usize) -> usize {
    let word = |at: usize| i16::from_le_bytes([stream[at], stream[at + 1]]);
    if word(at) != 0 {
        at += 16;
        while word(at) != 0 {
            at += 16;
        }
    }
    at + 2
}

impl Scene<'_, '_> {
    /// F581: the terrain record at `at` (after its `0xff` marker); the stream
    /// position after it. `result` is -1 when the ship is clear of the surface and 0
    /// when it touches.
    #[allow(clippy::too_many_arguments)]
    fn terrain_record(
        &mut self,
        regs: &mut Registers,
        result: &mut i32,
        stream: &[u8],
        at: usize,
        planet: &Hull,
        radius: i32,
        frame: &Frame,
    ) -> Result<usize, Fault> {
        let header = &stream[at..at + 6];
        let kind = usize::from(header[4]) | usize::from(header[5]) << 8;
        regs.planet = 0xff;
        let mut corners = [[0; 3]; 4];
        for (corner, index) in corners.iter_mut().zip(&header[..4]) {
            *corner = frame.mesh.vertex(*index)?;
        }
        let shift = frame.mesh.zoom() - frame.scale;
        for corner in &mut corners {
            for v in corner.iter_mut() {
                *v = if shift < 0 {
                    *v >> (-shift & 31)
                } else {
                    v.wrapping_shl(shift as u32 & 31)
                };
            }
        }
        let [a, b, c, d] = corners;
        let from_a = |p: [i32; 3]| -> [i32; 3] { std::array::from_fn(|k| p[k].wrapping_sub(a[k])) };
        let (b, c, d) = (from_a(b), from_a(c), from_a(d));
        let mut a: [i32; 3] = std::array::from_fn(|k| a[k].wrapping_sub(frame.relpos[k]));
        let distance = length(a) as i32;
        let limit = |r: i64| shift64(r, -frame.scale) as i32;
        if distance.wrapping_sub(limit(planet.outer)) >= radius {
            *result = -1;
            regs.planet = 0;
            return Ok(skip_features(stream, at + 6));
        }
        if distance.wrapping_sub(limit(planet.inner)) <= radius {
            *result = 0;
            regs.planet = 1;
            return Ok(skip_features(stream, at + 6));
        }
        // The turn that puts the query point on the z axis.
        let mut m = [0x7fff_ffff, 0, 0, 0, 0x7fff_ffff, 0, 0, 0, 0x7fff_ffff];
        if a[0] != 0 {
            let flat = length([a[0], a[1], 0]) as i32;
            let (x, y) = (unit(a[0], flat), unit(a[1], flat));
            m = [y, x, 0, -x, y, 0, 0, 0, 0x7fff_ffff];
            a[1] = flat;
        }
        if a[1] != 0 {
            let steep = length([0, a[1], a[2]]) as i32;
            let (y, z) = (unit(a[1], steep), unit(a[2], steep));
            m[2] = multiply(m[1], y);
            m[1] = multiply(m[1], z);
            m[5] = multiply(m[4], y);
            m[4] = multiply(m[4], z);
            m[7] = -y;
            m[8] = z;
        }
        let rows = [b, c, d].map(|v| crate::frames::vec_mat_mul(v, &m));
        let matrix: [i32; 9] = std::array::from_fn(|k| rows[k / 3][k % 3]);
        let translation = [0, 0, distance];
        let limit_max = 0x7735_9400i32 >> (frame.scale & 31);
        let found = self.terrain.distance(
            planet.seed,
            limit_max,
            &stream[at + 6..],
            translation,
            matrix,
            kind,
            self.detail,
        )?;
        let surface = found.value;
        *result = if (radius >> 16) > surface { 0 } else { -1 };
        if limit_max > surface {
            regs.distance = shift64(i64::from(surface), frame.scale);
        }
        regs.altitude = regs.distance;
        if *result == 0 {
            regs.distance = 0;
            regs.planet = 1;
        }
        Ok(at + 6 + found.consumed)
    }

    /// F582: the entries of a collision stream against a sphere. Returns 0 for a
    /// collision, -1 for none and 1 when the stream ran out; `chunk` is the entry
    /// that decided.
    fn stream(
        &mut self,
        regs: &mut Registers,
        chunk: &mut u8,
        frame: &Frame,
        stream: &[u8],
        planet: &Hull,
        radius: i32,
    ) -> Result<i32, Fault> {
        let mut at = 0;
        let byte = |at: usize| stream[at];
        let skip_list = |mut at: usize| {
            while stream[at] != 0 {
                at += 1;
            }
            at + 1
        };
        loop {
            let kind = byte(at);
            at += 1;
            if kind == 0 {
                *chunk = 0;
                return Ok(1);
            }
            let flags = byte(at);
            at += 1;
            if kind & 0x20 != 0 {
                let index = usize::from(byte(at));
                let word = planet.words.get(index).copied().unwrap_or(0);
                let control = byte(at + 1);
                at += 2;
                let shifted = u32::from(word) >> (control & 0x1f);
                let flip = u32::from(((control as i8) >> 7) as u8 & 1);
                if (shifted ^ flip) & 1 != 0 {
                    // Disabled: skip the entry.
                    at = match kind & 0xc0 {
                        0 => {
                            let first = byte(at);
                            at += 1;
                            if first == 0xff {
                                skip_features(stream, at + 6)
                            } else if first == 0 {
                                at
                            } else {
                                skip_list(at)
                            }
                        }
                        0x80 => at + 4,
                        _ => skip_list(at),
                    };
                    continue;
                }
            }
            match kind & 0xc0 {
                0 => {
                    let first = byte(at);
                    at += 1;
                    if first == 0xff {
                        let mut result = 0;
                        at = self.terrain_record(
                            regs,
                            &mut result,
                            stream,
                            at,
                            planet,
                            radius,
                            frame,
                        )?;
                        if result == 0 {
                            regs.more |= flags;
                            *chunk = kind;
                            return Ok(0);
                        }
                        regs.more |= flags & 0x7f;
                    } else {
                        let mut index = first;
                        while index != 0 {
                            if outside(radius, index, frame)? {
                                regs.more |= flags & 0x7f;
                                *chunk = kind;
                                return Ok(-1);
                            }
                            index = byte(at);
                            at += 1;
                        }
                    }
                }
                0x40 => loop {
                    let index = byte(at);
                    at += 1;
                    if index == 0 {
                        regs.more |= flags & 0x7f;
                        *chunk = kind;
                        return Ok(-1);
                    }
                    if !outside(radius, index, frame)? {
                        at = skip_list(at);
                        break;
                    }
                },
                0x80 => {
                    let model = u32::from(byte(at)) << 8 | u32::from(byte(at + 1));
                    let orientation = byte(at + 2);
                    let vertex = byte(at + 3);
                    at += 4;
                    let result = self.child(
                        regs,
                        chunk,
                        frame,
                        model,
                        vertex,
                        orientation,
                        radius,
                        planet,
                    )?;
                    regs.chunk |= *chunk;
                    if result < 0 {
                        regs.more |= flags & 0x7f;
                        *chunk = kind;
                        return Ok(-1);
                    }
                    if result == 0 {
                        regs.more |= flags;
                        if flags & 0x80 != 0 {
                            *chunk = kind;
                            return Ok(0);
                        }
                    }
                }
                _ => {
                    let mut index = byte(at);
                    at += 1;
                    loop {
                        if index == 0 {
                            regs.more |= flags;
                            if flags & 0x80 != 0 {
                                *chunk = kind;
                                return Ok(0);
                            }
                            return Err(Fault::ZeroSurface);
                        }
                        if outside(radius, index, frame)? {
                            at = skip_list(at);
                            break;
                        }
                        index = byte(at);
                        at += 1;
                    }
                }
            }
        }
    }

    /// F580: a child model placed at a vertex of the parent mesh, with one of six
    /// axis permutations and sign flips.
    #[allow(clippy::too_many_arguments)]
    fn child(
        &mut self,
        regs: &mut Registers,
        out: &mut u8,
        frame: &Frame,
        model: u32,
        vertex: u8,
        orientation: u8,
        radius: i32,
        planet: &Hull,
    ) -> Result<i32, Fault> {
        let shape = self.shapes.shape(model).ok_or(Fault::Missing)?;
        let mut at = frame.mesh.vertex(vertex)?;
        let shift = frame.mesh.zoom() - frame.scale;
        for v in &mut at {
            if shift < 0 {
                *v >>= -shift & 31;
            } else if shift > 0 {
                *v = v.wrapping_shl(shift as u32 & 31);
            }
        }
        let mut t: [i32; 3] = std::array::from_fn(|k| frame.relpos[k].wrapping_sub(at[k]));
        let relpos = if orientation == 0 {
            t
        } else {
            for (bit, k) in [(0x20, 0), (0x10, 1), (0x08, 2)] {
                if orientation & bit != 0 {
                    t[k] = t[k].wrapping_neg();
                }
            }
            match orientation & 7 {
                0 => t,
                1 => [t[2], t[0], t[1]],
                2 => [t[1], t[2], t[0]],
                3 => [t[2], t[1], t[0]],
                4 => [t[1], t[0], t[2]],
                5 => [t[0], t[2], t[1]],
                _ => return Err(Fault::Uninitialised),
            }
        };
        let inner = Frame {
            relpos,
            mesh: shape.mesh,
            scale: frame.scale,
        };
        self.stream(regs, out, &inner, shape.stream, planet, radius)
    }

    /// F579: the stream of the planet's shape against a sphere at `position`
    /// (64-bit coordinates relative to the planet), `radius` in the position's
    /// units.
    #[allow(clippy::too_many_arguments)]
    fn place(
        &mut self,
        regs: &mut Registers,
        out: &mut u8,
        ship: &Hull,
        planet: &Hull,
        shape: &Shape,
        position: [i64; 3],
        radius: i32,
    ) -> Result<i32, Fault> {
        let mesh = shape.mesh;
        let (cut, shift) = crate::frames::truncate(position, 0x1c);
        let mut scale = shift as i32;
        let mut relpos = cut.map(|v| v as i32);
        let excess = scale - (mesh.zoom() - 0x17);
        if excess < 0 {
            scale -= excess;
            relpos = relpos.map(|v| v >> (-excess & 31));
        }
        let radius = if scale < 0 {
            radius.wrapping_shl((-scale) as u32 & 31)
        } else if scale > 0 {
            radius >> (scale & 31)
        } else {
            radius
        };
        if ship.rotating == 0 || ship.parent != planet.index {
            relpos = transform_rows(relpos, &planet.orientation);
        }
        let frame = Frame {
            relpos,
            mesh,
            scale,
        };
        self.stream(regs, out, &frame, shape.stream, planet, radius)
    }

    /// F576: a ship (`ship`, with its own spheres) against the complex shape
    /// `planet`, `relpos` being the ship's position relative to it. Returns 0 for a
    /// collision and -1 for none, and leaves what it found in `regs`.
    pub fn hit(
        &mut self,
        regs: &mut Registers,
        ship: &Hull,
        ship_shape: &Shape,
        planet: &Hull,
        planet_shape: &Shape,
        relpos: [i64; 3],
    ) -> Result<i32, Fault> {
        regs.more = 0;
        let scale = 0x17 - ship_shape.mesh.scale;
        let mut back = ship_shape.back.len();
        let mut radius = ship.inner as i32;
        let mut position = relpos;
        let mut vector = [0i32; 3];
        loop {
            let mut chunk = 0u8;
            let found = self.place(
                regs,
                &mut chunk,
                ship,
                planet,
                planet_shape,
                position,
                radius,
            )?;
            if found > 0 {
                regs.more |= 0x80;
            }
            if regs.more != 0 {
                regs.object = planet.index;
                regs.flags = regs.more;
                regs.chunk = chunk;
            }
            if found >= 0 {
                return Ok(0);
            }
            if !ship.complex {
                return Ok(-1);
            }
            let word = |at: usize| {
                i32::from(i16::from_le_bytes([
                    ship_shape.back[at],
                    ship_shape.back[at + 1],
                ]))
            };
            back -= 2;
            let distance = word(back);
            if distance == 0 {
                return Ok(-1);
            }
            back -= 2;
            let control = word(back);
            match control & 6 {
                0 => {
                    vector = [
                        ship.orientation[0],
                        ship.orientation[1],
                        ship.orientation[2],
                    ]
                }
                2 => {
                    vector = [
                        ship.orientation[3],
                        ship.orientation[4],
                        ship.orientation[5],
                    ]
                }
                4 => {
                    vector = [
                        ship.orientation[6],
                        ship.orientation[7],
                        ship.orientation[8],
                    ]
                }
                _ => {}
            }
            if control & 1 != 0 {
                vector = vector.map(i32::wrapping_neg);
            }
            vector = vector.map(|v| (v >> 16).wrapping_mul(distance));
            radius = (control & 0xfff8) << 15;
            if scale != 0 {
                vector = vector.map(|v| v >> (scale & 31));
                radius >>= scale & 31;
            }
            position = std::array::from_fn(|k| relpos[k].wrapping_add(i64::from(vector[k])));
        }
    }
}
