use ffe_core::model_math::SineTable;
use ffe_rs::model_viewer::{ModelViewer, Shape, ViewerModel};

fn table() -> SineTable {
    let bytes: Vec<u8> = (0..16384i32).flat_map(|i| (i * 4).to_le_bytes()).collect();
    SineTable::from_bytes(&bytes).unwrap()
}

/// Pairs (10, 0, 0)/(-10, 0, 0) and (0, 10, 4)/(0, 10, 4); a triangle on v0, v1, v2
/// with material 0x0123 and implicit normal 0, then the terminator.
fn triangle_model(index: u32) -> ViewerModel {
    let vertices = [1, 0, 10, 0, 0, 0, 1, 0, 0, 10, 4, 0];
    let commands: Vec<u8> = [
        3u16,
        0x0123,
        u16::from_le_bytes([1, 0]),
        u16::from_le_bytes([0, 2]),
        0,
    ]
    .iter()
    .flat_map(|w| w.to_le_bytes())
    .collect();
    ViewerModel::new(index, 4, 4, &vertices, &commands, &[], 2)
}

#[test]
fn viewer_executes_models_wraps_selection_and_fits_them_on_screen() {
    let mut viewer =
        ModelViewer::from_models(vec![triangle_model(9), triangle_model(3)], table()).unwrap();
    assert_eq!(viewer.len(), 2);
    // Sorted by table index.
    assert_eq!(viewer.model().index, 3);
    viewer.step(-1);
    assert_eq!(viewer.model().index, 9);
    viewer.step(1);
    let frame = viewer.render().unwrap();
    assert_eq!(frame.skipped, 0);
    let [Shape::Fill(points, rgb)] = frame.shapes.as_slice() else {
        panic!("expected one filled triangle, got {:?}", frame.shapes);
    };
    assert_eq!(points.len(), 3);
    assert!(points
        .iter()
        .all(|(x, y)| (0..320).contains(x) && (0..158).contains(y)));
    assert_ne!(*rgb, [0; 3]);
    // Rendering is repeatable and the camera changes the projection.
    assert_eq!(viewer.render().unwrap(), frame);
    viewer.turn(1, 0);
    assert_ne!(viewer.render().unwrap(), frame);
    assert!(ModelViewer::from_models(Vec::new(), table()).is_err());
}

#[test]
fn viewer_clock_drives_the_original_animation_inputs() {
    use ffe_core::model_animation::START_TICS;
    let mut viewer = ModelViewer::from_models(vec![triangle_model(3)], table()).unwrap();
    assert_eq!(viewer.animation(), None);
    assert_eq!(viewer.clock(), (START_TICS, 0, 1));
    // One second at acceleration 1: 49,700 tics; global 0 is tics >> 10.
    viewer.advance(1000);
    assert_eq!(viewer.clock().0, START_TICS + 49_700);
    assert_eq!(
        viewer.inputs().globals[0],
        ((START_TICS + 49_700) >> 10) as u16
    );
    // The gear/door word opens at half the frame time per frame (F865).
    viewer.toggle_door();
    viewer.advance(20);
    assert_eq!(viewer.clock().1, 497);
    viewer.cycle_rate();
    assert_eq!(viewer.clock().2, 10);
    // The test input overrides every global and local, then the clock resumes.
    viewer.set_animation(32768);
    assert_eq!(viewer.inputs().globals[7], 32768);
    viewer.use_clock();
    assert_eq!(viewer.inputs().locals[0], 0);
    // Station orientation: identity for speed 0, a turn about z otherwise.
    let rows = viewer.station_rows(0x1234_5678, 0);
    assert!((rows[0][0] - 1.0).abs() < 1e-6 && rows[2][2] > 0.999);
    let rows = viewer.station_rows(0x0040_0000, 10);
    // (The synthetic sine table is small: only the signs are meaningful.)
    assert!(rows[0][0] < 0.0 && rows[1][1] < 0.0 && rows[0][1] == 0.0);
}
