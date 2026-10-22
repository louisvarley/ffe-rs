use ffe_core::model::{multiply, VertexContext, IDENTITY};
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{execute_live, Execution, InstanceContext, Primitive};
use ffe_core::model_instance::{Vertex, Vertices};
use ffe_core::model_math::table_root;
use ffe_core::model_normals::Normal;
use ffe_core::model_polygon::{
    circle, circle_point, curve_edges, decode_segments, edges, fill_path, near_point, Entry,
    Segment,
};

const VISIBLE: [Normal; 4] = [
    Normal {
        culled: false,
        colour: [0; 3],
    },
    Normal {
        culled: false,
        colour: [0; 3],
    },
    Normal {
        culled: false,
        colour: [0; 3],
    },
    Normal {
        culled: false,
        colour: [0; 3],
    },
];

/// Literal pairs: 0/1 = (±39, 39), 2/3 = (±39, -40), 4/5 = (0, 0, -100); at
/// position z 100 the first four are in front of the near plane, 4/5 behind it.
const RECORDS: [u8; 18] = [
    0, 0, 40, 40, 0, 0, //
    0, 0, 40, 0xd8, 0, 0, //
    0, 0, 0, 0, 0x9c, 0,
];
/// One normal pair: record 0 (normals 2/3), vector (0, 90, 90).
const NORMALS: [u8; 6] = [0, 0, 0, 90, 90, 0];

fn polygon(flag: bool, segments: &[u8]) -> Vec<u8> {
    let mut bytes = vec![5, 0, 0x00, 0x0f, 1 | if flag { 0x80 } else { 0 }];
    bytes.push(segments.len() as u8);
    bytes.extend_from_slice(segments);
    bytes.extend_from_slice(&[0, 0]);
    bytes
}

fn run(stream: &[u8], normals: &[u8]) -> Result<(Execution, Vec<Vertex>), String> {
    let mut vertices = Vertices::new(&RECORDS, 6)?.with_normal_records(normals);
    let result = execute_live(
        stream,
        &mut vertices,
        &mut VertexContext::default(),
        &VisibilityContext {
            depth: 100,
            scale: 0,
            detail: [0, 0],
            normals: &VISIBLE,
        },
        None,
        InstanceContext {
            position: [0, 0, 100],
            generation: 2,
            key_scale: 0,
            operand_scale: 2,
            textures: true,
        },
        16,
    )?;
    Ok((result, vertices.slots()))
}

#[test]
fn segments_decode_with_their_operands_and_reject_malformed_lists() {
    // 1: word low byte unused, then P1, start, end, P2; 2: start, end, spare;
    // 3: vertex; 4: P1, end, P2; 5: close; 6: centre, normal, radius.
    let bytes = [
        0x7f, 0x02, 1, 0, 2, 3, //
        0, 0x04, 1, 0x55, //
        2, 0x06, //
        3, 0x08, 2, 1, //
        0, 0x0a, //
        1, 0x0c, 2, 9, //
        0, 0,
    ];
    assert_eq!(
        decode_segments(&bytes, 4).unwrap(),
        [
            Segment::CurveStart {
                start: 0,
                controls: [1, 3],
                end: 2
            },
            Segment::LineStart { start: 0, end: 1 },
            Segment::Line { vertex: 2 },
            Segment::Curve {
                controls: [3, 1],
                end: 2
            },
            Segment::Close,
            Segment::Circle {
                centre: 1,
                normal: 2,
                radius: 9
            },
        ]
    );
    let error = |bytes: &[u8]| decode_segments(bytes, 4).unwrap_err();
    assert!(error(&[0, 0x0e, 0, 0]).contains("outside DATA_007817"));
    assert!(error(&[0, 0x80, 0, 0]).contains("outside DATA_007817"));
    assert!(error(&[0, 0, 0, 0]).contains("does not end the declared length"));
    assert!(error(&[4, 0x06, 0, 0]).contains("out-of-range vertex"));
    assert!(error(&[0x80, 0x06, 0, 0]).contains("out-of-range vertex"));
    assert!(error(&[0, 0x04, 1]).contains("past their declared length"));
}

