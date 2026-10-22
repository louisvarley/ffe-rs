//! Bounded object-space vertex programs. Drawing commands and projection are separate.

pub const IDENTITY: [i32; 9] = [i32::MAX, 0, 0, 0, i32::MAX, 0, 0, 0, i32::MAX];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexContext {
    /// Instance frame at +0x14: vertex evaluation (F1478) uses it, and opcode 28's
    /// base-relative forms copy it into [`Self::basis`].
    pub matrix: [i32; 9],
    pub scale: u32,
    pub globals: [u16; 64],
    pub locals: [i32; 64],
    /// Instance colour at +0xf4..+0xf6, set by opcode 26 and passed to child models.
    pub colour: [u8; 3],
    /// Current basis at +0x9c: opcode 28 rotates or reloads it; child models and
    /// text frames (F1740) read it. Vertex evaluation does not.
    pub basis: [i32; 9],
    /// Frame handedness word at +0xf8.
    pub base_handedness: u32,
    /// Current basis handedness word at +0xfc, written by those forms.
    pub handedness: u32,
    /// Instance vector at +0x38..+0x40 that opcode 25 aligns the basis with.
    pub direction: [i32; 3],
    /// Bit `i` set: local `i` (+0x54 + 4i) holds no defined value in this instance.
    /// The original leaves locals 3..=11 of a child instance (F1750) as stack data
    /// or parent vertex pointers; reading one is an explicit error. Zero (every
    /// local caller-defined) for a top-level instance.
    pub undefined_locals: u32,
}

impl Default for VertexContext {
    fn default() -> Self {
        Self {
            matrix: IDENTITY,
            scale: 0,
            globals: [0; 64],
            locals: [0; 64],
            colour: [0; 3],
            basis: IDENTITY,
            base_handedness: 0,
            handedness: 0,
            direction: [0; 3],
            undefined_locals: 0,
        }
    }
}

/// Signed high product after a one-bit shift, exactly F1521 (not rounded Q31).
pub fn multiply(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 31) as i32
}

/// Initial state of the F1476 generator (`DATA_008818`, `DATA_008820`).
pub const JITTER_SEED: [u32; 2] = [0x1234_5678, 0x4567_89ab];

/// F1476: a = s0; s0 += s1; s1 = rotate-left-16(a); return s1. The original shares
/// this state with other code; callers own it here (see vertex opcodes 7/8).
pub fn jitter(state: &mut [u32; 2]) -> i32 {
    let a = state[0];
    state[0] = a.wrapping_add(state[1]);
    state[1] = a.rotate_left(16);
    state[1] as i32
}

pub fn variable(selector: u8, context: &VertexContext) -> i32 {
    let index = usize::from(selector & 63);
    match selector >> 6 {
        0 => index as i32,
        1 => (index as i32) << 10,
        2 => i32::from(context.globals[index]),
        _ => context.locals[index],
    }
}

/// [`variable`] that refuses a local the instance leaves undefined
/// ([`VertexContext::undefined_locals`]).
pub fn checked_variable(selector: u8, context: &VertexContext) -> Result<i32, String> {
    let index = u32::from(selector & 63);
    if selector >> 6 == 3 && index < 32 && context.undefined_locals & (1 << index) != 0 {
        return Err(format!(
            "Local {index} is uninitialised in this child instance in the original."
        ));
    }
    Ok(variable(selector, context))
}

/// External vertex sources of a child instance: F1750 stores the parent's cache entry
/// for the call's position vertex at +0x70 and up to four extra vertices at +0x74..+0x80;
/// F1478 resolves vertex -k from the pointer at +0x6c + 4k. Entries are the parent's
/// resolved (projected) vertices; `None` marks a pointer the call did not set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalVertices {
    /// Child instance position (+0x4): an external object position is view - position.
    pub position: [i32; 3],
    /// Child instance generation (+0xf0) stamped on a copied external slot.
    pub generation: u32,
    /// Vertices -1..-5.
    pub sources: [Option<ExternalSource>; 5],
}

