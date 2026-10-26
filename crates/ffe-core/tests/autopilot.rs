use ffe_core::autopilot::{face_target, normalise_velocity, request};

#[test]
fn behind_target_uses_source_half_turn_and_velocity_uses_bit_normalisation() {
    let matrix = [[0x7f000000, 0, 0], [0, 0x7f000000, 0], [0, 0, 0x7f000000]];
    assert_eq!(face_target(matrix, [0, 0, -4096]), [0x4000, 0, 0x4000]);
    assert_eq!(face_target(matrix, [0, 0, 4096]), [0, 0, 0]);
    let mut v = [1, 0, 0];
    assert_eq!(normalise_velocity(&mut v), -14);
    assert_eq!(v, [16384, 0, 0]);
}

#[test]
#[ignore = "Local original executable oracle: FFE_AUTOPILOT_VECTORS"]
fn executed_original_guidance_and_turn_words() {
    let path = std::env::var_os("FFE_AUTOPILOT_VECTORS").unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    let mut count = 0;
    for line in text.lines() {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let array = |name: &str| {
            v[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_i64().unwrap() as i32)
                .collect::<Vec<_>>()
        };
        let m = array("matrix");
        let matrix = std::array::from_fn(|k| std::array::from_fn(|j| m[k * 3 + j]));
        let position: [i32; 3] = array("position").try_into().unwrap();
        let velocity: [i32; 3] = array("velocity").try_into().unwrap();
        let (result, cap) = request(
            matrix,
            velocity,
            v["vshift"].as_i64().unwrap() as i32,
            position,
            v["shift"].as_i64().unwrap() as i32,
            v["modifier"].as_i64().unwrap() as i32,
        );
        assert_eq!(result.to_vec(), array("request"), "request case {count}");
        assert_eq!(cap.to_vec(), array("cap"), "cap case {count}");
        assert_eq!(
            face_target(matrix, position.map(|v| v as i16))
                .map(i32::from)
                .to_vec(),
            array("turns"),
            "turn case {count}"
        );
        let t = array("target_matrix");
        let target = std::array::from_fn(|k| std::array::from_fn(|j| t[k * 3 + j]));
        assert_eq!(
            ffe_core::autopilot::match_orientation(matrix, target, true)
                .map(i32::from)
                .to_vec(),
            array("match_turns"),
            "orientation match case {count}"
        );
        count += 1;
    }
    assert_eq!(count, 512);
}

#[test]
fn f1020_requires_a_target_the_fitted_computer_and_a_flying_state() {
    use ffe_core::autopilot::can_engage;
    for mode in 0..=255u8 {
        assert_eq!(can_engage(mode, true, 1 << 21), mode < 0x28);
        assert!(!can_engage(mode, false, 1 << 21));
        assert!(!can_engage(mode, true, 0));
    }
}
