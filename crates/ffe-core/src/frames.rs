//! Frames of reference, ported from `cphys.txt` and `ffe.asm`: an object's
//! position and velocity are relative to its parent body (the byte `+0x56`),
//! and F606 moves it between frames as it approaches and leaves bodies.
//! F604 enters the parent's own parent frame, F605 enters a given body's
//! frame, F590 measures the distance between two bodies through the cached
//! chain F1677/F1678 builds, and F595 gives a body's velocity.
//!
//! A rotating frame (`rotating`, `+0x57`) is supported in F1677 (the position
//! is turned by the parent's orientation) and in the switches F572/F573; a
//! station's own velocity (`+0x87` of 1, which F595 takes from its spin) returns
//! `None`, and `docs/KNOWN_ISSUES.md` records the gap. Verified against the
//! executed original (`docs/notes/frames.md`, `docs/notes/collision.md`).

/// Objects in the original's list: indexes `0..=0x72` (`index * 0x152 + 0x74`).
pub const OBJECTS: usize = 0x73;

/// The animation words of an object (`+0x9c` to the end of the object), which a
/// shape's collision stream may test.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Words(pub [u16; 91]);

impl Default for Words {
    fn default() -> Self {
        Self([0; 91])
    }
}

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
    /// The orientation (`+0x0..+0x23`), the rows right, up, forward in Q31.
    pub orientation: [i32; 9],
    /// The frame in which the rotating-frame orientation was last updated
    /// (`+0x58`), set by F572.
    pub updated: u8,
    /// Non-zero hides the body from collisions (`+0x8b`).
    pub hidden: u8,
    /// The model dword (`+0x82`); 0x99 is special-cased by the collision scan.
    pub model: u32,
    /// The outer (influence) radius (`+0x138`) and the collision radius
    /// (`+0x140`), 64-bit.
    pub outer_radius: i64,
    pub inner_radius: i64,
    /// The class byte (`+0x14c`): negative for a complex collision shape.
    pub shape: u8,
    /// The countdown before the frame may switch between linear and rotating
    /// (`+0x148`).
    pub delay: i32,
    /// The terrain seed (`+0xa0`) and the animation words (`+0x9c`), which the
    /// complex shapes' collision tests read.
    pub seed: u32,
    pub words: Words,
}

impl Body {
    /// Copy the fields the player's [`Motion`](crate::physics::Motion) owns
    /// into this body.
    pub fn take_motion(&mut self, motion: &crate::physics::Motion) {
        self.position = motion.position;
        self.velocity = motion.velocity;
        self.parent = motion.parent;
        self.rotating = motion.rotating;
        self.index = motion.craft;
        self.delay = motion.switch_delay;
        self.updated = motion.updated;
        for (row, target) in motion
            .orientation
            .iter()
            .zip(self.orientation.chunks_mut(3))
        {
            target.copy_from_slice(row);
        }
    }

    /// Copy the fields a frame switch can change back into the player's motion.
    pub fn give_motion(&self, motion: &mut crate::physics::Motion) {
        motion.position = self.position;
        motion.velocity = self.velocity;
        motion.parent = self.parent;
        motion.rotating = self.rotating;
        motion.switch_delay = self.delay;
        motion.updated = self.updated;
        for (row, source) in motion
            .orientation
            .iter_mut()
            .zip(self.orientation.chunks(3))
        {
            row.copy_from_slice(source);
        }
    }
}

/// The object list with the per-object in-use flag (bit 0x10 of the byte at
/// `list + index`), the frame stamp (`DATA_008811`) and the F148 events raised.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bodies {
    pub body: Vec<Body>,
    pub live: Vec<bool>,
    pub stamp: u8,
    /// Bit 2 of the per-object flag byte: a physical object for the collision scan.
    pub physical: Vec<bool>,
    /// `DATA_008704:DATA_008703`, the collision distance, which persists
    /// between scans (F570 resets only the low dword).
    pub distance: i64,
    /// `DATA_008706:DATA_008705`, the altitude F581 leaves, which persists
    /// between scans (F570 does not clear it; a pair that finds the body inside
    /// a planet's inner radius reads whatever an earlier terrain record left).
    pub altitude: i64,
    /// The frame's time (`DATA_009155`), which counts the frame-switch delay down.
    pub frame_tics: u32,
    /// `DATA_008857`: the player's craft number, which F575's bounce compares with
    /// a body's `+0x86`. Zero when no game is running (the original leaves it
    /// clear until a game starts).
    pub player: u8,
    /// The `(a, b)` pairs passed to F148 (always `(0xb, 0)`: frame changed).
    pub events: Vec<(u32, u32)>,
}

