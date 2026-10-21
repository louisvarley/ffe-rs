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

    /// F1741's 0x8000 form -> F1742/F1743; other transform forms remain unsupported.
    pub fn rotate_command(
        &self,
        context: &mut VertexContext,
        code: u16,
        operand: i16,
    ) -> Result<(), String> {
        if code & 31 != 28 || code & 0xc000 != 0x8000 {
            return Err("Only local-basis rotation transform commands are supported.".to_owned());
        }
        let angle = angle_operand(operand, context);
        self.rotate(&mut context.matrix, ((code >> 5) & 3) as u8, angle)
    }
}
