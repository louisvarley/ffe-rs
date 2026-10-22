use ffe_content::AssetKind;
use ffe_core::model::VertexContext;
use ffe_core::model_commands::{material, VisibilityContext};
use ffe_core::model_execute::{execute_live, InstanceContext};
use ffe_core::model_instance::Vertices;
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::Normal;
use ffe_core::model_texture::{
    draw_quad, draw_triangle, flat_colour, pack_colour, quad_uv, textured_colours, textured_path,
    triangle_uv, ENTRY_COLOURS,
};
use ffe_rs::textures::TextureSet;
use serde::Deserialize;

fn cache() -> ffe_content::VerifiedCache {
    let path = std::env::var_os("FFE_TEXTURE_CACHE").unwrap();
    ffe_content::read(&std::fs::read(path).unwrap()).unwrap()
}

fn fnv(bytes: &[u8]) -> String {
    let value = bytes.iter().fold(14695981039346656037u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(1099511628211)
    });
    format!("{value:016x}")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raster {
    case: usize,
    kind: usize,
    tex: u32,
    points: Vec<[i16; 2]>,
    written: usize,
    bounds: [usize; 4],
    pixels_fnv1a64: String,
}

/// Original F1644/F1647 rasterisation (`--texture-raster`): 4096 synthetic
/// triangles/quads over ten bitmaps (64/128 wide, masked and opaque, the 64x512
/// block 94), every orientation, clipped, degenerate and twisted shapes. The whole
/// 320x200 buffer must match.
#[test]
#[ignore = "Local-only original texture rasteriser; use tools/reference/verify-textures.py"]
fn executed_reference_texture_raster() {
    let cache = cache();
    let textures = TextureSet::from_cache(&cache);
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_TEXTURE_RASTER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 8 << 20);
    let slots: [u8; 256] = std::array::from_fn(|i| ((i * 7 + 1) & 0x7f) as u8);
    let mut cases = 0;
    let mut unmodelled_cases = Vec::new();
    for line in input.lines() {
        let case: Raster = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        assert_eq!(case.points.len(), case.kind);
        let map = textures.map(case.tex & 0xfff).unwrap();
        let mut buffer = vec![0xffu8; 320 * 200];
        let plot = |x: i32, y: i32, texel: u8| {
            buffer[y as usize * 320 + x as usize] = slots[usize::from(texel)]
        };
        let unmodelled = if case.kind == 3 {
            let p = [case.points[0], case.points[1], case.points[2]];
            draw_triangle(p, triangle_uv(map.width), map, plot)
        } else {
            let p = [
                case.points[0],
                case.points[1],
                case.points[2],
                case.points[3],
            ];
            draw_quad(p, quad_uv(map.width, (case.tex >> 12 & 7) as u8), map, plot)
        };
        let mut bounds = [usize::MAX, usize::MAX, 0, 0];
        let mut written = 0;
        for (i, _) in buffer.iter().enumerate().filter(|(_, p)| **p != 0xff) {
            written += 1;
            let (x, y) = (i % 320, i / 320);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
        }
        if written == 0 {
            bounds = [0; 4];
        }
        if unmodelled > 0 {
            // Reads outside the bitmap take adjacent original memory; not modelled.
            unmodelled_cases.push(case.case);
            continue;
        }
        assert_eq!(
            (written, bounds, fnv(&buffer)),
            (case.written, case.bounds, case.pixels_fnv1a64.clone()),
            "texture raster case {}",
            case.case
        );
        cases += 1;
    }
    eprintln!(
        "compared {cases} raster cases; {} with unmodelled out-of-bitmap reads: {:?}",
        unmodelled_cases.len(),
        unmodelled_cases
    );
    assert_eq!(cases + unmodelled_cases.len(), 4096);
    assert!(
        unmodelled_cases.len() * 50 < 4096,
        "too many unmodelled cases"
    );
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Queue {
    model: u32,
    #[serde(default)]
    view: usize,
    #[serde(default)]
    null: u32,
    #[serde(default)]
    faces: Vec<Vec<i64>>,
    #[serde(default)]
    alive: u32,
}

fn pack_point([x, y]: [i32; 2]) -> i64 {
    i64::from(x as u16 as u32 | (y as u16 as u32) << 16)
}

