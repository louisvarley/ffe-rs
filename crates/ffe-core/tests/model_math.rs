use ffe_core::model::{multiply, VertexContext};
use ffe_core::model_commands::{arithmetic, arithmetic_with_table};
use ffe_core::model_math::{angle_operand, SineTable};

#[test]
fn packed_angles_and_rotation_command_validation_preserve_context_on_error() {
    let table = SineTable::from_bytes(&table_bytes()).unwrap();
    let mut context = VertexContext::default();
    context.globals[0] = 12345;
    context.globals[63] = 54321;
    context.locals[8] = -65537;
    for (encoded, expected) in [
        (0, 0),
        (1, 512),
        (0x3f, 32256),
        (0x40, -32768),
        (0x7f, -512),
        (0x80, 12345),
        (0xbf, 54321),
        (0xc8, -65537),
        (0xff80u16 as i16, 12345),
    ] {
        assert_eq!(angle_operand(encoded, &context), expected);
    }
    let matrix = context.basis;
    // Not opcode 28, vertex-defined basis, axis 3 (both forms), undefined permutation 7.
    for code in [0, 0xc01c, 0x801d, 0x807c, 0x007c, 0x039c] {
        assert!(table.rotate_command(&mut context, code, 1).is_err());
        assert_eq!(context.basis, matrix);
        assert_eq!(context.handedness, 0);
    }
    table.rotate_command(&mut context, 0x801c, 0xc8).unwrap();
    let mut expected = matrix;
    table.rotate(&mut expected, 0, -65537).unwrap();
    assert_eq!(context.basis, expected);
    assert_eq!(
        context.matrix,
        ffe_core::model::IDENTITY,
        "vertex frame is untouched"
    );
    assert_eq!(context.locals[8], -65537);
    assert_eq!(context.globals[0], 12345);
    assert!(ffe_core::model_commands::decode(&[0x1c, 0x80, 0xc8, 0, 0, 0], 2).is_ok());

    // Base-relative form: permutation 1 with source row 0 negated, no rotation.
    let mut based = VertexContext {
        matrix: [1, 2, 3, 4, 5, 6, 7, 8, 9],
        base_handedness: 0x1234,
        ..Default::default()
    };
    table
        .rotate_command(&mut based, 0x1000 | 1 << 7 | 28, 0)
        .unwrap();
    assert_eq!(based.basis, [7, 8, 9, -1, -2, -3, 4, 5, 6]);
    assert_eq!(based.matrix, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(based.handedness, !0x1234);
    assert!(ffe_core::model_commands::decode(&[0x1c, 0xc0, 0, 0], 2).is_err());
}

fn table_bytes() -> Vec<u8> {
    (0..16384i32)
        .flat_map(|i| (i * 65536).to_le_bytes())
        .collect()
}

#[test]
fn sine_quadrants_wrap_and_use_last_quarter_entry_not_an_extra_endpoint() {
    let bytes = table_bytes();
    let table = SineTable::from_bytes(&bytes).unwrap();
    for (angle, index, sign) in [
        (0, 0, 1),
        (1, 1, 1),
        (0x3fff, 16383, 1),
        (0x4000, 16383, 1),
        (0x4001, 16382, 1),
        (0x7fff, 0, 1),
        (0x8000, 0, -1),
        (0x8001, 1, -1),
        (0xc000, 16383, -1),
        (-1, 0, -1),
    ] {
        assert_eq!(table.sin32(angle), index * 65536 * sign);
        assert_eq!(table.sin32(angle + 65536), table.sin32(angle));
    }
    assert!(SineTable::from_bytes(&bytes[..bytes.len() - 1]).is_err());
    let mut bad = bytes.clone();
    bad[..4].copy_from_slice(&1i32.to_le_bytes());
    assert!(SineTable::from_bytes(&bad).is_err());
    bad = bytes.clone();
    bad[4..8].copy_from_slice(&(-1i32).to_le_bytes());
    assert!(SineTable::from_bytes(&bad).is_err());
    bad = bytes;
    bad[8..12].copy_from_slice(&0i32.to_le_bytes());
    assert!(SineTable::from_bytes(&bad).is_err());
}

#[test]
fn rotation_uses_per_term_q31_wrapping_negation_and_preserves_unselected_basis() {
    let table = SineTable::from_bytes(&table_bytes()).unwrap();
    let original = [
        i32::MIN,
        i32::MAX,
        -1,
        1,
        65536,
        -65536,
        123456789,
        -987654321,
        0,
    ];
    for (axis, a, b, unchanged) in [(0, 3, 6, 0), (1, 6, 0, 3), (2, 0, 3, 6)] {
        let mut matrix = original;
        let angle = 12345;
        let sine = table.sin32(angle);
        let cosine = table.sin32(angle + 0x4000);
        table.rotate(&mut matrix, axis, angle).unwrap();
        for c in 0..3 {
            assert_eq!(
                matrix[a + c],
                multiply(original[a + c], cosine).wrapping_add(multiply(original[b + c], sine))
            );
            assert_eq!(
                matrix[b + c],
                multiply(original[a + c].wrapping_neg(), sine)
                    .wrapping_add(multiply(original[b + c], cosine))
            );
            assert_eq!(matrix[unchanged + c], original[unchanged + c]);
        }
    }
    let mut matrix = original;
    assert!(table.rotate(&mut matrix, 3, 0).is_err());
    assert_eq!(matrix, original);
}

#[test]
fn trig_arithmetic_commits_low_word_only_and_missing_table_preserves_locals() {
    let table = SineTable::from_bytes(&table_bytes()).unwrap();
    for op in [13, 14] {
        let mut context = VertexContext::default();
        context.locals[8] = i32::MIN;
        context.locals[9] = -12345;
        context.locals[3] = 123;
        let code = 3 << 8 | op << 4 | 13;
        assert!(arithmetic(code, [0xc8, 0xc9], &mut context).is_err());
        assert_eq!(context.locals[3], 123);
        let angle = context.locals[9] + if op == 14 { 0x4000 } else { 0 };
        let expected = multiply(context.locals[8], table.sin32(angle)) as u16;
        assert_eq!(
            arithmetic_with_table(code, [0xc8, 0xc9], &mut context, Some(&table)).unwrap(),
            expected
        );
        assert_eq!(context.locals[3], i32::from(expected));
    }
}

#[test]
fn square_root_length_and_vertex_basis_follow_the_original_integer_steps() {
    use ffe_core::model_math::{fraction, length, square_root, vertex_basis};
    // F1502 is approximate and never returns below 0xffff: every final trial
    // succeeds for a zero input.
    assert_eq!(square_root(0), 0xffff);
    assert_eq!(square_root(4), 114_687);
    assert_eq!(square_root(1 << 30), 1_518_500_252);
    assert_eq!(square_root(u32::MAX), u32::MAX);
    // F1466 is exact for these and never below the largest component.
    assert_eq!(length([3, 4, 0]), 5);
    assert_eq!(length([300_000, -400_000, 0]), 500_000);
    assert_eq!(length([0, 0, i32::MIN]), 0x8000_0000);
    // F1523 saturates instead of dividing when the quotient would overflow.
    assert_eq!(fraction(1, 0), u32::MAX);
    assert_eq!(fraction(1, 2), 1 << 30);
    // z along +z, x = (C - A) x z, y = z x x.
    assert_eq!(
        vertex_basis([0, 0, 0], [0, 0, 100], [100, 0, 0]),
        [0, -2126008811, 0, 2104959219, 0, 0, 0, 0, 2126221433]
    );
}

#[test]
fn every_text_frame_uses_f1747s_handedness_fixup() {
    use ffe_core::model_math::text_frame;
    let frame = [1, 2, 3, 4, 5, 6, 7, 8, 9];
    let basis = [11, 12, 13, 21, 22, 23, 31, 32, 33];
    // Frame mode 6 copies the parent basis and handedness (F1740). A
    // right-handed parent keeps the basis; a left-handed one (0xffff_ffff, as
    // F1739's `not` writes) negates exactly the first row (F1747).
    let (rows, handed) = text_frame(&frame, 0, &basis, 0, 0x0600).unwrap();
    assert_eq!(rows, basis);
    assert_eq!(handed, 0);
    let (rows, handed) = text_frame(&frame, 0, &basis, !0, 0x0600).unwrap();
    assert_eq!(rows, [-11, -12, -13, 21, 22, 23, 31, 32, 33]);
    assert_eq!(handed, !0);
    // Other modes permute the frame through F1739; the handedness word it
    // returns drives the same first-row fixup. A negated source row 1 (0x1000)
    // toggles handedness, so destination row 0 (source row 1 here) is negated
    // once by F1739 and once by F1747.
    let word = 0x1000 | 1 << 8; // permutation 1: destinations [1, 2, 0]
    let (rows, handed) = text_frame(&frame, 0, &basis, 0, word).unwrap();
    assert_eq!(rows, [-7, -8, -9, 1, 2, 3, -4, -5, -6]);
    assert_eq!(handed, !0);
}
