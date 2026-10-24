//! The physics object list and the creation of a system's bodies, ported from
//! `csysgen.txt` and `ffe.asm`: F927 (take a free slot), F1533 (initialise an
//! object from a template and its model), F880 (turn the generated system
//! objects into physics objects at a given time), F879 (place a starport on its
//! planet) and the orbit helpers F1506-F1510, F1674 and F1675. The list is kept
//! as the original lays it out: a flag byte per object, then 0x152-byte objects.
//! Verified against the executed original (`docs/notes/objects.md`).

use crate::collision::{reach, Radii, Reach};
use crate::ffp::{self, Ffp};
use crate::galaxy::{tables, Tables};
use crate::landing::{settle, Pair, Surface};
use crate::model::multiply;
use crate::model_math::SineTable;
pub use crate::surface::SurfaceTools;
use crate::surface::{Hull, Registers, Scene};
use crate::sysgen::OBJECT_BYTES as SYSTEM_OBJECT_BYTES;
use crate::terrain::Terrain;

/// Bytes per physics object.
pub const OBJECT_BYTES: usize = 0x152;
/// Objects in the list (indexes 0 to 0x72).
pub const OBJECTS: usize = 0x73;
/// Where object 0 starts: the flag bytes come first.
pub const FIRST_OBJECT: usize = 0x74;
/// Bytes of the list.
pub const LIST_BYTES: usize = FIRST_OBJECT + OBJECT_BYTES * OBJECTS;

/// The model fields F1533 reads, from the cache's model record and statistics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModelInfo {
    /// The dword at `+0x2c`: the radius word and the word after it.
    pub radius_dword: u32,
    pub scale: i32,
    pub scale_offset: i32,
    pub flags: u8,
    pub complex: bool,
    pub radius: i16,
    pub collision_radius: i16,
    pub stats: Option<[u8; 48]>,
}

/// The cache's model records and statistics, and the hyperdrive-mass table.
pub struct Models<'a> {
    pub records: &'a [u8],
    pub stats: &'a [u8],
    pub gameplay: &'a [u8],
}

impl Models<'_> {
    pub fn info(&self, model: u32) -> ModelInfo {
        let slot = model as usize;
        let r = &self.records[20 * slot..20 * slot + 20];
        let stats: [u8; 48] = self.stats[48 * slot..48 * slot + 48].try_into().unwrap();
        ModelInfo {
            radius_dword: u32::from(u16::from_le_bytes([r[10], r[11]]))
                | u32::from(u16::from_le_bytes([r[16], r[17]])) << 16,
            scale: i32::from_le_bytes(r[0..4].try_into().unwrap()),
            scale_offset: i32::from_le_bytes(r[4..8].try_into().unwrap()),
            flags: r[8],
            complex: r[9] != 0,
            radius: i16::from_le_bytes([r[10], r[11]]),
            collision_radius: i16::from_le_bytes([r[12], r[13]]),
            stats: (r[14] != 0).then_some(stats),
        }
    }

    /// The hold of a ship model, tonnes: the statistics' cargo word (`+8`) less the
    /// hyperdrive's mass when it has one (F1533 puts it at `+0x116`, F992 copies it
    /// into the player's hold). `None` for a model without statistics.
    pub fn cargo_space(&self, model: u32) -> Option<i32> {
        let stats = self.info(model).stats?;
        let word = |at: usize| u16::from_le_bytes([stats[at], stats[at + 1]]);
        let mut space = i32::from(word(8) as i16);
        let drive = word(0x14);
        if (drive as i16) < 0 {
            space = space.wrapping_sub(self.hyperdrive(drive as u8 as i8));
        }
        Some(space)
    }

    /// `DATA_007210`, indexed by a signed byte (negative indexes read the table
    /// before it, which the gameplay block holds).
    fn hyperdrive(&self, index: i8) -> i32 {
        let at = 144i64 + 4 * i64::from(index);
        let at = usize::try_from(at).expect("hyperdrive index inside the block");
        i32::from_le_bytes(self.gameplay[at..at + 4].try_into().unwrap())
    }
}

