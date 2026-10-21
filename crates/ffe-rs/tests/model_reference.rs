use ffe_core::model::{evaluate, VertexContext, IDENTITY};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    model: u32,
    matrix: usize,
    scale: u32,
    animation: i32,
    vertices: u32,
    coordinates_fnv1a64: String,
}

#[test]
#[ignore = "Local-only executed JJFFE comparison; use tools/reference/verify-models.py"]
fn executed_reference_station_vertex_programs() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_MODEL_CACHE").unwrap()).unwrap())
            .unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_MODEL_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 128 * 1024);
    let matrices = [
        IDENTITY,
        [0, i32::MAX, 0, -i32::MAX, 0, 0, 0, 0, i32::MAX],
        [
            1073741824, -536870912, 268435456, 536870912, 1073741824, -268435456, -268435456,
            268435456, 1073741824,
        ],
        [i32::MIN, 0, 0, 0, i32::MIN, 0, 0, 0, i32::MIN],
    ];
    let animation = [-65536, 0, 1, 16384, 32768, 65535, 65536, i32::MAX];
    let mut count = 0;
    for (i, line) in input.lines().enumerate() {
        let v: Vector = serde_json::from_str(line).unwrap();
        assert_eq!(v.model, [74, 75][i / 96]);
        assert_eq!(v.matrix, i / 24 % 4);
        assert_eq!(v.animation, animation[i % 8]);
        let id = format!("base:model-vertices/{:03}", v.model);
        let descriptor = &cache
            .manifest()
            .files
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .asset;
        let (vertices, scale) = match descriptor {
            ffe_content::AssetKind::ModelVertices {
                vertex_count,
                scale,
                ..
            } => (*vertex_count, *scale),
            _ => panic!("Expected model vertex program"),
        };
        assert_eq!(v.vertices, vertices);
        assert_eq!(v.scale, [0, scale, 31][i / 8 % 3]);
        let context = VertexContext {
            matrix: matrices[v.matrix],
            scale: v.scale,
            globals: std::array::from_fn(|i| (v.animation as u16).wrapping_add(i as u16 * 257)),
            locals: std::array::from_fn(|i| v.animation ^ (i as i32 * 0x10203)),
        };
        let points = evaluate(cache.asset(&id).unwrap(), vertices, &context).unwrap();
        let hash = points
            .iter()
            .flatten()
            .flat_map(|n| n.to_le_bytes())
            .fold(14695981039346656037u64, |hash, b| {
                (hash ^ u64::from(b)).wrapping_mul(1099511628211)
            });
        assert_eq!(format!("{hash:016x}"), v.coordinates_fnv1a64, "case {i}");
        count += 1;
    }
    assert_eq!(count, 192);
}
