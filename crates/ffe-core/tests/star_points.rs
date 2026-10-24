use ffe_core::model::VertexContext;
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{execute_live, InstanceContext, Primitive};
use ffe_core::model_instance::Vertices;
use ffe_core::star_points::{StarField, StarTables, TABLE_BYTES, TYPES};

/// Tables whose entry `t` has material `0x100 + t`, size `(t + 1) << 7`, label limit
/// `1000 * (t + 1)` and stalk `0x200 + t`.
fn tables() -> StarTables {
    let mut bytes = Vec::with_capacity(TABLE_BYTES);
    for table in 0..4u32 {
        for t in 0..TYPES as u32 {
            let value = match table {
                0 => 0x100 + t,
                1 => (t + 1) << 7,
                2 => 1000 * (t + 1),
                _ => 0x200 + t,
            };
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    StarTables::new(&bytes).unwrap()
}

/// F841's record for a star: opcode 9, a zero, then three signed coordinate bytes.
fn record(x: u8, y: u8, z: u8) -> [u8; 6] {
    [9, 0, x, y, z, 0]
}

fn run(
    field: &StarField,
    records: &[u8],
    position: [i32; 3],
) -> Result<ffe_core::model_execute::Execution, String> {
    let tables = tables();
    let mut vertices = Vertices::new(records, 2 * field.types.len() as u32)
        .unwrap()
        .with_stars(&tables, field);
    let mut context = VertexContext::default();
    context.globals[1] = if field.stalks { 0x20 } else { 0 };
    context.globals[2] = field.code as u16;
    context.globals[3] = (field.code >> 16) as u16;
    context.globals[4] = field.types.len() as u16;
    let visibility = VisibilityContext {
        depth: 1,
        scale: 0,
        detail: [0, 0],
        normals: &[],
    };
    execute_live(
        &[0x1e, 0x80, 0, 0],
        &mut vertices,
        &mut context,
        &visibility,
        None,
        InstanceContext {
            position,
            generation: 2,
            key_scale: 0,
            operand_scale: 0,
            textures: false,
        },
        8,
    )
}

#[test]
fn the_tables_are_validated() {
    assert!(StarTables::new(&[0; TABLE_BYTES - 4]).is_err());
    let mut bad = vec![0u8; TABLE_BYTES];
    bad[2] = 1; // material 0x10000
    assert!(StarTables::new(&bad).is_err());
    assert_eq!(StarTables::index(5), Some(5));
    assert_eq!(StarTables::index(0x85), Some(5));
    assert_eq!(StarTables::index(14), None);
}

#[test]
fn each_star_gets_a_point_a_stalk_and_a_label_with_its_number() {
    let field = StarField {
        types: vec![1, 3, 2],
        stalks: true,
        code: 0x0123_4567,
    };
    let mut records = Vec::new();
    for k in 0..3u8 {
        records.extend_from_slice(&record(k, 2 * k, 10));
    }
    let result = run(&field, &records, [0, 0, 600]).unwrap();
    assert_eq!(result.points.len(), 3);
    assert_eq!(result.lines.len(), 3);
    for (star, (point, line)) in result.points.iter().zip(&result.lines).enumerate() {
        let kind = usize::from(field.types[star]);
        assert_eq!(u32::from(point.material), 0x100 + kind as u32);
        assert_eq!(point.encoded_angle, (kind as i16 + 1));
        assert_eq!(point.vertex, 2 * star as u8);
        assert_eq!(u32::from(line.material), 0x200 + kind as u32);
        assert_eq!(line.vertices, [2 * star as u8, 2 * star as u8 + 1]);
    }
    // Vertex slot 0 is within every type's limit at this depth, so every star is labelled
    // with its number in bits 26..31 of the sector code (bits 24 and 25 are kept).
    assert_eq!(result.system_labels.len(), 3);
    for (star, label) in result.system_labels.iter().enumerate() {
        assert_eq!(label.selector, 0x8400);
        assert_eq!(
            label.code,
            (star as u32) << 26 | (0x0123_4567 & 0x03ff_ffff)
        );
        assert_eq!(label.vertex, 2 * star as u8);
    }
    // The queue holds the three points, three lines and the labels that fall on the view.
    let labelled = result
        .ordered
        .iter()
        .filter(|p| matches!(p, Primitive::SystemLabel(_)))
        .count();
    assert_eq!(
        labelled,
        result
            .system_labels
            .iter()
            .filter(|l| l.key.is_some())
            .count()
    );
}

#[test]
fn stalks_are_drawn_only_when_global_word_one_says_so() {
    let field = StarField {
        types: vec![0],
        stalks: false,
        code: 0,
    };
    let result = run(&field, &record(1, 2, 3), [0, 0, 600]).unwrap();
    assert_eq!((result.points.len(), result.lines.len()), (1, 0));
}

#[test]
fn a_label_needs_vertex_zero_inside_the_type_limit() {
    // Type 0's limit is 1000: at depth 5000 the first star's slot is beyond it, so no
    // star is labelled, whatever its own type allows.
    let field = StarField {
        types: vec![0, 9],
        stalks: false,
        code: 0,
    };
    let mut records = Vec::new();
    records.extend_from_slice(&record(1, 2, 3));
    records.extend_from_slice(&record(4, 5, 6));
    let far = run(&field, &records, [0, 0, 5000]).unwrap();
    assert_eq!(
        far.system_labels.len(),
        1,
        "only star 9 (limit 10000) passes"
    );
    assert_eq!(far.system_labels[0].code >> 26, 1);
    let near = run(&field, &records, [0, 0, 500]).unwrap();
    assert_eq!(near.system_labels.len(), 2);
}

#[test]
fn bad_star_fields_fail_rather_than_guess() {
    let field = StarField {
        types: vec![14],
        stalks: false,
        code: 0,
    };
    assert!(run(&field, &record(1, 2, 3), [0, 0, 600]).is_err());
    // Without a star list the program cannot run its light points.
    let binding = record(1, 2, 3);
    let mut vertices = Vertices::new(&binding, 2).unwrap();
    let mut context = VertexContext::default();
    context.globals[4] = 1;
    let visibility = VisibilityContext {
        depth: 1,
        scale: 0,
        detail: [0, 0],
        normals: &[],
    };
    let error = execute_live(
        &[0x1e, 0x80, 0, 0],
        &mut vertices,
        &mut context,
        &visibility,
        None,
        InstanceContext {
            position: [0, 0, 600],
            generation: 2,
            key_scale: 0,
            operand_scale: 0,
            textures: false,
        },
        8,
    )
    .unwrap_err();
    assert!(error.contains("point tables"), "{error}");
}
