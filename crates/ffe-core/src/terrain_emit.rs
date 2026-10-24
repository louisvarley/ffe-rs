//! The planet surface's emitter, ported from `ffe.asm`: F1841 (the colour tables of a
//! planet type), F1840 (a triangle of the subdivision into queue entries for the
//! flat, textured and explicit-coordinate draw routines F1804 to F1808), with F1837
//! (a point where an edge crosses sea level), F1838 and F1839 (colour slots), F1778
//! (the order queue) and F1326 (a nibble colour packed to 12 bits). Verified against
//! the executed original (`docs/notes/terrain.md`).

use crate::model::multiply;
use crate::model_instance::project;
use crate::model_math::fraction;
use crate::terrain::{tables, Fault, Terrain};
use crate::terrain_draw::{Drawn, Emission, Scene};

/// The words the order queue may hold before the original exits (F1778).
pub const QUEUE_WORDS: usize = 0xc350;

/// The queue entries' draw routines, as the harness numbers them.
pub const FLAT_TRIANGLE: u32 = 0xf180_0004;
pub const TEXTURE_QUAD: u32 = 0xf180_0005;
pub const DETAIL_QUAD: u32 = 0xf180_0006;
pub const TEXTURE_TRIANGLE: u32 = 0xf180_0007;
pub const DETAIL_TRIANGLE: u32 = 0xf180_0008;

/// F1778 for a queue that is not in a group: every entry is a header (its depth key
/// and the position of the previous header) and the words the draw routine reads.
#[derive(Debug, Default, Clone)]
pub struct Queue {
    pub words: Vec<u32>,
    /// The last header's position (`*DATA_007832`), zero before any.
    head: u32,
}

impl Queue {
    /// The entries from the last queued back to the first: `(key, header position)`.
    pub fn headers(&self) -> Vec<(u32, usize)> {
        let mut out = Vec::new();
        let mut link = self.head;
        while link != 0 && out.len() < self.words.len() {
            let at = (link & 0x00ff_ffff) as usize;
            out.push((self.words[at], at));
            link = self.words[at + 1];
        }
        out
    }

    /// A new entry of `count` words after its header; returns where the words start.
    pub fn entry(&mut self, key: u32, count: usize) -> Result<usize, Fault> {
        let header = self.words.len();
        self.words.push(key);
        self.words.push(self.head);
        self.head = 0xf200_0000 | header as u32;
        let start = self.words.len();
        self.words.resize(start + count, 0);
        if self.words.len() > QUEUE_WORDS {
            return Err(Fault::QueueFull);
        }
        Ok(start)
    }
}

/// What the emitter reads of the instance and the game.
#[derive(Debug, Clone, Copy)]
pub struct Shading {
    /// The instance's `+0x134`: the light direction in model space.
    pub light: [i32; 3],
    /// The instance's `+0x84`: the eight shade colours.
    pub shades: [[u8; 3]; 8],
    /// `DATA_008812`.
    pub detail: i32,
    /// What F1877 leaves: `DATA_009262` non-zero with `DATA_009263`, `DATA_009265`
    /// and `DATA_009267`.
    pub atmosphere: Option<Atmosphere>,
    /// `DATA_009259`: the surface colour of the record's material (F1692, F1691).
    pub material: [u8; 3],
}

/// `DATA_009262..9267` as F1877 sets them for a planet with a haze.
#[derive(Debug, Clone, Copy)]
pub struct Atmosphere {
    pub offset: i32,
    pub colour: u32,
    pub divisor: i32,
}

/// The colour tables F1841 builds from a planet type's palette (`DATA_007846` as four
/// rows of eight dwords, `DATA_007848` 22 dwords and `DATA_007849` 21 rows of seven).
#[derive(Debug, Clone)]
pub struct Colours {
    pub sea: [[u32; 8]; 4],
    pub steps: [u32; 22],
    pub land: [[u32; 7]; 21],
    /// `DATA_007852` (F1843, the polar types): 32 dwords.
    pub bands: [u32; 32],
}

