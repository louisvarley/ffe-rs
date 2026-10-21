//! Bounded object-space vertex programs. Drawing commands and projection are separate.

pub const IDENTITY: [i32; 9] = [i32::MAX, 0, 0, 0, i32::MAX, 0, 0, 0, i32::MAX];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VertexContext {
    pub matrix: [i32; 9],
    pub scale: u32,
    pub globals: [u16; 64],
    pub locals: [i32; 64],
}

impl Default for VertexContext {
    fn default() -> Self {
        Self {
            matrix: IDENTITY,
            scale: 0,
            globals: [0; 64],
            locals: [0; 64],
        }
    }
}

/// Signed high product after a one-bit shift, exactly F1521 (not rounded Q31).
pub fn multiply(a: i32, b: i32) -> i32 {
    ((i64::from(a) * i64::from(b)) >> 31) as i32
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

pub fn validate_records(records: &[u8], count: u32) -> Result<(), String> {
    if count == 0 || count > 128 || count % 2 != 0 || records.len() != count as usize / 2 * 6 {
        return Err("Expected 1..64 six-byte vertex pairs (2..128 vertices).".to_owned());
    }
    for (index, record) in records.chunks_exact(6).enumerate() {
        let references: &[u8] = match record[0] {
            0..=2 | 9 | 10 => &[],
            5 | 6 => &record[4..5],
            11 | 12 | 17..=20 => &record[3..5],
            15 | 16 => &record[2..5],
            opcode => {
                return Err(format!(
                    "Vertex pair {index}: unsupported opcode {opcode:#x}."
                ))
            }
        };
        if references.iter().any(|value| u32::from(*value) >= count) {
            return Err(format!(
                "Vertex pair {index}: external/out-of-range vertex reference."
            ));
        }
    }
    Ok(())
}

/// Each evaluation has a fresh memo table: animation/matrix changes never reuse stale vertices.
pub fn evaluate(
    records: &[u8],
    count: u32,
    context: &VertexContext,
) -> Result<Vec<[i32; 3]>, String> {
    validate_records(records, count)?;
    if context.scale > 31 {
        return Err("Vertex scale must be in 0..31.".to_owned());
    }
    let mut evaluator = ObjectCache::new(records, count)?;
    (0..count as u8)
        .map(|index| evaluator.vertex(index, context, 0))
        .collect()
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CachedVertex {
    pub object: [i32; 3],
    pub view: [i32; 3],
    pub screen: [i16; 2],
    pub stamp: u32,
}

#[derive(Clone)]
pub(crate) struct ObjectCache<'a> {
    records: &'a [u8],
    pub slots: Vec<CachedVertex>,
    visiting: Vec<bool>,
}

impl<'a> ObjectCache<'a> {
    pub fn new(records: &'a [u8], count: u32) -> Result<Self, String> {
        validate_records(records, count)?;
        Ok(Self {
            records,
            slots: vec![CachedVertex::default(); count as usize],
            visiting: vec![false; count as usize],
        })
    }

    pub fn vertex(
        &mut self,
        index: u8,
        context: &VertexContext,
        depth: usize,
    ) -> Result<[i32; 3], String> {
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
            11 | 12 | 15..=20 => {
                let a = self.vertex(r[3] ^ mirror, context, depth + 1)?;
                let b = self.vertex(r[4] ^ mirror, context, depth + 1)?;
                match r[0] {
                    11 | 12 => std::array::from_fn(|i| a[i].wrapping_add(b[i]) / 2),
                    15 | 16 => {
                        let c = self.vertex(r[2] ^ mirror, context, depth + 1)?;
                        std::array::from_fn(|i| a[i].wrapping_sub(b[i]).wrapping_add(c[i]))
                    }
                    17 | 18 => std::array::from_fn(|i| a[i].wrapping_add(b[i])),
                    _ => {
                        let fraction = variable(r[2], context).wrapping_shl(14);
                        std::array::from_fn(|i| {
                            a[i].wrapping_add(
                                multiply(b[i].wrapping_sub(a[i]), fraction).wrapping_mul(2),
                            )
                        })
                    }
                }
            }
            _ => unreachable!("records validated before evaluation"),
        };
        self.visiting[slot] = false;
        if !matches!(r[0], 0..=2 | 5 | 6 | 9 | 10) {
            self.slots[slot].object = value;
            self.slots[slot].stamp = 1;
        }
        Ok(value)
    }
}
