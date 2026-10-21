use ffe_core::model::VertexContext;
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{
    execute, execute_live, ColourTarget, Emission, InstanceContext, Primitive,
};
use ffe_core::model_instance::Vertices;
use ffe_core::model_math::{angle_operand, SineTable};
use ffe_core::model_normals::Normal;

fn bytes(words: &[u16]) -> Vec<u8> {
    words.iter().flat_map(|w| w.to_le_bytes()).collect()
}

const VISIBLE_NORMAL: Normal = Normal {
    culled: false,
    colour: [0; 3],
};
static VISIBLE: [Normal; 16] = [VISIBLE_NORMAL; 16];

fn visibility() -> VisibilityContext<'static> {
    VisibilityContext {
        depth: 100,
        scale: 0,
        detail: [0, 0],
        normals: &VISIBLE,
    }
}

#[test]
fn execution_follows_loops_records_repeated_emissions_and_commits_low_word_locals() {
    let stream = bytes(&[
        13, 0x00c8, 7, 0x0123, 0x0200, 0x0402, 0x1d, 0x01c0, 0xff14, 0x00c0, 0,
    ]);
    let mut context = VertexContext::default();
    context.locals[8] = 3;
    let result = execute(&stream, 8, &mut context, &visibility(), None, 32).unwrap();
    assert_eq!(context.locals[0], 0);
    assert_eq!(context.locals[8], 3);
    assert_eq!(result.commands, [0, 4, 12, 16, 4, 12, 16, 4, 12, 16, 20]);
    assert_eq!(result.emissions.len(), 6);
    assert_eq!(
        result
            .emissions
            .iter()
            .map(|e| e.context.locals[0])
            .collect::<Vec<_>>(),
        [3, 3, 2, 2, 1, 1]
    );
    assert_eq!(result.emissions[0].face.vertices, [3, 1, 5]);
    assert_eq!(result.emissions[1].face.vertices, [2, 0, 4]);
    assert_eq!(result.emissions[0].face.normal, 3);
    let mut short = VertexContext::default();
    short.locals[8] = 3;
    let before = short.clone();
    assert!(execute(&stream, 8, &mut short, &visibility(), None, 10)
        .unwrap_err()
        .contains("exceeded its 10-command budget"));
    assert_eq!(short, before);
    assert_eq!(
        execute(&stream, 8, &mut short, &visibility(), None, 11)
            .unwrap()
            .commands
            .len(),
        11
    );
}

#[test]
fn execution_is_transactional_for_budget_arithmetic_unsupported_alias_and_rotation_errors() {
    let mut context = VertexContext::default();
    context.locals[8] = 9;
    context.locals[9] = 0;
    let original = context.clone();
    for (words, budget, expected) in [
        (
            vec![13, 0x00c8, 0xffd4, 0x00c8, 0],
            8,
            "exceeded its 8-command budget",
        ),
        (
            vec![13, 0x00c8, 0x8d, 0xc9c8, 0],
            8,
            "unsigned division by zero",
        ),
        (
            vec![13, 0x00c8, 15, 0],
            8,
            "Unsupported command 0x000f at 0x4",
        ),
        (
            vec![13, 0x00c8, 13, 0x00d2, 0],
            8,
            "alias legacy instance fields",
        ),
        (
            vec![13, 0x00c8, 0x801c, 1, 0],
            8,
            "needs the original sine table",
        ),
        (vec![13, 0x00c8, 0], 1, "exceeded its 1-command budget"),
    ] {
        assert!(
            execute(&bytes(&words), 8, &mut context, &visibility(), None, budget)
                .unwrap_err()
                .contains(expected)
        );
        assert_eq!(context, original);
    }
    for budget in [0, 4097, usize::MAX] {
        assert!(execute(&bytes(&[0]), 8, &mut context, &visibility(), None, budget).is_err());
        assert_eq!(context, original);
    }
}

#[test]
fn execution_skips_unreached_unsupported_records_and_handles_conditional_returns() {
    let stream = bytes(&[0x93, 0, 1, 0, 0, 0, 0]);
    let mut context = VertexContext::default();
    let result = execute(&stream, 8, &mut context, &visibility(), None, 2).unwrap();
    assert_eq!(result.commands, [0, 12]);
    let normals = [Normal {
        culled: true,
        colour: [0; 3],
    }];
    let inputs = VisibilityContext {
        normals: &normals,
        ..visibility()
    };
    let result = execute(&bytes(&[11, 0x8000, 0]), 8, &mut context, &inputs, None, 1).unwrap();
    assert_eq!(result.commands, [0]);
    assert!(execute(&bytes(&[11, 0x8001, 0]), 8, &mut context, &inputs, None, 1).is_err());
    assert!(execute(&bytes(&[0x13, 0, 3, 0]), 8, &mut context, &inputs, None, 2).is_err());
}

