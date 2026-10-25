//! Textured model faces: the original face emitters' textured queue entries
//! (F1708/F1712 with F1692/F1695/F1326), their flat fallback colour (F1693/F1691)
//! and the affine texture rasterisers they queue (F1807/F1805 -> F1644/F1647 ->
//! F1643/F1642/F1646/F1649 and the span routines F1650-F1653).
//!
//! A textured material (bits 14-15 non-zero) selects a 30-byte record of the
//! original descriptor table `DATA_007811` by `material & 0xfff`. The record holds a
//! bitmap dword (bitmap id in bits 0-11, quad orientation in bits 12-14) and up to
//! seven nibble colours. Bitmap texels are slot numbers into the per-face colour
//! list (`DATA_009252`), not palette indices. See `docs/notes/textures.md`.

/// Records in the original `DATA_007811` table (floppy 1.02 and JJFFE 2.8a7).
pub const DESCRIPTOR_COUNT: usize = 148;
/// Bytes per descriptor record: dword bitmap, dword count, 7 x RGB, one pad byte.
pub const DESCRIPTOR_BYTES: usize = 30;
/// Bitmap ids F1620 resolves: the primary table (96) followed in memory by the
/// secondary table (251); ids 96.. read the secondary table.
pub const BITMAP_IDS: u32 = 96 + 251;
/// Slots F1807/F1805 copy into `DATA_009252` for every textured face.
pub const ENTRY_COLOURS: usize = 7;

/// One `DATA_007811` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// The record's first dword as queued: bitmap id (bits 0-11) and quad
    /// orientation (bits 12-14).
    pub bitmap: u32,
    /// Nibble colours, `count` of them (1..=7).
    pub colours: Vec<[u8; 3]>,
}

impl Descriptor {
    /// F1643/F1620 bitmap id.
    pub fn bitmap_id(&self) -> u32 {
        self.bitmap & 0xfff
    }

    /// F1647 orientation (`dword >> 12 & 7`); triangles (F1644) ignore it.
    pub fn orientation(&self) -> u8 {
        (self.bitmap >> 12 & 7) as u8
    }
}

/// The validated `DATA_007811` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptors {
    records: Vec<Descriptor>,
}