impl Colours {
    /// F1841: `palette` is the type's table entry (48 bytes), `ramps` the exe's
    /// `DATA_007850` (22 groups of four dwords) and `blends` its `DATA_007851` (21
    /// groups of seven by four dwords).
    pub fn build(entry: &[u8], ramps: &[u8], blends: &[u8], bands: &[u8]) -> Self {
        let dword =
            |bytes: &[u8], at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
        let colour = |k: usize| dword(entry, 0xc + 4 * k);
        let mix = |bytes: &[u8], at: usize| {
            let first = colour(dword(bytes, at) as usize).wrapping_mul(dword(bytes, at + 4));
            let second = colour(dword(bytes, at + 8) as usize).wrapping_mul(dword(bytes, at + 12));
            (first.wrapping_add(second) as i32 >> 4) as u32 & 0xf0f0f
        };
        let mut steps = [0u32; 22];
        for (k, step) in steps.iter_mut().enumerate() {
            *step = mix(ramps, 16 * k);
        }
        let mut land = [[0u32; 7]; 21];
        for (i, row) in land.iter_mut().enumerate() {
            for (j, slot) in row.iter_mut().enumerate() {
                *slot = mix(blends, 112 * i + 16 * j);
            }
        }
        let mut sea = [[0u32; 8]; 4];
        for (i, row) in sea.iter_mut().enumerate() {
            let t = (4 * i as u32 + 1) / 3;
            let (a, b) = (4 - t, t);
            let low = (colour(0)
                .wrapping_mul(a)
                .wrapping_add(colour(1).wrapping_mul(b))
                >> 2)
                & 0xf0f0f;
            let high = (colour(2)
                .wrapping_mul(a)
                .wrapping_add(colour(3).wrapping_mul(b))
                >> 2)
                & 0xf0f0f;
            for (j, slot) in row.iter_mut().take(7).enumerate() {
                let w = (8 * j as u32 + 3) / 6;
                *slot = (low.wrapping_mul(8 - w).wrapping_add(high.wrapping_mul(w)) >> 3) & 0xf0f0f;
            }
        }
        let mut table = [0u32; 32];
        for (k, band) in table.iter_mut().enumerate() {
            *band = mix(bands, 16 * k);
        }
        Self {
            sea,
            steps,
            land,
            bands: table,
        }
    }
}

/// F1326 on a packed colour dword (red in the low byte): `r << 8 + g << 4 + b`.
fn pack(colour: u32) -> u16 {
    let [r, g, b, _] = colour.to_le_bytes();
    (u16::from(r) << 8)
        .wrapping_add(u16::from(g) << 4)
        .wrapping_add(u16::from(b))
}

/// The base colour and the haze blend of F1838/F1839.
struct Blend {
    mix: i32,
    base: u32,
    colour: u32,
}

impl Blend {
    fn apply(&self, table: u32) -> u16 {
        if self.mix != 0 {
            let combined = (self.base << 3).wrapping_add(self.colour.wrapping_mul(self.mix as u32));
            let value = (table as i32).wrapping_mul(8 - self.mix);
            pack(((value.wrapping_add(combined as i32) >> 3) as u32) & 0xf0f0f)
        } else {
            pack(table.wrapping_add(self.base))
        }
    }
}

/// F1838: seven colour slots, one dword each.
fn slots(queue: &mut Queue, at: usize, blend: &Blend, table: &[u32]) {
    for (k, entry) in table.iter().take(7).enumerate() {
        queue.words[at + k] = u32::from(blend.apply(*entry));
    }
}

/// F1839: `count` colours as halfword pairs.
fn pairs(queue: &mut Queue, at: usize, blend: &Blend, table: &[u32], count: usize) {
    for k in 0..count / 2 {
        let low = blend.apply(table[2 * k]);
        let high = blend.apply(table[2 * k + 1]);
        queue.words[at + k] = u32::from(low) | u32::from(high) << 16;
    }
}

/// F1837: the screen position (a dword of two words) where the edge from `x` to
/// `y` crosses sea level.
fn crossing(t: &Terrain<'_>, x: usize, y: usize) -> u32 {
    let (nx, ny) = (t.node(x), t.node(y));
    let height = ny.height();
    let step = height >> 3;
    let mut near = fraction(height as u32, height.wrapping_sub(nx.height()) as u32) as i32;
    if near < 0 {
        near = 0;
    }
    let far = 0x7fff_ffff - near;
    let lifted =
        |at: usize, vector: usize| ny.dword(at).wrapping_sub(multiply(ny.dword(vector), step));
    let point = [
        multiply(nx.dword(4), far).wrapping_add(multiply(lifted(4, 0x28), near)),
        multiply(nx.dword(8), far).wrapping_add(multiply(lifted(8, 0x2c), near)),
        multiply(nx.dword(0xc), far).wrapping_add(multiply(lifted(0xc, 0x30), near)),
    ];
    match project(point) {
        Some([a, b]) => u32::from(a as u16) | u32::from(b as u16) << 16,
        None => 0,
    }
}