#[test]
fn execution_captures_rotated_context_at_face_emission_without_changing_visibility_inputs() {
    let table = SineTable::from_bytes(
        &(0..16384i32)
            .flat_map(|i| (i * 65536).to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut context = VertexContext::default();
    context.locals[9] = 12345;
    let mut expected = context.clone();
    table.rotate(&mut expected.basis, 1, 12345).unwrap();
    let stream = bytes(&[0x803c, 0xc9, 4, 0x234, 0x0200, 0x0604, 0, 0]);
    let result = execute(&stream, 8, &mut context, &visibility(), Some(&table), 3).unwrap();
    assert_eq!(context, expected);
    assert_eq!(result.emissions[0].context, expected);
    assert_eq!(result.emissions[0].face.vertices, [4, 0, 6, 2]);
}

// Two literal vertex pairs. Under the nominal 0x7fffffff identity, z=8/80 resolve
// to object z=7/79, so the low/high depth keys are 0/9 (not 1/10).
const LIVE_RECORDS: [u8; 12] = [1, 0, 0, 0, 8, 0, 1, 0, 0, 0, 80, 0];

fn live_instance(position: [i32; 3], key_scale: i32, operand_scale: i32) -> InstanceContext {
    InstanceContext {
        position,
        generation: 2,
        key_scale,
        operand_scale,
        textures: true,
    }
}

/// Ordinary triangle record with emitted vertex order [a, b, c].
fn triangle(material: u16, vertices: [u8; 3]) -> [u16; 4] {
    let [a, b, c] = vertices;
    [
        3,
        material,
        u16::from_le_bytes([b, a]),
        u16::from_le_bytes([0, c]),
    ]
}

fn depth_vertex(index: u8) -> u16 {
    (2 << 13) | (u16::from(index) << 5) | 6
}

fn depth_list(mode: u16, header: u8, list: &[u8]) -> Vec<u16> {
    let mut words = vec![(mode << 13) | (u16::from(header) << 5) | 6];
    for (index, vertex) in list.iter().enumerate() {
        let continuation = if index + 1 == list.len() { 0 } else { 255 };
        words.push(u16::from(*vertex) | (continuation << 8));
    }
    words
}

fn depth_offset_vertex(index: u8, amount: i16) -> Vec<u16> {
    vec![(7 << 13) | (u16::from(index) << 5) | 6, amount as u16]
}

const DEPTH_CLOSE: u16 = 0x0006;
const DEPTH_POSITION: u16 = 0xffe6;
const DEPTH_FIRST: u16 = 0xffc6;

fn face_emission(primitive: &Primitive) -> &Emission {
    match primitive {
        Primitive::Face(emission) => emission,
        other => panic!("expected a face primitive, got {other:?}"),
    }
}

#[test]
fn live_execution_orders_grouped_faces_by_group_key() {
    let mut words = Vec::new();
    words.push(depth_vertex(0));
    words.extend_from_slice(&triangle(0x0123, [1, 0, 2]));
    words.push(depth_vertex(2));
    words.extend_from_slice(&triangle(0x0456, [3, 2, 0]));
    words.push(DEPTH_CLOSE);
    words.push(0);
    let stream = bytes(&words);
    let mut context = VertexContext::default();
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0; 3], 0, 0),
        32,
    )
    .unwrap();
    assert_eq!(result.emissions.len(), 2);
    assert_eq!(result.emissions[0].face.material, 0x0123);
    assert_eq!(result.ordered.len(), 2);
    // Group key 9 (v2) sorts before group key 0 (v0).
    assert_eq!(face_emission(&result.ordered[0]).face.material, 0x0456);
    assert_eq!(face_emission(&result.ordered[1]).face.material, 0x0123);
    assert_eq!(result.ordered_keys, [9, 0]);
}

#[test]
fn live_execution_orders_standalone_faces_by_first_view_vertex() {
    let mut words = Vec::new();
    words.extend_from_slice(&triangle(0x0123, [0, 1, 2])); // first vertex key 0
    words.extend_from_slice(&triangle(0x0456, [2, 3, 1])); // first vertex key 9
    words.push(0);
    let stream = bytes(&words);
    let mut context = VertexContext::default();
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0; 3], 0, 0),
        8,
    )
    .unwrap();
    assert_eq!(result.emissions.len(), 2);
    assert_eq!(face_emission(&result.ordered[0]).face.material, 0x0456);
    assert_eq!(face_emission(&result.ordered[1]).face.material, 0x0123);
    assert_eq!(result.ordered_keys, [9, 0]);
}

#[test]
fn live_execution_resolves_depth_lists_and_offsets() {
    let mut words = Vec::new();
    words.extend(depth_list(1, 0, &[2])); // max(key 0, key 9) = 9
    words.extend_from_slice(&triangle(1, [0, 1, 2]));
    words.extend(depth_list(3, 0, &[2])); // min = 0
    words.extend_from_slice(&triangle(2, [0, 1, 2]));
    words.extend(depth_offset_vertex(0, 8)); // z 7 + 8 = 15 -> key 1
    words.extend_from_slice(&triangle(3, [0, 1, 2]));
    words.push(DEPTH_CLOSE);
    words.push(0);
    let stream = bytes(&words);
    let mut context = VertexContext::default();
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0; 3], 0, 0),
        32,
    )
    .unwrap();
    let materials: Vec<_> = result
        .ordered
        .iter()
        .map(|primitive| face_emission(primitive).face.material)
        .collect();
    assert_eq!(materials, [1, 3, 2]);
    assert_eq!(result.ordered_keys, [9, 1, 0]);
}

