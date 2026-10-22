use ffe_core::model::VertexContext;
use ffe_core::model_commands::{Thruster, VisibilityContext};
use ffe_core::model_execute::{execute_live, InstanceContext, Primitive, QueueEvent};
use ffe_core::model_instance::{Vertex, Vertices};
use ffe_core::model_normals::Normal;
use ffe_core::model_thruster::{
    base_colour, entries, line, ramp, scaled_width, special_divide, Instance, Shape, CORE_TEXTURE,
    SHAFT_TEXTURE,
};

fn vertex(screen: [i16; 2], z: i32) -> Vertex {
    Vertex {
        object: [0; 3],
        view: [0, 0, z],
        screen,
        stamp: 2,
    }
}

fn thruster(material: u16, width: i16) -> Thruster {
    Thruster {
        offset: 0,
        material,
        vertices: [0, 2],
        width,
    }
}

const PLAIN: Instance = Instance {
    scale: 6,
    key_scale: 0,
    colour: [0; 3],
};

#[test]
fn width_is_shifted_by_the_scale_and_divided_by_the_depth() {
    // F1666 halves both operands while the divisor exceeds 0x7fff.
    assert_eq!(special_divide(1000, 0x10000).unwrap(), 3);
    assert_eq!(special_divide(100, 0x10000).unwrap(), 0);
    assert_eq!(special_divide(-7, 0).unwrap(), -7);
    assert_eq!(special_divide(5, -2).unwrap(), -640);
    // Scale 0 shifts right by 6 (arithmetic), so -5 becomes -1, then -256 / 1000.
    assert_eq!(scaled_width(-5, 0, 1000).unwrap(), 0);
    assert_eq!(scaled_width(-500, 0, 100).unwrap(), -20);
    // x86 masks the shift count: 40 - 6 = 34 shifts by 2.
    assert_eq!(scaled_width(3, 40, 512).unwrap(), 6);
    assert_eq!(scaled_width(400, 6, 1007).unwrap(), 101);
}

#[test]
fn colours_fade_in_sevenths_from_the_material_colour() {
    // Local material: nibbles plus the instance colour; F1694 adds no normal colour.
    assert_eq!(base_colour(0x1123, [2, 2, 2]), [3, 4, 5]);
    assert_eq!(base_colour(0x2123, [2, 2, 2]), [1, 2, 3]);
    assert_eq!(base_colour(0x4123, [2, 2, 2]), [0, 0, 0]);
    let slots = ramp([7, 14, 255]);
    assert_eq!(slots[0], [2, 4, 72]);
    assert_eq!(slots[5], [7, 14, 255]);
}

#[test]
fn narrow_thrusters_and_lines_follow_f1704_clipping() {
    let a = vertex([100, 50], 1000);
    // Both on screen: stored x is x - 1.
    assert_eq!(
        line(&a, &vertex([0, 60], 1000)).unwrap(),
        Some([[99, 50], [0, 60]])
    );
    // The off-screen end is moved onto x = 319 along the line.
    assert_eq!(
        line(&a, &vertex([400, 90], 1000)).unwrap(),
        Some([[99, 50], [318, 79]])
    );
    // Both off to the left: the x outcodes reject the line.
    assert_eq!(
        line(&vertex([-10, 50], 1000), &vertex([-20, 60], 1000)).unwrap(),
        None
    );
    // Both behind the near plane once clipping is needed.
    assert_eq!(
        line(&vertex([-10, 50], 10), &vertex([400, 60], 20)).unwrap(),
        None
    );
    // A width of 1..=4 queues the line with A's key.
    let queued = entries(
        &thruster(0x0123, 12),
        &vertex([160, 79], 1007),
        Some(&vertex([160, 79], 1079)),
        PLAIN,
    )
    .unwrap();
    assert_eq!(queued.width, Some(3));
    assert_eq!(queued.entries.len(), 1);
    assert_eq!(queued.entries[0].key, 125);
    assert_eq!(
        queued.entries[0].shape,
        Shape::Line {
            points: [[159, 79], [159, 79]],
            colour: [1, 2, 3]
        }
    );
}

