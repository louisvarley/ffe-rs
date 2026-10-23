//! Frames of reference, ported from `cphys.txt` and `ffe.asm`: an object's
//! position and velocity are relative to its parent body (the byte `+0x56`),
//! and F606 moves it between frames as it approaches and leaves bodies.
//! F604 enters the parent's own parent frame, F605 enters a given body's
//! frame, F590 measures the distance between two bodies through the cached
//! chain F1677/F1678 builds, and F595 gives a body's velocity.
//!
//! Only non-rotating frames are ported: a body with `rotating` set (`+0x57`,
//! the stations) and a station's velocity (`+0x87` of 1, which F595 takes from
//! its spin) return `None`, and `docs/KNOWN_ISSUES.md` records the gap.
//! Verified against the executed original (`docs/notes/frames.md`).

/// Objects in the original's list: indexes `0..=0x72` (`index * 0x152 + 0x74`).
pub const OBJECTS: usize = 0x73;

/// The fields of an object the frame routines read and write.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Body {
    /// The position (`+0x3e`, `+0x46`, `+0x4e`) in the parent's frame.
    pub position: [i64; 3],
    /// Velocity (`+0x8c`, `+0x90`, `+0x94`).
    pub velocity: [i32; 3],
    /// The parent body's index (`+0x56`); zero for none.
    pub parent: u8,
    /// Non-zero for a rotating frame (`+0x57`).
    pub rotating: u8,
    /// The object's own index (`+0x86`).
    pub index: u8,
    /// The object class (`+0x87`); 1 is a station.
    pub kind: u8,
    /// The velocity-lock flag (`+0x14c` bit 0x20), which F595 reads for stations.
    pub locked: bool,
    /// The body's radius exponent (`+0xda`).
    pub radius: i16,
    /// The body's gravity: an ffp number, mantissa word and exponent word
    /// (`+0xa4`), read by F603 for the object in its frame.
    pub mass: u32,
    /// The frame stamp of the cached absolute position (`+0x25`).
    pub stamp: u8,
    /// The cached absolute position (`+0x26`).
    pub cache: [i64; 3],
}

/// The object list with the per-object in-use flag (bit 0x10 of the byte at
/// `list + index`), the frame stamp (`DATA_008811`) and the F148 events raised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bodies {
    pub body: Vec<Body>,
    pub live: Vec<bool>,
    pub stamp: u8,
    /// The `(a, b)` pairs passed to F148 (always `(0xb, 0)`: frame changed).
    pub events: Vec<(u32, u32)>,
}

impl Default for Bodies {
    fn default() -> Self {
        Bodies {
            body: vec![Body::default(); OBJECTS],
            live: vec![false; OBJECTS],
            stamp: 0,
            events: Vec::new(),
        }
    }
}

impl Bodies {
    /// F595: the velocity of a body in its own frame.
    pub fn velocity_of(&self, index: usize) -> Option<[i32; 3]> {
        let body = &self.body[index];
        if body.kind != 1 {
            Some(body.velocity)
        } else if body.locked {
            Some([0; 3])
        } else {
            None
        }
    }

    /// F1677: the body's position in the root frame, cached per frame stamp.
    pub fn absolute(&mut self, index: usize) -> Option<[i64; 3]> {
        let body = self.body[index];
        let result = if body.parent == 0 {
            body.position
        } else if body.stamp == self.stamp {
            return Some(body.cache);
        } else {
            if body.rotating != 0 {
                return None;
            }
            let parent = self.absolute(usize::from(body.parent))?;
            std::array::from_fn(|axis| parent[axis].wrapping_add(body.position[axis]))
        };
        self.body[index].cache = result;
        self.body[index].stamp = self.stamp;
        Some(result)
    }

