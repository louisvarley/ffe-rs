//! System generation, ported routine by routine from `csysgen.txt` and
//! `ffe.asm` (F872-F897): the list of 68-byte system objects (stars, planets,
//! moons, stations and starports) that F878 builds from a system's seeds.
//! Names are built as the original does: stars and pairs from the system name,
//! planets and moons from their parent's, and the rest by F893 through
//! `names::Names` (without name data the objects are only marked as named, which
//! later decisions read). The tables are the executable's, imported into the
//! content cache (`ffe_core::galaxy::tables`). Verified against the executed
//! original (`docs/notes/sysgen.md`).

use crate::ffp::{self, Ffp};
use crate::galaxy::{tables, Tables};
use crate::handcoded::{Step, STEPS};
use crate::names::Names;

/// Bytes per system object.
pub const OBJECT_BYTES: usize = 68;
/// Objects the generator may write (F881 stops at 0x3c; stations add a few).
pub const CAPACITY: usize = 0x50;
/// Mass above which a body is a star (an integer in the generator's units).
const STAR_MASS: u32 = 0x0103_6640;

/// The inputs of F878 (`SysGenParams`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Params {
    /// `+0`: the first seed; zero selects the hand-coded system.
    pub seed: u32,
    /// `+4`: the second seed, also the system's total mass.
    pub mass: u32,
    /// `+8`: the number of stars.
    pub stars: u32,
    /// `+0xc`: the system code.
    pub code: u32,
    /// `+0x10`: the population-like parameter (`DATA_008854`).
    pub population: i32,
    /// `+0x14`: the system's name, NUL-terminated.
    pub name: [u8; 20],
}

/// F872/F875: the hand-coded Sol-neighbourhood objects (`DATA_008792`, 146
/// objects), built the way the original builds them at start-up.
pub fn handcoded(tables: &Tables, names: Option<&Names>) -> Vec<u8> {
    let mut list = vec![0u8; 146 * OBJECT_BYTES + OBJECT_BYTES];
    let (mut first, mut second) = (tables.dword(tables::SEED), tables.dword(tables::SEED + 4));
    let random = |first: &mut u32, second: &mut u32| {
        let seed = (first.wrapping_shl(16)).wrapping_add(*second & 0xffff);
        *first = (first.wrapping_mul(0xc6b) / 0x27) & 0xffff;
        let value = *second;
        *second = (value.wrapping_mul(0xc41) / 0x27) & 0xffff;
        seed
    };
    for step in STEPS {
        match *step {
            Step::Body(at, a, text) => {
                let at = at as usize;
                let [mass, seed, p4, p5, parent, orbit, p8, p9, p10, _p11, kind, model] = a;
                let object = &mut list[at..at + OBJECT_BYTES];
                put32(object, 0, mass.wrapping_add(1));
                let seed = if seed != 0 {
                    seed.rotate_right(16)
                } else {
                    random(&mut first, &mut second)
                };
                put32(object, 4, seed);
                put_name(object, names, text);
                put16(object, 8, parent as u16);
                put16(object, 0xa, kind as u16);
                put16(object, 0xc, model as u16);
                put32(object, 0xe, orbit);
                put16(object, 0x12, ((p8 << 16) / 0x168) as u16);
                put32(object, 0x14, 0);
                put32(object, 0x18, 0);
                put16(object, 0x20, p4 as u16);
                put16(object, 0x22, ((p5 << 16) / 0xe10) as u16);
                put16(object, 0x24, (p9 as u16).wrapping_add(0x111));
                put16(object, 0x3a, 0);
                put16(object, 0x3c, 0);
                object[0x3e] = p10 as u8;
                object[0x3f] = if parent < 2 { parent as u8 } else { 2 };
                put16(object, 0x40, 0);
                put16(object, 0x42, 0);
            }
            Step::Port(at, [seed, latitude, longitude, parent, model], text) => {
                let at = at as usize;
                let object = &mut list[at..at + OBJECT_BYTES];
                put32(object, 0, 0xfff6_0000);
                let seed = if seed != 0 {
                    seed
                } else {
                    random(&mut first, &mut second)
                };
                put32(object, 4, seed);
                put_name(object, names, text);
                put16(object, 8, parent as u16);
                put16(object, 0xa, 0x23);
                put16(object, 0xc, model as u16);
                put32(object, 0xe, 0);
                let latitude = ((latitude << 16) / 0x8ca0) as u16;
                put16(object, 0x12, latitude.wrapping_neg().wrapping_sub(0x4000));
                put32(object, 0x14, 0);
                put32(object, 0x18, 0);
                put16(object, 0x20, 0);
                put16(object, 0x22, ((longitude << 16) / 0x8ca0) as u16);
                put16(object, 0x24, 0x125);
                put16(object, 0x3a, 0);
                put16(object, 0x3c, 0);
                object[0x3e] = 0;
                object[0x3f] = 3;
                put16(object, 0x40, 0);
                put16(object, 0x42, 0);
            }
            Step::End(at) => put32(&mut list[at as usize..], 0, 0),
        }
    }
    list
}