#[test]
fn live_execution_handles_position_and_first_depth_forms() {
    let mut words = vec![DEPTH_POSITION];
    words.extend_from_slice(&triangle(1, [0, 1, 2]));
    words.push(DEPTH_FIRST);
    words.extend_from_slice(&triangle(2, [0, 1, 2]));
    words.push(DEPTH_CLOSE);
    words.push(0);
    let stream = bytes(&words);
    let mut context = VertexContext::default();
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0, 0, 32], 0, 0),
        8,
    )
    .unwrap();
    let materials: Vec<_> = result
        .ordered
        .iter()
        .map(|primitive| face_emission(primitive).face.material)
        .collect();
    assert_eq!(materials, [2, 1]);
    assert_eq!(result.ordered_keys, [u32::MAX, 4]);
}

#[test]
fn live_execution_drops_empty_replaced_groups_and_keeps_members() {
    let mut words = Vec::new();
    words.push(depth_vertex(0)); // empty group key 0
    words.push(depth_vertex(2)); // closes it, opens group key 9
    words.extend_from_slice(&triangle(0x0123, [0, 1, 2]));
    words.push(DEPTH_CLOSE);
    words.push(0);
    let stream = bytes(&words);
    let mut context = VertexContext::default();
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0; 3], 0, 0),
        8,
    )
    .unwrap();
    assert_eq!(result.ordered.len(), 1);
    assert_eq!(face_emission(&result.ordered[0]).face.material, 0x0123);
    assert_eq!(result.ordered_keys, [9]);
}

#[test]
fn live_execution_is_transactional_and_closes_a_group_left_open_at_return() {
    let mut context = VertexContext::default();
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let before_context = context.clone();
    let before_slots = vertices.slots();

    let out_of_range = bytes(&[depth_vertex(4), 0]);
    assert!(execute_live(
        &out_of_range,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0; 3], 0, 0),
        8,
    )
    .is_err());
    assert_eq!(context, before_context);
    assert_eq!(vertices.slots(), before_slots);

    let mut words = Vec::new();
    words.push(depth_vertex(0));
    words.extend_from_slice(&triangle(0x0123, [0, 1, 2]));
    words.push(0);
    let open_group = bytes(&words);
    // F1683/F1684 close the group after a top-level model returns.
    let mut closing = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &open_group,
        &mut closing,
        &mut context.clone(),
        &visibility(),
        None,
        live_instance([0; 3], 0, 0),
        8,
    )
    .unwrap();
    assert_eq!(result.ordered.len(), 1);
    assert_eq!(result.ordered_keys, [0]);

    assert!(execute_live(
        &open_group,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0; 3], 0, 0),
        2,
    )
    .is_err());
    assert_eq!(context, before_context);
    assert_eq!(vertices.slots(), before_slots);
}

#[test]
fn live_execution_commits_and_reuses_the_vertex_cache() {
    let stream = bytes(&[depth_vertex(0), DEPTH_CLOSE, 0]);
    let mut context = VertexContext::default();
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([10, 20, 30], 0, 0),
        8,
    )
    .unwrap();
    assert_eq!(vertices.slots()[0].view, [10, 20, 37]);
    assert_eq!(vertices.slots()[0].stamp, 1);
    execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([100, 200, 300], 0, 0),
        8,
    )
    .unwrap();
    assert_eq!(vertices.slots()[0].view, [100, 200, 307]);
    assert_eq!(vertices.slots()[0].object, [0, 0, 7]);
    assert_eq!(vertices.slots()[0].stamp, 1);
}

#[test]
fn legacy_execute_still_rejects_depth_commands_and_leaves_ordered_empty() {
    let stream = bytes(&[depth_vertex(0), 0]);
    let mut context = VertexContext::default();
    let error = execute(&stream, 4, &mut context, &visibility(), None, 8).unwrap_err();
    assert_eq!(error, "Unsupported executed command 0x4006 at 0x0.");
    let result = execute(&bytes(&[0]), 4, &mut context, &visibility(), None, 8).unwrap();
    assert!(result.ordered.is_empty());
    assert!(result.ordered_keys.is_empty());
}

#[test]
fn live_execution_emits_points_and_lines_in_command_and_render_order() {
    let words = vec![1, 0x0123, 0x4000, 0x0080, 2, 0x0456, 0x0201, 0];
    let stream = bytes(&words);
    let mut context = VertexContext::default();
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0; 3], 0, 0),
        8,
    )
    .unwrap();
    assert_eq!(result.points.len(), 1);
    assert_eq!(result.points[0].material, 0x0123);
    assert_eq!(result.points[0].encoded_angle, 0x4000);
    assert_eq!(
        result.points[0].angle,
        angle_operand(0x4000, &VertexContext::default())
    );
    assert!(result.points[0].flag);
    assert_eq!(result.points[0].vertex, 0);
    assert_eq!(result.lines.len(), 1);
    assert_eq!(result.lines[0].material, 0x0456);
    assert_eq!(result.lines[0].vertices, [1, 2]);
    assert!(result.emissions.is_empty());
    // Point vertex 0 and line first vertex 1 both resolve to key 0; the earlier
    // point allocation wins the tie.
    assert!(matches!(result.ordered[0], Primitive::Point(_)));
    assert!(matches!(result.ordered[1], Primitive::Line(_)));
    assert_eq!(result.ordered_keys, [0, 0]);
}

