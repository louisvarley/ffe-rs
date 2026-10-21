use ffe_core::model::VertexContext;
use ffe_core::model_commands::{
    arithmetic, bit_branch, decode, material, visibility, visibility_branch, VisibilityContext,
};

#[test]
fn visibility_distinguishes_normal_flags_lod_thresholds_overflow_and_branch_inversion() {
    let normals = [
        ffe_core::model_normals::Normal {
            culled: false,
            colour: [0; 3],
        },
        ffe_core::model_normals::Normal {
            culled: true,
            colour: [0; 3],
        },
    ];
    let mut context = VisibilityContext {
        depth: 100,
        scale: 0,
        detail: [0, 0],
        normals: &normals,
    };
    assert!(visibility(31, &context).unwrap());
    assert!(!visibility(100, &context).unwrap());
    assert!(!visibility(i16::MIN, &context).unwrap());
    assert!(visibility(i16::MIN + 1, &context).unwrap());
    assert!(visibility(i16::MIN + 2, &context).is_err());
    assert_eq!(visibility_branch(11, 31, &context).unwrap(), None);
    assert_eq!(visibility_branch(12, 31, &context).unwrap(), Some(4));
    assert_eq!(visibility_branch(0x8b, 31, &context).unwrap(), Some(12));
    assert_eq!(visibility_branch(0xff8b, 31, &context).unwrap(), Some(-4));
    assert!(visibility_branch(19, 31, &context).is_err());
    context.scale = 31;
    assert!(!visibility(1, &context).unwrap());
    context.scale = 32;
    assert!(visibility(1, &context).unwrap());
    context.scale = 0;
    context.detail = [2, 999];
    assert!(!visibility(31, &context).unwrap());
    context.detail = [1, -2];
    assert!(visibility(31, &context).unwrap());
    context.scale = i32::MAX;
    context.detail = [3, 1];
    assert!(visibility(31, &context).unwrap());
    context.depth = -1;
    assert!(!visibility(0, &context).unwrap());
    assert!(visibility(i16::MIN + 1, &context).unwrap());
}

#[test]
fn faces_preserve_perimeter_material_reflection_and_triangle_normal_operand_position() {
    let bytes = [
        7, 0, 0x23, 0x41, 2, 4, 6, 8, 8, 0, 0x34, 0x12, 0, 2, 4, 6, 8, 0, 0, 0,
    ];
    let program = decode(&bytes, 10).unwrap();
    assert_eq!(
        program.commands.iter().map(|c| c.size).collect::<Vec<_>>(),
        [8, 10, 2]
    );
    assert_eq!(program.faces.len(), 4);
    assert_eq!(program.faces[0].vertices, [5, 3, 9]);
    assert_eq!(program.faces[0].normal, 7);
    assert!(program.faces[0].reflected);
    assert_eq!(program.faces[1].vertices, [4, 2, 8]);
    assert_eq!(program.faces[1].normal, 6);
    assert_eq!(program.faces[0].material, 0x4123);
    assert_eq!(program.faces[2].vertices, [5, 1, 7, 3]);
    assert_eq!(program.faces[2].normal, 9);
    assert_eq!(program.faces[3].vertices, [4, 0, 6, 2]);
    assert_eq!(program.faces[3].normal, 8);
}

#[test]
fn point_and_line_records_decode_material_angle_flags_and_vertices() {
    // point (opcode 1): material, packed angle, flag 0x80, vertex 3
    // line  (opcode 2): material, vertices [1, 4]
    let bytes = [
        1, 0, 0x23, 0x01, 0x00, 0x40, 0x80, 0x03, 2, 0, 0x56, 0x04, 0x01, 0x04, 0, 0,
    ];
    let program = decode(&bytes, 8).unwrap();
    assert_eq!(program.points.len(), 1);
    assert_eq!(program.points[0].material, 0x0123);
    assert_eq!(program.points[0].angle, 0x4000);
    assert!(program.points[0].flag);
    assert_eq!(program.points[0].vertex, 3);
    assert_eq!(program.lines.len(), 1);
    assert_eq!(program.lines[0].material, 0x0456);
    assert_eq!(program.lines[0].vertices, [1, 4]);

    let mut invalid = bytes;
    invalid[7] = 8;
    assert!(decode(&invalid, 8).is_err());
    let mut invalid = bytes;
    invalid[12] = 8;
    assert!(decode(&invalid, 8).is_err());
}