/// F1341: a 64-bit arithmetic shift (left for a positive count) with the
/// original's behaviour above 31.
pub(crate) fn shift64(value: i64, count: i32) -> i64 {
    let (low, high) = (value as u32, (value >> 32) as i32);
    let (low, high) = if count < 0 {
        let by = (-count) as u32;
        if by < 0x20 {
            (
                (low >> by) | (high as u32).wrapping_shl(32 - by),
                high >> by,
            )
        } else {
            ((high >> ((by - 0x20) & 31)) as u32, high >> 31)
        }
    } else if count > 0 {
        let by = count as u32;
        if by < 0x20 {
            (low << by, ((high as u32) << by | low >> (32 - by)) as i32)
        } else {
            (0, (low << ((by - 0x20) & 31)) as i32)
        }
    } else {
        (low, high)
    };
    (i64::from(high) << 32) | i64::from(low)
}

/// F1495: an ffp number as a 64-bit integer: the mantissa shifted by exponent - 15.
fn ffp_to_i64(value: Ffp) -> i64 {
    shift64(i64::from(value.mantissa), i32::from(value.exponent) - 15)
}

/// The physics list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhysList {
    pub bytes: Vec<u8>,
    /// `DATA_008834`: objects flagged 0x20.
    pub counter: i32,
    /// `DATA_008804` and `DATA_008807`: the clock (low, high).
    pub clock: [u32; 2],
    /// `DATA_008811`: the frame stamp.
    pub stamp: u8,
    /// `DATA_008704`: the high dword of the collision distance, which persists.
    pub distance_high: i32,
}

impl Default for PhysList {
    fn default() -> Self {
        PhysList {
            bytes: vec![0; LIST_BYTES],
            counter: 0,
            clock: [0; 2],
            stamp: 0,
            distance_high: 0,
        }
    }
}

/// Where a new object is cloned from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Template {
    /// `DATA_007134`, the default object.
    Default,
    /// An object already in the list.
    Object(usize),
}

impl PhysList {
    fn at(&self, object: usize, offset: usize) -> usize {
        FIRST_OBJECT + OBJECT_BYTES * object + offset
    }

    pub fn flag(&self, object: usize) -> u8 {
        self.bytes[object]
    }

    pub fn byte(&self, object: usize, offset: usize) -> u8 {
        self.bytes[self.at(object, offset)]
    }

    pub fn set_byte(&mut self, object: usize, offset: usize, value: u8) {
        let at = self.at(object, offset);
        self.bytes[at] = value;
    }

    pub fn word(&self, object: usize, offset: usize) -> u16 {
        let at = self.at(object, offset);
        u16::from_le_bytes([self.bytes[at], self.bytes[at + 1]])
    }

    pub fn set_word(&mut self, object: usize, offset: usize, value: u16) {
        let at = self.at(object, offset);
        self.bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }

    pub fn dword(&self, object: usize, offset: usize) -> u32 {
        let at = self.at(object, offset);
        u32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap())
    }

    pub fn set_dword(&mut self, object: usize, offset: usize, value: u32) {
        let at = self.at(object, offset);
        self.bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn qword(&self, object: usize, offset: usize) -> i64 {
        let at = self.at(object, offset);
        i64::from_le_bytes(self.bytes[at..at + 8].try_into().unwrap())
    }

    fn set_qword(&mut self, object: usize, offset: usize, value: i64) {
        let at = self.at(object, offset);
        self.bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }

    /// F927: take a free slot for a new object of the given flag kind, from the
    /// top of the list down (slots below 0x14 are kept back); when none is free
    /// the topmost object flagged 0x20 at index 0x14 or above is replaced.
    pub fn allocate(
        &mut self,
        tables: &Tables,
        models: &Models,
        template: Template,
        kind: u8,
        model: u32,
    ) -> Option<usize> {
        let mut slot = 0x72usize;
        while slot > 0 && self.bytes[slot] != 0 {
            slot -= 1;
        }
        if slot < 0x14 {
            slot = 0x72;
            loop {
                if self.bytes[slot] & 0x20 != 0 {
                    if slot < 0x14 {
                        return None;
                    }
                    self.counter -= 1;
                    break;
                }
                slot -= 1;
                if slot == 0 {
                    break;
                }
            }
            if slot == 0 {
                return None;
            }
        }
        self.bytes[slot] = kind;
        if kind & 0x20 != 0 {
            self.counter += 1;
        }
        self.initialise(tables, models, slot, template, model);
        self.set_dword(slot, 0x82, model);
        self.set_byte(slot, 0x86, slot as u8);
        Some(slot)
    }

    /// F1533: clone the template into the object and set what the model decides.
    pub fn initialise(
        &mut self,
        tables: &Tables,
        models: &Models,
        object: usize,
        template: Template,
        model: u32,
    ) {
        let index = self.byte(object, 0x86);
        let source: Vec<u8> = match template {
            Template::Default => tables.bytes(tables::TEMPLATES, OBJECT_BYTES).to_vec(),
            Template::Object(other) if other == object => Vec::new(),
            Template::Object(other) => {
                let at = self.at(other, 0);
                self.bytes[at..at + OBJECT_BYTES].to_vec()
            }
        };
        if !source.is_empty() {
            let at = self.at(object, 0);
            self.bytes[at..at + OBJECT_BYTES].copy_from_slice(&source);
            self.set_byte(object, 0x86, if model != 0 { index } else { 0 });
        }
        if model == 0 {
            return;
        }
        self.set_dword(object, 0xa8, self.clock[0]);
        self.set_dword(object, 0xac, self.clock[1]);
        self.set_dword(object, 0x82, model);
        let info = models.info(model);
        let exponent = (info.scale as i16)
            .wrapping_add(info.scale_offset as i16)
            .wrapping_add(7);
        self.set_qword(object, 0x138, ffp_to_i64(Ffp::new(info.radius, exponent)));
        self.set_qword(
            object,
            0x140,
            ffp_to_i64(Ffp::new(info.collision_radius, exponent)),
        );
        self.set_byte(object, 0x151, 0);
        self.set_byte(object, 0x25, 0);
        self.set_byte(object, 0x124, 0);
        let mut class = (info.flags >> 4) & 7;
        if info.complex {
            class |= 0x80;
        }
        self.set_byte(object, 0x14c, class);
        self.set_byte(object, 0x88, 0xff);
        let Some(stats) = info.stats else {
            return;
        };
        let word = |at: usize| u16::from_le_bytes([stats[at], stats[at + 1]]);
        self.set_word(object, 0xc4, word(0));
        let limit = word(2);
        self.set_word(object, 0xc6, limit);
        self.set_word(object, 0xc2, limit);
        self.set_word(object, 0xbe, limit);
        let negated = limit.wrapping_neg();
        self.set_word(object, 0xc0, negated);
        self.set_word(object, 0xbc, negated);
        let class = if (negated as i16) >= 0x1329 {
            2
        } else if (negated as i16) >= 0xaa5 {
            1
        } else {
            0
        };
        self.set_byte(object, 0x101, class);
        self.set_byte(object, 0xd1, stats[4]);
        self.set_word(object, 0xe4, word(6).wrapping_shl(2));
        let mut value = i32::from(word(8) as i16);
        let drive = word(0x14);
        if (drive as i16) < 0 {
            self.set_byte(object, 0xd0, drive as u8);
            value = value.wrapping_sub(models.hyperdrive(drive as u8 as i8));
        }
        self.set_word(object, 0x116, value as u16);
        self.set_word(object, 0xe0, 0);
        self.set_word(object, 0xe2, 0);
    }
}

// ---- the orbit helpers ----

/// F1506: the unit vector of a longitude and an angle.
fn direction(table: &SineTable, longitude: i32, angle: i32) -> [i32; 3] {
    let sin_angle = table.sin32(angle);
    [
        multiply(table.sin32(longitude.wrapping_add(0x4000)), sin_angle).wrapping_neg(),
        multiply(table.sin32(longitude), sin_angle),
        table.sin32(angle.wrapping_add(0x4000)),
    ]
}

/// F1509 and F1510: the orbital phase, `((hi:lo) * rate) << 3` over the clock
/// words as the original passes them (the low clock word in the high position).
fn phase(rate: u32, high: u32, low: u32) -> i64 {
    let value = ((u64::from(high) << 32) | u64::from(low)) as i64;
    (value.wrapping_mul(i64::from(rate))) << 3
}