#[test]
fn live_execution_orders_points_and_lines_with_depth_groups() {
    let words = vec![
        0x4046, // depth vertex 2 -> group key 9
        2, 0x0456, 0x0201, // line in the group
        0x0006, // close
        1, 0x0123, 0x4000, 0x0080, // standalone point, vertex 0 -> key 0
        0,
    ];
    let stream = bytes(&words);
    let mut context = VertexContext::default();
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0; 3], 0, 0),
        8,
    )
    .unwrap();
    assert!(matches!(result.ordered[0], Primitive::Line(_)));
    assert!(matches!(result.ordered[1], Primitive::Point(_)));
    assert_eq!(result.ordered_keys, [9, 0]);
}

fn normals(culled: &[usize]) -> Vec<Normal> {
    (0..4)
        .map(|index| Normal {
            culled: culled.contains(&index),
            colour: [0; 3],
        })
        .collect()
}

#[test]
fn execution_emits_text_operands_and_the_normal_skip_in_command_order() {
    let stream = bytes(&[10, 0x0123, 0x5a02, 0x8104, 0x1234, 2, 0x0456, 0x0201, 0]);
    for (culled, expected) in [(vec![], false), (vec![2], true)] {
        let normals = normals(&culled);
        let inputs = VisibilityContext {
            normals: &normals,
            ..visibility()
        };
        let mut context = VertexContext::default();
        let result = execute(&stream, 8, &mut context, &inputs, None, 8).unwrap();
        assert_eq!(result.commands, [0, 10, 16]);
        assert_eq!(result.texts.len(), 1);
        let text = &result.texts[0];
        assert_eq!(text.culled, expected);
        assert_eq!(
            (
                text.text.material,
                text.text.normal,
                text.text.size,
                text.text.glyphs
            ),
            (0x0123, 2, 0xa, 5)
        );
        assert_eq!(
            (text.text.vertex, text.text.frame, text.text.string),
            (4, 0x8104, 0x1234)
        );
        assert_eq!(result.lines[0].vertices, [1, 2]);
    }
}

#[test]
fn execution_emits_submodel_calls_with_original_extra_vertex_order() {
    let plain = bytes(&[0x1c6e, 0x0305, 2, 0x0456, 0x0201, 0]);
    let mut context = VertexContext::default();
    let result = execute(&plain, 8, &mut context, &visibility(), None, 8).unwrap();
    assert_eq!(result.commands, [0, 4, 10]);
    let call = &result.submodels[0].submodel;
    assert_eq!((call.model, call.vertex, call.frame), (227, 5, 0x0305));
    assert_eq!(call.extra, None);
    assert!(call.extra_vertices().is_empty());

    for (extra, order) in [(0x0201u16, vec![3u8, 1, 2, 4]), (0x7f01, vec![3, 1])] {
        let stream = bytes(&[0x002e, 0x8306, extra, 0x0403, 2, 0x0456, 0x0201, 0]);
        let result = execute(&stream, 8, &mut context, &visibility(), None, 8).unwrap();
        assert_eq!(result.commands, [0, 8, 14]);
        let call = &result.submodels[0].submodel;
        assert_eq!((call.model, call.vertex, call.frame), (1, 6, 0x8306));
        assert_eq!(call.extra_vertices(), order);
        assert_eq!(result.lines[0].vertices, [1, 2]);
    }
}

#[test]
fn live_text_and_submodels_stay_out_of_render_order_and_errors_are_transactional() {
    let stream = bytes(&[10, 0x0123, 0x5a02, 0x8104, 0x1234, 0x002e, 0x0001, 0]);
    let records: Vec<u8> = (0..4).flat_map(|_| [1, 0, 0, 0, 80, 0]).collect();
    let mut vertices = Vertices::new(&records, 8).unwrap();
    let normals = normals(&[]);
    let inputs = VisibilityContext {
        normals: &normals,
        ..visibility()
    };
    let mut context = VertexContext::default();
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &inputs,
        None,
        live_instance([0; 3], 0, 0),
        8,
    )
    .unwrap();
    assert_eq!((result.texts.len(), result.submodels.len()), (1, 1));
    assert!(result.ordered.is_empty());
    assert_eq!(vertices.slots()[1].stamp, 2, "submodel vertex is projected");

    let before = context.clone();
    let mut fresh = Vertices::new(&records, 8).unwrap();
    let error = execute_live(
        &stream,
        &mut fresh,
        &mut context,
        &VisibilityContext {
            normals: &[],
            ..visibility()
        },
        None,
        live_instance([0; 3], 0, 0),
        8,
    )
    .unwrap_err();
    assert!(error.contains("out-of-range normal"), "{error}");
    assert_eq!(context, before);
    assert_eq!(fresh.slots(), Vertices::new(&records, 8).unwrap().slots());

    let external = bytes(&[0x002e, 0x8306, 0x0201, 0x0409, 0]);
    assert!(execute(&external, 8, &mut context, &visibility(), None, 8)
        .unwrap_err()
        .contains("Submodel references external/out-of-range vertex"));
}

