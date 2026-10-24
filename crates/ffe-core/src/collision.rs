//! The collision scan, ported from `ccoll.txt` and `ffe.asm`: F570 (called by
//! F568/F569) finds the player's distance to the body it orbits (the altitude
//! F1033 stores in `DATA_008867`) and switches it between the linear and the
//! rotating frame of reference (F572, F573) as it enters and leaves the body's
//! influence radius. F575 tests one pair of bodies; F577 is the simple-radius
//! test it uses and F578 the response to touching something other than the
//! body's parent.
//!
//! A complex shape (`+0x14c` negative) goes to F576 (`surface.rs`). A touch goes
//! through the landing, crash and bounce code of `landing.rs`; without the model
//! records a touch is declined. Verified against the executed original
//! (`docs/notes/collision.md`).

use crate::frames::{Bodies, Body};
use crate::landing::{self, Pair as LandingPair, Surface, Touch};
use crate::surface::{Hull, Registers, Scene, SurfaceTools};
use crate::terrain::Terrain;

/// The flag bits of `DATA_008696`.
pub const DIFFERENT_RADII: u8 = 0x01;
pub const DISTANCE_VALID: u8 = 0x02;
pub const LANDING: u8 = 0x40;
pub const CRASH: u8 = 0x80;

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
    /// The last touch the scan resolved, if any pair touched.
    pub touch: Option<Touch>,
    /// The damage the touches dealt: the amounts F953 was called with, added.
    pub damage: i64,
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
    /// follows in the original (`landing.rs`), with the sum of squares F577
    /// leaves.
    Contact { squares: u32 },
    /// In reach of a complex shape: F576 decides (`surface.rs`).
    Shape,
    /// In reach without touching: the distance F577 leaves in
    /// `DATA_008704:DATA_008703`, and whether the second body's radii differ.
    Near { distance: i64, different: bool },
}

/// F575 and F577 for the simple-radius case; a complex shape in reach is
/// `Reach::Shape` (F576 decides). `None` only if the arithmetic cannot follow.
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
        return Some(Reach::Shape);
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
        return Some(Reach::Contact {
            squares: distance_squared,
        });
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

/// What the touches of one scan found.
#[derive(Default)]
struct Tally {
    touch: Option<Touch>,
    damage: i64,
}

impl Tally {
    fn add(&mut self, touch: Touch) {
        if let Touch::Crash { damage } = touch {
            self.damage = self.damage.wrapping_add(i64::from(damage));
        }
        self.touch = Some(touch);
    }
}

