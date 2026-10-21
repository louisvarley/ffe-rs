use ffe_core::model::{evaluate, multiply, validate_records, variable, VertexContext};

#[test]
fn literal_pairs_use_signed_bytes_q31_per_term_and_mirror_before_rotation() {
    let records = [1, 0, 3, 254, 7, 0];
    let context = VertexContext {
        scale: 10,
        ..Default::default()
    };
    let points = evaluate(&records, 2, &context).unwrap();
    assert_eq!(points, [[3071, -2048, 7167], [-3071, -2048, 7167]]);
    let rotated = VertexContext {
        matrix: [0, i32::MAX, 0, -i32::MAX, 0, 0, 0, 0, i32::MAX],
        scale: 10,
        ..Default::default()
    };
    assert_eq!(
        evaluate(&records, 2, &rotated).unwrap(),
        [[2047, 3071, 7167], [2047, -3071, 7167]]
    );
    assert_eq!(multiply(i32::MIN, i32::MIN), i32::MIN);
    assert_eq!(multiply(1, i32::MAX), 0);
    assert_eq!(multiply(-1, i32::MAX), -1);
}

#[test]
fn computed_vertices_resolve_forward_references_negation_midpoints_and_variables() {
    let records = [
        11, 0, 0, 4, 6, 0, 5, 0, 0, 0, 4, 0, 1, 0, 4, 0, 0, 0, 1, 0, 8, 0, 0, 0, 19, 0, 0x81, 4, 6,
        0, 19, 0, 0xc2, 4, 6, 0,
    ];
    let mut context = VertexContext {
        scale: 10,
        ..Default::default()
    };
    context.globals[1] = 32768;
    context.locals[2] = 65536;
    let points = evaluate(&records, 12, &context).unwrap();
    assert_eq!(points[0][0], 6143);
    assert_eq!(points[2][0], -4095);
    assert_eq!(points[8][0], 6143);
    assert_eq!(points[10][0], 8191);
    context.locals[2] = 0;
    assert_eq!(evaluate(&records, 12, &context).unwrap()[10][0], 4095);
    assert_eq!(variable(63, &context), 63);
    assert_eq!(variable(0x7f, &context), 63 * 1024);
    assert_eq!(variable(0x81, &context), 32768);
}

#[test]
fn midpoint_truncates_negative_wrapped_sum_towards_zero() {
    let records = [1, 0, 255, 0, 0, 0, 1, 0, 254, 0, 0, 0, 11, 0, 0, 0, 2, 0];
    assert_eq!(
        evaluate(&records, 6, &VertexContext::default()).unwrap()[4][0],
        -1
    );
}

#[test]
fn malformed_unsupported_external_and_cyclic_programs_fail_explicitly() {
    for (records, count) in [
        (&[][..], 0),
        (&[1; 6][..], 1),
        (&[1; 5][..], 2),
        (&[1; 390][..], 130),
    ] {
        assert!(validate_records(records, count).is_err());
    }
    for opcode in [3, 4, 7, 8, 13, 14, 21, 23, 255] {
        assert!(evaluate(&[opcode, 0, 0, 0, 0, 0], 2, &VertexContext::default()).is_err());
    }
    assert!(evaluate(&[5, 0, 0, 0, 255, 0], 2, &VertexContext::default()).is_err());
    assert!(evaluate(&[5, 0, 0, 0, 0, 0], 2, &VertexContext::default())
        .unwrap_err()
        .contains("Cyclic"));
    let context = VertexContext {
        scale: 32,
        ..Default::default()
    };
    assert!(evaluate(&[1, 0, 0, 0, 0, 0], 2, &context).is_err());
}

#[test]
fn remaining_supported_vector_operations_and_maximum_count_are_bounded() {
    let records = [
        1, 0, 5, 2, 3, 0, 1, 0, 3, 4, 5, 0, 15, 0, 0, 0, 2, 0, 17, 0, 0, 0, 2, 0, 9, 0, 5, 2, 3, 0,
    ];
    let context = VertexContext {
        scale: 10,
        ..Default::default()
    };
    let points = evaluate(&records, 10, &context).unwrap();
    assert_eq!(points[4], [7167, -1, 1023]);
    assert_eq!(points[6], [8190, 6142, 8190]);
    assert_eq!(points[9], [0, 2047, 3071]);
    let maximum = [1, 0, 0, 0, 0, 0].repeat(64);
    assert_eq!(evaluate(&maximum, 128, &context).unwrap().len(), 128);
}

#[test]
fn jittered_pairs_draw_three_offsets_once_and_swap_them_for_the_mirror() {
    use ffe_core::model::{evaluate_with_jitter, jitter, JITTER_SEED};
    // Pair 0: literal (0, 0, 10) and its reflection. Pair 1: opcode 7 on vertex 0,
    // offsets shifted right by 20 bits.
    let records = [1, 0, 0, 0, 10, 0, 7, 0, 20, 0, 0, 0];
    let context = VertexContext::default();
    let mut state = JITTER_SEED;
    let points = evaluate_with_jitter(&records, 4, &context, &mut state).unwrap();
    let mut expected_state = JITTER_SEED;
    let offsets: [i32; 3] = std::array::from_fn(|_| jitter(&mut expected_state) >> 20);
    assert_eq!(state, expected_state, "exactly three draws for the pair");
    let [x, y, z] = offsets;
    assert_eq!(
        points[2],
        [points[0][0] + x, points[0][1] + y, points[0][2] + z]
    );
    assert_eq!(
        points[3],
        [points[1][0] + z, points[1][1] + y, points[1][2] + x],
        "the mirror takes z, y, x"
    );
    // The default evaluation starts from the original seed and leaves no state behind.
    assert_eq!(
        ffe_core::model::evaluate(&records, 4, &context).unwrap(),
        points
    );
    // Generator: a = s0; s0 += s1; s1 = rotl16(a).
    let mut s = [1, 2];
    assert_eq!(jitter(&mut s), 0x0001_0000);
    assert_eq!(s, [3, 0x0001_0000]);
}