/// The parts of a parent cache entry F1478 copies: screen (+0x0) and view (+0x4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalSource {
    pub view: [i32; 3],
    pub screen: [i16; 2],
}

/// Vertex opcodes whose dependency bytes the original reads sign-extended through
/// F1472/F1478 (verified paths: 3/4, 5..=8, 11/12, 13/14, 15..=20), so 0x80..0xff
/// are external.
fn signed_dependencies(opcode: u8) -> bool {
    matches!(opcode, 3 | 4 | 5..=8 | 11 | 12 | 13 | 14 | 15..=20)
}

pub fn validate_records(records: &[u8], count: u32) -> Result<(), String> {
    if count == 0 || count > 128 || count % 2 != 0 || records.len() != count as usize / 2 * 6 {
        return Err("Expected 1..64 six-byte vertex pairs (2..128 vertices).".to_owned());
    }
    let mut curve_references;
    for (index, record) in records.chunks_exact(6).enumerate() {
        let references: &[u8] = match record[0] {
            0..=2 | 9 | 10 => &[],
            5..=8 => &record[4..5],
            11 | 12 | 17..=20 => &record[3..5],
            // F1478's screen-space midpoints: the two sources are bytes 3 and 4
            // (case 11) or their partners (case 6).
            3 | 4 | 13 | 14 => &record[3..5],
            // Curve control points are bytes 3 and 4 and those plus 2 (mirror-adjusted).
            21 | 22 => &[
                record[3],
                record[4],
                record[3].wrapping_add(2),
                record[4].wrapping_add(2),
            ],
            15 | 16 => &record[2..5],
            // F1477 on the record selected by byte 2 (its own mirror bit) with an
            // F1474 offset: that record's linear or cubic control points.
            23 | 24 => {
                let selector = record[2];
                let base = usize::from(selector >> 1) * 6;
                let Some(source) = records.get(base..base + 6) else {
                    return Err(format!(
                        "Vertex pair {index}: external/out-of-range vertex reference."
                    ));
                };
                let m = selector & 1;
                curve_references = if matches!(source[0], 19 | 20) {
                    vec![source[3] ^ m, source[4] ^ m]
                } else {
                    vec![
                        source[3] ^ m,
                        source[4] ^ m,
                        source[4].wrapping_add(2) ^ m,
                        source[3].wrapping_add(2) ^ m,
                    ]
                };
                curve_references.push(selector);
                &curve_references
            }
            opcode => {
                return Err(format!(
                    "Vertex pair {index}: unsupported opcode {opcode:#x}."
                ))
            }
        };
        if references.iter().any(|value| {
            u32::from(*value) >= count && !(*value >= 0x80 && signed_dependencies(record[0]))
        }) {
            return Err(format!(
                "Vertex pair {index}: external/out-of-range vertex reference."
            ));
        }
    }
    Ok(())
}

/// Each evaluation has a fresh memo table: animation/matrix changes never reuse stale vertices.
/// Jittered vertices (opcodes 7/8) draw from [`JITTER_SEED`]; see [`evaluate_with_jitter`].
pub fn evaluate(
    records: &[u8],
    count: u32,
    context: &VertexContext,
) -> Result<Vec<[i32; 3]>, String> {
    evaluate_with_jitter(records, count, context, &mut JITTER_SEED.clone())
}

/// [`evaluate`] in index order with caller-owned F1476 state, advanced only on success.
pub fn evaluate_with_jitter(
    records: &[u8],
    count: u32,
    context: &VertexContext,
    jitter: &mut [u32; 2],
) -> Result<Vec<[i32; 3]>, String> {
    validate_records(records, count)?;
    if context.scale > 31 {
        return Err("Vertex scale must be in 0..31.".to_owned());
    }
    let mut evaluator = ObjectCache::new(records, count)?;
    evaluator.jitter = *jitter;
    let vertices = (0..count as u8)
        .map(|index| evaluator.vertex(index, context, 0))
        .collect::<Result<Vec<_>, _>>()?;
    *jitter = evaluator.jitter;
    Ok(vertices)
}

