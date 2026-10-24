use ffe_core::terrain::{Fault, Terrain};
use ffe_core::terrain_draw::Scene;
use ffe_core::terrain_emit::{draw_surface, Queue, Shading};
use serde::Deserialize;

fn cache_terrain() -> (Vec<u8>, Vec<u8>) {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap())
            .unwrap();
    (
        cache.asset("base:terrain/tables").unwrap().to_vec(),
        cache.asset("base:terrain/maps").unwrap().to_vec(),
    )
}

#[derive(Deserialize)]
struct Case {
    case: u32,
    #[serde(default)]
    model: i32,
    #[serde(default)]
    seed: u32,
    #[serde(default)]
    kind: usize,
    #[serde(default)]
    detail: i32,
    #[serde(default)]
    zoom: i32,
    #[serde(default)]
    origin: [i32; 3],
    #[serde(default)]
    vectors: [i32; 9],
    #[serde(default)]
    stream: Vec<u8>,
    fault: bool,
    #[serde(default)]
    overflow: u8,
    #[serde(default)]
    crashed: bool,
    #[serde(default)]
    signal: u8,
    #[serde(default)]
    emits: usize,
    #[serde(default)]
    hash: String,
    #[serde(default)]
    nodes: u32,
    #[serde(default)]
    edges: u32,
    #[serde(default)]
    first: Vec<Vec<i64>>,
    #[serde(default)]
    word0: i32,
    #[serde(default)]
    word8: i32,
    #[serde(default)]
    light: [i32; 3],
    #[serde(default)]
    shades: Vec<u8>,
    #[serde(default)]
    words: usize,
    #[serde(default)]
    queue_hash: String,
    #[serde(default)]
    queue: Vec<u32>,
}

fn shading(case: &Case) -> Shading {
    let mut shades = [[0u8; 3]; 8];
    for (k, shade) in shades.iter_mut().enumerate() {
        shade.copy_from_slice(&case.shades[3 * k..3 * k + 3]);
    }
    Shading {
        light: case.light,
        shades,
        detail: case.detail,
        atmosphere: None,
        material: [
            ((case.word0 >> 8) & 15) as u8,
            ((case.word0 >> 4) & 15) as u8,
            (case.word0 & 15) as u8,
        ],
    }
}

fn mix(hash: &mut u64, value: u32) {
    for k in 0..4 {
        *hash ^= u64::from((value >> (8 * k)) & 0xff);
        *hash = hash.wrapping_mul(1_099_511_628_211);
    }
}

/// The original F1876 in draw mode (`--surface-draw`): the triangles F1868 and
/// F1869 hand the emitter (both nodes' screen and view positions, region codes,
/// heights and levels, the normal, the depth key and the level), and the arena
/// counts, for planet-scale patches of the planet models' records and random ones.
#[test]
#[ignore = "Local-only original world data; use tools/reference/verify-world.py"]
fn executed_reference_surface_draw() {
    let (table, maps) = cache_terrain();
    let input = std::fs::read_to_string(std::env::var_os("FFE_SURFACE_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 256 << 20);
    let (mut cases, mut compared, mut emitted, mut queued, mut overflows) = (0, 0, 0, 0, 0);
    for line in input.lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        cases += 1;
        if case.fault {
            // The original ran out of order queue (or crashed) on this one.
            assert!(
                case.overflow != 0 && !case.crashed && case.signal == 0,
                "case {}: unexpected crash",
                case.case
            );
            if case.word8 == 0 {
                let mut terrain = Terrain::new(&table, &maps);
                let scene = Scene {
                    seed: case.seed,
                    origin: case.origin,
                    axes: case.vectors,
                    kind: case.kind,
                    detail: case.detail,
                    zoom: case.zoom,
                };
                let mut queue = Queue::default();
                let result = draw_surface(
                    &mut terrain,
                    &scene,
                    &case.stream,
                    &shading(&case),
                    &mut queue,
                );
                assert_eq!(
                    result.err(),
                    Some(Fault::QueueFull),
                    "case {}: the order queue should overflow",
                    case.case
                );
                overflows += 1;
            }
            continue;
        }
        let mut terrain = Terrain::new(&table, &maps);
        let scene = Scene {
            seed: case.seed,
            origin: case.origin,
            axes: case.vectors,
            kind: case.kind,
            detail: case.detail,
            zoom: case.zoom,
        };
        let mut hash = 14_695_981_039_346_656_037u64;
        let mut rows: Vec<Vec<i64>> = Vec::new();
        let mut count = 0usize;
        let drawn = terrain
            .draw(&scene, &case.stream, &mut |t, emission| {
                let mut row: Vec<i64> = Vec::with_capacity(35);
                for handle in emission.nodes {
                    let n = t.node(handle);
                    row.extend([
                        i64::from(n.word(0)),
                        i64::from(n.word(2)),
                        i64::from(n.dword(4)),
                        i64::from(n.dword(8)),
                        i64::from(n.dword(0xc)),
                        i64::from(n.word(0x24)),
                        i64::from(n.word(0x26)),
                        i64::from(n.dword(0x3c)),
                        i64::from(n.dword(0x40)),
                        i64::from(n.dword(0x44)),
                    ]);
                }
                row.extend(emission.normal.map(i64::from));
                row.push(i64::from(emission.key as i32));
                row.push(i64::from(emission.level));
                for value in &row {
                    mix(&mut hash, *value as u32);
                }
                if count < 48 {
                    rows.push(row);
                }
                count += 1;
                Ok(())
            })
            .unwrap_or_else(|fault| panic!("case {}: {fault:?}", case.case));
        let _ = drawn;
        let context = format!("case {} (model {})", case.case, case.model);
        assert_eq!(count, case.emits, "{context}: emissions");
        for (k, (got, want)) in rows.iter().zip(&case.first).enumerate() {
            assert_eq!(got, want, "{context}: emission {k}");
        }
        assert_eq!(format!("{hash:016x}"), case.hash, "{context}: hash");
        assert_eq!(terrain.nodes_out(), case.nodes, "{context}: nodes");
        assert_eq!(terrain.edges_out(), case.edges, "{context}: edges");
        if case.word8 == 0 {
            // The emitter, when F1877 has nothing to set up.
            let mut terrain = Terrain::new(&table, &maps);
            let mut queue = Queue::default();
            draw_surface(
                &mut terrain,
                &scene,
                &case.stream,
                &shading(&case),
                &mut queue,
            )
            .unwrap_or_else(|fault| panic!("{context}: {fault:?}"));
            if queue.words.len() != case.words {
                eprintln!("got  {:x?}", &queue.words[..queue.words.len().min(60)]);
                eprintln!("want {:x?}", &case.queue[..case.queue.len().min(60)]);
            }
            assert_eq!(queue.words.len(), case.words, "{context}: queue words");
            for (k, (got, want)) in queue.words.iter().zip(&case.queue).enumerate() {
                assert_eq!(got, want, "{context}: queue word {k}");
            }
            let mut hash = 14_695_981_039_346_656_037u64;
            for word in &queue.words {
                mix(&mut hash, *word);
            }
            assert_eq!(
                format!("{hash:016x}"),
                case.queue_hash,
                "{context}: queue hash"
            );
            queued += 1;
        }
        compared += 1;
        emitted += count;
    }
    assert!(compared > 350, "too few cases: {compared} of {cases}");
    eprintln!(
        "compared {compared} of {cases} cases ({emitted} triangles); {queued} queues, {overflows} overflows"
    );
}