impl Descriptors {
    /// Parse and validate the 148 x 30-byte table. Record 0 is all zero (texture 0
    /// is the untextured path); every other record has 1..=7 colours with nibble
    /// components, a bitmap id below [`BITMAP_IDS`], no dword bits above 14 and
    /// zero padding.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != DESCRIPTOR_COUNT * DESCRIPTOR_BYTES {
            return Err("Texture descriptor table must hold 148 records of 30 bytes.".to_owned());
        }
        let mut records = Vec::with_capacity(DESCRIPTOR_COUNT);
        for (index, record) in bytes.chunks_exact(DESCRIPTOR_BYTES).enumerate() {
            let dword = |at: usize| u32::from_le_bytes(record[at..at + 4].try_into().unwrap());
            let (bitmap, count) = (dword(0), dword(4) as usize);
            if index == 0 {
                if record.iter().any(|byte| *byte != 0) {
                    return Err("Texture descriptor 0 must be empty.".to_owned());
                }
                records.push(Descriptor {
                    bitmap: 0,
                    colours: Vec::new(),
                });
                continue;
            }
            let colours: Vec<[u8; 3]> = record[8..8 + 3 * count.min(ENTRY_COLOURS)]
                .chunks_exact(3)
                .map(|rgb| [rgb[0], rgb[1], rgb[2]])
                .collect();
            if !(1..=ENTRY_COLOURS).contains(&count)
                || bitmap >> 15 != 0
                || bitmap & 0xfff >= BITMAP_IDS
                || colours.iter().flatten().any(|component| *component > 15)
                || record[8 + 3 * count..].iter().any(|byte| *byte != 0)
            {
                return Err(format!("Invalid texture descriptor {index}."));
            }
            records.push(Descriptor { bitmap, colours });
        }
        Ok(Self { records })
    }

    /// The record F1708/F1712 address as `DATA_007811 + 30 * texture`. The original
    /// does not bound the index; ids beyond the table are an explicit error here.
    pub fn get(&self, texture: u16) -> Result<&Descriptor, String> {
        self.records
            .get(usize::from(texture))
            .ok_or_else(|| format!("Texture {texture} is beyond the descriptor table."))
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Entries of the original `DATA_007770` map (one per 12-bit colour).
pub const COLOUR_MAP_ENTRIES: usize = 4096;

/// Validate `DATA_007770`: each entry is 0 (not a fixed colour) or a fixed low
/// palette index plus one (1..=128).
pub fn validate_colour_map(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() != COLOUR_MAP_ENTRIES || bytes.iter().any(|entry| *entry > 128) {
        return Err("Invalid 12-bit colour to low-palette map.".to_owned());
    }
    Ok(())
}

/// F1327's fixed-colour path: the low palette index a 12-bit colour maps to, or
/// `None` when the original allocates it in the runtime upper palette instead.
pub fn fixed_palette_index(map: &[u8], colour: u16) -> Option<u8> {
    map.get(usize::from(colour))
        .and_then(|entry| entry.checked_sub(1))
}

/// F1326: nibble RGB packed as the original's 12-bit colour, `r << 8 | g << 4 | b`
/// (16-bit sum; components are not masked).
pub fn pack_colour([r, g, b]: [u8; 3]) -> u16 {
    (u16::from(r) << 8)
        .wrapping_add(u16::from(g) << 4)
        .wrapping_add(u16::from(b))
}

/// Whether F1708/F1712 take the textured path for a face that is not culled:
/// textures enabled (`DATA_008812 >= 0`), a texture id, every screen point inside
/// x -2000..=2320 / y -2000..=2158, and a cyclic screen perimeter of at least 6.
pub fn textured_path(enabled: bool, material: u16, screens: &[[i32; 2]]) -> bool {
    let decoded = crate::model_commands::material(material);
    if !enabled || decoded.texture == 0 || screens.is_empty() {
        return false;
    }
    let guarded = screens
        .iter()
        .all(|[x, y]| (-2000..=2320).contains(x) && (-2000..=2158).contains(y));
    let count = screens.len();
    let perimeter: i32 = (0..count)
        .map(|i| {
            let [ax, ay] = screens[i];
            let [bx, by] = screens[(i + 1) % count];
            (ax - bx).abs() + (ay - by).abs()
        })
        .sum();
    guarded && perimeter >= 6
}

/// The colour part of a textured F1807/F1805 queue entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TexturedColours {
    /// The descriptor's bitmap dword, queued unchanged.
    pub bitmap: u32,
    /// F1326 colours for the record's slots; the entry's remaining slots are not
    /// written by the emitter (stale queue memory in the original).
    pub colours: Vec<u16>,
}

/// F1708/F1712 textured entry colours. The shade is the instance colour (F1695,
/// local materials) plus the face normal's colour unless unlit, with byte
/// wrap-around; each record colour plus the shade is clamped to 15 per component
/// and packed by F1326.
pub fn textured_colours(
    material: u16,
    instance: [u8; 3],
    normal: [u8; 3],
    descriptors: &Descriptors,
) -> Result<TexturedColours, String> {
    let decoded = crate::model_commands::material(material);
    if decoded.texture == 0 {
        return Err("Material is not textured.".to_owned());
    }
    let descriptor = descriptors.get(decoded.texture)?;
    let mut shade = [0u8; 3];
    if decoded.local_colour {
        shade = std::array::from_fn(|i| shade[i].wrapping_add(instance[i]));
    }
    if !decoded.unlit {
        shade = std::array::from_fn(|i| shade[i].wrapping_add(normal[i]));
    }
    Ok(TexturedColours {
        bitmap: descriptor.bitmap,
        colours: descriptor
            .colours
            .iter()
            .map(|rgb| {
                pack_colour(std::array::from_fn(|i| {
                    rgb[i].wrapping_add(shade[i]).min(15)
                }))
            })
            .collect(),
    })
}

