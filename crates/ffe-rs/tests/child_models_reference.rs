mod common;

use ffe_convert::embedded::{
    decode_model_descriptor, decode_model_normals, decode_model_vertices, model_command_bytes,
};
use ffe_core::model::VertexContext;
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{
    execute_scene, ChildEnvironment, ChildModel, ChildModels, ChildOutcome, Execution,
    InstanceContext, QueueEvent,
};
use ffe_core::model_instance::Vertices;
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::Normal;
use ffe_core::model_texture::Descriptors;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Model {
    model: u32,
    #[serde(default)]
    null: u32,
    #[serde(default)]
    points: Vec<[i64; 5]>,
    #[serde(default)]
    nested: Vec<[u32; 4]>,
    #[serde(default)]
    queue: Vec<[u32; 3]>,
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

const OFFSET: u64 = 14695981039346656037;

fn fnv(seed: u64, bytes: impl IntoIterator<Item = u8>) -> u64 {
    bytes
        .into_iter()
        .fold(seed, |h, b| (h ^ u64::from(b)).wrapping_mul(1099511628211))
}

struct Loaded {
    commands: Vec<u8>,
    vertices: Vec<u8>,
    vertex_count: u32,
    normals: Vec<u8>,
    normal_count: u32,
    scale: i32,
    scale_offset: i32,
    radius: i32,
}

struct Table(Vec<Result<Loaded, String>>);

impl ChildModels for Table {
    fn model(&self, index: u16) -> Result<ChildModel<'_>, String> {
        let loaded = self
            .0
            .get(usize::from(index))
            .ok_or_else(|| format!("Child model {index} is outside the table."))?
            .as_ref()
            .map_err(|error| format!("Child model {index} not imported: {error}"))?;
        Ok(ChildModel {
            commands: &loaded.commands,
            vertices: &loaded.vertices,
            vertex_count: loaded.vertex_count,
            normals: &loaded.normals,
            normal_count: loaded.normal_count,
            scale: loaded.scale,
            scale_offset: loaded.scale_offset,
            radius: loaded.radius,
        })
    }
}

fn load(exe: &[u8], rva: u32, index: u32) -> Result<Loaded, String> {
    let commands = model_command_bytes(exe, rva, index)?;
    let vertices = decode_model_vertices(exe, rva, index)?;
    let normals = decode_model_normals(exe, rva, index)?;
    let descriptor = decode_model_descriptor(exe, rva, index)?;
    Ok(Loaded {
        commands: commands.bytes,
        vertices: vertices.bytes,
        vertex_count: vertices.vertex_count,
        normals: normals.bytes,
        normal_count: normals.normal_count,
        scale: descriptor.scale as i32,
        scale_offset: descriptor.scale_offset as i32,
        radius: descriptor.radius as i32,
    })
}

#[derive(Default)]
struct Stats {
    executed: usize,
    culled: usize,
    no_room: usize,
    depth: usize,
    child_faces: usize,
    glows: usize,
    lines: usize,
}

