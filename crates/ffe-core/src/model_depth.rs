//! Original render-order keys, not a z-buffer or Euclidean distance.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Close,
    Maximum(Vec<i8>),
    Minimum(Vec<i8>),
    Vertex(i8),
    Position,
    First,
    Offset { vertex: Option<i8>, amount: i16 },
}

pub struct Context<'a> {
    pub vertices: &'a [[i32; 3]],
    pub position: [i32; 3],
    pub key_scale: i32,
    pub operand_scale: i32,
}

/// F1696 structural decoding. Resolved points are separate from the legacy lazy cache.
pub fn decode(bytes: &[u8]) -> Result<(Command, usize), String> {
    if bytes.len() > 16384 {
        return Err("Depth-order command exceeds the bounded model stream size.".to_owned());
    }
    let read = |offset: usize| {
        bytes
            .get(offset..offset + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or_else(|| "Truncated depth-order command.".to_owned())
    };
    let code = read(0)?;
    if code & 31 != 6 {
        return Err("Not a depth-order command.".to_owned());
    }
    let vertex = (code >> 5) as u8 as i8;
    let mut size = 2;
    let command = match code >> 13 {
        1 | 3 => {
            let mut vertices = vec![vertex];
            loop {
                let pair = read(size)?;
                vertices.push(pair as u8 as i8);
                size += 2;
                if pair >> 8 != 255 {
                    break;
                }
            }
            if code >> 13 == 1 {
                Command::Maximum(vertices)
            } else {
                Command::Minimum(vertices)
            }
        }
        2 => Command::Vertex(vertex),
        7 => match code >> 5 {
            0x7ff => Command::Position,
            0x7fe => Command::First,
            _ => {
                let amount = read(2)? as i16;
                size = 4;
                Command::Offset {
                    vertex: (code >> 5 != 0x7fd).then_some(vertex),
                    amount,
                }
            }
        },
        _ => Command::Close,
    };
    Ok((command, size))
}

impl Command {
    /// Resolve only already evaluated view-space points. None closes the active group.
    pub fn resolve(&self, context: &Context<'_>) -> Result<Option<u32>, String> {
        let point = |index: i8| {
            usize::try_from(index)
                .ok()
                .and_then(|index| context.vertices.get(index))
                .copied()
                .ok_or_else(|| format!("Unresolved depth-order vertex {index}."))
        };
        let depth = match self {
            Self::Close => return Ok(None),
            Self::First => u32::MAX,
            Self::Position => key(context.position, context.key_scale),
            Self::Vertex(index) => key(point(*index)?, context.key_scale),
            Self::Maximum(indices) | Self::Minimum(indices) => {
                let mut depths = indices
                    .iter()
                    .map(|index| point(*index).map(|p| key(p, context.key_scale) as i32));
                let mut result = depths.next().ok_or("Empty depth-order vertex list.")??;
                for depth in depths {
                    let depth = depth?;
                    result = if matches!(self, Self::Maximum(_)) {
                        result.max(depth)
                    } else {
                        result.min(depth)
                    };
                }
                result as u32
            }
            Self::Offset { vertex, amount } => {
                let mut p = match vertex {
                    Some(index) => point(*index)?,
                    None => [0; 3],
                };
                p[2] = p[2]
                    .wrapping_add(i32::from(*amount).wrapping_shl(context.operand_scale as u32));
                key(p, context.key_scale)
            }
        };
        Ok(Some(depth))
    }
}

/// F1777: input is the view-space point and instance scale at +0x130.
pub fn key([x, y, z]: [i32; 3], scale: i32) -> u32 {
    if z <= 0 {
        return 1;
    }
    let magnitude = x.wrapping_abs().wrapping_add(y.wrapping_abs());
    let depth = (magnitude >> 3).wrapping_add(z) as u32;
    let shift = 3i32.wrapping_sub(scale);
    if shift >= 0 {
        depth.wrapping_shr(shift as u32)
    } else {
        let shift = shift.wrapping_neg() as u32;
        if depth < 0x80000000u32.wrapping_shr(shift) {
            depth.wrapping_shl(shift)
        } else {
            shift.wrapping_shl(25) | 0x80000000 | (z >> 7) as u32
        }
    }
}
