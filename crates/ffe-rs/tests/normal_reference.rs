use ffe_content::AssetKind;
use ffe_core::model_normals::{evaluate, NormalContext};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    model: u32,
    scale: u32,
    observer_scale: u32,
    observer: usize,
    light: usize,
    normals: u32,
    hash: String,
}

#[test]
#[ignore = "Local-only executed JJFFE normal comparison; use tools/reference/verify-normals.py"]
fn executed_reference_station_normal_visibility_and_lighting() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_NORMAL_CACHE").unwrap()).unwrap())
            .unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_NORMAL_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 128 * 1024);
    let observers = [
        [0, 0, 0],
        [0, 0, 100],
        [0, 0, -100],
        [100, 50, -25],
        [-100, -50, 25],
        [i32::MIN, i32::MAX, -1],
        [65536, -65536, 16384],
    ];
    let lights = [
        [0, 0, 0],
        [0, 0, -65536],
        [-65536, 32768, 16384],
        [i32::MIN, i32::MAX, -1],
    ];
    let mut count = 0;
    for (i, line) in input.lines().enumerate() {
        let vector: Vector = serde_json::from_str(line).unwrap();
        assert_eq!(vector.model, [74, 75][i / 252]);
        assert_eq!(vector.observer, i / 4 % 7);
        assert_eq!(vector.light, i % 4);
        let vid = format!("base:model-vertices/{:03}", vector.model);
        let nid = format!("base:model-normals/{:03}", vector.model);
        let vertices = cache.manifest().files.iter().find(|r| r.id == vid).unwrap();
        let normals = cache.manifest().files.iter().find(|r| r.id == nid).unwrap();
        let (vertex_count, scale) = match vertices.asset {
            AssetKind::ModelVertices {
                vertex_count,
                scale,
                ..
            } => (vertex_count, scale),
            _ => panic!("expected vertices"),
        };
        let normal_count = match normals.asset {
            AssetKind::ModelNormals { normal_count, .. } => normal_count,
            _ => panic!("expected normals"),
        };
        assert_eq!(vector.normals, normal_count);
        assert_eq!(vector.scale, [0, scale, 31][i / 84 % 3]);
        assert_eq!(vector.observer_scale, [0, scale, 31][i / 28 % 3]);
        let context = NormalContext {
            scale: vector.scale,
            observer_scale: vector.observer_scale,
            observer: observers[vector.observer],
            light: lights[vector.light],
            shades: std::array::from_fn(|i| std::array::from_fn(|j| (i * 17 + j * 3 + 1) as u8)),
        };
        let result = evaluate(
            cache.asset(&nid).unwrap(),
            normal_count,
            cache.asset(&vid).unwrap(),
            vertex_count,
            &context,
        )
        .unwrap();
        let hash = result
            .iter()
            .flat_map(|normal| {
                [
                    normal.colour[0],
                    normal.colour[1],
                    normal.colour[2],
                    u8::from(normal.culled),
                ]
            })
            .fold(14695981039346656037u64, |hash, b| {
                (hash ^ u64::from(b)).wrapping_mul(1099511628211)
            });
        assert_eq!(format!("{hash:016x}"), vector.hash, "normal context {i}");
        count += 1;
    }
    assert_eq!(count, 504);
}
