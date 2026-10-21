use ffe_convert::embedded::decode_model_vertices;
use ffe_core::model::{evaluate_with_jitter, VertexContext};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    model: u32,
    seed: usize,
    vertices: u32,
    coordinates: String,
    state: [u32; 2],
}

const SEEDS: [[u32; 2]; 2] = [[0x1234_5678, 0x4567_89ab], [0x9e37_79b9, 0x7f4a_7c15]];

fn hash(bytes: impl IntoIterator<Item = u8>) -> String {
    let value = bytes.into_iter().fold(14695981039346656037u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    });
    format!("{value:016x}")
}

/// Every model with opcode 7/8 vertex records, evaluated in index order by the
/// original F1472/F1478 with two F1476 seeds (`--jitter-vertices`): identity
/// frame, descriptor scale, zero variables.
#[test]
#[ignore = "Local-only jittered vertices; use tools/reference/verify-jitter.py"]
fn executed_reference_jittered_vertices() {
    let exe = std::fs::read(std::env::var_os("FFE_JITTER_EXE").unwrap()).unwrap();
    let table_rva = u32::from_str_radix(
        std::env::var("FFE_JITTER_TABLE_RVA")
            .unwrap()
            .trim_start_matches("0x"),
        16,
    )
    .unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_JITTER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 1 << 20);
    let (mut compared, mut skipped) = (0, Vec::new());
    for line in input.lines() {
        let v: Vector = serde_json::from_str(line).unwrap();
        // The importer validates vertex records; other unsupported opcodes are skipped.
        let Ok(model) = decode_model_vertices(&exe, table_rva, v.model) else {
            skipped.push(v.model);
            continue;
        };
        assert_eq!(model.vertex_count, v.vertices, "model {}", v.model);
        let context = VertexContext {
            scale: model.scale,
            ..Default::default()
        };
        let mut state = SEEDS[v.seed];
        let points =
            evaluate_with_jitter(&model.bytes, model.vertex_count, &context, &mut state).unwrap();
        let bytes = points.iter().flatten().flat_map(|c| c.to_le_bytes());
        assert_eq!(
            hash(bytes),
            v.coordinates,
            "model {} seed {}",
            v.model,
            v.seed
        );
        assert_eq!(state, v.state, "model {} seed {} state", v.model, v.seed);
        compared += 1;
    }
    skipped.dedup();
    eprintln!("compared {compared} evaluations; skipped models {skipped:?}");
    let minimum: usize = std::env::var("FFE_JITTER_MINIMUM").map_or(1, |m| m.parse().unwrap());
    assert!(compared >= minimum, "only {compared} evaluations compared");
}
