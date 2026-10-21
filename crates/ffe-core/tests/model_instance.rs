use ffe_core::model::VertexContext;
use ffe_core::model_instance::{project, Vertices, NEAR_SENTINEL};

#[test]
fn projection_preserves_near_boundary_shift_wrap_division_and_far_divisor() {
    for z in [i32::MIN, -1, 0, 63] {
        assert_eq!(project([1, 2, z]), None);
    }
    assert_eq!(project([64, -64, 64]), Some([416, 335]));
    assert_eq!(project([65536, 65536, 65536]), Some([416, -177]));
    assert_eq!(project([65536, 65536, 65537]), Some([416, -177]));
    assert_eq!(project([i32::MIN, i32::MAX, 64]), Some([160, 83]));
    assert_eq!(project([-1, 1, 1000]), Some([160, 79]));
    assert_eq!(project([i32::MAX, 0, 65537]), Some([4096, 0]));
}

#[test]
fn translation_wraps_and_only_projection_updates_generation() {
    let records = [1, 0, 2, 3, 4, 0];
    let context = VertexContext::default();
    let mut cache = Vertices::new(&records, 2).unwrap();
    let first = cache.resolve(0, &context, [10, 20, 100], 2, false).unwrap();
    assert_eq!(first.object, [1, 2, 3]);
    assert_eq!(first.view, [11, 22, 103]);
    assert_eq!(first.stamp, 1);
    assert_eq!(cache.slots()[1].object, [-1, 2, 3]);
    assert_eq!(cache.slots()[1].stamp, 1);
    let second = cache.resolve(0, &context, [20, 30, 200], 2, true).unwrap();
    assert_eq!(second.view, [21, 32, 203]);
    assert_eq!(second.screen, project(second.view).unwrap());
    assert_eq!(second.stamp, 2);
    assert_eq!(
        cache.resolve(0, &context, [0; 3], 2, false).unwrap(),
        second
    );
    let third = cache
        .resolve(0, &context, [i32::MAX, 0, 0], 3, false)
        .unwrap();
    assert_eq!(third.view, [i32::MIN, 2, 3]);
    assert_eq!(third.stamp, 2);
    assert_eq!(
        cache.resolve(0, &context, [0; 3], 3, true).unwrap().screen,
        NEAR_SENTINEL
    );
}

#[test]
fn paired_slots_and_first_use_animation_state_persist_until_explicit_clear() {
    let records = [1, 0, 4, 0, 0, 0, 1, 0, 8, 0, 0, 0, 19, 0, 0xc8, 0, 2, 0];
    let mut context = VertexContext {
        scale: 10,
        ..Default::default()
    };
    let mut cache = Vertices::new(&records, 6).unwrap();
    let initial = cache.resolve(4, &context, [0, 0, 100], 2, false).unwrap();
    assert_eq!(initial.object, [4095, 0, 0]);
    assert_eq!(cache.slots()[5].stamp, 0);
    context.locals[8] = 65536;
    context.matrix = [0; 9];
    assert_eq!(
        cache.resolve(4, &context, [0; 3], 3, false).unwrap().object,
        initial.object
    );
    // The other interpolation slot is first evaluated now, using the new local
    // but the old already memoized literal pair coordinates.
    assert_eq!(
        cache.resolve(5, &context, [0; 3], 3, false).unwrap().object,
        [-8191, 0, 0]
    );
    cache.clear();
    assert!(cache.slots().iter().all(|s| *s == Default::default()));
    assert_eq!(
        cache.resolve(4, &context, [0; 3], 3, false).unwrap().object,
        [0; 3]
    );
}

#[test]
fn literal_pairs_memoize_until_cleared_and_projection_stamps_one_slot() {
    let records = [9, 0, 4, 2, 8, 0];
    let mut context = VertexContext::default();
    let mut cache = Vertices::new(&records, 2).unwrap();
    assert_eq!(
        cache.resolve(0, &context, [0; 3], 2, false).unwrap().stamp,
        1
    );
    assert_eq!(cache.slots()[0].object, [3, 1, 7]);
    context.matrix = [0; 9];
    // Both pair entries were evaluated together, so the new matrix cannot reach them.
    let reflected = cache.resolve(1, &context, [0; 3], 2, true).unwrap();
    assert_eq!(reflected.object, [0, 1, 7]);
    assert_eq!(reflected.screen, NEAR_SENTINEL);
    assert_eq!(cache.slots()[0].stamp, 1);
    assert_eq!(cache.slots()[1].stamp, 2);
    cache.clear();
    assert_eq!(
        cache.resolve(1, &context, [0; 3], 2, false).unwrap().object,
        [0; 3]
    );
}

#[test]
fn live_vertices_reject_unsupported_forms_indices_scales_and_cycles_atomically() {
    for screen_or_random in [[3, 0, 0, 0, 0, 0], [7, 0, 0, 0, 0, 0], [13, 0, 0, 0, 0, 0]] {
        assert!(Vertices::new(&screen_or_random, 2).is_err());
    }
    let records = [1, 0, 4, 0, 0, 0, 17, 0, 0, 0, 2, 0];
    let mut cache = Vertices::new(&records, 4).unwrap();
    let context = VertexContext::default();
    let before = cache.slots();
    assert!(cache.resolve(2, &context, [0; 3], 2, false).is_err());
    assert_eq!(cache.slots(), before);
    for (index, generation, scale) in [(-1, 2, 0), (4, 2, 0), (0, 0, 0), (0, 2, 32)] {
        let context = VertexContext {
            scale,
            ..Default::default()
        };
        assert!(cache
            .resolve(index, &context, [0; 3], generation, false)
            .is_err());
        assert_eq!(cache.slots(), before);
    }
    assert_eq!(
        cache.resolve(0, &context, [0; 3], 2, false).unwrap().object,
        [3, 0, 0]
    );
}