/// The probe's event streams for one frame, recursing into child runs where the
/// original runs them.
fn events(
    result: &Execution,
    depth: u32,
    nested: &mut Vec<[u32; 4]>,
    points: &mut Vec<[i64; 5]>,
    stats: &mut Stats,
    descriptors: &Descriptors,
) {
    let (mut f, mut p, mut l, mut t, mut s, mut g, mut b, mut c, mut labels, mut k) =
        (0, 0, 0, 0, 0, 0, 0, 0, 0, 0);
    let mut thrusters = 0;
    stats.depth = stats.depth.max(depth as usize);
    for offset in &result.commands {
        while let Some(event) = result.emissions.get(f).filter(|e| e.face.offset == *offset) {
            let face = &event.face;
            let mut packed = 0u32;
            for (i, v) in face.vertices.iter().enumerate() {
                packed |= u32::from(*v) << (8 * i);
            }
            if face.vertices.len() == 3 {
                packed |= 255 << 24;
            }
            nested.push([
                22,
                depth,
                u32::from(face.material) | u32::from(face.normal) << 16,
                packed,
            ]);
            if depth > 0 {
                stats.child_faces += 1;
            }
            f += 1;
        }
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
            common::polygon_events(polygon, nested);
            g += 1;
        }
        for sound in result.sounds.iter().filter(|e| e.offset == *offset) {
            nested.push([18, u32::from(sound.sample), u32::from(sound.volume), 0]);
        }
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
        if let Some(spline) = result.splines.get(c).filter(|e| e.spline.offset == *offset) {
            if let (Some(key), Some(points)) = (spline.key, spline.points) {
                let [p0, p1, p2, p3] =
                    points.map(|[x, y]| u32::from(x as u16) | u32::from(y as u16) << 16);
                let [r, gr, blue] = spline.colour.map(u32::from);
                nested.push([14, key, 0, 0]);
                nested.push([15, p0, p1, p2]);
                nested.push([16, p3, r | gr << 8 | blue << 16, 0]);
            }
            c += 1;
        }
        if let Some(thruster) = result
            .thrusters
            .get(thrusters)
            .filter(|e| e.thruster.offset == *offset)
        {
            common::thruster_events(
                thruster,
                common::thruster_line_colour(thruster, descriptors),
                nested,
            );
            for entry in &thruster.entries {
                match entry.shape {
                    ffe_core::model_thruster::Shape::Glow { .. } => stats.glows += 1,
                    ffe_core::model_thruster::Shape::Line { .. } => stats.lines += 1,
                }
            }
            thrusters += 1;
        }
        if let Some(text) = result.texts.get(t).filter(|e| e.text.offset == *offset) {
            if !text.culled {
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
            nested.push([8, u32::from(call.submodel.model), 0, 0]);
            let run = &result.children[k];
            assert_eq!(run.offset, *offset);
            let culled = run.outcome == ChildOutcome::Culled;
            nested.push([19, u32::from(culled), run.radius as u32, 0]);
            if let Some(setup) = &run.setup {
                let context = &setup.context;
                nested.push([9, u32::from(call.submodel.frame), 0, 0]);
                let [r, gr, blue] = context.colour.map(u32::from);
                nested.push([
                    20,
                    depth + 1,
                    setup.instance.operand_scale as u32,
                    r | gr << 8 | blue << 16,
                ]);
                nested.push([
                    23,
                    setup.instance.position[0] as u32,
                    setup.instance.position[1] as u32,
                    setup.instance.position[2] as u32,
                ]);
                let hash = fnv(
                    OFFSET,
                    context
                        .matrix
                        .iter()
                        .chain(&context.locals[..3])
                        .flat_map(|v| v.to_le_bytes()),
                );
                nested.push([
                    24,
                    hash as u32,
                    (hash >> 32) as u32,
                    setup.instance.key_scale as u32,
                ]);
                if let Some((scale, observer)) = setup.observer {
                    let hash = fnv(
                        OFFSET,
                        observer
                            .iter()
                            .chain(&context.direction)
                            .flat_map(|v| v.to_le_bytes()),
                    );
                    nested.push([25, scale, hash as u32, (hash >> 32) as u32]);
                }
                match &run.outcome {
                    ChildOutcome::Executed(child) => {
                        stats.executed += 1;
                        events(child, depth + 1, nested, points, stats, descriptors);
                        nested.push([21, 1, 0, 0]);
                    }
                    _ => {
                        stats.no_room += 1;
                        nested.push([21, 0, 0, 0]);
                    }
                }
            } else {
                assert!(culled);
                stats.culled += 1;
            }
            s += 1;
            k += 1;
        }
    }
}

