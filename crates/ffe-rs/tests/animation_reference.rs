//! Local-only comparisons with the executed original: F1675/F866 (`--animation`)
//! and F1537/F1680/F1683 (`--object-instance`). Run through
//! `tools/reference/verify-animation.py`.
use ffe_core::model_animation::{
    build_yzt, door_step, setup_display, setup_object, time_word, Camera, Descriptor, Draw,
    ObjectState, OrbitalRotation, TopLevel,
};
use ffe_core::model_math::{transform_rows, SineTable};
use serde_json::Value;

fn read(variable: &str) -> String {
    let path = std::env::var_os(variable).unwrap_or_else(|| panic!("{variable} is not set"));
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.len() < 64 << 20, "trace too large");
    text
}

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("not an integer: {value}"))
}

fn ints<const N: usize>(value: &Value) -> [i64; N] {
    let array = value.as_array().unwrap();
    assert_eq!(array.len(), N);
    std::array::from_fn(|i| int(&array[i]))
}

fn i32s<const N: usize>(value: &Value) -> [i32; N] {
    ints::<N>(value).map(|v| v as i32)
}

const SHADES: [[u8; 3]; 8] = {
    let mut shades = [[0u8; 3]; 8];
    let mut i = 0;
    while i < 24 {
        shades[i / 3][i % 3] = (i * 29 + 3) as u8;
        i += 1;
    }
    shades
};

#[test]
#[ignore = "Local-only F1675/F866 comparison; use tools/reference/verify-animation.py"]
fn executed_reference_animation() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_ANIMATION_CACHE").unwrap()).unwrap(),
    )
    .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let input = read("FFE_ANIMATION_VECTORS");
    let (mut matrices, mut orbitals, mut rebuilt, mut moved) = (0, 0, 0, 0);
    for line in input.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        match case["k"].as_str().unwrap() {
            "yzt" => {
                let (a, b) = (int(&case["a"]) as i32, int(&case["b"]) as i32);
                assert_eq!(
                    build_yzt(&table, a, b),
                    i32s::<9>(&case["m"]),
                    "F1675 {a} {b}"
                );
                matrices += 1;
            }
            "orbital" => {
                let flags = int(&case["flags"]) as u8;
                let previous = int(&case["previous"]) as u32;
                let tics = int(&case["tics"]) as u32;
                let frame = int(&case["frame"]) as u32;
                let word = int(&case["word"]) as u16;
                let state = int(&case["state"]) as i8;
                let label = line.to_string();
                assert_eq!(int(&case["out_tics"]) as u32, tics, "{label}");
                // Starports (0x20) run F865 and stop; stations (0x10) run F865
                // after the orbit; other orbitals leave the word alone.
                let expected_door = if flags & 0x30 != 0 {
                    door_step(word, state, frame)
                } else {
                    (word, state)
                };
                assert_eq!(
                    (int(&case["out_word"]) as u16, int(&case["out_state"]) as i8),
                    expected_door,
                    "F865 {label}"
                );
                if expected_door.0 != word {
                    moved += 1;
                }
                let rotation = OrbitalRotation {
                    speed: int(&case["speed"]) as i8,
                    orientation: int(&case["orientation"]) as i16,
                    station: flags & 0x10 != 0,
                };
                let expected = if flags & 0x20 != 0 {
                    None
                } else {
                    rotation.update(&table, previous, tics)
                };
                let actual = (!case["m"].is_null()).then(|| i32s::<9>(&case["m"]));
                assert_eq!(actual, expected, "F866 rotation {label}");
                if actual.is_some() {
                    rebuilt += 1;
                }
                orbitals += 1;
            }
            other => panic!("unknown record {other}"),
        }
    }
    assert_eq!((matrices, orbitals), (2640, 4096));
    eprintln!("F1675 {matrices}; F866 {orbitals}, rebuilt {rebuilt}, door moved {moved}");
    // The synthetic cases must exercise both outcomes of each rule.
    assert!(rebuilt > 500 && rebuilt < 3500, "rebuilt {rebuilt}");
    assert!(moved > 500, "door moved {moved}");
}