#[test]
fn colour_commands_update_instance_or_normal_zero_colour_and_select_alternatives() {
    let lighting: Vec<Normal> = (0..4u8)
        .map(|i| Normal {
            culled: false,
            colour: [i * 10, 1, 255],
        })
        .collect();
    let inputs = VisibilityContext {
        normals: &lighting,
        ..visibility()
    };
    // Lit nibble colour plus normal 2, wrapping per byte; then local + instance colour.
    let stream = bytes(&[26, 0x0123, 0x0002, 26, 0x3010, 0x0001, 0]);
    let mut context = VertexContext::default();
    let result = execute(&stream, 8, &mut context, &inputs, None, 8).unwrap();
    assert_eq!(result.commands, [0, 6, 12]);
    assert_eq!(result.colours[0].rgb, [21, 3, 2]);
    assert_eq!(
        result.colours[1].rgb,
        [21, 4, 2],
        "unlit: no lighting added"
    );
    assert_eq!(context.colour, [21, 4, 2]);

    // A selector makes the record 20 bytes; local 8 & 7 picks the alternative word.
    let mut stream = vec![26, 0x2111, 0xc800];
    stream.extend((2..=8).map(|k| 0x2000 | (k * 0x111)));
    stream.extend([2, 0x0456, 0x0201, 0]);
    for (local, rgb) in [(0, [1, 1, 1]), (3, [4, 4, 4]), (15, [8, 8, 8])] {
        let mut context = VertexContext::default();
        context.locals[8] = local;
        let result = execute(&bytes(&stream), 8, &mut context, &inputs, None, 8).unwrap();
        assert_eq!(result.commands, [0, 20, 26]);
        assert_eq!(context.colour, rgb, "local {local}");
    }

    // Bit 7 writes normal 0, which then lights a later command through normal 0.
    let stream = bytes(&[26, 0x2123, 0x0080, 26, 0x0000, 0x0000, 0]);
    let mut context = VertexContext::default();
    let result = execute(&stream, 8, &mut context, &inputs, None, 8).unwrap();
    assert_eq!(result.colours[0].target, ColourTarget::NormalZero);
    assert_eq!(result.normal_zero_colour, Some([1, 2, 3]));
    assert_eq!(context.colour, [1, 2, 3]);
}

#[test]
fn colour_errors_are_transactional() {
    let lighting = normals(&[]);
    let inputs = VisibilityContext {
        normals: &lighting,
        ..visibility()
    };
    let mut context = VertexContext {
        colour: [7, 8, 9],
        ..Default::default()
    };
    let before = context.clone();
    for (words, expected) in [
        (
            vec![26, 0x0010, 0x0000, 26, 0x4001, 0x0080, 0],
            "texture colour table",
        ),
        (
            vec![26, 0x0010, 0x0000, 26, 0x0010, 0x0009, 0],
            "out-of-range normal",
        ),
        (
            vec![26, 0x0010, 0xd200, 0, 0, 0, 0, 0, 0, 0, 0],
            "alias legacy instance",
        ),
    ] {
        let error = execute(&bytes(&words), 8, &mut context, &inputs, None, 8).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(context, before);
    }
}

#[test]
fn live_faces_are_queued_by_the_original_cull_guard_and_outcode_rules() {
    // Triangles on vertices 0, 1, 2: culled normal 1, flat material, textured material.
    let face = |material: u16, normal: u8| [3, material, 0x0001, u16::from_le_bytes([normal, 2])];
    let mut words = Vec::new();
    words.extend_from_slice(&face(0x0123, 1));
    words.extend_from_slice(&face(0x0123, 0));
    words.extend_from_slice(&face(0x4001, 0));
    words.push(0);
    let stream = bytes(&words);
    let mut normals = vec![VISIBLE_NORMAL; 2];
    normals[1].culled = true;
    let inputs = VisibilityContext {
        normals: &normals,
        ..visibility()
    };
    // (x offset, expected queued materials, expected screen x range of all points)
    for (x, queued, screens) in [
        (0, vec![0x0123, 0x4001], 0..320),
        // Off the left edge but inside the guard: only the textured face survives.
        (-300, vec![0x4001], -2000..0),
        // Beyond the guard: the textured face takes the flat path and is rejected too.
        (-1000, vec![], -100_000..0),
    ] {
        let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
        let result = execute_live(
            &stream,
            &mut vertices,
            &mut VertexContext::default(),
            &inputs,
            None,
            live_instance([x, 0, 100], 0, 0),
            8,
        )
        .unwrap();
        assert_eq!(result.emissions.len(), 3, "all faces are still emitted");
        let slots = vertices.slots();
        assert!(
            (0..3).all(|i| screens.contains(&i32::from(slots[i].screen[0]))),
            "precondition for x {x}: {:?}",
            &slots[..3]
        );
        let materials: Vec<u16> = result
            .ordered
            .iter()
            .map(|p| face_emission(p).face.material)
            .collect();
        assert_eq!(materials, queued, "x offset {x}");
        assert_eq!(result.queue.len(), queued.len());
    }
    // Every point behind the near plane is rejected whatever the material.
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut VertexContext::default(),
        &inputs,
        None,
        live_instance([0, 0, -500], 0, 0),
        8,
    )
    .unwrap();
    assert!(result.ordered.is_empty());
    assert!(result.queue.is_empty());
}