#[test]
fn front_polygons_queue_edges_and_close_back_to_their_start() {
    // F1724 0 -> 1, F1730 to 3, F1730 to 2, then F1721's own close back to 0.
    let stream = polygon(true, &[0, 0x04, 1, 0, 3, 0x06, 2, 0x06, 0, 0]);
    let (result, slots) = run(&stream, &[]).unwrap();
    let emission = &result.polygons[0];
    let s = |i: usize| slots[i].screen;
    assert_eq!(
        emission.outline.entries,
        [
            Entry::Edge([s(0), s(1)]),
            Entry::To(s(3)),
            Entry::To(s(2)),
            Entry::To(s(0))
        ]
    );
    assert!(!emission.outline.abandoned);
    assert_eq!(emission.colour, [15, 0, 0]);
    assert!(matches!(result.ordered[0], Primitive::Polygon(_)));
    let path = fill_path(&emission.outline.entries).unwrap();
    assert_eq!(path, [s(0), s(1), s(3), s(2), s(0), s(0)]);
    assert_eq!(edges(&emission.outline.entries).unwrap().len(), 4);
}

#[test]
fn near_plane_edges_clip_or_abandon_with_the_flag() {
    // F1724 0 -> 1, F1730 to 4 (behind), then the close re-enters to 0.
    let segments = [0, 0x04, 1, 0, 4, 0x06, 0, 0];
    let (result, slots) = run(&polygon(false, &segments), &[]).unwrap();
    let [v0, v1, v4] = [slots[0], slots[1], slots[4]];
    assert!(v4.view[2] < 0x40);
    assert_eq!(
        result.polygons[0].outline.entries,
        [
            Entry::Edge([v0.screen, v1.screen]),
            Entry::To(near_point(v4.view, v1.view)),
            Entry::To(near_point(v4.view, v0.view)),
            Entry::To(v0.screen),
        ]
    );
    // The flag abandons the polygon: an F1813 entry, still queued with its key.
    let (result, _) = run(&polygon(true, &segments), &[]).unwrap();
    assert!(result.polygons[0].outline.abandoned);
    assert!(result.polygons[0].outline.entries.is_empty());
    assert!(result.polygons[0].key.is_some());
    assert_eq!(result.ordered.len(), 1);
}

#[test]
fn circles_become_two_curves_from_the_rotated_normal() {
    // F1736: centre 0, normal 2 (record 0), radius 3 << scale 2.
    let stream = polygon(false, &[0, 0x0c, 2, 3, 0, 0]);
    let (result, slots) = run(&stream, &NORMALS).unwrap();
    let axis: [i32; 3] = std::array::from_fn(|j| {
        multiply(IDENTITY[j], 0)
            .wrapping_add(multiply(IDENTITY[3 + j], 90))
            .wrapping_add(multiply(IDENTITY[6 + j], 90))
    });
    let [a, b] = circle(slots[0].view, axis, 12).unwrap();
    assert_eq!(
        result.polygons[0].outline.entries,
        [Entry::Curve(a), Entry::Curve(b)]
    );
    assert!(run(&stream, &[])
        .unwrap_err()
        .contains("not a model normal record"));
    // A circle behind the plane with the flag: F1726 skips from the circle's own
    // data bytes. Radius 2 reads as an index-1 word and realigns on the terminator;
    // radius 0 would stop early and misalign the program.
    let aligned = polygon(true, &[4, 0x0c, 2, 2, 0, 0x04, 1, 0, 0, 0]);
    assert!(
        run(&aligned, &NORMALS).unwrap().0.polygons[0]
            .outline
            .abandoned
    );
    let misaligned = polygon(true, &[4, 0x0c, 2, 0, 0, 0x04, 1, 0, 0, 0]);
    assert!(run(&misaligned, &NORMALS)
        .unwrap_err()
        .contains("misaligned terminator"));
}

#[test]
fn segments_that_read_state_left_by_earlier_polygons_are_errors() {
    let stream = polygon(false, &[2, 0x06, 0, 0]);
    assert!(run(&stream, &[])
        .unwrap_err()
        .contains("before this polygon set it"));
}

#[test]
fn helpers_follow_the_original_arithmetic() {
    assert_eq!(
        [0, 4, 9, 1 << 30, u32::MAX].map(table_root),
        [0, 2, 3, 32768, 65519]
    );
    // At z = 0x40 the projection is a shift by 2.
    assert_eq!(near_point([100, 0, 0], [100, 0, 128]), [560, 79]);
    assert_eq!(circle_point([100, 50, 256]), Some([260, 29]));
    assert_eq!(circle_point([100, 50, 63]), None);
    // A straight curve: 8 forward-difference steps (distance 15) plus the end edge.
    let line = curve_edges([[0, 0], [10, 0], [20, 0], [30, 0]]);
    assert_eq!(line.len(), 9);
    assert_eq!((line[0][0], line[8][1]), ([0, 0], [30, 0]));
    assert!(line.windows(2).all(|pair| pair[0][1] == pair[1][0]));
    assert!(edges(&[Entry::To([1, 2])])
        .unwrap_err()
        .contains("queue header"));
}