    /// F590: the vector from body `a` to body `b` cut to 13 bits, and the
    /// shift the cut took (zero when the vector was already small).
    pub fn relative(&mut self, a: usize, b: usize) -> Option<([i32; 3], i32)> {
        self.absolute(a)?;
        let (pa, pb) = (self.body[a], self.body[b]);
        let vector: [i64; 3] = if pb.parent == pa.parent && pb.rotating == pa.rotating {
            std::array::from_fn(|i| pb.position[i].wrapping_sub(pa.position[i]))
        } else if pa.parent == pb.index {
            pa.position.map(|v| 0i64.wrapping_sub(v))
        } else {
            let there = self.absolute(b)?;
            let here = self.body[a].cache;
            std::array::from_fn(|i| there[i].wrapping_sub(here[i]))
        };
        let cut = crate::model_animation::vector_msb(vector) as i32 - 0xd;
        let (vector, shift) = if cut > 0 {
            (
                crate::model_animation::shift_vector(vector, cut as u32),
                cut,
            )
        } else {
            (vector, 0)
        };
        Some((vector.map(|v| v as i32), shift))
    }

    /// F604: move body `a` out of `parent` into the parent's own frame: the
    /// parent's position and velocity are added.
    pub fn enter_grandparent(&mut self, a: usize, parent: usize) -> Option<()> {
        let velocity = self.velocity_of(parent)?;
        let p = self.body[parent];
        let body = &mut self.body[a];
        body.parent = p.parent;
        for (coordinate, delta) in body.position.iter_mut().zip(p.position) {
            *coordinate = coordinate.wrapping_add(delta);
        }
        for (component, delta) in body.velocity.iter_mut().zip(velocity) {
            *component = component.wrapping_add(delta);
        }
        self.events.push((0xb, 0));
        Some(())
    }

    /// F605: move body `a` into the frame of body `parent`: the parent's
    /// position and velocity are subtracted.
    pub fn enter(&mut self, a: usize, parent: usize) -> Option<()> {
        let velocity = self.velocity_of(parent)?;
        let p = self.body[parent];
        let body = &mut self.body[a];
        body.parent = parent as u8;
        for (coordinate, delta) in body.position.iter_mut().zip(p.position) {
            *coordinate = coordinate.wrapping_sub(delta);
        }
        for (component, delta) in body.velocity.iter_mut().zip(velocity) {
            *component = component.wrapping_sub(delta);
        }
        self.events.push((0xb, 0));
        Some(())
    }

    /// F606: choose the frame body `a` belongs in. Its parent's distance
    /// (`(shift - radius) * 4 + 0x20`) is compared with its grandparent's
    /// (`shift - radius + 0x28`): the grandparent wins at or below it, and
    /// `a` leaves. Otherwise every live body is scored: one in the same frame
    /// by `(shift - radius) << 4`, one whose parent is the grandparent by
    /// `(shift - radius) * 4 + 0x20`; a closer sibling becomes the new frame,
    /// a closer cousin has `a` leave to the grandparent frame.
    pub fn select_frame(&mut self, a: usize) -> Option<()> {
        if self.body[a].rotating != 0 {
            return Some(());
        }
        let mut current = usize::from(self.body[a].parent);
        let mut best;
        let grandparent;
        if current == 0 {
            best = 0x32;
            grandparent = 0;
        } else {
            let (_, shift) = self.relative(a, current)?;
            best = ((shift - i32::from(self.body[current].radius)) << 2) + 0x20;
            grandparent = usize::from(self.body[current].parent);
            if grandparent != 0 {
                let (_, shift) = self.relative(a, grandparent)?;
                let outer = shift - i32::from(self.body[grandparent].radius) + 0x28;
                if outer <= best {
                    return self.enter_grandparent(a, current);
                }
            }
        }
        for k in (1..OBJECTS).rev() {
            if !self.live[k] {
                continue;
            }
            let candidate = self.body[k];
            if candidate.parent == self.body[a].parent {
                let (_, shift) = self.relative(a, k)?;
                let distance = (shift - i32::from(candidate.radius)) << 4;
                if distance < best {
                    current = k;
                    best = distance;
                }
            } else if usize::from(candidate.parent) == grandparent {
                let (_, shift) = self.relative(a, k)?;
                let distance = ((shift - i32::from(candidate.radius)) << 2) + 0x20;
                if distance < best {
                    let parent = usize::from(self.body[a].parent);
                    return self.enter_grandparent(a, parent);
                }
            }
        }
        if usize::from(self.body[a].parent) != current {
            self.enter(a, current)?;
        }
        Some(())
    }
}