#[test]
fn physical_decoder_handles_early_returns_variable_blocks_and_validates_branches() {
    // Conditional jumps to the final return, crossing another physical return and a list block.
    let bytes = [0x93, 0, 1, 0, 0, 0, 6, 0x60, 2, 255, 4, 0, 0, 0];
    let program = decode(&bytes, 6).unwrap();
    assert_eq!(
        program
            .commands
            .iter()
            .map(|c| c.offset)
            .collect::<Vec<_>>(),
        [0, 4, 6, 12]
    );
    assert_eq!(program.commands[2].size, 6);
    let mut invalid = bytes;
    invalid[0] = 0x73; // Target is inside a list operand.
    assert!(decode(&invalid, 6).is_err());
    invalid[0] = 0xf3; // Target is beyond the payload.
    assert!(decode(&invalid, 6).is_err());
    for data in [
        vec![],
        vec![5, 0, 0, 0],
        vec![0xff, 0xff, 0, 0],
        vec![3, 0, 0, 0, 0, 1, 2, 9],
        vec![6, 0x60, 0, 255],
    ] {
        assert!(decode(&data, 6).is_err());
    }
    assert!(decode(&[7, 0, 0, 0, 0, 1, 2, 126, 0, 0], 6).is_err());
    for end in 0..bytes.len() {
        // A prefix ending at a real return is valid; it still cannot panic on any prefix.
        let _ = decode(&bytes[..end], 6);
    }
    assert!(decode(&vec![0; 16386], 6).is_err());
    for count in [0, 1, 129, u32::MAX] {
        assert!(decode(&[0, 0], count).is_err());
    }
}

#[test]
fn arithmetic_masks_results_wraps_and_leaves_context_unchanged_on_errors() {
    let mut context = VertexContext::default();
    context.locals[8] = i32::MAX;
    context.locals[9] = 1;
    assert_eq!(arithmetic(0x20d, [0xc8, 0xc9], &mut context).unwrap(), 0);
    assert_eq!(context.locals[2], 0);
    context.locals[8] = -7;
    context.locals[9] = 0;
    assert_eq!(arithmetic(0x3d, [0xc8, 0xc9], &mut context).unwrap(), 65529);
    context.locals[9] = 2;
    assert_eq!(arithmetic(0x3d, [0xc8, 0xc9], &mut context).unwrap(), 65533);
    context.locals[8] = i32::MIN;
    context.locals[9] = -1;
    let before = context.locals;
    assert!(arithmetic(0x3d, [0xc8, 0xc9], &mut context).is_err());
    assert_eq!(context.locals, before);
    for code in [0x8d, 0xad, 0xdd, 0xed, 0] {
        assert!(arithmetic(code, [0xc8, 0], &mut context).is_err());
        assert_eq!(context.locals, before);
    }
    context.locals[8] = 31;
    context.locals[9] = 32;
    context.globals[63] = 12345;
    assert_eq!(arithmetic(0xad, [0xc8, 0xc9], &mut context).unwrap(), 12345);
}

#[test]
fn bit_branches_distinguish_zero_mask_shift_wrapping_negative_offsets_and_returns() {
    let context = VertexContext::default();
    assert_eq!(bit_branch(19, [0, 0], &context).unwrap(), None);
    assert_eq!(bit_branch(20, [0, 0], &context).unwrap(), Some(4));
    assert_eq!(bit_branch(0x93, [0, 0], &context).unwrap(), Some(12));
    assert_eq!(bit_branch(0xffd3, [0, 0], &context).unwrap(), Some(0));
    assert_eq!(bit_branch(0x94, [1, 33], &context).unwrap(), Some(12));
    assert_eq!(bit_branch(0x94, [1, 2], &context).unwrap(), Some(4));
    assert!(bit_branch(13, [0, 0], &context).is_err());
    assert_eq!(material(0x3123).rgb, [1, 2, 3]);
    assert!(material(0x3123).unlit && material(0x3123).local_colour);
    assert_eq!(material(0x4123).rgb, [0, 0, 0]);
    assert_eq!(material(0x4123).texture, 0x123);
}

#[test]
fn declaration_data_after_the_final_terminator_is_not_decoded_as_commands() {
    let words = |w: &[u16]| -> Vec<u8> { w.iter().flat_map(|v| v.to_le_bytes()).collect() };
    // A line, the terminator, then two data words that are not a valid command.
    let program = decode(&words(&[2, 0x0456, 0x0201, 0, 0x07d4, 0x1f40]), 6).unwrap();
    assert_eq!(program.trailing, 4);
    assert_eq!(program.commands.len(), 2);
    assert_eq!(program.lines.len(), 1);
    // Data ending in zero words: the longest valid prefix still ends at a terminator.
    let program = decode(&words(&[0x070e, 0, 0, 0x07d4, 0x1f40]), 6).unwrap();
    assert_eq!(
        (program.commands.len(), program.trailing),
        (2, 4),
        "submodel + terminator"
    );
    // A branch into the data region makes every prefix invalid.
    let into_data = words(&[0x004b, 0x0000, 0, 0x07d4, 0x1f40]);
    assert!(decode(&into_data, 6).is_err());
    // Without any terminator nothing is guessed.
    assert!(decode(&words(&[2, 0x0456, 0x0201, 0x07d4]), 6).is_err());
}