#[test]
fn live_polygons_skip_by_their_length_and_queue_by_their_first_segment_vertex() {
    // Polygon: material 0x0123, normal 1 (bit 7 flag set), L = 4 segment bytes whose
    // first byte (vertex 2, high bit masked) keys it; then a line on vertices 1/2.
    let words = [5, 0x0123, 0x0481, 0x0082, 0x0000, 2, 0x0456, 0x0201, 0];
    let stream = bytes(&words);
    for (culled, queued) in [(false, true), (true, false)] {
        let mut normals = vec![VISIBLE_NORMAL; 2];
        normals[1].culled = culled;
        let inputs = VisibilityContext {
            normals: &normals,
            ..visibility()
        };
        let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
        let result = execute_live(
            &stream,
            &mut vertices,
            &mut VertexContext::default(),
            &inputs,
            None,
            live_instance([0, 0, 100], 0, 0),
            8,
        )
        .unwrap();
        assert_eq!(result.commands, [0, 10, 16], "6 + L bytes, culled or not");
        let polygon = &result.polygons[0].polygon;
        assert_eq!(
            (
                polygon.material,
                polygon.normal,
                polygon.flag,
                polygon.vertex
            ),
            (0x0123, 1, true, 2)
        );
        assert_eq!(polygon.segments, [0x82, 0x00, 0x00, 0x00]);
        assert_eq!(result.polygons[0].key.is_some(), queued);
        assert_eq!(
            result
                .ordered
                .iter()
                .filter(|p| matches!(p, Primitive::Polygon(_)))
                .count(),
            usize::from(queued)
        );
        assert_eq!(result.lines[0].vertices, [1, 2]);
    }
    // The key vertex must be a model vertex.
    let external = bytes(&[5, 0x0123, 0x0201, 0x0009, 0x0000, 0]);
    assert!(execute(
        &external,
        4,
        &mut VertexContext::default(),
        &visibility(),
        None,
        8
    )
    .unwrap_err()
    .contains("Polygon references external/out-of-range vertex"));
}

#[test]
fn thrusters_are_eight_byte_operand_events_outside_render_order() {
    // Thruster: material 0x0123, vertices 2/3, width -5; then a line on 1/2.
    let words = [9, 0x0123, 0x0302, 0xfffb, 2, 0x0456, 0x0201, 0];
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &bytes(&words),
        &mut vertices,
        &mut VertexContext::default(),
        &visibility(),
        None,
        live_instance([0, 0, 1000], 0, 0),
        8,
    )
    .unwrap();
    assert_eq!(result.commands, [0, 8, 14]);
    let thruster = &result.thrusters[0].thruster;
    assert_eq!(
        (thruster.material, thruster.vertices, thruster.width),
        (0x0123, [2, 3], -5)
    );
    assert!(result
        .ordered
        .iter()
        .all(|p| matches!(p, Primitive::Line(_))));
    // Both ends are projected when the first is deeper than 0x200.
    assert!(vertices.slots()[2].stamp == 2 && vertices.slots()[3].stamp == 2);
    let external = bytes(&[9, 0x0123, 0x0902, 0, 0]);
    assert!(execute(
        &external,
        4,
        &mut VertexContext::default(),
        &visibility(),
        None,
        8
    )
    .unwrap_err()
    .contains("Thruster references external/out-of-range vertex"));
}

#[test]
fn coloured_child_calls_carry_material_scale_and_offset_extras() {
    use ffe_core::model_commands::ChildScale;
    // Model 228 at vertex 2: frame 0x0402, byte 3 = 0xd0 (literal -3), material 0x0123.
    let plain = bytes(&[0x1c9b, 0x0402, 0xd000, 0x0123, 2, 0x0456, 0x0201, 0]);
    let mut context = VertexContext::default();
    let result = execute(&plain, 8, &mut context, &visibility(), None, 8).unwrap();
    assert_eq!(result.commands, [0, 8, 14]);
    let call = &result.submodels[0].submodel;
    assert_eq!(
        (
            call.model,
            call.vertex,
            call.frame,
            call.material,
            call.scale
        ),
        (228, 2, 0x0402, Some(0x0123), ChildScale::Literal(-3))
    );
    assert_eq!(call.extra, None);
    // Byte 3 bit 3 selects local 5; byte 1 bit 7 adds four extras after the material.
    let extended = bytes(&[
        0x1c9b, 0x8402, 0x5800, 0x0123, 0x0201, 0x0403, 2, 0x0456, 0x0201, 0,
    ]);
    let result = execute(&extended, 8, &mut context, &visibility(), None, 8).unwrap();
    assert_eq!(result.commands, [0, 12, 18]);
    let call = &result.submodels[0].submodel;
    assert_eq!(call.scale, ChildScale::Local(5));
    assert_eq!(call.extra_vertices(), [3, 1, 2, 4]);
    // Opcode 14 inherits the instance colour and adds no scale.
    let inherited = bytes(&[0x1c6e, 0x0305, 0]);
    let result = execute(&inherited, 8, &mut context, &visibility(), None, 8).unwrap();
    let call = &result.submodels[0].submodel;
    assert_eq!((call.material, call.scale), (None, ChildScale::Literal(0)));
}

