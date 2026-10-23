use ffe_core::terrain::{Node, Terrain, NIL};
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
#[serde(deny_unknown_fields)]
struct Edge {
    head: usize,
    mid: Option<usize>,
    node: Vec<u8>,
    links: [i32; 2],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mid {
    case: u32,
    which: usize,
    level: i32,
    base: i32,
    preset: u32,
    pool: u32,
    layers: Vec<u8>,
    before: Vec<Vec<u8>>,
    after: Vec<Vec<u8>>,
    edge: Edge,
    nodes: u32,
    edges: u32,
}

fn node(bytes: &[u8]) -> Node {
    Node {
        bytes: bytes.try_into().unwrap(),
    }
}

/// The original F1849-F1867 (`--terrain-mid`) on synthetic nodes: every routine
/// against random node fields, levels and layer tables, comparing the four nodes,
/// the edge record's links and the arena counts.
#[test]
#[ignore = "Local-only original world data; use tools/reference/verify-world.py"]
fn executed_reference_terrain_midpoints() {
    let (table, maps) = cache_terrain();
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_TERRAIN_MID_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 64 << 20);
    let (mut cases, mut created, mut coarse) = (0, 0, 0);
    for line in input.lines() {
        let case: Mid = serde_json::from_str(line).unwrap();
        let mut terrain = Terrain::new(&table, &maps);
        terrain.level_base = case.base;
        for (k, code) in case.layers.iter().enumerate() {
            terrain.layers[k / 64][k % 64 / 2][k % 2] = *code;
        }
        let handles: Vec<usize> = (0..4).map(|_| terrain.alloc_node().unwrap()).collect();
        for (handle, bytes) in handles.iter().zip(&case.before) {
            *terrain.node_mut(*handle) = node(bytes);
        }
        let edge = terrain.alloc_edge().unwrap();
        terrain.edge_mut(edge).head = handles[case.edge.head];
        if case.preset != 0 {
            terrain.edge_mut(edge).mid = handles[3];
        }
        terrain
            .midpoint(
                case.which, edge, handles[0], handles[1], handles[2], case.level,
            )
            .unwrap();
        let context = format!(
            "case {} routine {} level {}",
            case.case, case.which, case.level
        );
        for (k, (handle, want)) in handles.iter().zip(&case.after).enumerate() {
            assert_eq!(
                terrain.node(*handle).bytes.as_slice(),
                want.as_slice(),
                "node {k}, {context}"
            );
        }
        let record = *terrain.edge(edge);
        let identify = |link: usize| {
            if link == NIL {
                -1
            } else {
                let head = terrain.edge(link).head;
                handles
                    .iter()
                    .position(|h| *h == head)
                    .map_or(9, |k| k as i32)
            }
        };
        assert_eq!(
            [identify(record.first), identify(record.second)],
            case.edge.links,
            "links, {context}"
        );
        match case.edge.mid {
            Some(3) => assert_eq!(record.mid, handles[3], "mid, {context}"),
            Some(_) => {
                assert_ne!(record.mid, NIL);
                let made = terrain.node(record.mid).bytes;
                // The original leaves the free-list pointer in a word it never writes.
                let mut want = case.edge.node.clone();
                let pointer = u32::from_le_bytes(want[0..4].try_into().unwrap());
                if made[0..4] == [0; 4] && pointer.wrapping_sub(case.pool) < 108_000 {
                    want[0..4].copy_from_slice(&[0; 4]);
                }
                assert_eq!(made.as_slice(), want.as_slice(), "new node, {context}");
                created += 1;
            }
            None => assert_eq!(record.mid, NIL, "mid, {context}"),
        }
        assert_eq!(
            (terrain.nodes_out(), terrain.edges_out()),
            (case.nodes, case.edges),
            "arena counts, {context}"
        );
        coarse += usize::from(terrain.node(handles[0]).level() < case.level);
        cases += 1;
    }
    assert!(cases >= 6000 && created > 4000);
    eprintln!("compared {cases} midpoint cases ({created} new nodes, {coarse} with a coarse end)");
}
