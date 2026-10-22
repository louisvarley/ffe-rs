use ffe_core::model::IDENTITY;
use ffe_core::model_animation::{
    build_yzt, door_step, rows_product, setup_display, setup_object, shift_vector, station_speeds,
    time_word, top_level_locals, vector_msb, Camera, Descriptor, Draw, ObjectState,
    OrbitalRotation, START_TICS,
};
use ffe_core::model_math::SineTable;

/// A synthetic quarter wave reaching 0x7fff_fffc at index 0x3fff.
fn table() -> SineTable {
    let bytes: Vec<u8> = (0..16384i64)
        .flat_map(|i| ((i * 0x7fff_fffc / 0x3fff) as i32).to_le_bytes())
        .collect();
    SineTable::from_bytes(&bytes).unwrap()
}

#[test]
fn global_word_zero_is_tics_shifted_and_truncated() {
    assert_eq!(time_word(START_TICS), 0);
    assert_eq!(time_word(0x0000_0400), 1);
    assert_eq!(time_word(0x03ff_fc00), 0xffff);
    assert_eq!(time_word(0x0400_0000), 0);
    assert_eq!(time_word(u32::MAX), 0xffff);
}

#[test]
fn door_word_follows_f865() {
    // Opening: half the frame time per frame, clamped at 0xffff, bit 0 cleared.
    assert_eq!(door_step(0, 1, 994), (497, 1));
    assert_eq!(door_step(0xff00, 1, 994), (0xffff, 0));
    assert_eq!(door_step(0xff00, 3, 994), (0xffff, 2));
    // Closing: clamped at 0 with the state set to -2.
    assert_eq!(door_step(1000, -1, 994), (503, -1));
    assert_eq!(door_step(100, -1, 994), (0, -2));
    assert_eq!(door_step(0, -2, 994), (0, -2));
    // Idle.
    assert_eq!(door_step(1234, 0, 994), (1234, 0));
}

#[test]
fn yzt_matrix_spins_about_its_own_z_axis() {
    let table = table();
    let m = build_yzt(&table, 0, 0);
    assert_eq!(m[7], 0);
    assert_eq!([m[1], m[2], m[3], m[5], m[6]], [0; 5]);
    // A quarter turn of a with b = 0 swaps x and y.
    let q = build_yzt(&table, 0x4000, 0);
    assert!(q[1] > 0x7fff_0000 && q[3] < -0x7fff_0000);
    assert_eq!(q[8], m[8]);
    // b tilts the z axis (row 2) to (sin b, 0, cos b) whatever a is.
    let t = build_yzt(&table, 0x1234, 0x4000);
    assert!(t[6] > 0x7fff_0000 && t[7] == 0);
}

#[test]
fn orbital_rotation_rebuilds_when_bits_twenty_up_change() {
    let table = table();
    let station = OrbitalRotation::station(10);
    // Speed 10 shifts by 9: bits 20.. of the shifted value change every 2^11 tics.
    assert_eq!(station.update(&table, 0, 0x7ff), None);
    assert!(station.update(&table, 0x7ff, 0x800).is_some());
    assert_eq!(station.angle(0x0080_0000), Some(0));
    assert_eq!(station.angle(0x0040_0000), Some(0x8000));
    assert_eq!(
        station.update(&table, 0, 0x0040_0000),
        Some(build_yzt(&table, 0x8000, 0))
    );
    // Speed 0 never rotates; a negative speed is not shifted.
    assert_eq!(
        OrbitalRotation::station(0).update(&table, 0, u32::MAX),
        None
    );
    assert_eq!(
        OrbitalRotation::station(-1).angle(0x1234_5678),
        Some(0x1234)
    );
    // Planets take the angle second.
    let planet = OrbitalRotation {
        speed: 1,
        orientation: 0x100,
        station: false,
    };
    assert_eq!(
        planet.matrix(&table, 0x4000_0000),
        Some(build_yzt(&table, 0x100, 0x4000))
    );
    assert_eq!(station_speeds(0x4a), &[10]);
    assert_eq!(station_speeds(0x4c), &[12, 11]);
    assert!(station_speeds(1).is_empty());
}

#[test]
fn vector_helpers_follow_the_original() {
    assert_eq!(vector_msb([0, 0, 0]), 0);
    assert_eq!(vector_msb([1, 0, 0]), 0);
    assert_eq!(vector_msb([2, -3, 0]), 1);
    assert_eq!(vector_msb([0, 0, 1 << 40]), 40);
    assert_eq!(vector_msb([-(1 << 33), 0, 0]), 33);
    assert_eq!(
        shift_vector([1 << 40, -(1 << 40), 5], 8),
        [1 << 32, -(1 << 32), 0]
    );
    assert_eq!(shift_vector([1 << 40, -1, 0], 32), [1 << 8, -1, 0]);
    assert_eq!(shift_vector([1 << 40, -1, 0], 36), [1 << 4, -1, 0]);
    assert_eq!(rows_product(&IDENTITY, &IDENTITY)[0], 0x7fff_fffe);
}

#[test]
fn instance_set_up_defines_local_zero_and_the_shades_only() {
    let shades = [[1, 2, 3]; 8];
    let (locals, undefined) = top_level_locals(&shades);
    assert_eq!(locals[0], 0);
    assert_eq!(locals[12], i32::from_le_bytes([1, 2, 3, 1]));
    assert_eq!(undefined, 0b1111_1111_1110);
    let descriptor = Descriptor {
        scale: 4,
        scale_offset: 0,
        radius: 100,
        far: 30,
        dot: 20,
    };
    let mut object = ObjectState {
        display_position: [0, 0, 5000],
        ..Default::default()
    };
    object.words[1] = 0xabcd;
    let drawn = setup_display(&object, &descriptor, [0, 0, 0x8000], &shades).unwrap();
    assert_eq!(drawn.context.globals[1], 0xabcd);
    assert_eq!(drawn.context.colour, [1; 3]);
    assert_eq!(drawn.instance.key_scale, 0);
    assert_eq!(drawn.instance.operand_scale, 4);
    // Entirely behind the camera: culled.
    object.display_position = [0, 0, -100_000];
    assert!(setup_display(&object, &descriptor, [0; 3], &shades).is_none());
    // F1680: near and in front runs the model; very far is not drawn.
    let camera = Camera {
        matrix: IDENTITY,
        position: [0; 3],
    };
    object.position = [0, 0, 5000];
    assert!(matches!(
        setup_object(&object, &camera, &descriptor, [0; 3], &shades),
        Draw::Model { .. }
    ));
    object.position = [0, 0, 1 << 50];
    assert!(matches!(
        setup_object(&object, &camera, &descriptor, [0; 3], &shades),
        Draw::Far { .. }
    ));
}