/// A vertex's screen and view position, as the original's cache entry holds them:
/// the two screen words and the three view dwords (entry +0..+0x10).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Entry {
    pub screen: [i16; 2],
    pub view: [i32; 3],
}

/// The screen and view position every vertex ends up with, in index order. The
/// original's object-space path (F1472) leaves the screen and view of a vertex
/// unwritten unless something projected it: the screen-space opcodes project
/// their two sources, so those entries and their sources carry a position.
/// Each vertex is read as the original's caller reads it, right after its own
/// evaluation, so a later vertex cannot change what an earlier one reported.
pub fn evaluate_entries(
    records: &[u8],
    count: u32,
    context: &VertexContext,
) -> Result<Vec<Entry>, String> {
    evaluate_entry_slots(records, count, context, &mut JITTER_SEED.clone())
}

fn evaluate_entry_slots(
    records: &[u8],
    count: u32,
    context: &VertexContext,
    jitter: &mut [u32; 2],
) -> Result<Vec<Entry>, String> {
    validate_records(records, count)?;
    if context.scale > 31 {
        return Err("Vertex scale must be in 0..31.".to_owned());
    }
    let mut evaluator = ObjectCache::new(records, count)?;
    evaluator.jitter = *jitter;
    let mut entries = Vec::with_capacity(count as usize);
    for index in 0..count as u8 {
        evaluator.vertex(index, context, 0)?;
        let slot = evaluator.slots[usize::from(index)];
        entries.push(Entry {
            screen: slot.screen,
            view: slot.view,
        });
    }
    *jitter = evaluator.jitter;
    Ok(entries)
}

