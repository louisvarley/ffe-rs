//! Synthetic opcode 10 text rendering: a two-vertex parent places a string whose
//! glyph program runs over the font model's vertices.
use ffe_core::model_math::SineTable;
use ffe_rs::model_viewer::{GlyphSet, ModelViewer, Shape, ViewerModel};

fn table() -> SineTable {
    let bytes: Vec<u8> = (0..16384i32).flat_map(|i| (i * 4).to_le_bytes()).collect();
    SineTable::from_bytes(&bytes).unwrap()
}

/// Font model (slot 0): vertices 0/1 at the origin and 2/3 at (10, 0, 0).
fn font_model() -> ViewerModel {
    let vertices = [1, 0, 0, 0, 0, 0, 1, 0, 10, 0, 0, 0];
    ViewerModel::new(0, 4, 0, &vertices, &[0, 0], &[], 2)
}

/// One glyph program: a point on vertex 0, then a return whose bits 6.. name the
/// next pen vertex (0, so the pen does not move).
fn glyph_set() -> GlyphSet {
    // Every character in the set aliases the one program (as lowercase aliases
    // uppercase in the original fonts).
    let count = ffe_core::model_commands::GLYPH_CHAR_LIMIT;
    // An 8-byte point record on vertex 0 (material 0x0fff, angle 0, flag 0), then
    // a return word naming pen vertex 0.
    let program = [1u8, 0, 0xff, 0x0f, 0, 0, 0, 0, 0, 0];
    let mut payload: Vec<u8> = std::iter::repeat_n(0u32, count)
        .flat_map(u32::to_le_bytes)
        .collect();
    payload.extend_from_slice(&program);
    GlyphSet::from_payload(&payload, count as u32, 4, 0).unwrap()
}

/// Parent model (slot 9): vertex 0 at (0, 0, 40) and one text command.
fn text_model(string: u16, glyphs: u8) -> ViewerModel {
    let vertices = [1, 0, 0, 0, 40, 0];
    // 10-byte record: opcode, material, normal, (size | glyph set), vertex,
    // frame high byte, string; then the return.
    let commands: Vec<u8> = [
        10u16,
        0x0fff,                               // material
        u16::from_le_bytes([0, glyphs << 4]), // normal 0, size 0, glyph set
        0,                                    // vertex 0, frame high byte 0
        string,
        0, // return
    ]
    .iter()
    .flat_map(|word| word.to_le_bytes())
    .collect();
    ViewerModel::new(9, 2, 0, &vertices, &commands, &[], 2)
}

#[test]
fn text_records_run_their_glyph_programs_and_move_the_pen() {
    let mut viewer =
        ModelViewer::from_models(vec![font_model(), text_model(0x4000, 0)], table()).unwrap();
    viewer.set_model_strings(vec![b"AA".to_vec()]);
    viewer.set_glyph_sets(vec![Some(glyph_set())]);
    // Sorted by index: slot 0 first, so select the parent.
    assert_eq!(viewer.model().index, 0);
    viewer.step(1);
    assert_eq!(viewer.model().index, 9);

    let frame = viewer.render().unwrap();
    assert_eq!(frame.skipped, 0);
    assert_eq!(frame.texts, 1);
    let points: Vec<_> = frame
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Point(position, _) => Some(*position),
            _ => None,
        })
        .collect();
    assert_eq!(
        points.len(),
        2,
        "one point per character: {:?}",
        frame.shapes
    );

    // A code outside the model string table is reported, not guessed.
    let mut unknown =
        ModelViewer::from_models(vec![font_model(), text_model(0x9c00, 0)], table()).unwrap();
    unknown.set_glyph_sets(vec![Some(glyph_set())]);
    unknown.step(1);
    let frame = unknown.render().unwrap();
    assert_eq!(frame.skipped, 1);
    assert!(frame.shapes.is_empty());

    // A glyph set the cache does not have is skipped too.
    let mut missing =
        ModelViewer::from_models(vec![font_model(), text_model(0x4000, 1)], table()).unwrap();
    missing.set_model_strings(vec![b"AA".to_vec()]);
    missing.set_glyph_sets(vec![Some(glyph_set())]);
    missing.step(1);
    assert_eq!(missing.render().unwrap().skipped, 1);
}
