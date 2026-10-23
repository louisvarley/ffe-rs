//! The collision scan's distance path, ported from `ccoll.txt` and `ffe.asm`:
//! F570 (called by F568/F569) finds the player's distance to the body it orbits
//! (the altitude F1033 stores in `DATA_008867`) and switches it between the
//! linear and the rotating frame of reference (F572, F573) as it enters and
//! leaves the body's influence radius. F575 tests one pair of bodies; F577 is
//! the simple-radius test it uses.
//!
//! Only the paths that never touch anything are ported: a complex shape
//! (`+0x14c` negative; F576) and an actual collision (F577 reporting contact,
//! which leads to the landing, crash and bounce code of F575/F578) return
//! `None`. Verified against the executed original (`docs/notes/collision.md`).

use crate::frames::Bodies;

/// The flag bits of `DATA_008696`.
pub const DIFFERENT_RADII: u8 = 0x01;
pub const DISTANCE_VALID: u8 = 0x02;

/// What F570 reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Contact {
    /// The flags (`DATA_008696`).
    pub flags: u8,
    /// The distance (`DATA_008703` unless invalid, then -1).
    pub distance: i32,
    /// The collision object index (`DATA_008698`; zero when none).
    pub object: u8,
    /// The pad index (`DATA_008701 & 0x1f`).
    pub pad: u8,
}

/// F1522: the high dword of the signed square.
fn square_high(value: i32) -> u32 {
    ((i64::from(value) * i64::from(value)) >> 32) as u32
}

/// A body's two radii (`+0x138` and `+0x140`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Radii {
    pub outer: i64,
    pub inner: i64,
}

/// What F575 finds for a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// Out of each other's reach (F575 returns zero).
    Far,
    /// Touching (F577 reports a collision): the landing, crash and bounce code
    /// follows in the original and is not ported.
    Contact,
    /// In reach without touching: the distance F577 leaves in
    /// `DATA_008704:DATA_008703`, and whether the second body's radii differ.
    Near { distance: i64, different: bool },
}

/// F575 and F577 for the simple-radius case: `None` for a complex shape
/// (`complex`), which is not ported.
pub fn reach(relative: [i64; 3], a: Radii, b: Radii, complex: bool) -> Option<Reach> {
    let magnitude = relative.map(|v| v.wrapping_abs());
    let sum = magnitude[0]
        .wrapping_add(magnitude[1])
        .wrapping_add(magnitude[2])
        >> 1;
    let outside = sum.wrapping_sub(a.outer);
    if outside >= 0 && outside.wrapping_sub(b.outer) >= 0 {
        return Some(Reach::Far);
    }
    if complex {
        return None;
    }
    // F577: the simple-radius test on a copy scaled to fit 32 bits.
    let high = (magnitude[0] >> 32) | (magnitude[1] >> 32) | (magnitude[2] >> 32) | (b.outer >> 32);
    let low =
        (magnitude[0] as u32) | (magnitude[1] as u32) | (magnitude[2] as u32) | (b.outer as u32);
    let shift = if high as u32 != 0 {
        msb(high as u32) as i32 + 3
    } else {
        msb(low) as i32 - 0x1d
    };
    let (axes, radius): ([i32; 3], u32);
    if shift <= 0 {
        let count = (-shift) as u32;
        radius = (a.inner as u32)
            .wrapping_add(b.inner as u32)
            .wrapping_shl(count);
        axes = relative.map(|v| (v as i32).wrapping_shl(count));
    } else {
        let scaled = crate::model_animation::shift_vector(relative, shift as u32);
        axes = scaled.map(|v| v as i32);
        radius = (b.outer.wrapping_add(a.outer) >> shift) as u32;
    }
    let distance_squared = square_high(axes[0])
        .wrapping_add(square_high(axes[1]))
        .wrapping_add(square_high(axes[2]));
    if square_high(radius as i32) > distance_squared {
        return Some(Reach::Contact);
    }
    let mut low = crate::model_math::square_root(distance_squared >> 1).wrapping_mul(2) as i32;
    if low < 0 {
        low = 0;
    }
    let value = i64::from(low);
    let distance = if shift >= 0 {
        value << shift
    } else {
        value >> -shift
    };
    Some(Reach::Near {
        distance,
        different: b.inner as i32 != b.outer as i32,
    })
}

