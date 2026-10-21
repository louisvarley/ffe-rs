use ffe_core::model::VertexContext;
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{execute_live, InstanceContext, Primitive};
use ffe_core::model_instance::Vertices;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Trace {
    events: Vec<Event>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Event {
    Point {
        material: u16,
        angle: i16,
        vertex: u8,
        flag: u8,
    },
    Line {
        material: u16,
        vertices: [u8; 2],
    },
}

#[test]
#[ignore = "Local-only point/line handler operands; use tools/reference/verify-point-line.py"]
fn executed_reference_point_line_operands() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_POINT_LINE_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4096);
    let trace: Trace = serde_json::from_str(input.trim()).unwrap();
    assert_eq!(trace.events.len(), 2);

    let words = [1u16, 0x0123, 0x4000, 0x0080, 2, 0x0456, 0x0201, 0];
    let stream: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let records: [u8; 12] = [1, 0, 0, 0, 8, 0, 1, 0, 0, 0, 80, 0];
    let mut vertices = Vertices::new(&records, 4).unwrap();
    let mut context = VertexContext::default();
    let visibility = VisibilityContext {
        depth: 100,
        scale: 0,
        detail: [0, 0],
        normals: &[],
    };
    let instance = InstanceContext {
        position: [0; 3],
        generation: 2,
        key_scale: 0,
        operand_scale: 0,
        textures: true,
    };
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility,
        None,
        instance,
        8,
    )
    .unwrap();

    assert_eq!(result.points.len(), 1);
    assert_eq!(result.lines.len(), 1);
    match &trace.events[0] {
        Event::Point {
            material,
            angle,
            vertex,
            flag,
        } => {
            assert_eq!(result.points[0].material, *material);
            assert_eq!(result.points[0].encoded_angle, *angle);
            assert_eq!(result.points[0].vertex, *vertex);
            assert_eq!(result.points[0].flag, *flag != 0);
        }
        Event::Line { .. } => panic!("expected the first trace event to be a point"),
    }
    match &trace.events[1] {
        Event::Line { material, vertices } => {
            assert_eq!(result.lines[0].material, *material);
            assert_eq!(result.lines[0].vertices, *vertices);
        }
        Event::Point { .. } => panic!("expected the second trace event to be a line"),
    }
    assert!(result.emissions.is_empty());
    assert!(matches!(result.ordered[0], Primitive::Point(_)));
    assert!(matches!(result.ordered[1], Primitive::Line(_)));
    assert_eq!(result.ordered_keys, [0, 0]);
}
