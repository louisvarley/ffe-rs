//! F1480/F1481 normal visibility and lighting in model-local coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Normal {
    pub culled: bool,
    pub colour: [u8; 3],
}

#[derive(Debug, Clone, Default)]
pub struct NormalContext {
    pub scale: u32,
    pub observer_scale: u32,
    pub observer: [i32; 3],
    pub light: [i32; 3],
    pub shades: [[u8; 3]; 8],
}

pub fn validate_records(records: &[u8], count: u32, vertex_count: u32) -> Result<(), String> {
    if vertex_count == 0 || vertex_count > 128 || vertex_count % 2 != 0 {
        return Err("Invalid normal anchor vertex count.".to_owned());
    }
    if !(2..=256).contains(&count)
        || count % 2 != 0
        || records.len() != (count as usize / 2 - 1) * 6
    {
        return Err("Expected 2..256 paired normals, with two implicit entries.".to_owned());
    }
    for (i, record) in records.chunks_exact(6).enumerate() {
        if u32::from(record[0]) >= vertex_count {
            return Err(format!(
                "Normal pair {} has an external/out-of-range anchor.",
                i + 1
            ));
        }
    }
    Ok(())
}

pub fn validate(
    records: &[u8],
    count: u32,
    vertices: &[u8],
    vertex_count: u32,
) -> Result<(), String> {
    crate::model::validate_records(vertices, vertex_count)?;
    validate_records(records, count, vertex_count)?;
    for (i, record) in records.chunks_exact(6).enumerate() {
        // F1481 reads raw signed coordinates, not the computed-vertex evaluator.
        if !matches!(vertices[usize::from(record[0] / 2) * 6], 0..=2) {
            return Err(format!(
                "Normal pair {} requires an unsupported nonliteral anchor.",
                i + 1
            ));
        }
    }
    Ok(())
}

pub fn evaluate(
    records: &[u8],
    count: u32,
    vertices: &[u8],
    vertex_count: u32,
    context: &NormalContext,
) -> Result<Vec<Normal>, String> {
    validate(records, count, vertices, vertex_count)?;
    if context.scale > 31 || context.observer_scale > 31 {
        return Err("Normal coordinate scales must be in 0..31.".to_owned());
    }
    let mut result = vec![
        Normal {
            culled: false,
            colour: context.shades[3],
        },
        Normal {
            culled: false,
            colour: context.shades[4],
        },
    ];
    for record in records.chunks_exact(6) {
        for mirror in 0..2 {
            let mut normal = [2, 3, 4].map(|i| i32::from(record[i] as i8) << 8);
            if mirror == 1 {
                normal[0] = normal[0].wrapping_neg();
            }
            let vertex = &vertices[usize::from(record[0] / 2) * 6..];
            let mut anchor = [2, 3, 4].map(|i| i32::from(vertex[i] as i8));
            if (u32::from(record[0]) ^ mirror) & 1 != 0 {
                anchor[0] = anchor[0].wrapping_neg();
            }
            for (i, coordinate) in anchor.iter_mut().enumerate() {
                *coordinate = if context.scale >= context.observer_scale {
                    coordinate.wrapping_shl(context.scale - context.observer_scale)
                } else {
                    coordinate.wrapping_shr(context.observer_scale - context.scale)
                };
                *coordinate = coordinate.wrapping_sub(context.observer[i]);
            }
            let light_dot = dot(context.light, normal);
            let colour = if light_dot < 0 {
                context.shades[((light_dot >> 27) & 7) as usize]
            } else {
                [0; 3]
            };
            result.push(Normal {
                culled: dot(anchor, normal) >= 0,
                colour,
            });
        }
    }
    Ok(result)
}

fn dot(a: [i32; 3], b: [i32; 3]) -> i32 {
    a.into_iter()
        .zip(b)
        .fold(0i32, |sum, (a, b)| sum.wrapping_add(a.wrapping_mul(b)))
}