/// The raw object bytes the landing code reads, filled from the fields `Body`
/// keeps: the orientation (`+0`), the position (`+0x3e`, `+0x46`, `+0x4e`), the
/// model, kind and index (`+0x82`, `+0x87`, `+0x86`), the velocity (`+0x8c`),
/// the animation words from `+0x9c` (the landing word `+0x9e` is `words[1]`) and
/// the radii (`+0x138`, `+0x140`).
fn object_bytes(body: &Body) -> Vec<u8> {
    let mut bytes = vec![0u8; crate::objects::OBJECT_BYTES];
    for (k, value) in body.orientation.iter().enumerate() {
        bytes[4 * k..4 * k + 4].copy_from_slice(&value.to_le_bytes());
    }
    for (k, value) in body.position.iter().enumerate() {
        let at = 0x3e + 8 * k;
        bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
    for (k, value) in body.velocity.iter().enumerate() {
        let at = 0x8c + 4 * k;
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes[0x82..0x86].copy_from_slice(&body.model.to_le_bytes());
    bytes[0x86] = body.index;
    bytes[0x87] = body.kind;
    for (k, word) in body.words.0.iter().enumerate() {
        let at = 0x9c + 2 * k;
        bytes[at..at + 2].copy_from_slice(&word.to_le_bytes());
    }
    bytes[0x138..0x140].copy_from_slice(&body.outer_radius.to_le_bytes());
    bytes[0x140..0x148].copy_from_slice(&body.inner_radius.to_le_bytes());
    bytes
}

/// Copy back the fields F575's contact code changes: the orientation, position
/// and velocity.
fn store_object(bytes: &[u8], body: &mut Body) {
    for k in 0..9 {
        body.orientation[k] = i32::from_le_bytes(bytes[4 * k..4 * k + 4].try_into().unwrap());
    }
    for k in 0..3 {
        let at = 0x3e + 8 * k;
        body.position[k] = i64::from_le_bytes(bytes[at..at + 8].try_into().unwrap());
        let at = 0x8c + 4 * k;
        body.velocity[k] = i32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    }
}

impl Bodies {
    /// F575 for the pair (`a`, `other`) with `relative` the position of `a`
    /// relative to `other`. Returns the original's return value: zero when the
    /// bodies are out of each other's reach, otherwise the flags
    /// (`DATA_008696`, held in `regs`, which this updates). `crash` is the crash
    /// mask (`DATA_008700`).
    #[allow(clippy::too_many_arguments)]
    fn pair(
        &mut self,
        regs: &mut Registers,
        tools: Option<&SurfaceTools>,
        relative: [i64; 3],
        a: usize,
        other: usize,
        crash: u8,
        tally: &mut Tally,
    ) -> Option<u8> {
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
            Reach::Far => Some(0),
            Reach::Contact { squares } => {
                if pb.index == pa.parent {
                    // JUMP_002408: a sphere touch of the body's own parent.
                    let tools = tools?;
                    self.touched(
                        regs,
                        tools,
                        relative,
                        a,
                        other,
                        crash,
                        Surface::Sphere { squares },
                        tally,
                    )
                } else {
                    self.impact(regs, tools, a, other, crash, tally)
                }
            }
            Reach::Shape => {
                let tools = tools?;
                let ship_shape = tools.shapes.shape(pa.model)?;
                let planet_shape = tools.shapes.shape(pb.model)?;
                let mut terrain = Terrain::new(tools.terrain_table, tools.terrain_maps);
                let mut scene = Scene {
                    terrain: &mut terrain,
                    shapes: tools.shapes,
                    detail: tools.detail,
                };
                regs.planet = 0;
                regs.distance = self.distance;
                let hit = scene
                    .hit(
                        regs,
                        &Hull::from_body(&pa),
                        &ship_shape,
                        &Hull::from_body(&pb),
                        &planet_shape,
                        relative,
                    )
                    .ok()?;
                self.distance = regs.distance;
                self.altitude = regs.altitude;
                if hit != 0 {
                    return Some(regs.flags);
                }
                if regs.planet == 0 {
                    // F576 found no surface: F578's response, as for F577.
                    return self.impact(regs, Some(tools), a, other, crash, tally);
                }
                if regs.planet != 1 {
                    // 0xff: the tail's sphere branch reads F577's squares, which
                    // this path never set (the original reads an unset local).
                    return None;
                }
                self.touched(
                    regs,
                    tools,
                    relative,
                    a,
                    other,
                    crash,
                    Surface::Terrain {
                        altitude: regs.altitude,
                    },
                    tally,
                )
            }
            Reach::Near {
                distance,
                different,
            } => {
                regs.flags |= DISTANCE_VALID;
                self.distance = distance;
                if different {
                    regs.flags |= DIFFERENT_RADII;
                }
                Some(regs.flags)
            }
        }
    }

    /// F575's tail for a pair that touched (`JUMP_002410` onwards).
    #[allow(clippy::too_many_arguments)]
    fn touched(
        &mut self,
        regs: &mut Registers,
        tools: &SurfaceTools,
        relative: [i64; 3],
        a: usize,
        other: usize,
        crash: u8,
        surface: Surface,
        tally: &mut Tally,
    ) -> Option<u8> {
        let (pa, pb) = (self.body[a], self.body[other]);
        let body_model = tools.shapes.model(pa.model)?;
        let planet_model = tools.shapes.model(pb.model)?;
        let mut body_bytes = object_bytes(&pa);
        let planet_bytes = object_bytes(&pb);
        let mut pair = LandingPair {
            body: &mut body_bytes,
            planet: &planet_bytes,
            body_model,
            planet_model,
        };
        let (flags, touch) =
            landing::contact(&mut pair, relative, surface, regs.flags, crash, self.player);
        regs.flags = flags;
        if regs.object == 0 {
            regs.object = pb.index;
        }
        store_object(&body_bytes, &mut self.body[a]);
        tally.add(touch);
        Some(regs.flags)
    }

    /// F578: the response when `a` touches a body that is not its parent: flag
    /// 0x80, the bodies' momentum exchange (when both are ships, their statistics
    /// are there and the mask is set) and the damage F953 is called with.
    fn impact(
        &mut self,
        regs: &mut Registers,
        tools: Option<&SurfaceTools>,
        a: usize,
        other: usize,
        crash: u8,
        tally: &mut Tally,
    ) -> Option<u8> {
        regs.flags |= CRASH;
        let (pa, pb) = (self.body[a], self.body[other]);
        let mut damage = 0;
        if pa.parent == pb.index || pb.kind == 1 {
            let body = &mut self.body[a];
            for v in &mut body.velocity {
                *v = v.wrapping_shr(2).wrapping_neg();
            }
            if pa.model > 0xd {
                damage = body
                    .velocity
                    .iter()
                    .fold(0i32, |sum, v| sum.wrapping_add(v.wrapping_abs()))
                    .wrapping_add(0xa);
            }
        } else if pa.model > 0xd && pb.model > 0xd && crash != 0 {
            let stats = |model: u32| tools.and_then(|t| t.shapes.model(model)?.stats);
            if let (Some(stats_a), Some(stats_b)) = (stats(pa.model), stats(pb.model)) {
                let mass = |stats: &[u8; 48]| u32::from(u16::from_le_bytes([stats[6], stats[7]]));
                let (first, second, second_mass) = if mass(&stats_a) < mass(&stats_b) {
                    (other, a, stats_a)
                } else {
                    (a, other, stats_b)
                };
                let relative = std::array::from_fn(|k| {
                    self.body[second].position[k].wrapping_sub(self.body[first].position[k])
                });
                let (cut, _) = crate::frames::truncate(relative, 0x1d);
                let direction =
                    crate::physics::normalise([cut[0] as i32, cut[1] as i32, cut[2] as i32]);
                let velocity: [i32; 3] = std::array::from_fn(|k| {
                    self.body[second].velocity[k].wrapping_sub(self.body[first].velocity[k])
                });
                let along = (0..3).fold(0i32, |sum, k| {
                    sum.wrapping_add(crate::model::multiply(direction[k], velocity[k]))
                });
                let push = along.wrapping_mul(2).wrapping_neg();
                if push >= 0 {
                    for (v, component) in self.body[second].velocity.iter_mut().zip(direction) {
                        *v = v.wrapping_add(crate::model::multiply(component, push));
                    }
                    damage =
                        ((u64::from(mass(&second_mass)) * u64::from(push as u32)) / 5000) as i32;
                }
            }
        }
        tally.add(Touch::Crash { damage });
        Some(regs.flags)
    }

    /// F569/F570 for body `a`: the distance to its parent, the frame switch, and
    /// the scan of the other physical bodies in the same frame. `crash` is the
    /// crash mask (`DATA_008700`: 0xff for the player's scan F569, zero for
    /// F568). Complex shapes need `tools`; without them a touch is declined.
    pub fn collide_with(
        &mut self,
        a: usize,
        tools: Option<&SurfaceTools>,
        crash: u8,
    ) -> Option<Contact> {
        let mut regs = Registers::default();
        let mut tally = Tally::default();
        regs.altitude = self.altitude;
        self.distance = (self.distance & !0xffff_ffff) | 0xffff_ffff;
        let parent = self.body[a].parent;
        let mut before = 0u8;
        if parent != 0 {
            let parent = usize::from(parent);
            let position = self.body[a].position;
            let result = self.pair(&mut regs, tools, position, a, parent, crash, &mut tally)?;
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
            before = regs.flags;
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
            self.pair(&mut regs, tools, relative, a, k, crash, &mut tally)?;
        }
        regs.flags |= before;
        let distance = if self.distance >> 32 != 0 || (self.distance as i32) < 0 {
            -1
        } else {
            self.distance as i32
        };
        Some(Contact {
            flags: regs.flags,
            distance,
            object: regs.object,
            pad: regs.chunk & 0x1f,
            touch: tally.touch,
            damage: tally.damage,
        })
    }

    /// [`collide_with`](Self::collide_with) without complex shapes, as the
    /// player's scan (F569, crash mask 0xff).
    pub fn collide(&mut self, a: usize) -> Option<Contact> {
        self.collide_with(a, None, 0xff)
    }
}

fn msb(value: u32) -> u32 {
    31u32.saturating_sub(value.leading_zeros())
}
