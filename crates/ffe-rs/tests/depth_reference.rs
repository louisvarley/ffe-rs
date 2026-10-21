use ffe_core::model_depth::key;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    scale: i32,
    point: [i32; 3],
    key: u32,
}

#[test]
#[ignore = "Local-only original depth comparison; use tools/reference/verify-depth.py"]
fn executed_reference_render_order_depth_keys() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_DEPTH_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4 * 1024 * 1024);
    let values = [
        i32::MIN,
        -65536,
        -1,
        0,
        1,
        7,
        8,
        65536,
        0x3fffffff,
        0x40000000,
        i32::MAX,
    ];
    let scales = [
        i32::MIN,
        -33,
        -29,
        -1,
        0,
        3,
        4,
        31,
        32,
        34,
        35,
        36,
        i32::MAX,
    ];
    let mut count = 0;
    for (i, line) in input.lines().enumerate() {
        let v: Vector = serde_json::from_str(line).unwrap();
        assert!(i < 17303);
        assert_eq!(v.scale, scales[i / 1331]);
        assert_eq!(
            v.point,
            [values[i / 121 % 11], values[i / 11 % 11], values[i % 11]]
        );
        assert_eq!(key(v.point, v.scale), v.key, "depth case {i}");
        count += 1;
    }
    assert_eq!(count, 17303);
}