/// F873/F874's `strcpy` of a hand-coded object's name (`DATA_007139 + offset`).
fn put_name(object: &mut [u8], names: Option<&Names>, offset: u32) {
    let Some(names) = names else { return };
    let text = names.handcoded(offset as usize);
    for (k, byte) in text.iter().chain(&[0]).enumerate() {
        if let Some(slot) = object.get_mut(0x26 + k) {
            *slot = *byte;
        }
    }
}

fn put16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

/// The generator state: the object list and the original's globals.
pub struct Generator<'a> {
    tables: Tables<'a>,
    templates: Vec<u8>,
    /// The object list (`CAPACITY` objects).
    pub list: Vec<u8>,
    /// Objects generated so far (`*objcount`).
    pub count: u32,
    /// The system's random state (`DATA_008843`, `DATA_008844`).
    pub state: [u32; 2],
    generated: u32,
    objects: u8,
    letter: u8,
    depth: u8,
    /// The first-, second- and third-level counters of F881's planet names.
    counters: [u8; 3],
    in_pair: bool,
    /// Objects whose names depend on uninitialised memory in the original: F877's
    /// counters for a pair's first star when the mass is below a star's.
    pub uncertain: Vec<usize>,
    stars_left: u8,
    total_mass: u32,
    code: u32,
    population: i32,
    /// `DATA_008855`: starports and stations made.
    pub starports: u8,
    /// `DATA_008856`: stations made.
    pub stations: u8,
    /// `DATA_007128`: the allegiance F871 reports.
    pub allegiance: u32,
    /// The name data; without it objects are marked as named but keep their bytes.
    names: Option<&'a Names<'a>>,
}

impl<'a> Generator<'a> {
    pub fn new(tables: Tables<'a>) -> Self {
        let templates = handcoded(&tables, None);
        Generator {
            tables,
            templates,
            list: vec![0; CAPACITY * OBJECT_BYTES],
            count: 0,
            state: [0; 2],
            generated: 0,
            objects: 0,
            letter: 0,
            depth: 0,
            counters: [0; 3],
            in_pair: false,
            uncertain: Vec::new(),
            stars_left: 0,
            total_mass: 0,
            code: 0,
            population: 0,
            starports: 0,
            stations: 0,
            allegiance: 0,
            names: None,
        }
    }

    /// Sets the name data (also naming the hand-coded objects the templates hold).
    pub fn set_names(&mut self, names: &'a Names<'a>) {
        self.names = Some(names);
        self.templates = handcoded(&self.tables, Some(names));
    }

    // ---- object field access (offsets as in the 68-byte record) ----

    fn byte(&self, object: usize, at: usize) -> u8 {
        self.list[object * OBJECT_BYTES + at]
    }

    fn set_byte(&mut self, object: usize, at: usize, value: u8) {
        self.list[object * OBJECT_BYTES + at] = value;
    }

    fn word(&self, object: usize, at: usize) -> u16 {
        let i = object * OBJECT_BYTES + at;
        u16::from_le_bytes([self.list[i], self.list[i + 1]])
    }

    fn set_word(&mut self, object: usize, at: usize, value: u16) {
        put16(&mut self.list[object * OBJECT_BYTES..], at, value);
    }

    fn dword(&self, object: usize, at: usize) -> u32 {
        let i = object * OBJECT_BYTES + at;
        u32::from_le_bytes(self.list[i..i + 4].try_into().unwrap())
    }

    fn set_dword(&mut self, object: usize, at: usize, value: u32) {
        put32(&mut self.list[object * OBJECT_BYTES..], at, value);
    }

    fn table(&self, base: usize, index: usize) -> u32 {
        self.tables.dword(base + 4 * index)
    }

    // ---- the routines ----

    /// F894: the generator's random number.
    pub fn random(&mut self) -> u32 {
        let [a, b] = self.state;
        let sum = b.wrapping_add(a);
        let first = a.rotate_left(3).wrapping_add(sum);
        let second = sum.rotate_left(5);
        self.state = [first.wrapping_add(second), second];
        self.state[0]
    }

    /// F896: a table word by the top ten bits of a 14-bit position.
    fn spacing(&self, value: u32) -> u16 {
        let word = value as u16;
        if word > 0x4000 {
            0xffff
        } else {
            self.table(tables::SPACING, usize::from(word >> 6)) as u16
        }
    }

    /// F893: the object is marked as named and, with the name data, named: types 8
    /// and 9 become planet names (0x10) and 6 and 7 starport names (0x11) when the
    /// allegiance is 3.
    fn name(&mut self, object: usize, kind: u32) {
        let flags = self.byte(object, 0x3f) | 0x80;
        self.set_byte(object, 0x3f, flags);
        let Some(names) = self.names else { return };
        let kind = match (self.allegiance, kind) {
            (3, 8 | 9) => 0x10,
            (3, 6 | 7) => 0x11,
            _ => kind,
        };
        let seed = self.dword(object, 4);
        if let Some(mut text) = names.generate(kind, seed) {
            text.push(0);
            let at = object * OBJECT_BYTES + 0x26;
            let end = (at + text.len()).min(self.list.len());
            self.list[at..end].copy_from_slice(&text[..end - at]);
        }
    }

