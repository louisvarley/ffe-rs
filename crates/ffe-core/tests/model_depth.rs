use ffe_core::model_depth::key;
use ffe_core::model_depth::{decode, Command, Context};

#[test]
fn depth_commands_decode_all_modes_and_preserve_exact_operand_consumption() {
    for mode in [0, 4, 5, 6] {
        let word = (mode << 13 | 6u16).to_le_bytes();
        assert_eq!(decode(&word).unwrap(), (Command::Close, 2));
    }
    assert_eq!(
        decode(&[0x46, 0x20, 4, 255, 6, 0]).unwrap(),
        (Command::Maximum(vec![2, 4, 6]), 6)
    );
    assert_eq!(
        decode(&[0x46, 0x60, 4, 0]).unwrap(),
        (Command::Minimum(vec![2, 4]), 4)
    );
    assert_eq!(decode(&[0x46, 0x40]).unwrap(), (Command::Vertex(2), 2));
    assert_eq!(decode(&[0xe6, 0xff]).unwrap(), (Command::Position, 2));
    assert_eq!(decode(&[0xc6, 0xff]).unwrap(), (Command::First, 2));
    assert_eq!(
        decode(&[0xa6, 0xff, 0xff, 0xff]).unwrap(),
        (
            Command::Offset {
                vertex: None,
                amount: -1
            },
            4
        )
    );
    assert_eq!(
        decode(&[0x46, 0xe0, 1, 0]).unwrap(),
        (
            Command::Offset {
                vertex: Some(2),
                amount: 1
            },
            4
        )
    );
    // A negative cached index is structurally valid but cannot resolve through native points.
    assert_eq!(decode(&[0xe6, 0x5f]).unwrap(), (Command::Vertex(-1), 2));
}

#[test]
fn depth_commands_use_signed_list_extrema_but_keep_unsigned_keys() {
    let vertices = [[0, 0, 0x3fffffff], [0, 0, 0x40000000], [0, 0, -1]];
    let context = Context {
        vertices: &vertices,
        position: [0, 0, 123],
        key_scale: 4,
        operand_scale: 0,
    };
    assert_eq!(Command::Close.resolve(&context).unwrap(), None);
    assert_eq!(Command::First.resolve(&context).unwrap(), Some(u32::MAX));
    assert_eq!(Command::Position.resolve(&context).unwrap(), Some(246));
    assert_eq!(Command::Vertex(2).resolve(&context).unwrap(), Some(1));
    assert_eq!(
        Command::Maximum(vec![0, 1, 2]).resolve(&context).unwrap(),
        Some(0x7ffffffe)
    );
    assert_eq!(
        Command::Minimum(vec![0, 1, 2]).resolve(&context).unwrap(),
        Some(0x82800000)
    );
}

#[test]
fn depth_offsets_wrap_addition_and_mask_signed_shift_counts() {
    let vertices = [[1, 2, i32::MAX]];
    let mut context = Context {
        vertices: &vertices,
        position: [0; 3],
        key_scale: 3,
        operand_scale: 32,
    };
    let shifted = Command::Offset {
        vertex: Some(0),
        amount: 1,
    };
    assert_eq!(shifted.resolve(&context).unwrap(), Some(1));
    let literal = Command::Offset {
        vertex: None,
        amount: 7,
    };
    assert_eq!(literal.resolve(&context).unwrap(), Some(7));
    context.operand_scale = -1;
    assert_eq!(literal.resolve(&context).unwrap(), Some(1));
    context.operand_scale = 2;
    assert_eq!(literal.resolve(&context).unwrap(), Some(28));
}

#[test]
fn depth_commands_reject_truncation_wrong_opcode_and_unresolved_vertices() {
    assert!(decode(&vec![0; 16385]).is_err());
    for bytes in [
        vec![],
        vec![6],
        vec![0, 0],
        vec![6, 0x20],
        vec![6, 0x60, 0, 255],
        vec![0xa6, 0xff],
    ] {
        assert!(decode(&bytes).is_err());
    }
    let context = Context {
        vertices: &[],
        position: [0; 3],
        key_scale: 0,
        operand_scale: 0,
    };
    for command in [
        Command::Vertex(-1),
        Command::Vertex(0),
        Command::Maximum(vec![]),
        Command::Minimum(vec![-1]),
        Command::Maximum(vec![0]),
        Command::Offset {
            vertex: Some(0),
            amount: 1,
        },
    ] {
        assert!(command.resolve(&context).is_err());
    }
    for end in 0..6 {
        let _ = decode(&[0x46, 0x20, 4, 255, 6, 0][..end]);
    }
}

#[test]
fn depth_key_preserves_behind_camera_wrapping_magnitudes_and_masked_shifts() {
    for z in [i32::MIN, -1, 0] {
        assert_eq!(key([i32::MIN, i32::MAX, z], i32::MIN), 1);
    }
    assert_eq!(key([0, 0, 80], 0), 10);
    assert_eq!(key([8, -8, 80], 3), 82);
    assert_eq!(key([i32::MIN, 0, 1], 3), (-268435455i32) as u32);
    assert_eq!(key([i32::MIN, i32::MIN, 80], 3), 80);
    assert_eq!(key([0, 0, 80], -29), 80);
    assert_eq!(key([0, 0, 80], 35), 80);
    assert_eq!(key([0, 0, 1], 4), 2);
}

#[test]
fn depth_key_overflow_switches_at_unsigned_threshold_including_equality() {
    assert_eq!(key([0, 0, 0x3fffffff], 4), 0x7ffffffe);
    assert_eq!(key([0, 0, 0x40000000], 4), 0x82800000);
    assert_eq!(key([0, 0, i32::MAX], 4), 0x82ffffff);
    assert_eq!(key([0, 0, 1], 34), 0xbe000000);
    assert_eq!(key([0, 0, 1], 35), 1);
}