fn compare(label: &str, actual: &TopLevel, expected: &Value) {
    let c = &actual.context;
    assert_eq!(
        actual.instance.position,
        i32s::<3>(&expected["position"]),
        "{label} position"
    );
    assert_eq!(c.matrix, i32s::<9>(&expected["matrix"]), "{label} matrix");
    assert_eq!(
        c.direction,
        i32s::<3>(&expected["direction"]),
        "{label} +0x38"
    );
    assert_eq!(
        i64::from(actual.observer_scale),
        int(&expected["observer_scale"]),
        "{label} +0x44"
    );
    assert_eq!(
        actual.observer,
        i32s::<3>(&expected["observer"]),
        "{label} +0x48"
    );
    assert_eq!(
        i64::from(c.locals[0]),
        int(&expected["local0"]),
        "{label} local 0"
    );
    assert_eq!(int(&expected["shades"]), 1, "{label} shades at +0x84");
    let shade_bytes: Vec<u8> = SHADES.iter().flatten().copied().collect();
    for k in 0..6 {
        assert_eq!(
            c.locals[12 + k].to_le_bytes(),
            shade_bytes[4 * k..4 * k + 4],
            "{label} local {}",
            12 + k
        );
    }
    assert_eq!(
        i64::from(actual.instance.key_scale),
        int(&expected["key_scale"]),
        "{label} +0x130"
    );
    assert_eq!(
        actual.light,
        i32s::<3>(&expected["light"]),
        "{label} +0x134"
    );
    assert_eq!(
        i64::from(c.scale),
        int(&expected["scale"]),
        "{label} +0x148"
    );
    assert_eq!(
        i64::from(actual.instance.operand_scale),
        int(&expected["scale"]),
        "{label} operand scale"
    );
    assert_eq!(int(&expected["owner"]), 1, "{label} +0x14c is the object");
    assert_eq!(
        c.colour.map(i64::from),
        ints::<3>(&expected["colour"]),
        "{label} colour"
    );
    assert_eq!(
        i64::from(c.base_handedness),
        int(&expected["base_handedness"]),
        "{label} +0xf8"
    );
    assert_eq!(
        c.undefined_locals, 0b1111_1111_1110,
        "{label} undefined locals"
    );
}

