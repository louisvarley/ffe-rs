//! Event encodings shared by the whole-table reference tests.
#![allow(dead_code)] // Each test binary uses a different subset.
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

/// A screen point as the probe packs it: x | y << 16.
pub fn pack_point([x, y]: [i16; 2]) -> u32 {
    u32::from(x as u16) | u32::from(y as u16) << 16
}

fn pack_rgb([r, g, b]: [u8; 3]) -> u32 {
    u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16
}

/// F1738's queued entries as the probe records them. A glow (F1805): key (41),
/// points 0..2 (42), point 3 and texture word (43), ramp slots 1..3 (44) and 4..6
/// (45). A narrow thruster: the F1704 call with both screens and the F1691 colour
/// (46), then, when F1704 queues it, the key (47) and the stored end points (48). `line_colour` is F1691's colour
/// (`model_thruster::line_colour`); `None` records [`TEXTURED`] (not compared).
pub fn thruster_events(
    emission: &ffe_core::model_execute::ThrusterEmission,
    line_colour: Option<[u8; 3]>,
    nested: &mut Vec<[u32; 4]>,
) {
    use ffe_core::model_thruster::Shape;
    let colour = line_colour.map_or(TEXTURED, pack_rgb);
    // F1738 hands a narrow thruster to F1704 whether or not F1704 then queues it.
    if let (Some(width), Some([a, b])) = (emission.width, emission.screens) {
        if width != 0 && width <= 4 {
            nested.push([46, pack_point(a), pack_point(b), colour]);
        }
    }
    for entry in &emission.entries {
        match &entry.shape {
            Shape::Glow {
                points,
                texture,
                colours,
            } => {
                let [p0, p1, p2, p3] = points.map(pack_point);
                let [c1, c2, c3, c4, c5, c6] = colours.map(pack_rgb);
                nested.push([41, entry.key, 0, 0]);
                nested.push([42, p0, p1, p2]);
                nested.push([43, p3, *texture, 0]);
                nested.push([44, c1, c2, c3]);
                nested.push([45, c4, c5, c6]);
            }
            Shape::Line { points, .. } => line_events(entry.key, *points, nested),
        }
    }
}

/// F1691's colour for a thruster's F1704 line: the F1694 colour of its first
/// entry plus the texture's first descriptor colour; `None` (not compared) for a
/// texture the descriptor table does not hold.
pub fn thruster_line_colour(
    emission: &ffe_core::model_execute::ThrusterEmission,
    descriptors: &ffe_core::model_texture::Descriptors,
) -> Option<[u8; 3]> {
    let base =
        ffe_core::model_thruster::base_colour(emission.thruster.material, emission.context.colour);
    ffe_core::model_thruster::line_colour(emission.thruster.material, base, descriptors).ok()
}

/// F1704's events (47, 48) for a queued line.
pub fn line_events(key: u32, points: [[i32; 2]; 2], nested: &mut Vec<[u32; 4]>) {
    nested.push([47, key, 0, 0]);
    let [p0, p1] = points.map(|[x, y]| (x as u32 & 0xffff) | (y as u32) << 16);
    nested.push([48, p0, p1, 0]);
}

/// Mark the original's thruster line colours like ours where ours are [`TEXTURED`].
pub fn mask_thruster_lines(ours: &[[u32; 4]], theirs: &mut [[u32; 4]]) {
    for (event, original) in ours.iter().zip(theirs.iter_mut()) {
        if event[0] == 46 && original[0] == 46 && event[3] == TEXTURED {
            original[3] = TEXTURED;
        }
    }
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