/// F1658: shift a vector right until its highest bit is `bits` or lower;
/// returns the vector and the shift taken (zero when it already fit).
pub fn truncate(vector: [i64; 3], bits: u32) -> ([i64; 3], u32) {
    let cut = crate::model_animation::vector_msb(vector) as i32 - bits as i32;
    if cut > 0 {
        (
            crate::model_animation::shift_vector(vector, cut as u32),
            cut as u32,
        )
    } else {
        (vector, 0)
    }
}

/// F1517: a 32-bit vector turned into 64-bit coordinates, shifted left by `bits`
/// (the high dword takes the 32-bit shift once `bits` reaches 32).
pub fn shift_up(vector: [i32; 3], bits: u32) -> [i64; 3] {
    vector.map(|v| {
        if bits >= 0x20 {
            i64::from(v.wrapping_shl(bits - 0x20)) << 32
        } else {
            i64::from(v) << bits
        }
    })
}

/// F1669: the vector times the matrix, summing F1521 products down columns.
pub fn vec_mat_mul(v: [i32; 3], m: &[i32; 9]) -> [i32; 3] {
    std::array::from_fn(|j| {
        (0..3).fold(0i32, |sum, k| {
            sum.wrapping_add(crate::model::multiply(v[k], m[3 * k + j]))
        })
    })
}

/// F1672: the matrix product `a * b` through F1521 products.
pub fn mat_mat_mul(a: &[i32; 9], b: &[i32; 9]) -> [i32; 9] {
    std::array::from_fn(|k| {
        let (i, j) = (k / 3, k % 3);
        (0..3).fold(0i32, |sum, c| {
            sum.wrapping_add(crate::model::multiply(a[3 * i + c], b[3 * c + j]))
        })
    })
}

impl Default for Bodies {
    fn default() -> Self {
        Bodies {
            body: vec![Body::default(); OBJECTS],
            live: vec![false; OBJECTS],
            stamp: 0,
            physical: vec![false; OBJECTS],
            distance: 0,
            altitude: 0,
            frame_tics: 0,
            player: 0,
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
            let parent_index = usize::from(body.parent);
            let parent = self.absolute(parent_index)?;
            if body.rotating != 0 {
                // The position is turned by the parent's orientation (F1669)
                // and shifted back (F1517) before it is added.
                let (vector, shift) = truncate(body.position, 0x1c);
                let turned = vec_mat_mul(
                    vector.map(|v| v as i32),
                    &self.body[parent_index].orientation,
                );
                let offset = shift_up(turned, shift);
                std::array::from_fn(|axis| parent[axis].wrapping_add(offset[axis]))
            } else {
                std::array::from_fn(|axis| parent[axis].wrapping_add(body.position[axis]))
            }
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

impl Bodies {
    /// F572: switch body `a` into the rotating frame of `parent`: position,
    /// orientation and velocity are turned by the parent's orientation (F1670,
    /// F1673), the position shifted back (F1517); the update stamp is set to the
    /// previous frame.
    pub fn to_rotating(&mut self, a: usize, parent: usize) {
        let matrix = self.body[parent].orientation;
        let stamp = self.stamp;
        let body = &mut self.body[a];
        let (vector, shift) = truncate(body.position, 0x1c);
        let turned = crate::model_math::transform_rows(vector.map(|v| v as i32), &matrix);
        body.position = shift_up(turned, shift);
        body.orientation = crate::model_animation::rows_product(&body.orientation, &matrix);
        body.updated = stamp.wrapping_sub(1);
        body.rotating = 1;
        // When a body is its own parent the original's orientation update is
        // in place, so the velocity is turned by the new orientation.
        let matrix = if parent == a {
            body.orientation
        } else {
            matrix
        };
        body.velocity = crate::model_math::transform_rows(body.velocity, &matrix);
    }

    /// F573: switch body `a` back to the linear frame of `parent` (the inverse
    /// of [`Bodies::to_rotating`] through F1669 and F1672).
    pub fn to_linear(&mut self, a: usize, parent: usize) {
        let matrix = self.body[parent].orientation;
        let body = &mut self.body[a];
        let (vector, shift) = truncate(body.position, 0x1c);
        let turned = vec_mat_mul(vector.map(|v| v as i32), &matrix);
        body.position = shift_up(turned, shift);
        body.orientation = mat_mat_mul(&body.orientation, &matrix);
        body.rotating = 0;
        let matrix = if parent == a {
            body.orientation
        } else {
            matrix
        };
        body.velocity = vec_mat_mul(body.velocity, &matrix);
    }
}