fn screen(t: &Terrain<'_>, node: usize) -> u32 {
    let n = t.node(node);
    u32::from(n.word(0) as u16) | u32::from(n.word(2) as u16) << 16
}

/// `sar 0x12` of a height into a word, clamped to 0x1ff.
fn band(height: i32) -> u16 {
    let value = (height >> 18) as i16;
    value.min(0x1ff) as u16
}

/// The base colour the light gives a triangle (the opening of F1840, F1842 and
/// F1844).
fn lit(shading: &Shading, vector: [i32; 3]) -> u32 {
    let dot = shading
        .light
        .iter()
        .zip(vector)
        .fold(0i32, |sum, (l, v)| sum.wrapping_add(l.wrapping_mul(v)))
        .wrapping_neg();
    if dot < 0 {
        let mut index = (dot as u32) >> 27;
        if index < 0x18 {
            index = 0;
        }
        let [r, g, b] = shading.shades[(index & 7) as usize];
        u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16
    } else {
        0
    }
}

/// F1844 and F1845 (planet types 8 and 9): one flat triangle in the material's
/// colour, lit for F1844.
fn plain(
    t: &Terrain<'_>,
    shading: &Shading,
    queue: &mut Queue,
    emission: &Emission,
    shaded: bool,
) -> Result<(), Fault> {
    let mut colour = if shaded {
        lit(shading, emission.normal)
    } else {
        0
    };
    let [r, g, b] = shading.material;
    colour = colour.wrapping_add(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16);
    let at = queue.entry(emission.key, 5)?;
    let [a, b, c] = emission.nodes;
    queue.words[at..at + 5].copy_from_slice(&[
        FLAT_TRIANGLE,
        screen(t, a),
        screen(t, b),
        screen(t, c),
        u32::from(pack(colour)),
    ]);
    Ok(())
}

/// F1842 (planet types 3, 5 and 6): one explicit-coordinate triangle across the
/// 32 band colours.
fn banded(
    t: &Terrain<'_>,
    shading: &Shading,
    colours: &Colours,
    queue: &mut Queue,
    emission: &Emission,
) -> Result<(), Fault> {
    let base = lit(shading, emission.normal);
    let mut nodes = emission.nodes;
    let height = |n: usize| t.node(n).height();
    if height(nodes[1]) > height(nodes[0]) {
        nodes.swap(0, 1);
    }
    if height(nodes[2]) > height(nodes[0]) {
        nodes.swap(0, 2);
    }
    if height(nodes[1]) < height(nodes[2]) {
        nodes.swap(1, 2);
    }
    let [a, b, c] = nodes;
    let at = queue.entry(emission.key, 0x19)?;
    queue.words[at..at + 4].copy_from_slice(&[
        DETAIL_TRIANGLE,
        screen(t, a),
        screen(t, b),
        screen(t, c),
    ]);
    let (mut first, second, mut third) = (
        (height(a) >> 18) as i16,
        (height(b) >> 18) as i16,
        (height(c) >> 18) as i16,
    );
    if i32::from(first) - i32::from(third) < 0x10 {
        third = third.wrapping_sub(8);
        if third < 0 {
            third = 0;
            first = 0x10;
        } else {
            first = third.wrapping_add(0x10);
            if first > 0x1ff {
                first = 0x1ff;
                third = 0x1ef;
            }
        }
    }
    let words = [
        0,
        first as u16,
        0x3f,
        second as u16,
        0,
        third as u16,
        0x5f,
        0,
    ];
    for k in 0..4 {
        queue.words[at + 4 + k] = u32::from(words[2 * k]) | u32::from(words[2 * k + 1]) << 16;
    }
    queue.words[at + 8] = 0x20;
    let blend = Blend {
        mix: 0,
        base,
        colour: 0,
    };
    pairs(queue, at + 9, &blend, &colours.bands, 0x20);
    Ok(())
}

/// The emitter of the planet type (`DATA_007843`'s first dword, as a rank).
pub fn emit(
    t: &Terrain<'_>,
    kind: usize,
    shading: &Shading,
    colours: &Colours,
    queue: &mut Queue,
    emission: &Emission,
) -> Result<(), Fault> {
    match t.type_dword(kind, 0) {
        2 => banded(t, shading, colours, queue, emission),
        3 => plain(t, shading, queue, emission, true),
        4 => plain(t, shading, queue, emission, false),
        _ => surface(t, shading, colours, queue, emission),
    }
}

