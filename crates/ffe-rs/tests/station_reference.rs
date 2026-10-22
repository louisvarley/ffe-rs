use ffe_core::model::VertexContext;
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{execute_live, InstanceContext, QueueEvent};
use ffe_core::model_instance::Vertices;
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::Normal;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Station {
    model: u32,
    faces: usize,
    emissions: String,
    points: Vec<[i64; 5]>,
    nested: Vec<[u32; 4]>,
    locals: String,
    basis: [i32; 9],
    colour: [u8; 3],
    handedness: u32,
    alive: u32,
}

fn hash(bytes: impl IntoIterator<Item = u8>) -> String {
    let value = bytes.into_iter().fold(14695981039346656037u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    });
    format!("{value:016x}")
}

/// The adapter's `--station` inputs: identity frame and basis, position z = 100
/// (which is also the visibility depth at +0xc), generation 2, zero scales and
/// globals, every normal visible with zero colour.
#[test]
#[ignore = "Local-only whole station programs; use tools/reference/verify-station.py"]
fn executed_reference_station_programs() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_STATION_CACHE").unwrap()).unwrap())
            .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_STATION_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 65536);
    let stations: Vec<Station> = input
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        stations.iter().map(|s| s.model).collect::<Vec<_>>(),
        [74, 75]
    );
    let normals = vec![
        Normal {
            culled: false,
            colour: [0; 3]
        };
        32768
    ];
    let visibility = VisibilityContext {
        depth: 100,
        scale: 0,
        detail: [0, 0],
        normals: &normals,
    };
    for (station, vertex_count) in stations.iter().zip([54u32, 88]) {
        let index = station.model;
        let bytes = cache
            .asset(&format!("base:model-commands/{index:03}"))
            .unwrap();
        let records = cache
            .asset(&format!("base:model-vertices/{index:03}"))
            .unwrap();
        let mut vertices = Vertices::new(records, vertex_count).unwrap();
        let mut context = VertexContext::default();
        let instance = InstanceContext {
            position: [0, 0, 100],
            generation: 2,
            key_scale: 0,
            operand_scale: 0,
            textures: true,
        };
        let result = execute_live(
            bytes,
            &mut vertices,
            &mut context,
            &visibility,
            Some(&table),
            instance,
            4096,
        )
        .unwrap();

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
        assert_eq!(result.emissions.len(), station.faces, "model {index} faces");
        assert_eq!(hash(emitted), station.emissions, "model {index} faces");

        // Points and lines in command order, as the probe records them.
        let mut points = Vec::new();
        let (mut p, mut l, mut t, mut s, mut g, mut b) = (0, 0, 0, 0, 0, 0);
        let mut nested = Vec::new();
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
            if let Some(text) = result.texts.get(t).filter(|e| e.text.offset == *offset) {
                if !text.culled {
                    // Station mode reports no room for glyph buffers after F1745.
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
                // Station mode culls every child, so extras and frames are not reached.
                // F1749 calls F1471 (the probe's event 7) only on a cache miss; in the
                // probe build only earlier child calls have projected a vertex.
                if projected.insert(call.submodel.vertex) {
                    nested.push([7, u32::from(call.submodel.vertex), 0, 0]);
                }
                nested.push([8, u32::from(call.submodel.model), 0, 0]);
                s += 1;
            }
        }
        assert_eq!(
            (p, l, t, s),
            (
                result.points.len(),
                result.lines.len(),
                result.texts.len(),
                result.submodels.len()
            ),
            "model {index} events not matched to visited commands"
        );
        assert_eq!(points, station.points, "model {index} points/lines");
        assert_eq!(nested, station.nested, "model {index} text/submodel calls");
        assert_eq!(
            hash(context.locals[..8].iter().flat_map(|x| x.to_le_bytes())),
            station.locals,
            "model {index} locals"
        );
        assert_eq!(context.basis, station.basis, "model {index} basis");
        assert_eq!(context.colour, station.colour, "model {index} colour");
        assert_eq!(
            context.handedness, station.handedness,
            "model {index} handedness"
        );
        assert_eq!(station.alive, 1);
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Queue {
    model: u32,
    view: usize,
    queue: Vec<[u32; 3]>,
    alive: u32,
}

/// Same inputs as `--station`, with faces drawn by the original F1708/F1712: the
/// original's face allocations and depth-group changes must match Rust's queue log.
#[test]
#[ignore = "Local-only station draw queues; use tools/reference/verify-station-queue.py"]
fn executed_reference_station_queues() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_STATION_CACHE").unwrap()).unwrap())
            .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_STATION_QUEUE_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 65536);
    let queues: Vec<Queue> = input
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        queues.iter().map(|q| (q.model, q.view)).collect::<Vec<_>>(),
        [74, 75]
            .into_iter()
            .flat_map(|m| (0..6).map(move |v| (m, v)))
            .collect::<Vec<_>>()
    );
    // The adapter's six views: position (+0x4, z doubling as visibility depth) and frame.
    let positions = [
        [0, 0, 100],
        [0, 0, 600],
        [150, -80, 400],
        [-40, 30, 250],
        [-260, 0, 300],
        [0, 10, 40],
    ];
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
    let normals = vec![
        Normal {
            culled: false,
            colour: [0; 3]
        };
        32768
    ];
    for queue in &queues {
        let index = queue.model;
        let vertex_count = if index == 74 { 54 } else { 88 };
        let position = positions[queue.view];
        let visibility = VisibilityContext {
            depth: position[2],
            scale: 0,
            detail: [0, 0],
            normals: &normals,
        };
        let mut context = VertexContext::default();
        if queue.view == 3 {
            context.matrix = tilted;
        }
        let bytes = cache
            .asset(&format!("base:model-commands/{index:03}"))
            .unwrap();
        let records = cache
            .asset(&format!("base:model-vertices/{index:03}"))
            .unwrap();
        let mut vertices = Vertices::new(records, vertex_count).unwrap();
        let instance = InstanceContext {
            position,
            generation: 2,
            key_scale: 0,
            operand_scale: 0,
            textures: true,
        };
        let result = execute_live(
            bytes,
            &mut vertices,
            &mut context,
            &visibility,
            Some(&table),
            instance,
            4096,
        )
        .unwrap();
        let events: Vec<[u32; 3]> = result
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
                // Points and lines stay hooked in the probe and are not queued there.
                QueueEvent::Point(_) | QueueEvent::Line(_) => None,
            })
            .collect();
        assert_eq!(
            events, queue.queue,
            "model {index} view {} draw queue",
            queue.view
        );
        assert_eq!(queue.alive, 1);
    }
}