/// Check that every vertex of a program evaluates. Child-only programs may read
/// their parent's vertices (negative indices); with no parent here, slots -1..-5
/// stand in at the origin and a vertex that needs any other parent slot is not
/// checked. Every other error remains.
pub fn validate_program(records: &[u8], count: u32, scale: u32) -> Result<(), String> {
    validate_records(records, count)?;
    if scale > 31 {
        return Err("Vertex scale must be in 0..31.".to_owned());
    }
    let origin = ExternalSource {
        view: [0; 3],
        screen: [0; 2],
    };
    let context = VertexContext {
        scale,
        ..Default::default()
    };
    for index in 0..count as u8 {
        // A fresh cache per vertex: a failed evaluation leaves no partial state.
        let mut evaluator = ObjectCache::new(records, count)?;
        evaluator.externals = Some(ExternalVertices {
            position: [0; 3],
            generation: 1,
            sources: [Some(origin); 5],
        });
        match evaluator.vertex(index, &context, 0) {
            // A parent slot only some callers supply (e.g. -6): not checkable alone.
            Err(error) if error.starts_with("External vertex -") => {}
            result => {
                result?;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CachedVertex {
    pub object: [i32; 3],
    pub view: [i32; 3],
    pub screen: [i16; 2],
    pub stamp: u32,
    /// Set for the screen-space opcodes (3/4, 13/14): the original writes their
    /// screen position itself, so the caller must not project them again.
    pub screen_space: bool,
}

#[derive(Clone)]
pub(crate) struct ObjectCache<'a> {
    records: &'a [u8],
    pub slots: Vec<CachedVertex>,
    visiting: Vec<bool>,
    /// F1476 state consumed by jittered vertices, in evaluation order.
    pub jitter: [u32; 2],
    /// Parent vertices of a child instance; `None` at top level.
    pub externals: Option<ExternalVertices>,
    /// Cache slots -1..-6 (F1745 reserves six below the instance's own slots).
    pub external_slots: [CachedVertex; 6],
}

impl<'a> ObjectCache<'a> {
    pub fn new(records: &'a [u8], count: u32) -> Result<Self, String> {
        validate_records(records, count)?;
        Ok(Self {
            records,
            slots: vec![CachedVertex::default(); count as usize],
            visiting: vec![false; count as usize],
            jitter: JITTER_SEED,
            externals: None,
            external_slots: [CachedVertex::default(); 6],
        })
    }

    /// F1478's negative-index path: on a zero stamp, copy the parent's screen and view
    /// from the pointer at +0x6c + 4k, set object = view - position and stamp the
    /// slot with the instance generation (also when reached through F1472).
    pub fn external(&mut self, index: u8) -> Result<CachedVertex, String> {
        let k = usize::from((index as i8).unsigned_abs());
        let Some(externals) = self.externals else {
            return Err(format!(
                "External vertex -{k} needs a parent instance (top-level model)."
            ));
        };
        if k == 0 || k > 6 {
            return Err(format!(
                "External vertex -{k} lies outside the six reserved slots."
            ));
        }
        if self.external_slots[k - 1].stamp != 0 {
            return Ok(self.external_slots[k - 1]);
        }
        let source = externals
            .sources
            .get(k - 1)
            .copied()
            .flatten()
            .ok_or_else(|| {
                format!("External vertex -{k} has no parent vertex pointer in this call.")
            })?;
        let slot = CachedVertex {
            object: std::array::from_fn(|i| source.view[i].wrapping_sub(externals.position[i])),
            view: source.view,
            screen: source.screen,
            stamp: externals.generation,
            screen_space: false,
        };
        self.external_slots[k - 1] = slot;
        Ok(slot)
    }

    pub fn vertex(
        &mut self,
        index: u8,
        context: &VertexContext,
        depth: usize,
    ) -> Result<[i32; 3], String> {
        if index >= 0x80 {
            return Ok(self.external(index)?.object);
        }
        let slot = usize::from(index);
        if slot >= self.slots.len() {
            return Err(format!("External/out-of-range vertex {index}."));
        }
        if depth >= 128 || self.visiting[slot] {
            return Err(format!("Cyclic/deep vertex dependency at {index}."));
        }
        if self.slots[slot].stamp != 0 {
            return Ok(self.slots[slot].object);
        }
        self.visiting[slot] = true;
        let offset = slot / 2 * 6;
        let r: [u8; 6] = std::array::from_fn(|i| self.records[offset + i]);
        let mirror = index & 1;
        let value = match r[0] {
            0..=2 | 9 | 10 => {
                let input =
                    [r[2], r[3], r[4]].map(|v| i32::from(v as i8).wrapping_shl(context.scale));
                let x: [i32; 3] =
                    std::array::from_fn(|axis| multiply(input[0], context.matrix[axis]));
                let ordinary: [i32; 3] = std::array::from_fn(|axis| {
                    x[axis]
                        .wrapping_add(multiply(input[1], context.matrix[3 + axis]))
                        .wrapping_add(multiply(input[2], context.matrix[6 + axis]))
                });
                let reflected = std::array::from_fn(|axis| {
                    ordinary[axis].wrapping_sub(if r[0] <= 2 {
                        x[axis].wrapping_mul(2)
                    } else {
                        x[axis]
                    })
                });
                let pair = slot & !1;
                self.slots[pair].object = ordinary;
                self.slots[pair + 1].object = reflected;
                self.slots[pair].stamp = 1;
                self.slots[pair + 1].stamp = 1;
                self.slots[slot].object
            }
            5 | 6 => {
                let pair = slot & !1;
                let ordinary = self
                    .vertex(r[4], context, depth + 1)?
                    .map(i32::wrapping_neg);
                self.slots[pair].object = ordinary;
                let reflected = self
                    .vertex(r[4] ^ 1, context, depth + 1)?
                    .map(i32::wrapping_neg);
                self.slots[pair + 1].object = reflected;
                self.slots[pair].stamp = 1;
                self.slots[pair + 1].stamp = 1;
                self.slots[slot].object
            }
            7 | 8 => {
                // Base vertex, three draws shifted by byte 2, then the mirror; the mirror
                // takes the offsets in z, y, x order, exactly as the original does.
                let pair = slot & !1;
                let base = self.vertex(r[4], context, depth + 1)?;
                let shift = u32::from(r[2] & 31);
                let offsets: [i32; 3] = std::array::from_fn(|_| jitter(&mut self.jitter) >> shift);
                let ordinary = std::array::from_fn(|i| base[i].wrapping_add(offsets[i]));
                let mirror = self.vertex(r[4] ^ 1, context, depth + 1)?;
                let reflected = std::array::from_fn(|i| mirror[i].wrapping_add(offsets[2 - i]));
                self.slots[pair].object = ordinary;
                self.slots[pair + 1].object = reflected;
                self.slots[pair].stamp = 1;
                self.slots[pair + 1].stamp = 1;
                self.slots[slot].object
            }
            19..=22 => self.curve(index, 0, context, depth)?,
            3 | 4 => {
                // F1478's case 11: a screen-space midpoint of two vertices. The
                // original writes no object coordinates and no view x/y for it
                // (the entry keeps the arena's stale bytes), takes the nearer of
                // the two depths and averages the two screen positions. The
                // ordinary entry uses the plain sources, the mirror their
                // partners; both are written together, as the original does.
                let pair = slot & !1;
                let ordinary = self.screen_midpoint(r[3], r[4], context, depth + 1)?;
                let reflected = self.screen_midpoint(r[3] ^ 1, r[4] ^ 1, context, depth + 1)?;
                // Case 11 leaves the view x/y stale as well: only the screen
                // position and the nearer depth are written.
                let ordinary = CachedVertex {
                    view: [0, 0, ordinary.view[2]],
                    ..ordinary
                };
                let reflected = CachedVertex {
                    view: [0, 0, reflected.view[2]],
                    ..reflected
                };
                self.slots[pair] = ordinary;
                self.slots[pair + 1] = reflected;
                self.slots[slot].object
            }
            13 | 14 => {
                // F1478's case 6: the same midpoint, but the original keeps the
                // first source's view x/y and writes only the requested entry,
                // so the mirror is computed on its own request.
                let mirror = index & 1;
                let entry =
                    self.screen_midpoint(r[3] ^ mirror, r[4] ^ mirror, context, depth + 1)?;
                self.slots[slot] = entry;
                self.slots[slot].object
            }
            23 | 24 => {
                let encoded = i16::from_le_bytes([r[3], r[4]]);
                if encoded as u16 & 0xc0 >= 0x80 {
                    checked_variable(encoded as u8, context)?;
                }
                let offset = crate::model_math::angle_operand(encoded, context);
                self.curve(r[2], offset, context, depth)?
            }
            11 | 12 | 15..=18 => {
                let a = self.vertex(r[3] ^ mirror, context, depth + 1)?;
                let b = self.vertex(r[4] ^ mirror, context, depth + 1)?;
                match r[0] {
                    11 | 12 => std::array::from_fn(|i| a[i].wrapping_add(b[i]) / 2),
                    15 | 16 => {
                        let c = self.vertex(r[2] ^ mirror, context, depth + 1)?;
                        std::array::from_fn(|i| a[i].wrapping_sub(b[i]).wrapping_add(c[i]))
                    }
                    _ => std::array::from_fn(|i| a[i].wrapping_add(b[i])),
                }
            }
            _ => unreachable!("records validated before evaluation"),
        };
        self.visiting[slot] = false;
        if !matches!(r[0], 0..=2 | 5..=10) {
            self.slots[slot].object = value;
            self.slots[slot].stamp = 1;
        }
        Ok(value)
    }

    /// F1472's view of a resolved vertex: the object position plus the instance
    /// position, projected. An external vertex keeps the screen and view the
    /// parent copied into it, and a screen-space vertex keeps the screen
    /// position the original computed for it.
    fn entry(
        &mut self,
        index: u8,
        context: &VertexContext,
        depth: usize,
    ) -> Result<CachedVertex, String> {
        if index >= 0x80 {
            // F1478's external path copies the parent's screen and view.
            return self.external(index);
        }
        // F1472: the object path writes no screen or view, so the source keeps
        // whatever it had - the screen-space opcodes average that, not a fresh
        // projection.
        self.vertex(index, context, depth)?;
        Ok(self.slots[usize::from(index)])
    }

    /// The screen-space midpoint of two resolved vertices (F1478): the average
    /// of their screen positions and the nearer of their two depths.
    fn screen_midpoint(
        &mut self,
        first: u8,
        second: u8,
        context: &VertexContext,
        depth: usize,
    ) -> Result<CachedVertex, String> {
        let a = self.entry(first, context, depth)?;
        let b = self.entry(second, context, depth)?;
        let screen = [
            (i32::from(a.screen[0]) + i32::from(b.screen[0])) / 2,
            (i32::from(a.screen[1]) + i32::from(b.screen[1])) / 2,
        ];
        Ok(CachedVertex {
            // The original writes no object coordinates for these vertices: the
            // entry keeps the arena's stale bytes, which are zero in a fresh one.
            object: [0; 3],
            view: [a.view[0], a.view[1], a.view[2].min(b.view[2])],
            screen: [screen[0] as i16, screen[1] as i16],
            stamp: 1,
            screen_space: true,
        })
    }

    /// F1477 on the record holding `selector` (mirror bit `selector & 1`), with
    /// t = (variable(byte 2) + `offset`) << 14. Records 0x13/0x14 interpolate
    /// P0 = byte 3 to P1 = byte 4; every other record is the cubic through bytes 3,
    /// 4, 4 + 2 and 3 + 2, whose t^2 term takes 4·P0, as the original computes it.
    fn curve(
        &mut self,
        selector: u8,
        offset: i32,
        context: &VertexContext,
        depth: usize,
    ) -> Result<[i32; 3], String> {
        let base = usize::from(selector >> 1) * 6;
        let r: [u8; 6] = std::array::from_fn(|i| self.records[base + i]);
        let m = selector & 1;
        let p0 = self.vertex(r[3] ^ m, context, depth + 1)?;
        let p1 = self.vertex(r[4] ^ m, context, depth + 1)?;
        if matches!(r[0], 19 | 20) {
            let t = checked_variable(r[2], context)?
                .wrapping_add(offset)
                .wrapping_shl(14);
            return Ok(std::array::from_fn(|i| {
                p0[i].wrapping_add(multiply(p1[i].wrapping_sub(p0[i]), t).wrapping_mul(2))
            }));
        }
        let p2 = self.vertex(r[4].wrapping_add(2) ^ m, context, depth + 1)?;
        let p3 = self.vertex(r[3].wrapping_add(2) ^ m, context, depth + 1)?;
        let t = checked_variable(r[2], context)?
            .wrapping_add(offset)
            .wrapping_shl(14);
        let t2 = multiply(t, t);
        let t3 = multiply(t2, t);
        Ok(std::array::from_fn(|i| {
            let linear = p1[i].wrapping_mul(3).wrapping_sub(p0[i].wrapping_mul(3));
            let square = p2[i]
                .wrapping_mul(3)
                .wrapping_sub(p1[i].wrapping_mul(6))
                .wrapping_add(p0[i].wrapping_shl(2));
            let cube = p3[i]
                .wrapping_sub(p2[i].wrapping_mul(3))
                .wrapping_add(p1[i].wrapping_mul(3))
                .wrapping_sub(p0[i].wrapping_mul(2));
            p0[i]
                .wrapping_add(multiply(linear, t).wrapping_mul(2))
                .wrapping_add(multiply(square, t2).wrapping_shl(2))
                .wrapping_add(multiply(cube, t3).wrapping_shl(3))
        }))
    }
}
