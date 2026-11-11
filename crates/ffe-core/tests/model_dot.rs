use ffe_core::model_dot::{
    particle, particle_clipped, point_dot, sprite, DotShape, DotTables, Span, TABLE_BYTES,
};

fn tables() -> DotTables {
    // The original's sizes: the core grows from nothing to nine, the halo from two to ten.
    let mut bytes = vec![0, 2, 2, 3, 4, 6, 7, 8, 9];
    bytes.extend([2, 3, 5, 5, 6, 7, 8, 9, 10]);
    DotTables::new(&bytes).unwrap()
}

fn pixels(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.width as usize).sum()
}

#[test]
fn the_tables_are_validated() {
    assert!(DotTables::new(&[0; TABLE_BYTES - 1]).is_err());
    let mut bad = vec![0u8; TABLE_BYTES];
    bad[3] = 200;
    assert!(DotTables::new(&bad).is_err());
}

#[test]
fn small_particles_are_the_hand_drawn_patterns() {
    let counts: Vec<usize> = (0..6)
        .map(|size| pixels(&particle(100, 50, size)))
        .collect();
    assert_eq!(counts, [1, 4, 5, 9, 12, 13]);
    // Their centre must be inside x 2..=317 and y 2..=155, else nothing is drawn.
    assert!(particle(1, 50, 3).is_empty());
    assert!(particle(100, 156, 3).is_empty());
    assert!(particle(100, 155, 3).len() == 3);
}

#[test]
fn large_particles_are_circles_clipped_to_the_view() {
    let ten = particle(160, 79, 10);
    assert!(pixels(&ten) > 60 && pixels(&ten) < 100, "{}", pixels(&ten));
    assert_eq!(ten.iter().map(|s| s.y).min(), Some(74));
    assert_eq!(ten.iter().map(|s| s.y).max(), Some(83));
    // Beyond the size limit nothing is drawn; off the view the clipped form draws nothing.
    assert!(particle(160, 79, 948).is_empty());
    assert!(particle_clipped(-20, 79, 10).is_empty());
    assert!(!particle_clipped(-3, 79, 10).is_empty());
    assert!(particle_clipped(160, 300, 10).is_empty());
    // The rows stay inside the view and the runs inside the width.
    for span in particle_clipped(2, 3, 30) {
        assert!((0..158).contains(&span.y));
        assert!(span.x >= 0 && span.x + span.width <= 320);
    }
}

#[test]
fn an_unlit_point_is_a_dot_by_its_size() {
    let tables = tables();
    // Radius 2048 at depth 40,000: a size of (2048 << 8) / 40,000 = 13, so one particle.
    let far = point_dot(
        0x2c00,
        1024,
        9,
        [0; 3],
        [160, 79],
        [0, 0],
        40_000,
        &tables,
        [0; 3],
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        far.shape,
        DotShape::One {
            size: 13,
            colour: [12, 0, 0]
        }
    );
    // Radius 2048 at depth 100,000 is 5: a halo of size 6 and a core of size 4, the halo a
    // third of the way to the sky.
    let near = point_dot(
        0x2cc0,
        1024,
        9,
        [0; 3],
        [160, 79],
        [0, 0],
        100_000,
        &tables,
        [3, 3, 3],
    )
    .unwrap()
    .unwrap();
    assert_eq!(
        near.shape,
        DotShape::Two {
            halo_size: 6,
            halo: [9, 9, 1],
            core_size: 4,
            core: [12, 12, 0]
        }
    );
    // Too near to draw, off the view, and a lit material.
    assert_eq!(
        point_dot(
            0x2c00,
            1024,
            9,
            [0; 3],
            [160, 79],
            [0, 0],
            0x3f,
            &tables,
            [0; 3]
        ),
        Ok(None)
    );
    assert_eq!(
        point_dot(
            0x2c00,
            1024,
            9,
            [0; 3],
            [900, 79],
            [0, 0],
            100_000,
            &tables,
            [0; 3]
        ),
        Ok(None)
    );
    assert!(point_dot(
        0x0c00,
        1024,
        9,
        [0; 3],
        [160, 79],
        [0, 0],
        100_000,
        &tables,
        [0; 3]
    )
    .is_err());
}

#[test]
fn a_sprite_is_vertically_symmetric_and_ends_on_a_negative_byte() {
    let at = |x, y, width| Span { x, y, width };
    // F1609 reads each byte before stepping out a row: centre 3, then 2 on
    // rows 4 and 6, then 1 on rows 3 and 7; -1 ends it. The routine's x + 1
    // with runs of 2h - 1 from x + 1 - h centres every row on x.
    let mut spans = sprite(10, 5, &[3, 2, 1, -1]);
    spans.sort_by_key(|span| span.y);
    assert_eq!(
        spans,
        [
            at(10, 3, 1),
            at(9, 4, 3),
            at(8, 5, 5),
            at(9, 6, 3),
            at(10, 7, 1)
        ]
    );
    // DATA_007784's first record, a cross-shaped glint: a 9-pixel centre row,
    // two empty steps (0), then single pixels two rows further out each way.
    let mut glint = sprite(100, 50, &[5, 0, 0, 1, 1, -1, 0, 0, 0]);
    glint.sort_by_key(|span| span.y);
    assert_eq!(
        glint,
        [
            at(100, 46, 1),
            at(100, 47, 1),
            at(96, 50, 9),
            at(100, 53, 1),
            at(100, 54, 1)
        ]
    );
    // Rows outside the view (y >= 0x9e, or negative) are skipped; runs clip.
    assert!(sprite(10, 200, &[2, -1]).is_empty());
    let edge = sprite(0, 157, &[3, 2, -1]);
    assert!(edge.iter().all(|span| span.x >= 0 && span.y < 158));
    assert_eq!(edge.len(), 2, "row 158 is outside the view");
    // A non-positive centre draws nothing; empty data draws nothing.
    assert!(sprite(10, 5, &[-1]).is_empty());
    assert!(sprite(10, 5, &[]).is_empty());
}
