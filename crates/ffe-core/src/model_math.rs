//! Integer trigonometry and local basis rotation used by model commands.
use crate::model::{multiply, variable, VertexContext};

/// F1502: the original's bit-by-bit square root (`sqrt(x) << 16`, truncated).
pub fn square_root(x: u32) -> u32 {
    let mut step = 0u32;
    let mut bit = 0x8000_0000u32;
    while bit > x {
        step += 1;
        bit >>= 2;
    }
    let (mut root, mut accumulated) = (0u32, 0u32);
    while step <= 15 {
        let trial = (root >> (step.wrapping_sub(1) & 31))
            .wrapping_add(accumulated)
            .wrapping_add(0x8000_0000 >> (2 * step));
        if trial <= x {
            accumulated = trial;
            root = root.wrapping_add(0x8000_0000 >> step);
        }
        step += 1;
    }
    while step <= 31 {
        let trial = (root >> (step - 1)).wrapping_add(accumulated);
        if trial <= x {
            accumulated = trial;
            root = root.wrapping_add(0x8000_0000 >> step);
        }
        step += 1;
    }
    root
}

/// F1467: square root through the 16-bit table `DATA_007977`, `sqrt(x) << 5`
/// truncated per range. Entry i of the 2048 entries F1467 reads is exactly
/// `isqrt(i << 21)` (checked against the supplied table and the executed original),
/// so the table is computed rather than imported.
pub fn table_root(x: u32) -> u32 {
    let entry = |index: u32| (u64::from(index) << 21).isqrt() as u32;
    match x {
        0x4000_0000.. => entry(x >> 21),
        0x1000_0000.. => entry(x >> 19) >> 1,
        0x0400_0000.. => entry(x >> 17) >> 2,
        0x0040_0000.. => entry(x >> 15) >> 3,
        0x0004_0000.. => entry(x >> 11) >> 5,
        0x0000_4000.. => entry(x >> 7) >> 7,
        0x0000_0400.. => entry(x >> 3) >> 9,
        _ => entry(x * 2) >> 11,
    }
}

/// F1466: vector length, normalised by the highest set bit, never shorter than
/// the largest absolute component (unsigned comparisons, as the original).
pub fn length(vector: [i32; 3]) -> u32 {
    let absolute = vector.map(i32::wrapping_abs);
    let bits = absolute.iter().fold(0u32, |a, v| a | *v as u32);
    // F1656: index of the highest set bit, zero for zero.
    let msb = 31u32.saturating_sub(bits.leading_zeros()) as i32;
    let shift = 30 - msb;
    let scaled = absolute.map(|v| v.wrapping_shl(shift as u32));
    let sum = scaled
        .iter()
        .fold(0i32, |a, v| a.wrapping_add(multiply(*v, *v) >> 2));
    let mut result = if shift > 0 {
        square_root(sum as u32) >> ((shift - 1) & 31)
    } else {
        square_root(sum as u32).wrapping_mul(2)
    };
    for value in absolute {
        result = result.max(value as u32);
    }
    result
}

/// F1523: unsigned `(a << 31) / b`, saturating to `u32::MAX` when it would overflow.
pub fn fraction(a: u32, b: u32) -> u32 {
    if a >> 1 >= b {
        u32::MAX
    } else {
        ((u64::from(a) << 31) / u64::from(b)) as u32
    }
}

/// F1741 normalisation: each component divided by `length + 1` through F1523,
/// keeping its sign.
fn normalise(vector: [i32; 3]) -> [i32; 3] {
    let scale = length(vector).wrapping_add(1);
    vector.map(|c| {
        if c < 0 {
            (fraction(c.wrapping_neg() as u32, scale) as i32).wrapping_neg()
        } else {
            fraction(c as u32, scale) as i32
        }
    })
}

/// F1741 0xc000: basis rows from object-space vertices A, B, C. z = |B - A|, x =
/// |(C - A) x z|, y = z x x, written row by row exactly as the original orders them.
pub fn vertex_basis(a: [i32; 3], b: [i32; 3], c: [i32; 3]) -> [i32; 9] {
    let z = normalise(std::array::from_fn(|i| b[i].wrapping_sub(a[i])));
    let d: [i32; 3] = std::array::from_fn(|i| c[i].wrapping_sub(a[i]));
    let x = normalise([
        multiply(d[1], z[2]).wrapping_sub(multiply(d[2], z[1])),
        multiply(d[2], z[0]).wrapping_sub(multiply(d[0], z[2])),
        multiply(d[0], z[1]).wrapping_sub(multiply(d[1], z[0])),
    ]);
    let y = [
        multiply(z[1], x[2]).wrapping_sub(multiply(z[2], x[1])),
        multiply(z[2], x[0]).wrapping_sub(multiply(z[0], x[2])),
        multiply(z[0], x[1]).wrapping_sub(multiply(z[1], x[0])),
    ];
    [x[0], x[1], x[2], y[0], y[1], y[2], z[0], z[1], z[2]]
}