/// Original F1708/F1712 per face (`--texture-queue`, every slot, four views): the
/// textured entry (screen points in queue order, descriptor dword, F1326 colours of
/// record + shade clamped to 15), the on-screen flat entry and the F1691 flat colour
/// (record colour 0 for textured materials).
#[test]
#[ignore = "Local-only original textured face queue; use tools/reference/verify-textures.py"]
fn executed_reference_texture_queue() {
    let cache = cache();
    let textures = TextureSet::from_cache(&cache);
    let descriptors = textures.descriptors().expect("cache v8 descriptors");
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_TEXTURE_QUEUE_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 64 << 20);
    let normals: Vec<Normal> = (0..32768u32)
        .map(|i| Normal {
            culled: i % 7 == 3,
            colour: [(i * 3 % 9) as u8, (i * 5 % 10) as u8, (i * 7 % 11) as u8],
        })
        .collect();
    let positions = [[0, 0, 2000], [0, 0, 600], [300, -200, 3000], [0, 0, 12000]];
    let tilted = [
        1518500249,
        0,
        -1518500249,
        0,
        2147483647,
        0,
        1518500249,
        0,
        1518500249,
    ];
    let (mut compared, mut textured, mut flat, mut skipped) = (0usize, 0usize, 0usize, 0usize);
    let mut mismatches = Vec::new();
    for line in input.lines() {
        let original: Queue = serde_json::from_str(line).unwrap();
        let index = original.model;
        if original.null != 0 {
            continue;
        }
        let (Some(commands), Some(records)) = (
            cache.asset(&format!("base:model-commands/{index:03}")),
            cache.asset(&format!("base:model-vertices/{index:03}")),
        ) else {
            skipped += 1;
            continue;
        };
        let (vertex_count, scale) = cache
            .manifest()
            .files
            .iter()
            .find_map(|r| match r.asset {
                AssetKind::ModelVertices {
                    index: i,
                    vertex_count,
                    scale,
                    ..
                } if i == index => Some((vertex_count, scale)),
                _ => None,
            })
            .unwrap();
        let position = positions[original.view];
        let visibility = VisibilityContext {
            depth: position[2],
            scale: scale as i32,
            detail: [0, 0],
            normals: &normals,
        };
        let mut live = Vertices::new(records, vertex_count).unwrap();
        live.set_jitter([0x12345678, 0x456789ab]);
        let mut context = VertexContext {
            scale,
            colour: [1, 2, 3],
            ..Default::default()
        };
        if original.view >= 2 {
            context.matrix = tilted;
        }
        let instance = InstanceContext {
            position,
            generation: 2,
            key_scale: 0,
            operand_scale: scale as i32,
            textures: true,
        };
        let Ok(result) = execute_live(
            commands,
            &mut live,
            &mut context,
            &visibility,
            Some(&table),
            instance,
            4096,
        ) else {
            skipped += 1;
            continue;
        };
        if result.normal_zero_colour.is_some() {
            // Normal 0 rewritten mid-program (F1762); its per-face timing is not
            // carried by the emissions, so these runs are not compared.
            skipped += 1;
            continue;
        }
        let mut rows = Vec::new();
        for emission in &result.emissions {
            let face = &emission.face;
            let normal = &normals[usize::from(face.normal)];
            let kind = face.vertices.len() as i64;
            let mut row = vec![kind, i64::from(face.material), i64::from(face.normal)];
            if normal.culled {
                row.extend([0, -1]);
                rows.push(row);
                continue;
            }
            let mut screens = Vec::new();
            for vertex in &face.vertices {
                let v = live
                    .resolve(*vertex as i8, &emission.context, position, 2, true)
                    .unwrap();
                screens.push([i32::from(v.screen[0]), i32::from(v.screen[1])]);
            }
            let colour = emission.context.colour;
            if textured_path(true, face.material, &screens) {
                let entry =
                    textured_colours(face.material, colour, normal.colour, descriptors).unwrap();
                row.extend([1, -1]);
                row.extend(screens.iter().map(|s| pack_point(*s)));
                row.push(i64::from(entry.bitmap));
                let mut words = entry
                    .colours
                    .iter()
                    .map(|c| i64::from(*c))
                    .collect::<Vec<_>>();
                words.resize(ENTRY_COLOURS, 0);
                row.extend(words);
                textured += 1;
            } else {
                let rgb = flat_colour(face.material, colour, normal.colour, descriptors).unwrap();
                let packed = i64::from(rgb[0]) | i64::from(rgb[1]) << 8 | i64::from(rgb[2]) << 16;
                let on_screen = screens
                    .iter()
                    .all(|[x, y]| (0..320).contains(x) && (0..158).contains(y));
                if on_screen {
                    row.extend([2, packed]);
                    row.extend(screens.iter().map(|s| pack_point(*s)));
                    row.push(i64::from(pack_colour(rgb)));
                    if material(face.material).texture != 0 {
                        flat += 1;
                    }
                } else {
                    row.extend([0, packed]);
                }
            }
            rows.push(row);
        }
        if rows != original.faces || original.alive != 1 {
            let first = rows
                .iter()
                .zip(&original.faces)
                .position(|(a, b)| a != b)
                .unwrap_or(rows.len().min(original.faces.len()));
            mismatches.push(format!(
                "  model {index} view {}: {} vs {} faces, first difference at {first}: {:?} vs {:?}",
                original.view,
                rows.len(),
                original.faces.len(),
                rows.get(first),
                original.faces.get(first)
            ));
        }
        compared += 1;
    }
    eprintln!(
        "compared {compared} model views ({textured} textured entries, {flat} on-screen flat textured-material faces), skipped {skipped}, mismatched {}",
        mismatches.len()
    );
    for mismatch in mismatches.iter().take(20) {
        eprintln!("{mismatch}");
    }
    let minimum: usize = std::env::var("FFE_TEXTURE_QUEUE_MINIMUM")
        .map(|v| v.parse().unwrap())
        .unwrap_or(1);
    assert!(compared >= minimum && textured > 0);
    assert!(mismatches.is_empty());
}