    /// The NUL-terminated text of an object's name field.
    fn text(&self, object: usize) -> Vec<u8> {
        let at = object * OBJECT_BYTES + 0x26;
        let bytes = &self.list[at..];
        bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())].to_vec()
    }

    /// Writes a NUL-terminated name into an object's name field.
    fn put_text(&mut self, object: usize, text: &[u8]) {
        let at = object * OBJECT_BYTES + 0x26;
        for (k, byte) in text.iter().chain(&[0]).enumerate() {
            if let Some(slot) = self.list.get_mut(at + k) {
                *slot = *byte;
            }
        }
    }

    /// F882: the orbit, velocity, seed, mass, angles and temperature of a body.
    fn orbit(&mut self, object: usize, parent: u32, p4: u32, p5: u16, mass: u32) {
        let orbit = split_multiply(p4, i32::from(p5)) as u32;
        self.set_dword(object, 0xe, orbit);
        if orbit != 0 {
            let mut radius = ffp::from_int(orbit as i32);
            radius.exponent = radius.exponent.wrapping_add(0x19);
            self.set_dword(object, 0xe, radius.to_dword());
            if parent != 0 {
                let host = parent as usize - 1;
                let mut radius = radius;
                if self.word(host, 0xa) == 0 && (object as u32).wrapping_sub(1) <= parent {
                    radius.exponent = radius.exponent.wrapping_add(1);
                }
                let host_mass = Ffp::from_dword(self.dword(host, 0));
                let cube = ffp::multiply(ffp::multiply(radius, radius), radius);
                let speed = ffp::sqrt(ffp::divide(cube, host_mass));
                let scaled = (i32::from(speed.mantissa) * 0x5edb) >> 15;
                let value = Ffp::new(scaled as i16, speed.exponent);
                self.set_dword(object, 0x14, value.to_dword());
            } else {
                self.set_dword(object, 0x14, 0);
            }
            self.set_word(object, 0x12, self.state[0] as u16);
        }
        self.set_dword(object, 4, self.state[1]);
        self.generated += 1;
        self.set_word(object, 8, parent as u16);
        self.set_dword(object, 0, ffp::from_int(mass as i32).to_dword());
        let exponent = self.word(object, 2).wrapping_add(0x3f);
        self.set_word(object, 2, exponent);
        // The original squares a 16-bit value and shifts the low word by 24.
        self.set_word(object, 0x20, 0);
        let a = self.state[0] as u16;
        let rotated = a.rotate_right(3);
        let squared = i32::from(rotated).wrapping_mul(i32::from(rotated)) >> 20;
        let mut longitude = squared as u16;
        if parent != 0 {
            longitude = longitude.wrapping_add(self.word(parent as usize - 1, 0x42));
        }
        self.set_word(object, 0x22, longitude);
        let seed = self.dword(object, 4);
        let product = seed.wrapping_mul(seed);
        self.set_word(object, 0x42, ((product as u16 as i16 as i32) >> 17) as u16);
        let orientation = self.word(object, 0x42);
        self.set_byte(object, 0x3e, (orientation & 3) as u8);
        self.set_byte(object, 0x3f, self.depth);
        if self.word(object, 0x24) >= 0x3e8 {
            return;
        }
        // The light of every star above this body warms it.
        let mut glow = Ffp::new(0, 0);
        let mut at = object;
        loop {
            let parent = self.word(at, 8);
            if parent == 0 {
                break;
            }
            let host = parent as usize - 1;
            if self.dword(host, 0x1c) != 0 {
                let mut light = ffp::from_int(self.dword(host, 0x1c) as i32);
                light.exponent = light.exponent.wrapping_add(0x32);
                let light = ffp::divide(light, Ffp::from_dword(self.dword(at, 0xe)));
                glow = ffp::add(glow, ffp::multiply(light, light));
            }
            at = host;
        }
        let heat = ffp::to_int(ffp::sqrt(ffp::sqrt(glow)));
        let temperature = self.word(object, 0x24).wrapping_add(heat as u16);
        self.set_word(object, 0x24, temperature);
    }

    /// F881: a star, planet or moon of the given mass; returns its shape word.
    fn body(&mut self, mass: u32, parent: u32, p5: u32, p6: u16, name: &[u8]) -> u16 {
        let object = self.count as usize;
        if self.count >= 0x3c {
            return 0xffff;
        }
        self.objects = self.objects.wrapping_add(1);
        let mut index = 0;
        while self.table(tables::MASS, index) != 0 && self.table(tables::MASS, index) <= mass {
            index += 1;
        }
        self.set_dword(object, 0x1c, self.table(tables::LIGHT, index));
        self.set_word(object, 0xa, self.table(tables::TYPE, index) as u16);
        self.set_word(object, 0x24, self.table(tables::TEMPERATURE, index) as u16);
        self.set_word(object, 0xc, self.table(tables::MODEL, index) as u16);
        let shape = self.table(tables::SHAPE, index) as u16;
        let handler = HANDLERS.get(index).copied().unwrap_or(0);
        if mass >= STAR_MASS {
            let mut text = name.to_vec();
            text.push(b' ');
            self.letter = self.letter.wrapping_add(1);
            if parent != 0 {
                text.push(self.letter.wrapping_add(0x40));
            }
            self.put_text(object, &text);
        } else if parent != 0 && self.byte(parent as usize - 1, 0x3f) & 0x80 != 0 {
            self.set_dword(object, 4, self.state[1]);
            self.name(object, 0xa);
        } else {
            if self.in_pair {
                self.uncertain.push(object);
            }
            let mut text = self.text(parent.wrapping_sub(1) as usize);
            match self.depth {
                0 => {
                    self.counters[0] = self.counters[0].wrapping_add(1);
                    let n = self.counters[0];
                    if n > 9 {
                        text.push(b'1');
                        text.push(n.wrapping_add(0x26));
                    } else {
                        text.push(n + 0x30);
                    }
                }
                1 => {
                    self.counters[1] = self.counters[1].wrapping_add(1);
                    text.push(self.counters[1].wrapping_add(0x60));
                }
                _ => {
                    self.counters[2] = self.counters[2].wrapping_add(1);
                    text.push(self.counters[2].wrapping_add(0x30));
                }
            }
            self.put_text(object, &text);
        }
        self.orbit(object, parent, p5, p6, mass);
        self.handle(handler, mass);
        self.count += 1;
        shape
    }

    fn handle(&mut self, handler: u16, mass: u32) {
        match handler {
            883 => self.rocky(mass),
            884 => self.small_rocky(),
            885 => self.barren(),
            886 => self.small_barren(),
            887 => self.asteroid(),
            888 => self.red_giant(),
            889 => self.yellow_star(),
            890 => self.white_star(),
            _ => {}
        }
    }

    fn temperature(&self) -> u16 {
        self.word(self.count as usize, 0x24)
    }

    fn set_type(&mut self, kind: u16, model: u16) {
        let object = self.count as usize;
        self.set_word(object, 0xc, model);
        self.set_word(object, 0xa, kind);
    }

    /// F883: rocky planets.
    fn rocky(&mut self, mass: u32) {
        let object = self.count as usize;
        let here = self.count;
        let population = self.population;
        let temperature = self.temperature();
        if temperature > 0x139 {
            self.set_type(0xc, 0x7b);
            self.set_word(object, 0x24, temperature.wrapping_add((mass >> 1) as u16));
            return;
        }
        if temperature <= 0xd7 {
            if temperature >= 0xaa {
                self.set_word(object, 0x24, temperature.wrapping_add(0x19));
                if population < 0xc8 {
                    self.set_type(6, 0x7b);
                    if !matches!(self.code, 0x1aa8_9738 | 0x02ae_1718 | 0x0eb6_973d) {
                        return;
                    }
                    self.set_word(object, 0xc, 0x85);
                    if self.code == 0x02ae_1718 && self.dword(object, 4) == 0x5b5f_f290 {
                        self.station(3, here + 1);
                    }
                    if self.code == 0x0eb6_973d && self.dword(object, 4) == 0x7074_0190 {
                        self.station(4, here + 1);
                    }
                    return;
                }
                self.terraformed(here);
                return;
            }
            if (0x42..=0x7b).contains(&temperature) {
                self.set_word(object, 0x24, temperature.wrapping_add(0x16));
                self.set_type(7, 0x7c);
                return;
            }
            self.small_rocky();
            return;
        }
        self.set_word(object, 0x24, temperature.wrapping_add(0x23));
        self.random();
        let draw = (self.state[0] & 0xff) >> 2;
        let limit = u32::from(self.temperature()).wrapping_sub(0xfa);
        if draw > limit {
            if population < 0x78 {
                self.set_type(8, 0x80);
                return;
            }
            self.terraformed(here);
            return;
        }
        if population < 4 {
            return;
        }
        self.name(object, 0x9);
        if population < 0x10 {
            return;
        }
        if self.state[0] & 0xf0 == 0 {
            self.set_word(object, 0xc, 0x84);
        }
        let temperature = self.temperature();
        if temperature > 0x118 {
            if temperature > 0x134 {
                self.set_word(object, 0xc, 0x83);
                if self.temperature() > 0x152 {
                    self.set_word(object, 0xc, 0x82);
                }
            }
            if self.state[0] & 0xf == 0 {
                self.set_word(object, 0xc, 0x80);
                self.starports(0x24, population >> 5);
            } else {
                self.starports(0, population >> 5);
            }
            if population < 0x1e {
                return;
            }
            let kind = if population >= 0xf2 {
                2
            } else if population >= 0x32 {
                1
            } else {
                0
            };
            self.station(kind, here + 1);
            return;
        }
        self.set_word(object, 0xc, 0x7f);
        self.starports(0x18, population >> 6);
        if population < 0x28 {
            return;
        }
        self.station(u32::from(population >= 0x50), here + 1);
    }

    /// The terraformed branch F883 and F884 share.
    fn terraformed(&mut self, here: u32) {
        let object = self.count as usize;
        let population = self.population;
        if self.temperature() < 0x127 {
            self.set_word(object, 0x24, 0x127);
        }
        self.set_type(0xb, 0x7e);
        self.name(object, 0x8);
        self.starports(0, population >> 5);
        if population < 0x1e {
            return;
        }
        let kind = if population >= 0xf2 {
            2
        } else if population >= 0x32 {
            1
        } else {
            0
        };
        self.station(kind, here + 1);
    }

    /// F884: small rocky planets.
    fn small_rocky(&mut self) {
        let object = self.count as usize;
        let here = self.count;
        let population = self.population;
        let temperature = self.temperature().wrapping_add(0x14);
        self.set_word(object, 0x24, temperature);
        if population >= 0x96 && (0x102..=0x139).contains(&temperature) {
            if temperature < 0x127 {
                self.set_word(object, 0x24, 0x127);
            }
            self.set_type(9, 0x7e);
            self.name(object, 0x8);
            self.starports(0, population >> 6);
            if population < 0x1e {
                return;
            }
            let kind = if population >= 0xb4 {
                2
            } else if population >= 0x32 {
                1
            } else {
                0
            };
            self.station(kind, here + 1);
            return;
        }
        self.set_type(4, 0x79);
        if temperature > 0x212 {
            return;
        }
        let warm = (population >= 0xc8 && temperature >= 0xa0)
            || (temperature >= 0xb4 && population > 6)
            || (population >= 0xfa && temperature > 0x87);
        if !warm {
            return;
        }
        if (self.word(object, 2) as i16) > 0x47 {
            self.set_word(object, 2, 0x47);
        }
        self.name(object, 0xa);
        if population < 0x1e {
            return;
        }
        self.starports(0xc, 0);
        if population < 0x64 {
            return;
        }
        self.station(u32::from(population >= 0xaa), here + 1);
    }

    /// F885: barren rocky planetoids.
    fn barren(&mut self) {
        let object = self.count as usize;
        let here = self.count;
        let population = self.population;
        let temperature = self.temperature();
        if !(0x96..=0x1f4).contains(&temperature) {
            return;
        }
        if population < 0xc8 && (temperature < 0xc8 || population <= 8) {
            return;
        }
        self.name(object, 0xa);
        if population < 0x2d {
            return;
        }
        self.starports(0xc, 0);
        if population < 0x96 {
            return;
        }
        self.station(0, here + 1);
    }

    /// F886: small barren spheres of rock.
    fn small_barren(&mut self) {
        let object = self.count as usize;
        let population = self.population;
        if self.temperature() < 0xdc || population <= 0x32 {
            return;
        }
        self.name(object, 0xa);
        if population < 0x50 {
            return;
        }
        self.starports(0xc, 0);
    }

    /// F887: asteroidal bodies.
    fn asteroid(&mut self) {
        let object = self.count as usize;
        self.set_word(object, 0, 0);
        self.set_word(object, 2, 0x38);
        if self.temperature() < 0xdc || self.population <= 0x32 {
            return;
        }
        self.name(object, 0xa);
    }

    /// F888: a red giant may become a white dwarf.
    fn red_giant(&mut self) {
        if self.generated <= 2 {
            return;
        }
        let object = self.count as usize;
        self.set_word(object, 0xc, 0x94);
        self.set_word(object, 0xa, 0x19);
        self.set_dword(object, 0x1c, 0x3e2);
        self.set_word(object, 0x24, 0x2c09);
        let exponent = self.word(object, 2).wrapping_sub(1);
        self.set_word(object, 2, exponent);
    }

    /// F889: a yellow star may become white.
    fn yellow_star(&mut self) {
        if self.random() & 0x8000_0000 != 0 {
            self.white_star();
        }
    }

    /// F890: a white star may become a red giant.
    fn white_star(&mut self) {
        if self.random() & 0x8000_0000 != 0 {
            self.red_giant();
        }
    }

    /// F891: starports around the body at `count` (named 'starport').
    fn starports(&mut self, kind: u32, population: i32) {
        let (mut a, mut b) = (self.state[0], self.state[1]);
        let saved_population = self.population;
        let parent = self.count + 1;
        if self.starports as i8 >= 0x12 {
            return;
        }
        let draw = (u64::from(b & 0xffff) * (population as i64 as u64)) as u32 >> 16;
        let draw = draw.min(3);
        let mut step = 0;
        while step <= draw {
            b = b.wrapping_add(a);
            a = a.rotate_left(5).wrapping_add(b);
            a = a.rotate_left(16).wrapping_add(b);
            self.count += 1;
            let object = self.count as usize;
            let template = 16 * OBJECT_BYTES;
            self.list[object * OBJECT_BYTES..(object + 1) * OBJECT_BYTES]
                .copy_from_slice(&self.templates[template..template + OBJECT_BYTES]);
            self.set_word(object, 0, 0);
            self.set_word(object, 2, 0x39);
            self.set_word(object, 8, parent as u16);
            self.set_dword(object, 4, a);
            self.generated += 1;
            let slot = (self.generated & 0xf) as usize;
            self.set_word(
                object,
                0x22,
                self.tables.dword(tables::STARPORT_LONGITUDE + 8 * slot) as u16,
            );
            self.set_word(
                object,
                0x12,
                self.tables.dword(tables::STARPORT_LATITUDE + 8 * slot) as u16,
            );
            self.name(object, 0x6);
            let model_index = ((saved_population >> 5) & 7) as u32
                + kind
                + ((self.dword(object, 4) & 0x3_0000) >> 16);
            let model = self
                .tables
                .dword(tables::STARPORT_MODEL + 4 * model_index as usize)
                as u16;
            self.set_word(object, 0xc, model);
            self.starports = self.starports.wrapping_add(1);
            if self.starports as i8 >= 0x12 {
                break;
            }
            step += 1;
        }
    }

    /// F892: a station of the given kind orbiting object `parent` (1-based).
    fn station(&mut self, kind: u32, parent: u32) {
        if self.starports as i8 >= 0x12 {
            return;
        }
        self.starports = self.starports.wrapping_add(1);
        self.stations = self.stations.wrapping_add(1);
        self.count += 1;
        let object = self.count as usize;
        let (template, model): (usize, u16) = match kind {
            0 => (2, self.table_station(0)),
            1 => (17, self.table_station(1)),
            2 => (12, self.table_station(2)),
            3 => (2, 0x45),
            _ => (12, 0x4f),
        };
        let from = template * OBJECT_BYTES;
        self.list[object * OBJECT_BYTES..(object + 1) * OBJECT_BYTES]
            .copy_from_slice(&self.templates[from..from + OBJECT_BYTES]);
        self.set_word(object, 0xc, model);
        self.set_word(object, 0, 0);
        self.set_word(object, 2, 0x39);
        self.set_dword(object, 4, self.state[0]);
        self.set_word(object, 8, parent as u16);
        self.generated += 1;
        let radius = Ffp::new(0x7fff, 0x22);
        self.set_dword(object, 0xe, radius.to_dword());
        let host = usize::from(self.word(object, 8)) - 1;
        let cube = ffp::multiply(ffp::multiply(radius, radius), radius);
        let speed = ffp::sqrt(ffp::divide(cube, Ffp::from_dword(self.dword(host, 0))));
        let scaled = (i32::from(speed.mantissa) * 0x5edb) >> 15;
        self.set_dword(
            object,
            0x14,
            Ffp::new(scaled as i16, speed.exponent).to_dword(),
        );
        if kind < 3 {
            self.name(object, 0x7);
        }
    }

    fn table_station(&self, kind: usize) -> u16 {
        let slot = ((self.state[0] & 0x300) >> 8) as usize;
        self.tables
            .dword(tables::STATION_MODEL + 16 * kind + 4 * slot) as u16
    }

    /// F877: a pair of stars.
    fn pair(&mut self, parent: u32, mass: u32, p5: u32, p6: u16, p7: u16, name: &[u8]) -> u16 {
        // F881's counters are locals of F877's own frame.
        let counters = std::mem::take(&mut self.counters);
        self.in_pair = true;
        let shape = self.pair_in(parent, mass, p5, p6, p7, name);
        self.in_pair = false;
        self.counters = counters;
        shape
    }

    fn pair_in(&mut self, parent: u32, mass: u32, p5: u32, p6: u16, p7: u16, name: &[u8]) -> u16 {
        if mass >= 0xffff_ffef || (0x0a21_fe80..0x0a6e_49c0).contains(&mass) {
            self.body(mass, parent, p5, p6, name);
            let first = self.count;
            self.body(mass >> 2, first, p5, p7, name);
            return p7 << 3;
        }
        self.random();
        let draw = (((self.state[0] as u16 | 0x8000) as i16 >> 2) as u16) as u32;
        let extra = draw.wrapping_mul(mass >> 16).max(STAR_MASS);
        let centre = self.count as usize;
        self.orbit(centre, parent, p5, p6, extra.wrapping_add(mass));
        self.set_word(centre, 0xa, 0);
        self.set_word(centre, 0xc, 0x17);
        let mut text = name.to_vec();
        text.push(if text.len() > 0xb { b'.' } else { b' ' });
        text.push(self.letter.wrapping_add(0x41));
        text.push(b',');
        text.push(self.letter.wrapping_add(0x42));
        self.put_text(centre, &text);
        self.count += 1;
        self.body(mass, centre as u32 + 1, p5, p7, name);
        let count = self.count as usize;
        self.set_word(count - 1, 0x12, 0);
        self.body(extra, centre as u32 + 1, p5, p7, name);
        let count = self.count as usize;
        self.set_word(count - 1, 0x12, 0x7fff);
        // JJFFE's fix: the pair's light is the sum of its stars'.
        let light = self
            .dword(count - 1, 0x1c)
            .wrapping_add(self.dword(count - 2, 0x1c));
        self.set_dword(count - 3, 0x1c, light);
        p7 << 3
    }

    /// F897: the planets (and, past a star's reach, further stars) around a body.
    #[allow(clippy::too_many_arguments)]
    fn planets(
        &mut self,
        parent: u32,
        random: u32,
        p5: u32,
        p6: u32,
        p7: u32,
        name: &[u8],
        mut counters: [u8; 3],
    ) {
        let reach = if p6 > 0x7fff_ffff {
            u32::MAX
        } else {
            p6.wrapping_add(p6)
        };
        let mut level = self.spacing(p5);
        let mut edi: u32 = 0;
        let mut si = p5 as u16;
        loop {
            edi = edi.wrapping_add(u32::from(si));
            if edi >= 0x10000 {
                break;
            }
            let draw = ((self.random() & 0xffff) >> 3) + (random & 0xffff);
            let scaled = if draw >= 0x10000 {
                u32::from(si)
            } else {
                u32::from(si).wrapping_mul(draw) >> 16
            };
            edi = (u32::from(si) >> 2) + (u32::from(si) >> 3) + scaled;
            if u32::from(si) + edi >= 0x10000 {
                continue;
            }
            let mut mix = self.random() & 0xffff;
            mix = mix.wrapping_mul(mix) >> 16;
            mix = mix.wrapping_mul(mix) >> 16;
            let target = u32::from(self.spacing(edi.wrapping_add(u32::from(si))));
            let step = target.wrapping_sub(u32::from(level)).wrapping_mul(mix) >> 16;
            level = level.wrapping_add(step as u16);
            let mut mass = split_multiply(p7, i32::from(step as u16)) as u32;
            if mass > 0x20 {
                if mass < 0x3a98 {
                    mass >>= 3;
                }
                let before = self.count;
                let shape;
                if mass > STAR_MASS && self.stars_left > 1 {
                    mass = mass.min(self.total_mass >> 1);
                    self.random();
                    let draw16 = ((self.state[1] & 0xffff) >> 11) as u16;
                    self.pair(
                        parent,
                        mass,
                        reach,
                        (edi >> 1).wrapping_add(u32::from(si)) as u16,
                        draw16,
                        name,
                    );
                    self.stars_left = self.stars_left.wrapping_sub(2);
                    shape = 0x3000u16;
                    mass >>= 2;
                } else {
                    if mass > STAR_MASS {
                        if self.stars_left != 0 {
                            self.stars_left -= 1;
                            let half = self.total_mass >> 1;
                            if mass > half {
                                mass = half;
                                if mass < STAR_MASS {
                                    mass = STAR_MASS;
                                }
                            }
                        } else {
                            mass = (self.random() & 0xffff) * 259;
                        }
                    }
                    self.counters = counters;
                    shape = self.body(
                        mass,
                        parent,
                        reach,
                        (edi >> 1).wrapping_add(u32::from(si)) as u16,
                        name,
                    );
                    counters = self.counters;
                }
                if mass <= 0x3a98 {
                    mass <<= 3;
                }
                let child_limit = reach >> 6;
                let child_reach = mass.min(child_limit);
                let saved = (self.state, self.depth);
                self.depth = if mass > STAR_MASS {
                    0
                } else {
                    self.depth.wrapping_add(1)
                };
                let rnd = self.random();
                let child_name = self.text(before as usize);
                self.planets(
                    before + 1,
                    rnd,
                    u32::from(shape),
                    child_reach,
                    mass >> 7,
                    &child_name,
                    counters,
                );
                self.state = saved.0;
                self.depth = saved.1;
            }
            si = si.wrapping_add(edi as u16);
        }
    }

    /// F878 for a hand-coded system: the run of `DATA_008792` objects that
    /// the system code selects through `DATA_007133` (143 when it has no entry)
    /// is converted to the generated form. The random state is left alone.
    fn copy_handcoded(&mut self) {
        let mut at = 0x8f;
        for pair in 0..12 {
            let code = self.tables.dword(tables::HANDCODED_INDEX + 8 * pair);
            if code == 0 {
                break;
            }
            if code == self.code {
                at = self.tables.dword(tables::HANDCODED_INDEX + 8 * pair + 4) as usize;
                break;
            }
        }
        while get32(&self.templates, at * OBJECT_BYTES) != 0 {
            let source: Vec<u8> =
                self.templates[at * OBJECT_BYTES..(at + 1) * OBJECT_BYTES].to_vec();
            let object = self.count as usize;
            let kind = u16::from_le_bytes([source[0xa], source[0xb]]);
            if kind != 0x23 {
                let mass = ffp::from_int(get32(&source, 0).wrapping_sub(1) as i32);
                self.set_dword(object, 0, mass.to_dword());
                let exponent = self.word(object, 2).wrapping_add(0x3f);
                self.set_word(object, 2, exponent);
                self.set_dword(object, 4, get32(&source, 4));
                self.set_word(object, 8, get16(&source, 8));
                self.set_word(object, 0xa, kind);
                let orbit = ffp::from_int(get32(&source, 0xe) as i32);
                self.set_dword(object, 0xe, orbit.to_dword());
                let exponent = self.word(object, 0x10).wrapping_add(0x19);
                self.set_word(object, 0x10, exponent);
                for field in [0x12, 0x20, 0x22, 0x24, 0x3a, 0x3c, 0x42, 0xc] {
                    self.set_word(object, field, get16(&source, field));
                }
                self.set_byte(object, 0x3e, source[0x3e]);
                self.objects = self.objects.wrapping_add(1);
                if (0x20..=0x22).contains(&kind) {
                    self.starports = self.starports.wrapping_add(1);
                    self.stations = self.stations.wrapping_add(1);
                }
                let parent = self.word(object, 8);
                if parent != 0 {
                    let radius = Ffp::from_dword(self.dword(object, 0xe));
                    let host_mass = Ffp::from_dword(self.dword(parent as usize - 1, 0));
                    let cube = ffp::multiply(ffp::multiply(radius, radius), radius);
                    let speed = ffp::sqrt(ffp::divide(cube, host_mass));
                    let scaled = (i32::from(speed.mantissa) * 0x5edb) >> 15;
                    self.set_dword(
                        object,
                        0x14,
                        Ffp::new(scaled as i16, speed.exponent).to_dword(),
                    );
                } else {
                    self.set_dword(object, 0x14, 0);
                }
                let length = source[0x26..]
                    .iter()
                    .position(|b| *b == 0)
                    .unwrap_or(source.len() - 0x26);
                self.put_text(object, &source[0x26..0x26 + length]);
            } else {
                self.list[object * OBJECT_BYTES..(object + 1) * OBJECT_BYTES]
                    .copy_from_slice(&source);
                self.starports = self.starports.wrapping_add(1);
            }
            self.count += 1;
            at += 1;
        }
        let end = self.count as usize;
        self.set_dword(end, 0, 0);
    }

    /// F878: generate a system's objects. Returns the number of objects (the
    /// list is zero-terminated). A zero first seed selects a hand-coded system.
    pub fn generate(&mut self, params: &Params) -> Result<u32, String> {
        self.count = 0;
        self.generated = 0;
        self.objects = 0;
        self.letter = 0;
        self.depth = 0;
        self.counters = [0; 3];
        self.uncertain.clear();
        self.starports = 0;
        self.stations = 0;
        self.total_mass = params.mass;
        self.code = params.code;
        self.population = params.population;
        self.allegiance = self.tables.control(params.code, [0, 0]).allegiance;
        let mass = params.mass;
        let length = params.name.iter().position(|b| *b == 0).unwrap_or(20);
        let name = params.name[..length].to_vec();
        if params.seed == 0 {
            self.copy_handcoded();
            return Ok(self.count);
        }
        self.state = [params.seed, params.mass];
        self.stars_left = params.stars as u8;
        match params.stars {
            0 => {
                let shape = self.body(mass, 0, 0, 0, &name);
                if shape == 0 {
                    return Ok(self.count);
                }
                let rnd = self.random();
                self.planets(1, rnd, u32::from(shape), mass, mass >> 8, &name, [0; 3]);
            }
            1 => {
                self.stars_left = self.stars_left.wrapping_sub(1);
                self.random();
                let scale = (self.state[1] & 0xffff) >> 3;
                let second = if mass >> 24 & 0xe0 != 0 {
                    u32::MAX
                } else {
                    mass << 3
                };
                let shape = self.pair(0, mass, second, 0, scale as u16, &name);
                let rnd = self.random();
                self.planets(1, rnd, u32::from(shape), second, mass >> 6, &name, [0; 3]);
            }
            _ => {
                self.random();
                let word = ((self.state[1] & 0xffff) >> 8) as u16;
                self.pair(0, mass, u32::MAX, 0, word, &name);
                let shape = word << 4;
                self.stars_left = self.stars_left.wrapping_sub(1);
                let mut next = mass.wrapping_add(0x081b_3200);
                if self.stars_left > 2 {
                    next = u32::MAX;
                }
                self.random();
                let rnd = ((self.state[0] as u16 | 0x8000) as i16 >> 2) as i32 as u32;
                self.planets(1, rnd, u32::from(shape), u32::MAX, next, &name, [0; 3]);
            }
        }
        let end = self.count as usize;
        self.set_dword(end, 0, 0);
        Ok(self.count)
    }
}

fn get16(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn get32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// F1515: `a * b >> 16` with `a` as two 16-bit halves.
fn split_multiply(a: u32, b: i32) -> i32 {
    let low = ((a & 0xffff) as i32).wrapping_mul(b) as u32 >> 16;
    let high = ((a >> 16) as i32).wrapping_mul(b);
    (low as i32).wrapping_add(high)
}

/// The handler (F-number) of each mass class (`DATA_007125`).
const HANDLERS: [u16; 23] = [
    0, 887, 886, 885, 884, 883, 0, 0, 0, 0, 0, 0, 0, 0, 0, 888, 889, 890, 890, 0, 0, 0, 0,
];
