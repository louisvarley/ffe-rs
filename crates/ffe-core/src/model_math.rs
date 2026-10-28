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
        let cosine = self.sin32(angle.wrapping_add(0x4000));
        let sine = self.sin32(angle);
        rotate_by(matrix, axis, cosine, sine)
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

/// F1744: turn two basis rows by a Q31 cosine and sine. Axis 0 turns rows 1
/// and 2, axis 1 rows 2 and 0, axis 2 rows 0 and 1 (`+0xa8/+0xb4`,
/// `+0xb4/+0x9c`, `+0x9c/+0xa8`).
pub fn rotate_by(matrix: &mut [i32; 9], axis: u8, cosine: i32, sine: i32) -> Result<(), String> {
    let (a, b) = match axis {
        0 => (3, 6),
        1 => (6, 0),
        2 => (0, 3),
        _ => return Err("Model rotation axis must be 0, 1 or 2.".to_owned()),
    };
    for component in 0..3 {
        let av = matrix[a + component];
        let bv = matrix[b + component];
        matrix[a + component] = multiply(av, cosine).wrapping_add(multiply(bv, sine));
        matrix[b + component] =
            multiply(av.wrapping_neg(), sine).wrapping_add(multiply(bv, cosine));
    }
    Ok(())
}

/// F1739 as opcode 28's base forms call it (F1741 passes `code << 1`).
fn base_basis(context: &mut VertexContext, code: u16) -> Result<(), String> {
    let (basis, handedness) = frame_rows(
        &context.matrix,
        context.base_handedness,
        u32::from(code) << 1,
    )
    .map_err(|_| "Base transform row permutation 7 is undefined.".to_owned())?;
    context.basis = basis;
    context.handedness = handedness;
    Ok(())
}

/// F1739: a permutation of the source frame's rows (`word >> 8 & 7`), negating
/// source rows 0/1/2 on bits 0x2000/0x1000/0x800. Odd permutations (3..5) and each
/// negation flip the handedness word; permutation 6 equals 0 and 7 is undefined in
/// the original (it writes through uninitialised row pointers).
pub fn frame_rows(
    source: &[i32; 9],
    handedness: u32,
    word: u32,
) -> Result<([i32; 9], u32), String> {
    let destinations: [usize; 3] = match word >> 8 & 7 {
        0 | 6 => [0, 1, 2],
        1 => [1, 2, 0],
        2 => [2, 0, 1],
        3 => [2, 1, 0],
        4 => [1, 0, 2],
        5 => [0, 2, 1],
        _ => return Err("Frame row permutation 7 is undefined in the original.".to_owned()),
    };
    let mut handedness = handedness;
    if matches!(word >> 8 & 7, 3..=5) {
        handedness = !handedness;
    }
    let mut rows = [0; 9];
    for (row, destination) in destinations.into_iter().enumerate() {
        let negate = word & (0x2000 >> row) != 0;
        for component in 0..3 {
            let value = source[row * 3 + component];
            rows[destination * 3 + component] = if negate { value.wrapping_neg() } else { value };
        }
        if negate {
            handedness = !handedness;
        }
    }
    Ok((rows, handedness))
}

/// F1740 and F1747's text-frame fixup. F1740 copies the parent basis and its
/// handedness for frame mode 6, otherwise it permutes the frame through
/// [`frame_rows`]. F1747 then negates the resulting frame's first row when the
/// handedness word is negative (`cmp [..+0xf8],0; jnl`, `ffe.asm:5f64b-5f658`).
/// The original applies this to every text instance (the GAME!TEK logo,
/// PRESENTS and DAVID BRABEN'S); without it the glyph geometry is mirrored.
pub fn text_frame(
    matrix: &[i32; 9],
    base_handedness: u32,
    basis: &[i32; 9],
    basis_handedness: u32,
    word: u32,
) -> Result<([i32; 9], u32), String> {
    let (mut rows, handedness) = if word >> 8 & 7 == 6 {
        (*basis, basis_handedness)
    } else {
        frame_rows(matrix, base_handedness, word)?
    };
    // F1747's `jnl` is a signed test; F1739/F1740 write 0 or 0xffff_ffff.
    if (handedness as i32) < 0 {
        for value in &mut rows[0..3] {
            *value = value.wrapping_neg();
        }
    }
    Ok((rows, handedness))
}

/// F1475: shift a vector right (arithmetically) until the OR of its component
/// magnitudes is below 0x4000; returns the shifted vector and the shift.
pub fn shift_down(vector: [i32; 3]) -> ([i32; 3], u32) {
    let mut magnitude = vector
        .iter()
        .fold(0u32, |acc, value| acc | value.wrapping_abs() as u32);
    let mut shift = 0;
    while magnitude >= 0x4000 {
        magnitude >>= 1;
        shift += 1;
    }
    (vector.map(|value| value >> shift), shift)
}

/// F1670: each matrix row dotted with the vector through F1521 products.
pub fn transform_rows(vector: [i32; 3], matrix: &[i32; 9]) -> [i32; 3] {
    std::array::from_fn(|row| {
        (0..3).fold(0i32, |sum, column| {
            sum.wrapping_add(multiply(vector[column], matrix[row * 3 + column]))
        })
    })
}