/// F1674: the starport orientation rows from a latitude and a longitude.
fn matrix_odd(table: &SineTable, latitude: i32, longitude: i32) -> [i32; 9] {
    let s_lat = table.sin32(latitude);
    let s_lon = table.sin32(longitude);
    let c_lat = table.sin32(latitude.wrapping_add(0x4000));
    let c_lon = table.sin32(longitude.wrapping_add(0x4000));
    let mut m = [0i32; 9];
    m[6] = s_lat;
    m[4] = s_lon;
    m[8] = c_lat;
    m[1] = c_lon;
    m[7] = 0;
    m[2] = multiply(s_lat, s_lon);
    m[3] = multiply(c_lon, c_lat);
    m[5] = multiply(c_lon, s_lat).wrapping_neg();
    m[0] = multiply(s_lon, c_lat).wrapping_neg();
    m[8] = m[8].wrapping_neg();
    m[6] = m[6].wrapping_neg();
    m
}

/// F1675: the orientation rows of a body from its tilt and spin.
fn matrix_yzt(table: &SineTable, tilt: i32, spin: i32) -> [i32; 9] {
    let mut m = [0i32; 9];
    m[1] = table.sin32(tilt);
    m[6] = table.sin32(spin);
    m[4] = table.sin32(tilt.wrapping_add(0x4000));
    m[8] = table.sin32(spin.wrapping_add(0x4000));
    m[7] = 0;
    m[5] = multiply(m[6], m[1]);
    m[0] = multiply(m[8], m[4]);
    m[3] = multiply(m[8], m[1]).wrapping_neg();
    m[2] = multiply(m[4], m[6]).wrapping_neg();
    m
}

impl PhysList {
    fn set_matrix(&mut self, object: usize, rows: [i32; 9]) {
        for (i, value) in rows.iter().enumerate() {
            self.set_dword(object, 4 * i, *value as u32);
        }
    }

    /// F1508: the sub-step interpolation of the position along the velocity.
    fn interpolate(&mut self, object: usize, fraction: u32) {
        let radius = i64::from(self.word(object, 0xb0) as i16);
        let velocity = [
            self.dword(object, 0x8c) as i32,
            self.dword(object, 0x90) as i32,
            self.dword(object, 0x94) as i32,
        ];
        let steps = [
            (
                (radius * i64::from(fraction >> 16)) as i32,
                i32::from(self.word(object, 0xb2) as i16) - 0x32,
            ),
            (
                (radius * i64::from(fraction & 0xffff)) as i32,
                i32::from(self.word(object, 0xb2) as i16) - 0x42,
            ),
        ];
        for (factor, shift) in steps {
            for (axis, v) in velocity.iter().enumerate() {
                let delta = shift64(i64::from(*v) * i64::from(factor), shift);
                let at = 0x3e + 8 * axis;
                let position = self.qword(object, at).wrapping_add(delta);
                self.set_qword(object, at, position);
            }
        }
    }

    /// F1507: the position on the orbit from the radius, longitude and phase.
    fn place_on_orbit(
        &mut self,
        table: &SineTable,
        object: usize,
        radius: Ffp,
        longitude: i32,
        phase: u32,
    ) {
        let angle = ((phase as i32) >> 16) & !0xf;
        let first = direction(table, longitude, angle);
        let second = direction(table, longitude, angle + 0x10);
        for axis in 0..3 {
            self.set_dword(
                object,
                0x8c + 4 * axis,
                second[axis].wrapping_sub(first[axis]) as u32,
            );
            let product = i64::from(radius.mantissa) * i64::from(first[axis]);
            let shifted = shift64(product, i32::from(radius.exponent) - 0x2e);
            self.set_qword(object, 0x3e + 8 * axis, shifted);
        }
        self.interpolate(object, phase & 0xfffff);
    }
}

