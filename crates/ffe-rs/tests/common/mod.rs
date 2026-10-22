//! Event encodings shared by the whole-table reference tests.
use ffe_core::model_commands::material;
use ffe_core::model_execute::PolygonEmission;
use ffe_core::model_polygon::Entry;

/// Stand-in for a textured fill colour, which F1691 derives from a table that is
/// not ported; such colours are not compared.
pub const TEXTURED: u32 = u32::MAX;

/// F1721 as the probe records it: the key (11), then the queued entries (30/31)
/// and the fill colour with the entry count (32), or the abandonment (33).
pub fn polygon_events(polygon: &PolygonEmission, nested: &mut Vec<[u32; 4]>) {
    let Some(key) = polygon.key else {
        return;
    };
    nested.push([11, key, 0, 0]);
    let pack = |[x, y]: [i16; 2]| u32::from(x as u16) | u32::from(y as u16) << 16;
    if polygon.outline.abandoned {
        nested.push([33, 1, 0, 0]);
        return;
    }
    for entry in &polygon.outline.entries {
        match *entry {
            Entry::Edge([a, b]) => nested.push([30, 1, pack(a), pack(b)]),
            Entry::To(a) => nested.push([30, 2, pack(a), 0]),
            Entry::Curve([a, b, c, d]) => {
                nested.push([30, 3, pack(a), pack(b)]);
                nested.push([31, pack(c), pack(d), 0]);
            }
        }
    }
    let [red, green, blue] = polygon.colour.map(u32::from);
    let textured = material(polygon.polygon.material).texture != 0;
    nested.push([
        32,
        if textured {
            TEXTURED
        } else {
            red | green << 8 | blue << 16
        },
        polygon.outline.entries.len() as u32,
        0,
    ]);
}

/// Mark the original's textured fill colours like ours, pairwise, and count
/// (fills, entries, textured fills) for reporting.
pub fn mask_textured(ours: &[[u32; 4]], theirs: &mut [[u32; 4]]) -> (usize, usize, usize) {
    let (mut fills, mut entries, mut textured) = (0, 0, 0);
    for (event, original) in ours.iter().zip(theirs.iter_mut()) {
        if event[0] == 32 && original[0] == 32 {
            fills += 1;
            entries += event[2] as usize;
            if event[1] == TEXTURED {
                original[1] = TEXTURED;
                textured += 1;
            }
        }
    }
    (fills, entries, textured)
}