#[test]
fn surfaces_span_their_entry_list_and_terminator() {
    // Surface: word 7, origin 0, axes 1/2/3, parameters 4/5/6, two 16-byte entries,
    // zero terminator; then a line on 1/2.
    let mut words = vec![31, 7, 0x0100, 0x0302, 4, 5, 6];
    words.extend([1u16; 8]);
    words.extend([2u16; 8]);
    words.extend([0, 2, 0x0456, 0x0201, 0]);
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &bytes(&words),
        &mut vertices,
        &mut VertexContext::default(),
        &visibility(),
        None,
        live_instance([0, 0, 100], 0, 0),
        8,
    )
    .unwrap();
    assert_eq!(result.commands, [0, 48, 54], "2 + 12 + 2 * 16 + 2 bytes");
    let surface = &result.surfaces[0].surface;
    assert_eq!(
        (
            surface.word,
            surface.origin,
            surface.axes,
            surface.parameters
        ),
        (7, 0, [1, 2, 3], [4, 5, 6])
    );
    assert_eq!(surface.entries.len(), 2);
    assert_eq!(
        surface.entries[1],
        [2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0, 2, 0]
    );
    assert!(result
        .ordered
        .iter()
        .all(|p| matches!(p, Primitive::Line(_))));
    // An unterminated entry list is a truncated command, never a guessed length.
    let mut open = vec![31, 7, 0x0100, 0x0302, 4, 5, 6];
    open.extend([1u16; 8]);
    assert!(execute(
        &bytes(&open),
        4,
        &mut VertexContext::default(),
        &visibility(),
        None,
        8
    )
    .unwrap_err()
    .contains("Truncated"));
}

#[test]
fn live_nested_blocks_key_by_vertex_depth_and_must_close() {
    // Block on vertex 2, a triangle inside, block end; then a standalone triangle.
    let mut words = vec![21 | (2 << 5)];
    words.extend_from_slice(&triangle(0x0123, [0, 1, 2]));
    words.push(0x8015);
    words.extend_from_slice(&triangle(0x0456, [2, 3, 0]));
    words.push(0);
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let result = execute_live(
        &bytes(&words),
        &mut vertices,
        &mut VertexContext::default(),
        &visibility(),
        None,
        live_instance([0, 0, 100], 0, 0),
        16,
    )
    .unwrap();
    assert_eq!(result.blocks.len(), 2);
    assert!(result.blocks[0].key.is_some() && result.blocks[1].key.is_none());
    assert_eq!(result.ordered.len(), 2);
    // An unterminated block cannot be finished, and nothing is committed.
    let mut open = vec![21 | (2 << 5)];
    open.extend_from_slice(&triangle(0x0123, [0, 1, 2]));
    open.push(0);
    let mut context = VertexContext::default();
    let before = context.clone();
    assert!(execute_live(
        &bytes(&open),
        &mut Vertices::new(&LIVE_RECORDS, 4).unwrap(),
        &mut context,
        &visibility(),
        None,
        live_instance([0, 0, 100], 0, 0),
        16,
    )
    .unwrap_err()
    .contains("Nested depth block is still open"));
    assert_eq!(context, before);
}

#[test]
fn beams_chains_and_direction_alignment() {
    // Beam (17): 14 bytes. Chain (24): material, size, pairs through the first
    // pair holding 0x7f. Then a line on 1/2.
    let words = [
        17, 0x0123, 0x0201, 1, 2, 3, 4, // beam
        24, 0x0456, 0x0010, 0x0102, 0x7f03, // chain: pairs [2,1], [3,0x7f]
        2, 0x0456, 0x0201, 0,
    ];
    let result = execute(
        &bytes(&words),
        8,
        &mut VertexContext::default(),
        &visibility(),
        None,
        8,
    )
    .unwrap();
    assert_eq!(result.commands, [0, 14, 24, 30]);
    let program = ffe_core::model_commands::decode(&bytes(&words), 8).unwrap();
    assert_eq!(program.beams[0].vertices, [1, 2]);
    assert_eq!(program.beams[0].parameters, [1, 2, 3, 4]);
    assert_eq!(program.chains[0].pairs, [[2, 1], [3, 0x7f]]);

    // Opcode 25 with bit 15 resets the basis; a zero direction pair rotates nothing.
    let mut context = VertexContext {
        basis: [1; 9],
        ..Default::default()
    };
    execute(
        &bytes(&[0x8019, 0x0019, 0]),
        8,
        &mut context,
        &visibility(),
        None,
        8,
    )
    .unwrap();
    assert_eq!(context.basis, ffe_core::model::IDENTITY);
    // A non-zero pair needs the original square-root table; axis 3 is undefined.
    let mut aimed = VertexContext {
        direction: [0, 3, 4],
        ..Default::default()
    };
    let before = aimed.clone();
    for code in [0x0019, 0x0079] {
        assert!(execute(&bytes(&[code, 0]), 8, &mut aimed, &visibility(), None, 8).is_err());
        assert_eq!(aimed, before);
    }
}