/// F574: the distance from a body to a planet along a vector, with the body put
/// on the surface and turned to it when they touch (F575's reorienting branch).
/// `None` when the touch is of another kind or a complex shape cannot be tested.
fn surface_distance(
    list: &mut PhysList,
    vector: [i64; 3],
    body: usize,
    planet: usize,
    tools: Option<&SurfaceTools>,
    models: &Models,
) -> Option<i32> {
    let radii = |list: &PhysList, object: usize| Radii {
        outer: list.qword(object, 0x138),
        inner: list.qword(object, 0x140),
    };
    let complex = list.byte(planet, 0x14c) & 0x80 != 0;
    let mut distance = (i64::from(list.distance_high) << 32) | 0xffff_ffff;
    let found = reach(vector, radii(list, body), radii(list, planet), complex)?;
    let object = |list: &PhysList, index: usize| {
        let at = FIRST_OBJECT + OBJECT_BYTES * index;
        list.bytes[at..at + OBJECT_BYTES].to_vec()
    };
    let settle_on = |list: &mut PhysList, surface: Surface| {
        let planet_bytes = object(list, planet);
        let at = FIRST_OBJECT + OBJECT_BYTES * body;
        let (body_model, planet_model) = (
            models.info(list.dword(body, 0x82)),
            models.info(list.dword(planet, 0x82)),
        );
        let mut pair = Pair {
            body: &mut list.bytes[at..at + OBJECT_BYTES],
            planet: &planet_bytes,
            body_model,
            planet_model,
        };
        settle(&mut pair, vector, surface, 0);
    };
    match found {
        Reach::Near {
            distance: value, ..
        } => distance = value,
        Reach::Contact { squares } => {
            // Only a touch of the body's own parent is settled here.
            if list.byte(planet, 0x86) != list.byte(body, 0x56) {
                return None;
            }
            settle_on(list, Surface::Sphere { squares });
        }
        Reach::Shape => {
            let tools = tools?;
            let (ship, planet_bytes) = (object(list, body), object(list, planet));
            let model = |bytes: &[u8]| u32::from_le_bytes(bytes[0x82..0x86].try_into().unwrap());
            let ship_shape = tools.shapes.shape(model(&ship))?;
            let planet_shape = tools.shapes.shape(model(&planet_bytes))?;
            let mut terrain = Terrain::new(tools.terrain_table, tools.terrain_maps);
            let mut scene = Scene {
                terrain: &mut terrain,
                shapes: tools.shapes,
                detail: tools.detail,
            };
            let mut regs = Registers {
                distance,
                ..Registers::default()
            };
            let hit = scene
                .hit(
                    &mut regs,
                    &Hull::from_object(&ship),
                    &ship_shape,
                    &Hull::from_object(&planet_bytes),
                    &planet_shape,
                    vector,
                )
                .ok()?;
            distance = regs.distance;
            if hit == 0 {
                // A hit on the terrain settles the body; any other is not ported.
                if regs.planet != 1 {
                    return None;
                }
                settle_on(
                    list,
                    Surface::Terrain {
                        altitude: regs.altitude,
                    },
                );
            }
        }
        Reach::Far => {}
    }
    list.distance_high = (distance >> 32) as i32;
    Some(if distance >> 32 != 0 || (distance as i32) < 0 {
        -1
    } else {
        distance as i32
    })
}

/// A system's objects turned into physics objects.
pub struct Placement<'a> {
    pub tables: &'a Tables<'a>,
    pub models: &'a Models<'a>,
    pub sine: &'a SineTable,
    /// The complex shapes' data; without it a starport on a planet cannot be placed.
    pub surface: Option<&'a SurfaceTools<'a>>,
}