/// F1693 + F1691: the colour F1708/F1712 give a face on the flat path. Material
/// RGB (zero when textured), plus the instance colour for local materials, plus
/// the normal's colour unless unlit (F1693 does not test the texture), plus the
/// first colour of the texture's descriptor when textured (F1691). Bytes wrap.
pub fn flat_colour(
    material: u16,
    instance: [u8; 3],
    normal: [u8; 3],
    descriptors: &Descriptors,
) -> Result<[u8; 3], String> {
    let decoded = crate::model_commands::material(material);
    let mut rgb = decoded.rgb;
    if decoded.local_colour {
        rgb = std::array::from_fn(|i| rgb[i].wrapping_add(instance[i]));
    }
    if !decoded.unlit {
        rgb = std::array::from_fn(|i| rgb[i].wrapping_add(normal[i]));
    }
    if decoded.texture != 0 {
        let first = descriptors
            .get(decoded.texture)?
            .colours
            .first()
            .copied()
            .unwrap_or([0; 3]);
        rgb = std::array::from_fn(|i| rgb[i].wrapping_add(first[i]));
    }
    Ok(rgb)
}

/// A bitmap as F1643 sees it: width 64 selects the 64-stride span routines, any
/// other width the 128-stride ones; bit 0 of the header flags (`masked_zero`)
/// selects the transparent variants, which skip texel 0.
#[derive(Debug, Clone, Copy)]
pub struct TextureMap<'a> {
    pub width: u32,
    pub masked_zero: bool,
    /// Texels from the bitmap's data (header + 8).
    pub texels: &'a [u8],
}

/// `DATA_007791` (64) / `DATA_007792` (128) corner coordinates: top-left,
/// top-right, bottom-left, bottom-right.
pub fn corners(width: u32) -> [[i16; 2]; 4] {
    let far = if width == 64 { 63 } else { 127 };
    [[0, 0], [far, 0], [0, far], [far, far]]
}

/// F1644: a triangle's points take corners 0, 1, 2 in order.
pub fn triangle_uv(width: u32) -> [[i16; 2]; 3] {
    let c = corners(width);
    [c[0], c[1], c[2]]
}

/// F1647: the eight corner assignments selected by `dword >> 12 & 7`
/// (`DATA_000047`), four rotations then four mirrored rotations.
pub fn quad_uv(width: u32, orientation: u8) -> [[i16; 2]; 4] {
    const ORDER: [[usize; 4]; 8] = [
        [0, 1, 3, 2],
        [1, 3, 2, 0],
        [3, 2, 0, 1],
        [2, 0, 1, 3],
        [0, 2, 3, 1],
        [2, 3, 1, 0],
        [3, 1, 0, 2],
        [1, 0, 2, 3],
    ];
    let c = corners(width);
    ORDER[usize::from(orientation & 7)].map(|i| c[i])
}

/// Rows the span routines draw by default (`y < 0x9e`) and the screen width (`0x140`).
pub const RASTER_HEIGHT: i32 = 158;
pub const RASTER_WIDTH: i32 = 320;

thread_local! {
    /// F917's tombstone covers the full 320x200 logical screen; the flight view's
    /// textured spans stop at 158. [`ViewHeightGuard`](crate::model_execute::ViewHeightGuard)
    /// raises this with the face-queue height.
    static CURRENT_HEIGHT: std::cell::Cell<i32> = const { std::cell::Cell::new(RASTER_HEIGHT) };
}

pub fn set_raster_height(height: i32) {
    CURRENT_HEIGHT.with(|value| value.set(height));
}

pub fn raster_height() -> i32 {
    CURRENT_HEIGHT.with(std::cell::Cell::get)
}
/// The span-routine state: per side 16.16 x, u, v and their per-row steps
/// (`DATA_009236..DATA_009247`), and the current row (`DATA_009248`).
struct Raster<'a, F: FnMut(i32, i32, u8)> {
    x: [i32; 2],
    dx: [i32; 2],
    u: [i32; 2],
    v: [i32; 2],
    du: [i32; 2],
    dv: [i32; 2],
    y: i32,
    map: TextureMap<'a>,
    plot: F,
    unmodelled: u32,
}