/// Every model whose scene the native executor completes, children included, must
/// match the original F1772 run of the slot with children executed (`--child-model`):
/// face operands with nesting depth, child set-up, cull/arena outcomes, nested
/// events, points/lines, the whole depth-queue log and the final root state.
#[test]
#[ignore = "Local-only child-model comparison; use tools/reference/verify-child-models.py"]
fn executed_reference_child_models() {
    let exe = std::fs::read(std::env::var_os("FFE_CHILD_EXE").unwrap()).unwrap();
    let rva = u32::from_str_radix(
        std::env::var("FFE_CHILD_TABLE_RVA")
            .unwrap()
            .trim_start_matches("0x"),
        16,
    )
    .unwrap();
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_CHILD_CACHE").unwrap()).unwrap())
            .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let descriptors =
        Descriptors::from_bytes(cache.asset("base:texture/descriptors").unwrap()).unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_CHILD_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 64 << 20);
    // The model's position z (`--child-model N Z`; 2000 by default).
    let depth: i32 = std::env::var("FFE_CHILD_DEPTH").map_or(2000, |value| value.parse().unwrap());
    let models = Table((0..466).map(|index| load(&exe, rva, index)).collect());
    let shade_bytes: Vec<u8> = (0..24u32).map(|i| (i * 29 + 3) as u8).collect();
    let shades: [[u8; 3]; 8] = std::array::from_fn(|i| {
        [
            shade_bytes[3 * i],
            shade_bytes[3 * i + 1],
            shade_bytes[3 * i + 2],
        ]
    });
    let environment = ChildEnvironment {
        models: &models,
        shades,
        light: [0x3000, -0x5000, 0x2000],
        arena: 0,
    };
    let normals = vec![
        Normal {
            culled: false,
            colour: [0; 3]
        };
        32768
    ];
    let (mut compared, mut skipped, mut mismatches) = (0, Vec::new(), Vec::new());
    let mut stats = Stats::default();
    let mut with_children = 0;
    for line in input.lines() {
        let original: Model = serde_json::from_str(line).unwrap();
        let index = original.model;
        // The reference binary embeds the JJFFE declarations; model 361's vertex data
        // differs from floppy 1.02 (v0.29), so its face keys cannot match.
        if original.null != 0 || index == 361 {
            continue;
        }
        let (Ok(commands), Ok(vertices)) = (
            model_command_bytes(&exe, rva, index),
            decode_model_vertices(&exe, rva, index),
        ) else {
            skipped.push(index);
            continue;
        };
        let scale = vertices.scale;
        let visibility = VisibilityContext {
            depth,
            scale: scale as i32,
            detail: [0, 0],
            normals: &normals,
        };
        let mut live = Vertices::new(&vertices.bytes, vertices.vertex_count).unwrap();
        let mut context = VertexContext {
            scale,
            ..Default::default()
        };
        for (k, chunk) in shade_bytes.chunks_exact(4).enumerate() {
            context.locals[12 + k] = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        }
        let instance = InstanceContext {
            position: [0, 0, depth],
            generation: 2,
            key_scale: 0,
            operand_scale: scale as i32,
            textures: true,
        };
        let result = match execute_scene(
            &commands.bytes,
            &mut live,
            &mut context,
            &visibility,
            Some(&table),
            instance,
            4096,
            &environment,
        ) {
            Ok(result) => result,
            Err(error) => {
                if std::env::var_os("FFE_CHILD_VERBOSE").is_some() {
                    eprintln!("  skipped model {index}: {error}");
                }
                skipped.push(index);
                continue;
            }
        };
        let (mut nested, mut points) = (Vec::new(), Vec::new());
        let before = stats.executed + stats.culled + stats.no_room;
        events(
            &result,
            0,
            &mut nested,
            &mut points,
            &mut stats,
            &descriptors,
        );
        if stats.executed + stats.culled + stats.no_room > before {
            with_children += 1;
        }
        let queue: Vec<[u32; 3]> = result
            .queue
            .iter()
            .filter_map(|event| match event {
                QueueEvent::Group(key) => Some([2, u32::from(key.is_some()), key.unwrap_or(0)]),
                QueueEvent::Face(key) => Some([1, 0, *key]),
                QueueEvent::Polygon(key) => Some([3, 0, *key]),
                QueueEvent::Nested(key) => Some([4, u32::from(key.is_some()), key.unwrap_or(0)]),
                QueueEvent::Spline(key) => Some([5, 0, *key]),
                QueueEvent::Label(key) => Some([6, 0, *key]),
                QueueEvent::Thruster(key) => Some([7, 0, *key]),
                QueueEvent::ThrusterLine(key) => Some([8, 0, *key]),
                QueueEvent::Point(_) | QueueEvent::Line(_) => None,
            })
            .collect();
        // Vertex-lookup events (1: text F1470, 7: child F1471) fire only on a cache miss.
        // Event 26 is the child's base handedness: the original leaves it as stack
        // memory for a child whose own handedness is undefined, so it is not compared.
        let lookups = |events: &[[u32; 4]]| -> Vec<[u32; 4]> {
            events
                .iter()
                .copied()
                .filter(|event| !matches!(event[0], 1 | 7 | 26))
                .collect()
        };
        let locals = format!(
            "{:016x}",
            fnv(
                OFFSET,
                context.locals[..8].iter().flat_map(|x| x.to_le_bytes())
            )
        );
        let (ours, mut theirs) = (lookups(&nested), lookups(&original.nested));
        common::mask_textured(&ours, &mut theirs);
        common::mask_thruster_lines(&ours, &mut theirs);
        let checks = [
            ("nested/face/child events", ours == theirs),
            ("points/lines", points == original.points),
            ("queue", queue == original.queue),
            ("locals", locals == original.locals),
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
            if std::env::var_os("FFE_CHILD_VERBOSE").is_some() {
                let (a, b) = (lookups(&nested), lookups(&original.nested));
                if let Some(i) = (0..a.len().max(b.len())).find(|i| a.get(*i) != b.get(*i)) {
                    eprintln!(
                        "  model {index} first nested difference at {i}: rust {:?} original {:?}",
                        a.get(i),
                        b.get(i)
                    );
                }
                if let Some(i) = (0..queue.len().max(original.queue.len()))
                    .find(|i| queue.get(*i) != original.queue.get(*i))
                {
                    eprintln!(
                        "  model {index} first queue difference at {i}: rust {:?} original {:?}",
                        queue.get(i),
                        original.queue.get(i)
                    );
                }
            }
            mismatches.push(format!("model {index}: {}", failed.join(", ")));
        }
        compared += 1;
    }
    eprintln!(
        "compared {compared} models ({with_children} with child calls); {} not executed natively; {} mismatched",
        skipped.len(),
        mismatches.len()
    );
    eprintln!(
        "children: {} executed ({} child faces, depth up to {}), {} culled, {} without arena room",
        stats.executed, stats.child_faces, stats.depth, stats.culled, stats.no_room
    );
    eprintln!(
        "thrusters: {} glow entries, {} line entries queued",
        stats.glows, stats.lines
    );
    eprintln!("not executed natively: {skipped:?}");
    for mismatch in &mismatches {
        eprintln!("  {mismatch}");
    }
    assert!(mismatches.is_empty(), "{} models differ", mismatches.len());
    let minimum: usize =
        std::env::var("FFE_CHILD_MINIMUM").map_or(1, |value| value.parse().unwrap());
    assert!(compared >= minimum, "only {compared} models compared");
}
