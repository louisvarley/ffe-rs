use ffe_convert::embedded::{decode_model_vertices, model_command_bytes};
use ffe_core::model::VertexContext;
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{execute_live, InstanceContext};
use ffe_core::model_instance::Vertices;
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::Normal;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Model {
    model: u32,
    #[serde(default)]
    null: u32,
    #[serde(default)]
    faces: usize,
    #[serde(default)]
    emissions: String,
    #[serde(default)]
    points: Vec<[i64; 5]>,
    #[serde(default)]
    nested: Vec<[u32; 4]>,
    #[serde(default)]
    locals: String,
    #[serde(default)]
    basis: [i32; 9],
    #[serde(default)]
    colour: [u8; 3],
    #[serde(default)]
    handedness: u32,
    #[serde(default)]
    alive: u32,
}

fn hash(bytes: impl IntoIterator<Item = u8>) -> String {
    let value = bytes.into_iter().fold(14695981039346656037u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    });
    format!("{value:016x}")
}

/// Every model the native executor completes must match the original F1772 run of
/// the same slot (`--station-model`): identity frame and basis, position z 2000,
/// the descriptor scale at +0x148 (vertex, operand and visibility scale alike),
/// every normal visible, glyphs/children recorded only.
#[test]
#[ignore = "Local-only whole-table comparison; use tools/reference/verify-station-models.py"]
fn executed_reference_station_models() {
    let exe = std::fs::read(std::env::var_os("FFE_STATION_EXE").unwrap()).unwrap();
    let table_rva = u32::from_str_radix(
        std::env::var("FFE_STATION_TABLE_RVA")
            .unwrap()
            .trim_start_matches("0x"),
        16,
    )
    .unwrap();
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_STATION_CACHE").unwrap()).unwrap())
            .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_STATION_MODELS_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 16 << 20);
    let normals = vec![
        Normal {
            culled: false,
            colour: [0; 3]
        };
        32768
    ];
    let (mut compared, mut skipped, mut mismatches) = (0, Vec::new(), Vec::new());
    for line in input.lines() {
        let original: Model = serde_json::from_str(line).unwrap();
        let index = original.model;
        if original.null != 0 {
            continue;
        }
        let (Ok(commands), Ok(vertices)) = (
            model_command_bytes(&exe, table_rva, index),
            decode_model_vertices(&exe, table_rva, index),
        ) else {
            skipped.push(index);
            continue;
        };
        let scale = vertices.scale;
        let visibility = VisibilityContext {
            depth: 2000,
            scale: scale as i32,
            detail: [0, 0],
            normals: &normals,
        };
        let mut live = Vertices::new(&vertices.bytes, vertices.vertex_count).unwrap();
        let mut context = VertexContext {
            scale,
            ..Default::default()
        };
        let instance = InstanceContext {
            position: [0, 0, 2000],
            generation: 2,
            key_scale: 0,
            operand_scale: scale as i32,
            textures: true,
        };
        let Ok(result) = execute_live(
            &commands.bytes,
            &mut live,
            &mut context,
            &visibility,
            Some(&table),
            instance,
            4096,
        ) else {
            skipped.push(index);
            continue;
        };

        let mut emitted = Vec::new();
        for event in &result.emissions {
            let face = &event.face;
            let mut args = vec![u32::from(face.material), u32::from(face.normal)];
            args.extend(face.vertices.iter().copied().map(u32::from));
            if args.len() == 5 {
                args.push(255);
            }
            emitted.extend(args.into_iter().flat_map(u32::to_le_bytes));
            emitted.extend(
                event.context.locals[..8]
                    .iter()
                    .flat_map(|x| x.to_le_bytes()),
            );
            emitted.extend(event.context.basis.into_iter().flat_map(i32::to_le_bytes));
        }
        let (mut p, mut l, mut t, mut s, mut g, mut b, mut c) = (0, 0, 0, 0, 0, 0, 0);
        let mut labels = 0;
        let (mut points, mut nested) = (Vec::new(), Vec::new());
        let mut projected = std::collections::HashSet::new();
        for offset in &result.commands {
            if let Some(point) = result.points.get(p).filter(|e| e.offset == *offset) {
                points.push([
                    1,
                    i64::from(point.material),
                    i64::from(point.encoded_angle),
                    i64::from(point.vertex),
                    if point.flag { 128 } else { 0 },
                ]);
                p += 1;
            }
            if let Some(line) = result.lines.get(l).filter(|e| e.offset == *offset) {
                points.push([
                    2,
                    i64::from(line.material),
                    i64::from(line.vertices[0]),
                    i64::from(line.vertices[1]),
                    0,
                ]);
                l += 1;
            }
            if let Some(block) = result.blocks.get(b).filter(|e| e.offset == *offset) {
                nested.push(match block.key {
                    Some(key) => [12, key, 0, 0],
                    None => [13, 0, 0, 0],
                });
                b += 1;
            }
            if let Some(polygon) = result
                .polygons
                .get(g)
                .filter(|e| e.polygon.offset == *offset)
            {
                if let Some(key) = polygon.key {
                    nested.push([11, key, 0, 0]);
                }
                g += 1;
            }
            // F1759 sound requests: sample and volume.
            for sound in result.sounds.iter().filter(|e| e.offset == *offset) {
                nested.push([18, u32::from(sound.sample), u32::from(sound.volume), 0]);
            }
            // F1764 label entries: key and the screen dword (x | y << 16).
            if let Some(label) = result
                .labels
                .get(labels)
                .filter(|e| e.label.offset == *offset)
            {
                if let Some(key) = label.key {
                    let [x, y] = label.screen;
                    nested.push([17, key, u32::from(x as u16) | u32::from(y as u16) << 16, 0]);
                }
                labels += 1;
            }
            // F1737 curve entries: key, then the screen points packed x | y << 16 and
            // the colour as F1326 receives it.
            if let Some(spline) = result.splines.get(c).filter(|e| e.spline.offset == *offset) {
                if let (Some(key), Some(points)) = (spline.key, spline.points) {
                    let [p0, p1, p2, p3] =
                        points.map(|[x, y]| u32::from(x as u16) | u32::from(y as u16) << 16);
                    let [r, g, blue] = spline.colour.map(u32::from);
                    nested.push([14, key, 0, 0]);
                    nested.push([15, p0, p1, p2]);
                    nested.push([16, p3, r | g << 8 | blue << 16, 0]);
                }
                c += 1;
            }
            if let Some(text) = result.texts.get(t).filter(|e| e.text.offset == *offset) {
                if !text.culled {
                    // F1747 calls F1470 only on a cache miss; F1470 does not stamp.
                    if !projected.contains(&text.text.vertex) {
                        nested.push([1, u32::from(text.text.vertex), 0, 0]);
                    }
                    nested.push([2, u32::from(text.text.glyphs), 0, 0]);
                    nested.push([3, u32::from(text.text.frame), 0, 0]);
                    nested.push([4, 0, 0, 4]);
                }
                t += 1;
            }
            if let Some(call) = result
                .submodels
                .get(s)
                .filter(|e| e.submodel.offset == *offset)
            {
                // F1749 calls F1471 (the probe's event 7) only on a cache miss; in the
                // probe build only earlier child calls have projected a vertex.
                if projected.insert(call.submodel.vertex) {
                    nested.push([7, u32::from(call.submodel.vertex), 0, 0]);
                }
                nested.push([8, u32::from(call.submodel.model), 0, 0]);
                s += 1;
            }
        }
        // Vertex-lookup events (1: text F1470, 7: child F1471) fire only on a cache
        // miss, and polygon segment handlers (not executed natively yet) project
        // vertices; compare every other nested event.
        let lookups = |events: &[[u32; 4]]| -> Vec<[u32; 4]> {
            events
                .iter()
                .copied()
                .filter(|event| !matches!(event[0], 1 | 7))
                .collect()
        };
        let nested_matches = lookups(&nested) == lookups(&original.nested);
        let checks = [
            ("faces", result.emissions.len() == original.faces),
            ("face stream", hash(emitted) == original.emissions),
            ("points/lines", points == original.points),
            ("text/submodel/polygon calls", nested_matches),
            (
                "locals",
                hash(context.locals[..8].iter().flat_map(|x| x.to_le_bytes())) == original.locals,
            ),
            ("basis", context.basis == original.basis),
            ("colour", context.colour == original.colour),
            ("handedness", context.handedness == original.handedness),
            ("alive", original.alive == 1),
        ];
        let failed: Vec<_> = checks
            .iter()
            .filter(|(_, ok)| !ok)
            .map(|(name, _)| *name)
            .collect();
        if !failed.is_empty() {
            mismatches.push(format!("model {index}: {}", failed.join(", ")));
        }
        compared += 1;
    }
    eprintln!(
        "compared {compared} models; {} not executed natively; {} mismatched",
        skipped.len(),
        mismatches.len()
    );
    for mismatch in &mismatches {
        eprintln!("  {mismatch}");
    }
    assert!(mismatches.is_empty(), "{} models differ", mismatches.len());
    let minimum: usize =
        std::env::var("FFE_STATION_MODELS_MINIMUM").map_or(1, |value| value.parse().unwrap());
    assert!(compared >= minimum, "only {compared} models compared");
}