/// F1474's packed signed angle is distinct from the byte variable decoder.
pub fn angle_operand(encoded: i16, context: &VertexContext) -> i32 {
    if encoded as u16 & 0xc0 >= 0x80 {
        variable(encoded as u8, context)
    } else {
        let high = (((encoded as u16 & 0x7f) << 9) as i16) as i32;
        high | ((i32::from(encoded) >> 7) & 0x1ff)
    }
}

#[derive(Debug, Clone)]
pub struct SineTable {
    values: Box<[i32]>,
}

impl SineTable {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() != 16384 * 4 {
            return Err("Sine table must contain 16384 little-endian i32 entries.".to_owned());
        }
        let values: Vec<_> = bytes
            .chunks_exact(4)
            .map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        if values[0] != 0 || values.iter().any(|v| *v < 0) || values.windows(2).any(|v| v[0] > v[1])
        {
            return Err(
                "Sine table must be a nonnegative increasing quarter wave from zero.".to_owned(),
            );
        }
        Ok(Self {
            values: values.into_boxed_slice(),
        })
    }

    pub fn sin32(&self, angle: i32) -> i32 {
        let index = (angle as u32 & 0x3fff) as usize;
        match angle as u32 >> 14 & 3 {
            0 => self.values[index],
            1 => self.values[0x3fff - index],
            2 => self.values[index].wrapping_neg(),
            _ => self.values[0x3fff - index].wrapping_neg(),
        }
    }

    /// F1744 with original sine/cosine inputs, not F1743's instance operand decoding.
    pub fn rotate(&self, matrix: &mut [i32; 9], axis: u8, angle: i32) -> Result<(), String> {
        let (a, b) = match axis {
            0 => (3, 6),
            1 => (6, 0),
            2 => (0, 3),
            _ => return Err("Model rotation axis must be 0, 1 or 2.".to_owned()),
        };
        let cosine = self.sin32(angle.wrapping_add(0x4000));
        let sine = self.sin32(angle);
        for component in 0..3 {
            let av = matrix[a + component];
            let bv = matrix[b + component];
            matrix[a + component] = multiply(av, cosine).wrapping_add(multiply(bv, sine));
            matrix[b + component] =
                multiply(av.wrapping_neg(), sine).wrapping_add(multiply(bv, cosine));
        }
        Ok(())
    }

    /// F1741: the 0x8000 local-basis rotation (F1742/F1743), or the 0x0000/0x4000
    /// base-relative forms (F1739, then F1743 only for a non-zero operand). The
    /// 0xc000 vertex-defined basis is not supported.
    pub fn rotate_command(
        &self,
        context: &mut VertexContext,
        code: u16,
        operand: i16,
    ) -> Result<(), String> {
        if code & 31 != 28 {
            return Err("Not a model transform command.".to_owned());
        }
        let axis = ((code >> 5) & 3) as u8;
        match code & 0xc000 {
            0x8000 => {}
            0xc000 => {
                return Err("Vertex-defined basis transform commands are not supported.".to_owned())
            }
            _ => {
                let mut working = context.clone();
                base_basis(&mut working, code)?;
                if operand != 0 {
                    let angle = angle_operand(operand, &working);
                    self.rotate(&mut working.basis, axis, angle)?;
                }
                *context = working;
                return Ok(());
            }
        }
        let angle = angle_operand(operand, context);
        self.rotate(&mut context.basis, axis, angle)
    }
}

/// F1739: copy the instance frame's rows into a permutation of the current basis rows
/// (code bits 7..9), negating source rows 0/1/2 on bits 12/11/10. Odd permutations (3..5) and each
/// negation flip the handedness word; permutation 7 is undefined in the original.
fn base_basis(context: &mut VertexContext, code: u16) -> Result<(), String> {
    let destinations: [usize; 3] = match code >> 7 & 7 {
        0 | 6 => [0, 1, 2],
        1 => [1, 2, 0],
        2 => [2, 0, 1],
        3 => [2, 1, 0],
        4 => [1, 0, 2],
        5 => [0, 2, 1],
        _ => return Err("Base transform row permutation 7 is undefined.".to_owned()),
    };
    let mut handedness = context.base_handedness;
    if matches!(code >> 7 & 7, 3..=5) {
        handedness = !handedness;
    }
    for (row, destination) in destinations.into_iter().enumerate() {
        let negate = code & (0x1000 >> row) != 0;
        for component in 0..3 {
            let value = context.matrix[row * 3 + component];
            context.basis[destination * 3 + component] =
                if negate { value.wrapping_neg() } else { value };
        }
        if negate {
            handedness = !handedness;
        }
    }
    context.handedness = handedness;
    Ok(())
}
