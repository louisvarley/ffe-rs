//! Integer trigonometry and local basis rotation used by model commands.
use crate::model::{multiply, variable, VertexContext};

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
                    self.rotate(&mut working.matrix, axis, angle)?;
                }
                *context = working;
                return Ok(());
            }
        }
        let angle = angle_operand(operand, context);
        self.rotate(&mut context.matrix, axis, angle)
    }
}

/// F1739: copy the base frame's rows into a permutation of the current basis rows
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
            let value = context.base[row * 3 + component];
            context.matrix[destination * 3 + component] =
                if negate { value.wrapping_neg() } else { value };
        }
        if negate {
            handedness = !handedness;
        }
    }
    context.handedness = handedness;
    Ok(())
}
