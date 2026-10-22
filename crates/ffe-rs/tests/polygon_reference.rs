use ffe_core::model_math::table_root;
use ffe_core::model_polygon::{circle, circle_point, curve_edges, near_point};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Row {
    k: String,
    #[serde(rename = "in")]
    input: Vec<i64>,
    out: Vec<i64>,
}

fn int(value: i64) -> i32 {
    i32::try_from(value).unwrap()
}

fn short(value: i64) -> i16 {
    i16::try_from(value).unwrap()
}

fn screen(point: [i16; 2]) -> [i64; 2] {
    point.map(i64::from)
}

/// The polygon helpers called directly in the original (`--polygon-helpers`):
/// F1467 table root, F1564 near-plane point, F1557 circle point, F1554 circle and
/// F1547 curve subdivision (the edges it hands to F1560).
#[test]
#[ignore = "Local-only polygon helpers; use tools/reference/verify-polygon.py"]
fn executed_reference_polygon_helpers() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_POLYGON_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 16 << 20);
    let mut counts = std::collections::BTreeMap::new();
    for (line_number, line) in input.lines().enumerate() {
        let row: Row = serde_json::from_str(line).unwrap();
        let i = &row.input;
        let actual: Vec<i64> = match row.k.as_str() {
            "root" => vec![i64::from(table_root(u32::try_from(i[0]).unwrap()))],
            "near" => screen(near_point(
                [int(i[0]), int(i[1]), int(i[2])],
                [int(i[3]), int(i[4]), int(i[5])],
            ))
            .to_vec(),
            "point" => match circle_point([int(i[0]), int(i[1]), int(i[2])]) {
                Some(p) => vec![1, i64::from(p[0]), i64::from(p[1])],
                None => vec![0, 0, 0],
            },
            "circle" => match circle(
                [int(i[0]), int(i[1]), int(i[2])],
                [int(i[3]), int(i[4]), int(i[5])],
                int(i[6]),
            ) {
                // The buffer holds +0x6c..+0x88: first curve, then the second.
                Some(curves) => std::iter::once(1)
                    .chain(curves.iter().flatten().flat_map(|p| screen(*p)))
                    .collect(),
                None => vec![0],
            },
            "curve" => {
                let points: [[i16; 2]; 4] =
                    std::array::from_fn(|p| [short(i[2 * p]), short(i[2 * p + 1])]);
                curve_edges(points)
                    .iter()
                    .flatten()
                    .flat_map(|p| screen(*p))
                    .collect()
            }
            other => panic!("unknown helper kind {other}"),
        };
        assert_eq!(
            actual,
            row.out,
            "line {} ({}) {:?}",
            line_number + 1,
            row.k,
            row.input
        );
        *counts.entry(row.k).or_insert(0usize) += 1;
    }
    eprintln!("compared {counts:?}");
    for kind in ["root", "near", "point", "circle", "curve"] {
        assert!(
            counts.get(kind).copied().unwrap_or(0) >= 2048,
            "too few {kind} rows"
        );
    }
}