#[test]
#[ignore = "Local-only F1537/F1680/F1683 comparison; use tools/reference/verify-animation.py"]
fn executed_reference_object_instances() {
    let input = read("FFE_OBJECT_INSTANCE_VECTORS");
    let mut counts = [0usize; 6]; // far, culled, dot, model, sleeping, display drawn
    let (mut frames, mut displays, mut display_culled) = (0, 0, 0);
    for line in input.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        match case["k"].as_str().unwrap() {
            "frame" => {
                frames += 1;
                let tics = int(&case["tics"]) as u32;
                let camera = Camera {
                    matrix: i32s::<9>(&case["camera"]["matrix"]),
                    position: ints::<3>(&case["camera"]["position"]),
                };
                // F1534: the light is the star direction in camera axes (F1670).
                let light = transform_rows(i32s::<3>(&case["star"]), &camera.matrix);
                assert_eq!(light, i32s::<3>(&case["light"]), "F1534 light");
                for object in case["objects"].as_array().unwrap() {
                    let label = format!("frame {frames} slot {}", object["slot"]);
                    let sleep = int(&object["sleep"]);
                    assert_eq!(int(&object["words_kept"]), 1, "{label} words 1..63");
                    if sleep != 0 {
                        // F1537: a sleeping object only counts down.
                        assert_eq!(int(&object["out_sleep"]), sleep - 1, "{label}");
                        assert_eq!(int(&object["out_word0"]), int(&object["word0"]), "{label}");
                        assert_eq!(int(&object["outcome"]), 0, "{label}");
                        counts[4] += 1;
                        continue;
                    }
                    assert_eq!(
                        int(&object["out_word0"]) as u16,
                        time_word(tics),
                        "{label} global 0"
                    );
                    let d = ints::<5>(&object["descriptor"]).map(|v| v as i32);
                    let descriptor = Descriptor {
                        scale: d[0],
                        scale_offset: d[1],
                        radius: d[2],
                        far: d[3],
                        dot: d[4],
                    };
                    let mut words = [0u16; 64];
                    words[0] = time_word(tics);
                    let state = ObjectState {
                        matrix: i32s::<9>(&object["matrix"]),
                        position: ints::<3>(&object["position"]),
                        scale: int(&object["scale"]) as i32,
                        words,
                        ..Default::default()
                    };
                    let screen = ints::<2>(&object["screen"]).map(|v| v as u16);
                    let off = [0x8000u16; 2];
                    let check_screen = |value: Option<[i16; 2]>| {
                        let expected = value.map(|p| p.map(|v| v as u16)).unwrap_or(off);
                        assert_eq!(screen, expected, "{label} screen +0x98");
                    };
                    let outcome = int(&object["outcome"]);
                    match setup_object(&state, &camera, &descriptor, light, &SHADES) {
                        Draw::Far { distance, outside } => {
                            assert_eq!(outcome, 0, "{label} far");
                            assert_eq!(int(&object["distance"]), i64::from(distance), "{label}");
                            assert_eq!(int(&object["outside"]), i64::from(outside), "{label}");
                            assert_eq!(screen, off, "{label}");
                            counts[0] += 1;
                        }
                        Draw::Culled { distance, screen } => {
                            assert_eq!(outcome, 0, "{label} culled");
                            assert_eq!(int(&object["distance"]), i64::from(distance), "{label}");
                            assert_eq!(int(&object["outside"]), 0, "{label}");
                            check_screen(screen);
                            counts[1] += 1;
                        }
                        Draw::Dot { distance, screen } => {
                            assert_eq!(outcome, 2, "{label} dot");
                            assert_eq!(int(&object["distance"]), i64::from(distance), "{label}");
                            check_screen(screen);
                            counts[2] += 1;
                        }
                        Draw::Model {
                            distance,
                            screen,
                            instance,
                        } => {
                            assert_eq!(outcome, 1, "{label} model");
                            assert_eq!(int(&object["distance"]), i64::from(distance), "{label}");
                            check_screen(screen);
                            compare(&label, &instance, &object["instance"]);
                            assert_eq!(instance.context.globals[0], time_word(tics));
                            counts[3] += 1;
                        }
                    }
                }
            }
            "display" => {
                displays += 1;
                let label = format!("display {displays} model {}", case["model"]);
                let d = ints::<3>(&case["descriptor"]).map(|v| v as i32);
                let descriptor = Descriptor {
                    scale: d[0],
                    scale_offset: d[1],
                    radius: d[2],
                    far: 0,
                    dot: 0,
                };
                let state = ObjectState {
                    matrix: i32s::<9>(&case["matrix"]),
                    display_position: i32s::<3>(&case["position"]),
                    ..Default::default()
                };
                let light = i32s::<3>(&case["in_light"]);
                match setup_display(&state, &descriptor, light, &SHADES) {
                    None => {
                        assert_eq!(int(&case["outcome"]), 0, "{label} culled");
                        display_culled += 1;
                    }
                    Some(instance) => {
                        assert_eq!(int(&case["outcome"]), 1, "{label} drawn");
                        assert_eq!(instance.context.matrix, state.matrix, "{label}");
                        compare(&label, &instance, &case["instance"]);
                        counts[5] += 1;
                    }
                }
            }
            other => panic!("unknown record {other}"),
        }
    }
    assert_eq!((frames, displays), (96, 4096));
    eprintln!(
        "far {} culled {} dot {} model {} sleeping {} display drawn {} culled {}",
        counts[0], counts[1], counts[2], counts[3], counts[4], counts[5], display_culled
    );
    for (count, name) in counts
        .iter()
        .zip(["far", "culled", "dot", "model", "sleeping", "display"])
    {
        assert!(*count >= 20, "too few {name} cases: {count}");
    }
    assert!(display_culled >= 20, "too few culled displays");
}