#[test]
fn wide_thrusters_queue_a_core_a_shaft_or_both() {
    let near = vertex([160, 79], 1007);
    let same = vertex([160, 79], 1079);
    // A zero screen length: the core alone, a square around (2A + B) / 3.
    let core = entries(&thruster(0x0123, 400), &near, Some(&same), PLAIN).unwrap();
    assert_eq!(core.width, Some(101));
    let [entry] = core.entries.as_slice() else {
        panic!("expected the core alone: {core:?}");
    };
    let Shape::Glow {
        points,
        texture,
        colours,
    } = &entry.shape
    else {
        panic!("expected a glow");
    };
    assert_eq!(*texture, CORE_TEXTURE);
    // width >> 2 = 25, -width >> 2 = -26.
    assert_eq!(*points, [[134, 104], [185, 104], [186, 54], [135, 54]]);
    assert_eq!(colours[5], [1, 2, 3]);
    // A screen length of 100 at width 40: the shaft alone, offset by 40 >> 2 across.
    let a = vertex([100, 79], 1007);
    let b = vertex([200, 79], 1079);
    let shaft = entries(&thruster(0x0123, 158), &a, Some(&b), PLAIN).unwrap();
    assert_eq!(shaft.width, Some(40));
    let [entry] = shaft.entries.as_slice() else {
        panic!("expected the shaft alone: {shaft:?}");
    };
    let Shape::Glow {
        points, texture, ..
    } = &entry.shape
    else {
        panic!("expected a glow");
    };
    assert_eq!(*texture, SHAFT_TEXTURE);
    assert_eq!(*points, [[100, 89], [200, 89], [200, 69], [100, 69]]);
    // Wider than the length but not eight times: both, core first, one key.
    let both = entries(&thruster(0x0123, 1600), &a, Some(&b), PLAIN).unwrap();
    let textures: Vec<_> = both
        .entries
        .iter()
        .map(|e| match e.shape {
            Shape::Glow { texture, .. } => texture,
            Shape::Line { .. } => 0,
        })
        .collect();
    assert_eq!(textures, [CORE_TEXTURE, SHAFT_TEXTURE]);
    assert!(both.entries.iter().all(|e| e.key == 125));
    // Not deeper than 0x200 at either end: nothing, and no width.
    let shallow = entries(
        &thruster(0x0123, 400),
        &vertex([1, 1], 0x200),
        Some(&b),
        PLAIN,
    );
    assert_eq!(shallow.unwrap().width, None);
    let shallow = entries(
        &thruster(0x0123, 400),
        &a,
        Some(&vertex([1, 1], 0x200)),
        PLAIN,
    );
    assert_eq!(shallow.unwrap().width, None);
    assert!(entries(&thruster(0x0123, 400), &a, None, PLAIN)
        .unwrap()
        .entries
        .is_empty());
}

// Two literal vertex pairs at object z 7 and 79 (see model_execute.rs).
const RECORDS: [u8; 12] = [1, 0, 0, 0, 8, 0, 1, 0, 0, 0, 80, 0];
const VISIBLE_NORMAL: Normal = Normal {
    culled: false,
    colour: [0; 3],
};
static VISIBLE: [Normal; 4] = [VISIBLE_NORMAL; 4];

fn run(width: u16, z: i32) -> (ffe_core::model_execute::Execution, Vec<u32>) {
    let words = [9u16, 0x1123, 0x0200, width, 0];
    let stream: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let mut vertices = Vertices::new(&RECORDS, 4).unwrap();
    let mut context = VertexContext {
        colour: [2, 2, 2],
        ..Default::default()
    };
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &VisibilityContext {
            depth: 100,
            scale: 0,
            detail: [0, 0],
            normals: &VISIBLE,
        },
        None,
        InstanceContext {
            position: [0, 0, z],
            generation: 2,
            key_scale: 0,
            operand_scale: 6,
            textures: true,
        },
        8,
    )
    .unwrap();
    let stamps = vertices.slots().iter().map(|v| v.stamp).collect();
    (result, stamps)
}

#[test]
fn executed_thrusters_join_the_render_order_and_resolve_b_only_when_a_is_deep() {
    let (result, stamps) = run(400, 1000);
    assert_eq!(stamps, [2, 1, 2, 1]);
    assert_eq!(result.queue, [QueueEvent::Thruster(125)]);
    let [Primitive::Thruster(entry)] = result.ordered.as_slice() else {
        panic!("expected one thruster entry: {:?}", result.ordered);
    };
    assert_eq!(entry.key, 125);
    let Shape::Glow { colours, .. } = &entry.shape else {
        panic!("expected a glow");
    };
    // Material 0x1123 is local: (1, 2, 3) + the instance colour (2, 2, 2).
    assert_eq!(colours[0], [0, 1, 1]);
    assert_eq!(colours[5], [3, 4, 5]);
    assert_eq!(result.thrusters[0].screens, Some([[160, 79], [160, 79]]));
    // A narrow thruster is an F1704 line.
    let (result, _) = run(12, 1000);
    assert_eq!(result.queue, [QueueEvent::ThrusterLine(125)]);
    // A not deeper than 0x200: B is never resolved and nothing is queued.
    let (result, stamps) = run(400, 300);
    assert_eq!(stamps, [2, 1, 0, 0]);
    assert!(result.ordered.is_empty() && result.queue.is_empty());
    assert_eq!(result.thrusters[0].width, None);
}