impl Contact {
    /// The altitude F1033 stores in `DATA_008867`: the distance, or
    /// 0x7fffffff when there is none.
    pub fn altitude(&self) -> i32 {
        if self.distance == -1 {
            0x7fff_ffff
        } else {
            self.distance
        }
    }
}

impl Bodies {
    /// F575 for the pair (`a`, `other`) with `relative` the position of `a`
    /// relative to `other`. Returns the original's return value: zero when the
    /// bodies are out of each other's reach, otherwise the flags
    /// (`DATA_008696`, which this updates).
    fn pair(&mut self, flags: &mut u8, relative: [i64; 3], a: usize, other: usize) -> Option<u8> {
        let (pa, pb) = (self.body[a], self.body[other]);
        let radii_a = Radii {
            outer: pa.outer_radius,
            inner: pa.inner_radius,
        };
        let radii_b = Radii {
            outer: pb.outer_radius,
            inner: pb.inner_radius,
        };
        match reach(relative, radii_a, radii_b, pb.shape & 0x80 != 0)? {
            Reach::Contact => None,
            Reach::Far => Some(0),
            Reach::Near {
                distance,
                different,
            } => {
                *flags |= DISTANCE_VALID;
                self.distance = distance;
                if different {
                    *flags |= DIFFERENT_RADII;
                }
                Some(*flags)
            }
        }
    }

    /// F569/F570 for body `a` (the player): the distance to its parent, the
    /// frame switch, and the scan of the other physical bodies in the same frame.
    pub fn collide(&mut self, a: usize) -> Option<Contact> {
        let mut flags = 0u8;
        self.distance = (self.distance & !0xffff_ffff) | 0xffff_ffff;
        let parent = self.body[a].parent;
        let mut before = 0u8;
        if parent != 0 {
            let parent = usize::from(parent);
            let position = self.body[a].position;
            let result = self.pair(&mut flags, position, a, parent)?;
            let delay = self.body[a].delay;
            if delay != 0 {
                let half = (self.frame_tics as i32) >> 1;
                let half = half + i32::from(half < 0 && self.frame_tics & 1 != 0);
                self.body[a].delay = if half > delay { 0 } else { delay - half };
            } else if result & DISTANCE_VALID != 0 {
                if self.body[a].rotating == 0 {
                    self.to_rotating(a, parent);
                    self.body[a].delay = 0x7f;
                }
            } else if self.body[a].rotating != 0 {
                self.to_linear(a, parent);
                self.body[a].delay = 0x7f;
            }
            before = flags;
        }
        let me = self.body[a];
        for k in (1..crate::frames::OBJECTS).rev() {
            if !self.physical[k] || k == usize::from(me.index) {
                continue;
            }
            let other = self.body[k];
            if other.hidden != 0 || me.parent != other.parent {
                continue;
            }
            if other.model == 0x99 && self.body[a].model != 0x99 {
                continue;
            }
            let relative = if self.body[a].rotating != other.rotating {
                let here = self.absolute(a)?;
                let there = self.absolute(k)?;
                std::array::from_fn(|i| here[i].wrapping_sub(there[i]))
            } else {
                std::array::from_fn(|i| self.body[a].position[i].wrapping_sub(other.position[i]))
            };
            self.pair(&mut flags, relative, a, k)?;
        }
        flags |= before;
        let distance = if self.distance >> 32 != 0 || (self.distance as i32) < 0 {
            -1
        } else {
            self.distance as i32
        };
        Some(Contact {
            flags,
            distance,
            object: 0,
            pad: 0,
        })
    }
}

fn msb(value: u32) -> u32 {
    31u32.saturating_sub(value.leading_zeros())
}