/// The truncating `(a << 16) / b` of F1642's sign-split division, or the
/// `0x7fffffff`/`0x80000001` stand-in for a zero height.
fn step(delta: i32, height: i32) -> i32 {
    if height == 0 {
        if delta >= 0 {
            0x7fff_ffff
        } else {
            0x8000_0001u32 as i32
        }
    } else {
        delta.wrapping_shl(16).wrapping_div(height)
    }
}

impl<F: FnMut(i32, i32, u8)> Raster<'_, F> {
    /// F1642: edge set-up for one side.
    fn edge(&mut self, side: usize, p1: [i16; 2], t1: [i16; 2], p2: [i16; 2], t2: [i16; 2]) {
        let height = i32::from(p2[1]) - i32::from(p1[1]);
        self.u[side] = i32::from(t1[0]) << 16;
        self.v[side] = i32::from(t1[1]) << 16;
        self.du[side] = step(i32::from(t2[0]) - i32::from(t1[0]), height);
        self.dv[side] = step(i32::from(t2[1]) - i32::from(t1[1]), height);
        self.x[side] = i32::from(p1[0]) << 16;
        self.dx[side] = step(i32::from(p2[0]) - i32::from(p1[0]), height);
    }

    fn advance(&mut self, rows: i32) {
        for side in 0..2 {
            self.x[side] = self.x[side].wrapping_add(rows.wrapping_mul(self.dx[side]));
            self.u[side] = self.u[side].wrapping_add(rows.wrapping_mul(self.du[side]));
            self.v[side] = self.v[side].wrapping_add(rows.wrapping_mul(self.dv[side]));
        }
        self.y = self.y.wrapping_add(rows);
    }

    /// F1650-F1653: draw `rows` rows from the current row (any count, including
    /// zero or negative, which only moves the edges).
    fn spans(&mut self, mut rows: i32) {
        if rows <= 0 || self.y >= raster_height() || self.y.wrapping_add(rows) <= 0 {
            self.advance(rows);
            return;
        }
        if self.y < 0 {
            let skip = -self.y;
            for side in 0..2 {
                self.x[side] = self.x[side].wrapping_add(skip.wrapping_mul(self.dx[side]));
                self.u[side] = self.u[side].wrapping_add(skip.wrapping_mul(self.du[side]));
                self.v[side] = self.v[side].wrapping_add(skip.wrapping_mul(self.dv[side]));
            }
            rows += self.y;
            self.y = 0;
        }
        while rows != 0 && self.y < raster_height() {
            let (left, right) = if self.x[0] < self.x[1] {
                (0, 1)
            } else {
                (1, 0)
            };
            let (mut start, end) = (self.x[left] >> 16, self.x[right] >> 16);
            let (mut u, mut v) = (self.u[left], self.v[left]);
            let mut width = end.wrapping_sub(start);
            if start < RASTER_WIDTH && end > 0 && width > 0 {
                let du = self.u[right].wrapping_sub(u) / width;
                let dv = self.v[right].wrapping_sub(v) / width;
                if start < 0 {
                    u = u.wrapping_sub(start.wrapping_mul(du));
                    v = v.wrapping_sub(start.wrapping_mul(dv));
                    width += start;
                    start = 0;
                }
                if end > RASTER_WIDTH {
                    width = RASTER_WIDTH - start;
                }
                let (shift, mask) = if self.map.width == 64 {
                    (10, !63)
                } else {
                    (9, !127)
                };
                for x in start..start + width.max(0) {
                    let offset = (u >> 16).wrapping_add((v >> shift) & mask);
                    match usize::try_from(offset)
                        .ok()
                        .and_then(|i| self.map.texels.get(i))
                    {
                        Some(&texel) if !(self.map.masked_zero && texel == 0) => {
                            (self.plot)(x, self.y, texel)
                        }
                        Some(_) => {}
                        None => self.unmodelled += 1,
                    }
                    u = u.wrapping_add(du);
                    v = v.wrapping_add(dv);
                }
            }
            self.advance(1);
            rows -= 1;
        }
        if rows > 0 {
            self.advance(rows);
        }
    }
}