/// F1840: queue the triangle.
fn surface(
    t: &Terrain<'_>,
    shading: &Shading,
    colours: &Colours,
    queue: &mut Queue,
    emission: &Emission,
) -> Result<(), Fault> {
    let key = emission.key;
    let vector = emission.normal;
    // The light on the triangle picks a base colour.
    let base = lit(shading, vector);
    // The nodes from the highest down.
    let mut nodes = emission.nodes;
    let height = |n: usize| t.node(n).height();
    if height(nodes[1]) > height(nodes[0]) {
        nodes.swap(0, 1);
    }
    if height(nodes[2]) > height(nodes[0]) {
        nodes.swap(0, 2);
    }
    if height(nodes[1]) < height(nodes[2]) {
        nodes.swap(1, 2);
    }
    let [a, b, c] = nodes;
    let mut mix = 0i32;
    if let Some(haze) = shading.atmosphere {
        let node = t.node(a);
        let mut distance = (node
            .x()
            .wrapping_abs()
            .wrapping_add(node.y().wrapping_abs())
            >> 3)
            .wrapping_add(node.z());
        distance = if haze.offset != 0 {
            distance.wrapping_sub(haze.offset)
        } else {
            distance
        };
        if haze.divisor == 0 {
            return Err(Fault::Division);
        }
        mix = distance.wrapping_div(haze.divisor).clamp(0, 8);
    }
    let blend = Blend {
        mix,
        base,
        colour: shading.atmosphere.map_or(0, |haze| haze.colour),
    };
    let below = nodes.iter().filter(|n| height(**n) <= 0).count();
    let flat = shading.detail < 0
        || (key > 0x1fff_ffff && below == 3)
        || (key > 0x7fff_ffff && height(a).wrapping_sub(height(c)) < 0x1fff_ffff);
    let (sa, sb, sc) = (screen(t, a), screen(t, b), screen(t, c));
    if flat {
        let index = {
            let t18 = height(a) >> 18;
            let v = t18.wrapping_mul(7).wrapping_mul(3);
            (v + if v < 0 { 0x1ff } else { 0 }) >> 9
        };
        let mut colour = base;
        let table = if height(a) > 0 {
            colours.land[index.min(0x14) as usize][0]
        } else {
            // At or below the sea the index is not positive: the rows start at three.
            colours.sea[(index + 3).clamp(0, 3) as usize][0]
        };
        if mix != 0 {
            let combined = (table as i32).wrapping_mul(8 - mix);
            let haze = blend.colour.wrapping_mul(mix as u32) as i32;
            colour = colour.wrapping_add(((combined.wrapping_add(haze) >> 3) as u32) & 0xf0f0f);
        } else {
            colour = colour.wrapping_add(table);
        }
        let at = queue.entry(key, 5)?;
        queue.words[at..at + 5].copy_from_slice(&[
            FLAT_TRIANGLE,
            sa,
            sb,
            sc,
            u32::from(pack(colour)),
        ]);
        return Ok(());
    }
    match below {
        0 => {
            if height(a).wrapping_sub(height(c)) < 0x40_0000 {
                let at = queue.entry(key, 0xc)?;
                queue.words[at..at + 5].copy_from_slice(&[TEXTURE_TRIANGLE, sa, sb, sc, 0x40]);
                let row = {
                    let v = (height(c) >> 18).wrapping_mul(7).wrapping_mul(3);
                    ((v + if v < 0 { 0x1ff } else { 0 }) >> 9).min(0x14) as usize
                };
                slots(queue, at + 5, &blend, &colours.land[row]);
            } else {
                let at = queue.entry(key, 0x14)?;
                queue.words[at..at + 4].copy_from_slice(&[DETAIL_TRIANGLE, sa, sb, sc]);
                let words = [
                    0,
                    band(height(a)),
                    0x3f,
                    band(height(b)),
                    0,
                    band(height(c)),
                    0x5e,
                    0,
                ];
                for k in 0..4 {
                    queue.words[at + 4 + k] =
                        u32::from(words[2 * k]) | u32::from(words[2 * k + 1]) << 16;
                }
                queue.words[at + 8] = 0x16;
                pairs(queue, at + 9, &blend, &colours.steps, 0x16);
            }
        }
        1 => {
            // C is below the sea: a quad above it (A, B and the two crossings) and the
            // triangle of C and the crossings.
            let first = crossing(t, c, b);
            let second = crossing(t, c, a);
            if height(a) < 0x40_0000 {
                let at = queue.entry(key, 0xd)?;
                queue.words[at..at + 5].copy_from_slice(&[TEXTURE_QUAD, sa, sb, first, second]);
                queue.words[at + 5] = 0x40;
                slots(queue, at + 6, &blend, &colours.land[0]);
            } else {
                let at = queue.entry(key, 0x16)?;
                queue.words[at..at + 5].copy_from_slice(&[DETAIL_QUAD, sa, sb, first, second]);
                let words = [
                    0,
                    band(height(a)),
                    0x3f,
                    band(height(b)),
                    0x3f,
                    0,
                    0,
                    0,
                    0x5e,
                    0,
                ];
                for k in 0..5 {
                    queue.words[at + 5 + k] =
                        u32::from(words[2 * k]) | u32::from(words[2 * k + 1]) << 16;
                }
                queue.words[at + 10] = 0x16;
                pairs(queue, at + 11, &blend, &colours.steps, 0x16);
            }
            sea_triangle(queue, key, &blend, colours, [sc, first, second])?;
        }
        2 => {
            // B and C are below the sea.
            let first = crossing(t, b, a);
            let second = crossing(t, c, a);
            let at = queue.entry(key, 0xd)?;
            queue.words[at..at + 5].copy_from_slice(&[TEXTURE_QUAD, sc, sb, first, second]);
            queue.words[at + 5] = 0x121;
            slots(queue, at + 6, &blend, &colours.sea[3]);
            if height(a) < 0x40_0000 {
                let at = queue.entry(key, 0xc)?;
                queue.words[at..at + 5].copy_from_slice(&[
                    TEXTURE_TRIANGLE,
                    first,
                    second,
                    sa,
                    0x40,
                ]);
                slots(queue, at + 5, &blend, &colours.land[0]);
            } else {
                let at = queue.entry(key, 0x14)?;
                queue.words[at..at + 4].copy_from_slice(&[DETAIL_TRIANGLE, first, second, sa]);
                let words = [0, 0, 0x3f, 0, 0, band(height(a)), 0x5e, 0];
                for k in 0..4 {
                    queue.words[at + 4 + k] =
                        u32::from(words[2 * k]) | u32::from(words[2 * k + 1]) << 16;
                }
                queue.words[at + 8] = 0x16;
                pairs(queue, at + 9, &blend, &colours.steps, 0x16);
            }
        }
        _ => {
            let sum = height(b).wrapping_add(height(a)).wrapping_add(height(c));
            let row = ((sum >> 25) + 3).max(0) as usize;
            let at = queue.entry(key, 0xc)?;
            queue.words[at..at + 5].copy_from_slice(&[TEXTURE_TRIANGLE, sa, sb, sc, 0x121]);
            slots(queue, at + 5, &blend, &colours.sea[row.min(3)]);
        }
    }
    Ok(())
}

