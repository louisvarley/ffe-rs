use ffe_convert::embedded::decode_model_vertices;
use ffe_core::model::{evaluate_entries, evaluate_with_jitter, VertexContext, JITTER_SEED};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Program {
    model: u32,
    #[serde(default)]
    null: u32,
    #[serde(default)]
    vertices: u32,
    /// FNV-1a of the reference build's own vertex records (JJFFE declarations).
    #[serde(default)]
    records: String,
    #[serde(default)]
    coordinates: String,
    /// FNV-1a of every vertex entry's screen words and view dwords.
    #[serde(default)]
    entries: String,
    #[serde(default)]
    state: [u32; 2],
}

fn hash(bytes: impl IntoIterator<Item = u8>) -> String {
    let value = bytes.into_iter().fold(14695981039346656037u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    });
    format!("{value:016x}")
}

/// Every vertex of every model slot the native evaluator supports, against the
/// original F1472/F1478 (`--vertex-program`): identity frame, descriptor scale,
/// zero variables, start-up F1476 seed, cache-checked index order.
#[test]
#[ignore = "Local-only vertex programs; use tools/reference/verify-vertex-programs.py"]
fn executed_reference_vertex_programs() {
    let exe = std::fs::read(std::env::var_os("FFE_VERTEX_EXE").unwrap()).unwrap();
    let table_rva = u32::from_str_radix(
        std::env::var("FFE_VERTEX_TABLE_RVA")
            .unwrap()
            .trim_start_matches("0x"),
        16,
    )
    .unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_VERTEX_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 1 << 20);
    let (mut compared, mut skipped, mut mismatched) = (0, 0, Vec::new());
    let mut data_differs = Vec::new();
    for line in input.lines() {
        let v: Program = serde_json::from_str(line).unwrap();
        if v.null != 0 {
            continue;
        }
        let Ok(model) = decode_model_vertices(&exe, table_rva, v.model) else {
            skipped += 1;
            continue;
        };
        assert_eq!(model.vertex_count, v.vertices, "model {}", v.model);
        // The reference is built from JJFFE's declarations; where they differ from
        // this release's bytes, the comparison is not meaningful.
        if hash(model.bytes.iter().copied()) != v.records {
            data_differs.push(v.model);
            continue;
        }
        let context = VertexContext {
            scale: model.scale,
            ..Default::default()
        };
        let mut state = JITTER_SEED;
        let points =
            evaluate_with_jitter(&model.bytes, model.vertex_count, &context, &mut state).unwrap();
        let actual = hash(points.iter().flatten().flat_map(|c| c.to_le_bytes()));
        // The entry bytes the original's cache holds: the screen words and the
        // three view dwords of every vertex, in index order.
        let entries = evaluate_entries(&model.bytes, model.vertex_count, &context).unwrap();
        let actual_entries = hash(entries.iter().flat_map(|entry| {
            entry
                .screen
                .iter()
                .flat_map(|s| s.to_le_bytes())
                .chain(entry.view.iter().flat_map(|v| v.to_le_bytes()))
        }));
        if actual != v.coordinates || actual_entries != v.entries || state != v.state {
            mismatched.push(v.model);
        }
        compared += 1;
    }
    eprintln!(
        "compared {compared} vertex programs; {skipped} unsupported; release data differs \
         from the reference for {data_differs:?}; mismatched {mismatched:?}"
    );
    assert!(mismatched.is_empty(), "{} models differ", mismatched.len());
    let minimum: usize = std::env::var("FFE_VERTEX_MINIMUM").map_or(1, |m| m.parse().unwrap());
    assert!(
        compared >= minimum,
        "only {compared} vertex programs compared"
    );
}