#[test]
fn opcode_30_restores_the_vertex_frame_from_the_basis() {
    // Bit 15: F1768 copies the basis (+0x9c) over the vertex frame (+0x14).
    let mut context = VertexContext {
        matrix: [7; 9],
        basis: [3; 9],
        ..Default::default()
    };
    execute(
        &bytes(&[0xc01e, 0]),
        8,
        &mut context,
        &visibility(),
        None,
        8,
    )
    .unwrap();
    assert_eq!(context.matrix, [3; 9]);
    assert_eq!(context.basis, [3; 9]);
    // Light points (0x801e) with a zero count (global word 4) change nothing.
    let before = context.clone();
    execute(
        &bytes(&[0x801e, 0]),
        8,
        &mut context,
        &visibility(),
        None,
        8,
    )
    .unwrap();
    assert_eq!(context, before);
    // A non-zero count needs the point tables; the vertex-base offset is not ported.
    context.globals[4] = 1;
    let before = context.clone();
    for code in [0x801e, 0x003e] {
        assert!(execute(&bytes(&[code, 0]), 8, &mut context, &visibility(), None, 8).is_err());
        assert_eq!(context, before);
    }
}

#[test]
fn opcode_22_queues_a_curve_keyed_by_its_third_vertex_byte() {
    // Material 0x0123, vertex bytes +2..+5 = 0, 1, 2, 3, normal 0.
    let stream = bytes(&[22, 0x0123, 0x0100, 0x0302, 0x0000, 0]);
    let program = ffe_core::model_commands::decode(&stream, 4).unwrap();
    assert_eq!(program.splines[0].vertices, [0, 1, 2, 3]);
    assert_eq!(program.commands[0].size, 10);

    let run = |z: i32| {
        let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
        let result = execute_live(
            &stream,
            &mut vertices,
            &mut VertexContext::default(),
            &visibility(),
            None,
            live_instance([0, 0, z], 0, 0),
            8,
        )
        .unwrap();
        (result, vertices)
    };
    let (result, mut vertices) = run(100);
    let instance = live_instance([0, 0, 100], 0, 0);
    let resolved: Vec<_> = (0..4)
        .map(|i| {
            vertices
                .resolve(i, &VertexContext::default(), instance.position, 2, true)
                .unwrap()
        })
        .collect();
    let spline = &result.splines[0];
    assert_eq!(
        spline.points,
        Some([2, 0, 3, 1].map(|i: usize| resolved[i].screen))
    );
    let key = ffe_core::model_depth::key(resolved[2].view, 0);
    assert_eq!(spline.key, Some(key));
    assert_eq!(spline.colour, [1, 2, 3]);
    assert_eq!(result.ordered_keys, [key]);
    assert!(matches!(result.ordered[0], Primitive::Spline(_)));

    // Vertices 0 and 1 are nearer than z = 0x40 without the offset: nothing queued.
    let (near, _) = run(0);
    assert_eq!(near.splines[0].key, None);
    assert!(near.ordered.is_empty());
}

#[test]
fn opcode_28_0xc000_builds_the_basis_from_three_object_vertices() {
    // Vertices 0, 2, 1 as A, B, C.
    let stream = bytes(&[0xc01c, 0x0200, 0x0001, 0]);
    let program = ffe_core::model_commands::decode(&stream, 4).unwrap();
    assert_eq!(program.commands[0].size, 6);
    let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
    let mut context = VertexContext::default();
    execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility(),
        None,
        live_instance([0, 0, 100], 0, 0),
        8,
    )
    .unwrap();
    let mut object = |i| vertices.object(i, &VertexContext::default()).unwrap();
    let [a, b, c] = [object(0), object(2), object(1)];
    assert_eq!(context.basis, ffe_core::model_math::vertex_basis(a, b, c));
    assert_ne!(context.basis, ffe_core::model::IDENTITY);
    // A vertex beyond the model is rejected when decoding.
    assert!(ffe_core::model_commands::decode(&bytes(&[0xc01c, 0x0204, 0x0001, 0]), 4).is_err());
}

#[test]
fn opcode_18_queues_a_label_only_inside_the_screen_window() {
    // Vertex 2 (code >> 5), string 0x1234.
    let stream = bytes(&[(2 << 5) | 18, 0x1234, 0]);
    let program = ffe_core::model_commands::decode(&stream, 4).unwrap();
    assert_eq!(program.labels[0].string, 0x1234);
    let run = |position: [i32; 3]| {
        let mut vertices = Vertices::new(&LIVE_RECORDS, 4).unwrap();
        execute_live(
            &stream,
            &mut vertices,
            &mut VertexContext::default(),
            &visibility(),
            None,
            live_instance(position, 0, 0),
            8,
        )
        .unwrap()
    };
    let visible = run([0, 0, 100]);
    let label = &visible.labels[0];
    let [x, y] = label.screen.map(i32::from);
    assert!((2..=317).contains(&x) && (4..=149).contains(&y));
    assert_eq!(visible.ordered_keys, [label.key.unwrap()]);
    assert!(matches!(visible.ordered[0], Primitive::Label(_)));
    // Far to the side, the point is projected but not queued.
    let aside = run([100_000, 0, 100]);
    assert_eq!(aside.labels[0].key, None);
    assert!(aside.ordered.is_empty());
    // A vertex beyond the model is rejected.
    assert!(ffe_core::model_commands::decode(&bytes(&[(4 << 5) | 18, 0, 0]), 4).is_err());
}