/// F1644/F1646 with F1650-F1653: an affine textured triangle. `plot(x, y, texel)`
/// receives each written pixel (x in 0..320, y in 0..158) with its raw texel; the
/// caller maps it through the slot table (`DATA_009252`). Returns the number of
/// texel reads outside the bitmap's data, which the original takes from adjacent
/// memory and this port skips.
pub fn draw_triangle(
    points: [[i16; 2]; 3],
    uv: [[i16; 2]; 3],
    map: TextureMap<'_>,
    plot: impl FnMut(i32, i32, u8),
) -> u32 {
    let (mut p, mut t) = (points, uv);
    while p[0][1] > p[1][1] || p[0][1] > p[2][1] {
        p.rotate_left(1);
        t.rotate_left(1);
    }
    let mut raster = new_raster(map, plot);
    raster.edge(0, p[0], t[0], p[1], t[1]);
    raster.edge(1, p[0], t[0], p[2], t[2]);
    raster.y = i32::from(p[0][1]);
    let y = p.map(|q| i32::from(q[1]));
    if y[1] < y[2] {
        raster.spans(y[1] - y[0]);
        raster.edge(0, p[1], t[1], p[2], t[2]);
        raster.spans(y[2] - y[1]);
    } else {
        raster.spans(y[2] - y[0]);
        raster.edge(1, p[2], t[2], p[1], t[1]);
        raster.spans(y[1] - y[2]);
    }
    raster.unmodelled
}

/// F1647/F1649 with F1650-F1653: an affine textured quad; see [`draw_triangle`].
pub fn draw_quad(
    points: [[i16; 2]; 4],
    uv: [[i16; 2]; 4],
    map: TextureMap<'_>,
    plot: impl FnMut(i32, i32, u8),
) -> u32 {
    let (mut p, mut t) = (points, uv);
    while p[0][1] > p[1][1] || p[0][1] > p[2][1] || p[0][1] > p[3][1] {
        p.rotate_left(1);
        t.rotate_left(1);
    }
    let mut raster = new_raster(map, plot);
    raster.edge(0, p[0], t[0], p[1], t[1]);
    raster.edge(1, p[0], t[0], p[3], t[3]);
    raster.y = i32::from(p[0][1]);
    let y = p.map(|q| i32::from(q[1]));
    if y[3] > y[1] {
        raster.spans(y[1] - y[0]);
        raster.edge(0, p[1], t[1], p[2], t[2]);
        if y[3] > y[2] {
            raster.spans(y[2] - y[1]);
            raster.edge(0, p[2], t[2], p[3], t[3]);
            raster.spans(y[3] - y[2]);
        } else {
            raster.spans(y[3] - y[1]);
            raster.edge(1, p[3], t[3], p[2], t[2]);
            raster.spans(y[2] - y[3]);
        }
    } else {
        raster.spans(y[3] - y[0]);
        raster.edge(1, p[3], t[3], p[2], t[2]);
        if y[1] < y[2] {
            raster.spans(y[1] - y[3]);
            raster.edge(0, p[1], t[1], p[2], t[2]);
            raster.spans(y[2] - y[1]);
        } else {
            raster.spans(y[2] - y[3]);
            raster.edge(1, p[2], t[2], p[1], t[1]);
            raster.spans(y[1] - y[2]);
        }
    }
    raster.unmodelled
}

fn new_raster<F: FnMut(i32, i32, u8)>(map: TextureMap<'_>, plot: F) -> Raster<'_, F> {
    Raster {
        x: [0; 2],
        dx: [0; 2],
        u: [0; 2],
        v: [0; 2],
        du: [0; 2],
        dv: [0; 2],
        y: 0,
        map,
        plot,
        unmodelled: 0,
    }
}
