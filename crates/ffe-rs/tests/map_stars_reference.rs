//! Local-only comparison of the galaxy map's star light points with the executed original:
//! F1769 (opcode 30 `0x801e` of model 0xab) over real F841 sector records (`--map-stars`).
//! Run through `tools/reference/verify-map-stars.py`.
use ffe_core::model_execute::Primitive;
use ffe_core::star_points::StarField;
use ffe_rs::model_viewer::ModelViewer;
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("not an integer: {value}"))
}

fn bytes(value: &Value) -> Vec<u8> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| int(v) as u8)
        .collect()
}

#[test]
#[ignore = "Local-only map star comparison; use tools/reference/verify-map-stars.py"]
fn executed_reference_map_stars() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_MAP_CACHE").expect("FFE_MAP_CACHE")).unwrap(),
    )
    .unwrap();
    let viewer = ModelViewer::new(&cache).unwrap();
    let path = std::env::var_os("FFE_MAP_VECTORS").expect("FFE_MAP_VECTORS is not set");
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.len() < 16 << 20, "trace too large");
    let (mut cases, mut stars, mut stalks, mut labels, mut hidden, mut queued) = (0, 0, 0, 0, 0, 0);
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        assert_eq!(int(&case["alive"]), 1, "{line}");
        let count = int(&case["n"]) as usize;
        let types: Vec<u8> = bytes(&case["types"]).iter().map(|t| t & 15).collect();
        // The original keeps the whole byte; the tables read its low nibble.
        let raw_types = bytes(&case["types"]);
        let field = StarField {
            types: raw_types.clone(),
            stalks: int(&case["flags"]) & 0x20 != 0,
            code: int(&case["code"]) as u32,
        };
        assert_eq!(types.len(), count);
        let matrix: Vec<i32> = case["matrix"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| int(v) as i32)
            .collect();
        let position: Vec<i32> = case["position"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| int(v) as i32)
            .collect();
        let result = viewer
            .execute_stars(
                &field,
                &bytes(&case["records"]),
                matrix.try_into().unwrap(),
                position.try_into().unwrap(),
            )
            .unwrap_or_else(|error| panic!("{error}: {line}"));
        // The original's calls in order: per star its stalk line (if any) then its point.
        let mut want = Vec::new();
        for call in case["calls"].as_array().unwrap() {
            want.push(
                call.as_array()
                    .unwrap()
                    .iter()
                    .map(int)
                    .collect::<Vec<i64>>(),
            );
        }
        let mut got: Vec<Vec<i64>> = Vec::new();
        let (mut p, mut l) = (0, 0);
        for _ in 0..count {
            if field.stalks {
                let line = &result.lines[l];
                got.push(vec![
                    2,
                    i64::from(line.material),
                    i64::from(line.vertices[0]),
                    i64::from(line.vertices[1]),
                    0,
                ]);
                l += 1;
            }
            let point = &result.points[p];
            got.push(vec![
                1,
                i64::from(point.material),
                i64::from(point.encoded_angle),
                i64::from(point.vertex),
                i64::from(point.flag),
            ]);
            p += 1;
        }
        assert_eq!((p, l), (result.points.len(), result.lines.len()), "{line}");
        assert_eq!(got, want, "calls differ: {line}");
        // Labels: the original's queued entries and their depth keys (F1764 queues only
        // inside x 2..=317, y 4..=149).
        let mut got_labels = Vec::new();
        let mut got_keys = Vec::new();
        for label in &result.system_labels {
            match label.key {
                Some(key) => {
                    let [x, y] = label.screen;
                    let screen = i64::from(x as u16) | i64::from(y as u16) << 16;
                    got_labels.push(vec![
                        i64::from(label.selector),
                        screen,
                        i64::from(label.code),
                    ]);
                    got_keys.push(vec![i64::from(key), screen]);
                    queued += 1;
                }
                None => hidden += 1,
            }
        }
        let list = |value: &Value| -> Vec<Vec<i64>> {
            value
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_array().unwrap().iter().map(int).collect())
                .collect()
        };
        assert_eq!(
            got_labels,
            list(&case["labels"]),
            "labels differ: {line} {:?}",
            result.system_labels
        );
        assert_eq!(got_keys, list(&case["keys"]), "label keys differ: {line}");
        // The order queue holds the same label keys, in queue order.
        let in_order: Vec<u32> = result
            .ordered
            .iter()
            .filter_map(|p| match p {
                Primitive::SystemLabel(label) => label.key,
                _ => None,
            })
            .collect();
        assert_eq!(in_order.len(), got_keys.len(), "{line}");
        cases += 1;
        stars += count;
        stalks += result.lines.len();
        labels += result.system_labels.len();
    }
    eprintln!(
        "{cases} sectors, {stars} stars, {stalks} stalks, {labels} labels ({queued} queued, {hidden} off view)"
    );
    assert!(cases >= 240 && stars > 1000 && stalks > 300 && queued > 100 && hidden > 50);
}