/// The sea part of a triangle that crosses it (the tail F1840 shares): the corner
/// below and the two crossings.
fn sea_triangle(
    queue: &mut Queue,
    key: u32,
    blend: &Blend,
    colours: &Colours,
    [x, y, z]: [u32; 3],
) -> Result<(), Fault> {
    let at = queue.entry(key, 0xc)?;
    queue.words[at..at + 5].copy_from_slice(&[TEXTURE_TRIANGLE, x, y, z, 0x121]);
    slots(queue, at + 5, blend, &colours.sea[3]);
    Ok(())
}

/// F1876 in draw mode as F1874 calls it: F1841's colour tables for the planet type,
/// then every triangle of the subdivision through F1840 into the order queue.
pub fn draw_surface(
    terrain: &mut Terrain<'_>,
    scene: &Scene,
    stream: &[u8],
    shading: &Shading,
    queue: &mut Queue,
) -> Result<Drawn, Fault> {
    let at = tables::TYPES + 48 * scene.kind;
    let entry = terrain.table[at..at + 48].to_vec();
    let ramps = terrain.table[tables::RAMPS..tables::RAMPS + 22 * 16].to_vec();
    let blends = terrain.table[tables::BLENDS..tables::BLENDS + 21 * 112].to_vec();
    let bands = terrain.table[tables::BANDS..tables::BANDS + 32 * 16].to_vec();
    let colours = Colours::build(&entry, &ramps, &blends, &bands);
    let kind = scene.kind;
    terrain.draw(scene, stream, &mut |t, emission| {
        emit(t, kind, shading, &colours, queue, &emission)
    })
}