impl Placement<'_> {
    /// F879: place a starport on its planet's surface at a latitude and longitude.
    fn starport(
        &self,
        list: &mut PhysList,
        object: usize,
        latitude: u16,
        longitude: u16,
    ) -> Option<()> {
        let planet = usize::from(list.byte(object, 0x56));
        let info = self.models.info(list.dword(planet, 0x82));
        list.set_matrix(
            object,
            matrix_odd(self.sine, i32::from(latitude), i32::from(longitude)),
        );
        let up = [
            list.dword(object, 0xc) as i32,
            list.dword(object, 0x10) as i32,
            list.dword(object, 0x14) as i32,
        ];
        let surface = info.radius_dword.wrapping_sub(1);
        let mut bits = info.scale.wrapping_add(info.scale_offset).wrapping_sub(8);
        let scaled = |factor: u32, shift: i32| -> [i64; 3] {
            std::array::from_fn(|axis| {
                let product = (i64::from(up[axis]) as u64).wrapping_mul(u64::from(factor)) as i64;
                shift64(product, shift.wrapping_sub(0x1f))
            })
        };
        let first = scaled(surface, bits);
        let mut distance =
            surface_distance(list, first, object, planet, self.surface, self.models)?;
        distance -= distance >> 3;
        let mut factor = surface;
        if bits < 0 {
            factor = factor.wrapping_sub((distance as u32).wrapping_shl((-bits) as u32));
        } else if bits < 0x10 {
            factor = factor
                .wrapping_shl(bits as u32)
                .wrapping_sub(distance as u32);
            bits = 0;
        } else {
            bits -= 0x10;
            factor = factor
                .wrapping_shl(16)
                .wrapping_sub((distance >> (bits as u32 & 31)) as u32);
        }
        let second = scaled(factor, bits);
        let distance = surface_distance(list, second, object, planet, self.surface, self.models)?;
        factor = if bits < 0 {
            factor.wrapping_sub((distance as u32).wrapping_shl((-bits) as u32))
        } else {
            factor.wrapping_sub((distance >> (bits as u32 & 31)) as u32)
        };
        let third = scaled(factor, bits);
        for (axis, value) in third.iter().enumerate() {
            list.set_qword(object, 0x3e + 8 * axis, *value);
        }
        Some(())
    }

    /// F880: create the physics objects of a generated system (`system` holds
    /// 68-byte objects ending in a zero mass), at the given clock. The system
    /// objects' `+0x18` receives the physics object's index.
    pub fn create(&self, list: &mut PhysList, system: &mut [u8], clock: [u32; 2]) {
        let mut index = 0usize;
        while u32::from_le_bytes(
            system[index * SYSTEM_OBJECT_BYTES..][..4]
                .try_into()
                .unwrap(),
        ) != 0
        {
            let at = index * SYSTEM_OBJECT_BYTES;
            let s = |system: &[u8], offset: usize| system[at + offset];
            let sw = |system: &[u8], offset: usize| {
                u16::from_le_bytes([system[at + offset], system[at + offset + 1]])
            };
            let sd = |system: &[u8], offset: usize| {
                u32::from_le_bytes(system[at + offset..at + offset + 4].try_into().unwrap())
            };
            let parent = sw(system, 8) as usize;
            let parent_object = (parent != 0).then(|| {
                let p = (parent - 1) * SYSTEM_OBJECT_BYTES;
                u32::from_le_bytes(system[p + 0x18..p + 0x1c].try_into().unwrap()) as usize
            });
            let template = parent_object.map_or(Template::Default, Template::Object);
            let Some(object) = list.allocate(
                self.tables,
                self.models,
                template,
                0x1d,
                u32::from(sw(system, 0xc)),
            ) else {
                return;
            };
            system[at + 0x18..at + 0x1c].copy_from_slice(&(object as u32).to_le_bytes());
            list.set_dword(object, 0xa0, sd(system, 4));
            list.set_byte(object, 0xc8, s(system, 0x3e));
            list.set_byte(object, 0xcb, s(system, 0x3f));
            list.set_word(object, 0xe0, sw(system, 0x24));
            list.set_word(object, 0xcc, sw(system, 0x42));
            let earth = Ffp::new(0x4082, -43);
            let mass = ffp::normalise(ffp::multiply(Ffp::from_dword(sd(system, 0)), earth));
            list.set_dword(object, 0xa4, mass.to_dword());
            let mut exponent = mass.exponent;
            let kind = sw(system, 0xa);
            if kind == 0 {
                let own = usize::from(list.byte(object, 0x86));
                list.bytes[own] = 0x11;
                exponent = exponent.wrapping_add(2);
            }
            if parent != 0 {
                let p = (parent - 1) * SYSTEM_OBJECT_BYTES;
                let parent_kind = u16::from_le_bytes([system[p + 0xa], system[p + 0xb]]);
                if parent_kind == 0 && (parent as u32 + 1) >= index as u32 {
                    exponent = exponent.wrapping_sub(6);
                }
            }
            list.set_word(object, 0xda, exponent as u16);
            list.set_dword(object, 0xb0, sd(system, 0xe));
            list.set_byte(object, 0x87, 1);
            let model = sw(system, 0xc);
            if kind >= 0x20 {
                let weight = list.word(object, 0xda).wrapping_add(4);
                list.set_word(object, 0xda, weight);
                let class = list.byte(object, 0x14c) | 0x10;
                list.set_byte(object, 0x14c, class);
                list.set_word(object, 0xce, 3);
                let own = usize::from(list.byte(object, 0x86));
                list.bytes[own] = 0x1f;
                if kind == 0x22 {
                    list.set_word(object, 0xce, 7);
                }
                list.set_byte(object, 0xca, 0);
                list.set_byte(object, 0xc9, 0);
                list.set_word(object, 0xd8, 0);
                let size = (list.word(object, 0xce) << 3).wrapping_add(8);
                list.set_word(object, 0xdc, size);
                if kind == 0x23 {
                    let planet = parent_object.expect("a starport has a parent");
                    let planet_index = list.byte(planet, 0x86);
                    list.set_byte(object, 0x56, planet_index);
                    let class = list.byte(object, 0x14c) | 0x20;
                    list.set_byte(object, 0x14c, class);
                    if (0x51..=0x5a).contains(&model) {
                        let class = list.byte(object, 0x14c) | 0x40;
                        list.set_byte(object, 0x14c, class);
                    }
                    if model != 0x51 && model != 0x52 {
                        list.set_word(object, 0xce, 5);
                    }
                    if matches!(model, 0x55 | 0x56 | 0x59) {
                        list.set_word(object, 0xce, 1);
                    }
                    let size = (list.word(object, 0xce) << 3).wrapping_add(8);
                    list.set_word(object, 0xdc, size);
                    let own = usize::from(list.byte(object, 0x86));
                    list.bytes[own] = 0x0f;
                    list.set_byte(object, 0x57, 1);
                    list.set_byte(object, 0x58, list.stamp.wrapping_sub(1));
                    list.set_dword(object, 0xc2, 0);
                    let _ = self.starport(list, object, sw(system, 0x12), sw(system, 0x22));
                }
            }
            if kind != 0x23 {
                if let Some(planet) = parent_object {
                    let planet_index = list.byte(planet, 0x86);
                    list.set_byte(object, 0x56, planet_index);
                }
                let rate = ffp::divide(
                    Ffp::new(0x5460, -18),
                    ffp::normalise(Ffp::from_dword(sd(system, 0x14))),
                );
                let rate = Ffp::new(rate.mantissa, rate.exponent.wrapping_add(0x40));
                let angular = ffp_rate(rate);
                list.set_dword(object, 0xc2, angular);
                list.set_dword(object, 0xa8, clock[0]);
                list.set_dword(object, 0xac, clock[1]);
                let value = phase(angular, clock[0], clock[1]);
                list.set_dword(object, 0xba, value as u32);
                let high = ((value >> 32) as u32).wrapping_add(u32::from(sw(system, 0x12)) << 16);
                list.set_dword(object, 0xbe, high);
                list.set_word(object, 0xc6, sw(system, 0x22));
                let radius = Ffp::from_dword(list.dword(object, 0xb0));
                self.orbit(
                    list,
                    object,
                    radius,
                    i32::from(sw(system, 0x22) as i16),
                    high,
                );
                let spin = clock[0] >> (0x11 - i32::from(list.byte(object, 0xc8) as i8)) as u32;
                let tilt = i32::from(list.word(object, 0xcc) as i16);
                list.set_matrix(object, matrix_yzt(self.sine, tilt, spin as i32));
            }
            // The name (`strcpy`, so up to and including the NUL).
            let source = &system[at + 0x26..];
            let length = source.iter().position(|b| *b == 0).unwrap_or(source.len());
            let to = list.at(object, 0x124);
            for (k, byte) in source[..length].iter().chain(&[0]).enumerate() {
                list.bytes[to + k] = *byte;
            }
            index += 1;
        }
    }

    fn orbit(&self, list: &mut PhysList, object: usize, radius: Ffp, longitude: i32, phase: u32) {
        list.place_on_orbit(self.sine, object, radius, longitude, phase);
    }
}

/// F1494: an ffp number as an unsigned integer (mantissa shifted by exponent - 15).
fn ffp_rate(value: Ffp) -> u32 {
    if value.exponent <= 15 {
        (i32::from(value.mantissa) as u32).wrapping_shr((15 - i32::from(value.exponent)) as u32)
    } else if value.exponent <= 0x20 {
        ((i32::from(value.mantissa)) << (i32::from(value.exponent) - 15)) as u32
    } else {
        u32::MAX
    }
}
