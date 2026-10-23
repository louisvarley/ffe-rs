//! The name generator, ported from `cname.txt` and `ffe.asm` (F791-F800, F921,
//! with the part of the string expander F1348 the name strings use): first names
//! and surnames, planet, station and starport names, and ship identifiers. The
//! strings come from the cache's `base:text/names`. Verified against the executed
//! original (`docs/notes/names.md`).

/// Name types F791 takes.
pub mod kind {
    pub const MALE_FULL: u32 = 1;
    pub const FEMALE_FULL: u32 = 2;
    pub const FIRST_OR_FULL: u32 = 3;
    pub const SURNAME: u32 = 4;
    pub const FIRST: u32 = 5;
    pub const PLANET: u32 = 0x10;
    pub const STATION: u32 = 0x11;
    pub const SHIP: u32 = 0x12;
    pub const MILITARY_SHIP: u32 = 0x13;
    pub const UNKNOWN_SHIP: u32 = 0x14;
}

/// The two-word state F921 steps (`p3` and the local `s1` of F791).
#[derive(Debug, Clone, Copy)]
struct Seeds {
    a: u32,
    b: u32,
}

impl Seeds {
    /// F921: a pseudo-random number below `range`.
    fn next(&mut self, range: u32) -> u32 {
        self.a = self.a.wrapping_add(self.b);
        self.b = self.b.rotate_left(5);
        self.a = self.a.wrapping_add(self.b).rotate_left(16);
        (self.a & 0xffff).wrapping_mul(range) >> 16
    }
}

/// The name data of the cache.
pub struct Names<'a> {
    ranges: [u32; 30],
    strings: Vec<&'a [u8]>,
}

impl<'a> Names<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.len() < 120 {
            return Err("Name text is too short.".to_owned());
        }
        let ranges = std::array::from_fn(|k| {
            u32::from_le_bytes(bytes[4 * k..4 * k + 4].try_into().unwrap())
        });
        let strings: Vec<&[u8]> = bytes[120..]
            .strip_suffix(&[0])
            .ok_or("Name text does not end in a terminator.")?
            .split(|b| *b == 0)
            .collect();
        if strings.len() != 510 {
            return Err("Name text must hold 510 strings.".to_owned());
        }
        Ok(Self { ranges, strings })
    }

    /// F800 with F1348: string `index & 0x1ff`, its `FF 30 16` code replaced by the
    /// surname part held in the variables.
    fn expand(&self, index: u32, surname: &[u8]) -> Vec<u8> {
        let source = self.strings[(index & 0x1ff) as usize];
        let mut out = Vec::new();
        let mut at = 0;
        while at < source.len() {
            if source[at] == 0xff && source[at + 1..].starts_with(&[0x30, 0x16]) {
                // F1343 runs F77 over what the code wrote: a bracket code (`[` and
                // two letters) leaves the bracket and drops the letters, as in the
                // one name string that holds one ("M[nich").
                let mut piece = 0;
                while piece < surname.len() {
                    out.push(surname[piece]);
                    piece += if surname[piece] == b'[' { 3 } else { 1 };
                }
                at += 3;
            } else {
                out.push(source[at]);
                at += 1;
            }
        }
        out
    }

    /// F793: a surname, either a plain surname or a first name with "s" and "on".
    fn surname(&self, seeds: &mut Seeds) -> Vec<u8> {
        if seeds.a & 0xff >= 0x19 {
            self.expand(0x59 + seeds.next(0x8a), &[])
        } else {
            let mut out = self.expand(0x4f + seeds.next(0xa), &[]);
            out.push(b's');
            if seeds.a >> 8 & 8 == 0 {
                out.extend_from_slice(b"on");
            }
            out
        }
    }

    /// F791 for the name types that need no game state (1 to 0x14); `None` for the
    /// others (the commander, ship type and crew names). `seed` is the random seed.
    pub fn generate(&self, kind: u32, seed: u32) -> Option<Vec<u8>> {
        let mut seeds = Seeds { a: seed, b: 0 };
        if kind < 6 {
            seeds.b = seeds.a.rotate_left(16).wrapping_add(seeds.a);
            seeds.a = seeds.a.rotate_left(7);
        } else if kind < 0x16 {
            let mut t = seeds.a.rotate_left(16);
            seeds.a = seeds.a.wrapping_add(t);
            t = t.rotate_left(7);
            seeds.a = seeds.a.wrapping_add(t);
            t = t.rotate_left(4);
            seeds.a = seeds.a.wrapping_add(t);
            seeds.b = t;
        }
        Some(match kind {
            1 | 2 => {
                let first = if kind == 1 {
                    seeds.next(0x69)
                } else {
                    seeds.next(0x3c) + 0xe0
                };
                let mut out = self.expand(first, &[]);
                out.push(b' ');
                out.extend(self.surname(&mut seeds));
                out
            }
            4 => {
                seeds.next(4);
                self.expand(seeds.next(0x8a) + 0x59, &[])
            }
            3 | 5 => {
                let first = if seeds.next(0xff) & 0x40 != 0 {
                    seeds.next(0x3c) + 0xe0
                } else {
                    seeds.next(0x69)
                };
                let mut out = self.expand(first, &[]);
                if kind == 3 {
                    out.push(b' ');
                    out.extend(self.surname(&mut seeds));
                }
                out
            }
            6..=14 => {
                let surname = self.surname(&mut seeds);
                let index =
                    seeds.next(self.ranges[12 + kind as usize]) + self.ranges[kind as usize];
                self.expand(index, &surname)
            }
            0x10 | 0x11 => {
                let surname = self.expand(0x18a + seeds.next(0x58), &[]);
                let (count, base) = if kind == 0x10 {
                    (self.ranges[28], self.ranges[16])
                } else {
                    (self.ranges[29], self.ranges[17])
                };
                let index = seeds.next(count) + base;
                self.expand(index, &surname)
            }
            0x12 => {
                let mut out = Vec::new();
                for _ in 0..2 {
                    out.push(0x41 + seeds.next(0x1a) as u8);
                }
                out.push(b'-');
                for _ in 0..3 {
                    out.push(0x30 + seeds.next(0xa) as u8);
                }
                out
            }
            0x13 => {
                let mut out = b" +".to_vec();
                for _ in 0..3 {
                    out.push(0x30 + seeds.next(0xa) as u8);
                }
                out.push(0x41 + seeds.next(0x1a) as u8);
                out
            }
            0x14 => {
                let mut out = Vec::new();
                for _ in 0..2 {
                    out.push(0x41 + seeds.next(0x1a) as u8);
                }
                out.extend_from_slice(b"*  ");
                out.push(0x30 + seeds.next(0xa) as u8);
                out
            }
            0 | 0xf | 0x15 => b"????".to_vec(),
            _ => return None,
        })
    }
}
