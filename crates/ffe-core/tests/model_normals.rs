use ffe_core::model_normals::{evaluate, NormalContext};

#[test]
fn paired_normal_visibility_uses_mirrored_literal_anchor_and_implicit_entries() {
    let vertices = [1, 0, 10, 20, 30, 0];
    let normals = [0, 0, 127, 0, 0, 0];
    let mut context = NormalContext {
        observer: [20, 0, 0],
        shades: std::array::from_fn(|i| [i as u8, 2, 3]),
        ..Default::default()
    };
    let result = evaluate(&normals, 4, &vertices, 2, &context).unwrap();
    assert_eq!(result[0].colour, [3, 2, 3]);
    assert_eq!(result[1].colour, [4, 2, 3]);
    assert!(!result[0].culled && !result[1].culled);
    assert!(!result[2].culled);
    assert!(result[3].culled);
    context.observer = [-20, 0, 0];
    let reversed = evaluate(&normals, 4, &vertices, 2, &context).unwrap();
    assert!(reversed[2].culled);
    assert!(!reversed[3].culled);
    context.light = [-10000, 0, 0];
    let lit = evaluate(&normals, 4, &vertices, 2, &context).unwrap();
    assert_ne!(lit[2].colour, [0; 3]);
    assert_eq!(lit[3].colour, [0; 3]);
}

#[test]
fn normals_validate_scales_counts_anchors_and_use_wrapped_products() {
    let vertices = [1, 0, 128, 127, 255, 0];
    let normals = [1, 0, 128, 127, 255, 0];
    let context = NormalContext {
        scale: 31,
        observer_scale: 0,
        observer: [i32::MIN, i32::MAX, -1],
        light: [i32::MIN, i32::MAX, -1],
        ..Default::default()
    };
    assert_eq!(
        evaluate(&normals, 4, &vertices, 2, &context).unwrap().len(),
        4
    );
    for count in [0, 1, 3, 6, 258, u32::MAX] {
        assert!(evaluate(&normals, count, &vertices, 2, &context).is_err());
    }
    for invalid in [
        NormalContext {
            scale: 32,
            ..context.clone()
        },
        NormalContext {
            observer_scale: 32,
            ..context.clone()
        },
    ] {
        assert!(evaluate(&normals, 4, &vertices, 2, &invalid).is_err());
    }
    assert!(evaluate(&[2, 0, 0, 0, 0, 0], 4, &vertices, 2, &context).is_err());
    assert!(evaluate(&normals, 4, &[11, 0, 0, 0, 0, 0], 2, &context).is_err());
    for end in 0..normals.len() {
        assert!(evaluate(&normals[..end], 4, &vertices, 2, &context).is_err());
    }
    assert_eq!(evaluate(&[], 2, &vertices, 2, &context).unwrap().len(), 2);
}
