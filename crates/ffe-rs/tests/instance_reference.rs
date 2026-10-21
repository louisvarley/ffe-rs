use ffe_core::model::VertexContext;
use ffe_core::model_instance::{project, Vertices};
use serde::Deserialize;

const PROJECTION_VALUES: [i32; 13] = [
    i32::MIN,
    -65537,
    -1,
    0,
    1,
    63,
    64,
    65535,
    65536,
    65537,
    0x1f0000,
    0x40000000,
    i32::MAX,
];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectionVector {
    point: [i32; 3],
    rejected: i32,
    screen: [i32; 2],
}

#[test]
#[ignore = "Local-only original projection; use tools/reference/verify-instance.py"]
fn executed_reference_integer_projection() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_PROJECTION_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 1024 * 1024);
    let mut count = 0;
    for (i, line) in input.lines().enumerate() {
        assert!(i < 2197);
        let v: ProjectionVector = serde_json::from_str(line).unwrap();
        assert_eq!(v.point[0], PROJECTION_VALUES[i / 169]);
        assert_eq!(v.point[1], PROJECTION_VALUES[i / 13 % 13]);
        assert_eq!(v.point[2], PROJECTION_VALUES[i % 13]);
        match project(v.point) {
            // A rejected point leaves the caller's screen words untouched.
            None => {
                assert_eq!(v.rejected, 1, "rejection case {i}");
                assert_eq!(v.screen, [1234, -1234], "untouched case {i}");
            }
            Some(screen) => {
                assert_eq!(v.rejected, 0, "acceptance case {i}");
                assert_eq!(screen.map(i32::from), v.screen, "projection case {i}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 2197);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LiveVector {
    seed: u32,
    step: u32,
    index: u32,
    projected: u32,
    generation: u32,
    object: String,
    view: String,
    screen: String,
    stamps: String,
}

fn fingerprint(bytes: impl Iterator<Item = u8>) -> String {
    let hash = bytes.fold(14695981039346656037u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
    });
    format!("{hash:016x}")
}

#[test]
#[ignore = "Local-only original lazy vertex slots; use tools/reference/verify-instance.py"]
fn executed_reference_live_vertex_slots() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_LIVE_VERTEX_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 8 * 1024 * 1024);
    #[rustfmt::skip]
    let records: [u8; 72] = [
        1, 0, 4, 2, 8, 0, 0, 0, 8, 4, 16, 0, 9, 0, 3, 5, 9, 0, 10, 0, 7, 11, 13, 0,
        5, 0, 0, 0, 0, 0, 11, 0, 0, 0, 2, 0, 15, 0, 2, 0, 4, 0, 17, 0, 0, 0, 2, 0,
        19, 0, 0xc8, 0, 2, 0, 20, 0, 0x81, 2, 4, 0, 6, 0, 0, 0, 2, 0, 2, 0, 255, 127, 128, 0,
    ];
    let matrices = [
        [i32::MAX, 0, 0, 0, i32::MAX, 0, 0, 0, i32::MAX],
        [0, i32::MAX, 0, -i32::MAX, 0, 0, 0, 0, i32::MAX],
        [
            1073741824, -536870912, 268435456, 536870912, 1073741824, -268435456, -268435456,
            268435456, 1073741824,
        ],
        [i32::MIN, 0, 0, 0, i32::MIN, 0, 0, 0, i32::MIN],
    ];
    let positions = [
        [0, 0, 0],
        [0, 0, 64],
        [100, -200, 65536],
        [-100, 200, 65537],
        [i32::MIN, i32::MAX, -1],
        [i32::MAX, i32::MIN, 0x40000000],
    ];
    let scales = [0u32, 10, 31];
    let mut lines = input.lines();
    let mut count = 0;
    for seed in 1..=128u32 {
        let mut cache = Vertices::new(&records, 24).unwrap();
        let mut context = VertexContext {
            scale: scales[seed as usize % 3],
            ..Default::default()
        };
        let mut state = seed;
        for step in 0..64u32 {
            if step % 16 == 0 {
                cache.clear();
            }
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let index = (state >> 16) % 24;
            let projected = state >> 24 & 1;
            let generation = 2 + step / 8 % 3;
            context.matrix = matrices[(seed + step / 8) as usize % 4];
            let position = positions[(seed + step / 4) as usize % 6];
            context.locals[8] = (step / 8 * 16384) as i32;
            context.globals[1] = (step / 8 * 8191) as u16;
            cache
                .resolve(index as i8, &context, position, generation, projected == 1)
                .unwrap();
            let slots = cache.slots();
            let v: LiveVector = serde_json::from_str(lines.next().unwrap()).unwrap();
            assert_eq!((v.seed, v.step), (seed, step));
            assert_eq!(
                (v.index, v.projected, v.generation),
                (index, projected, generation)
            );
            let case = format!("seed {seed} step {step}");
            assert_eq!(
                fingerprint(
                    slots
                        .iter()
                        .flat_map(|s| s.object)
                        .flat_map(i32::to_le_bytes)
                ),
                v.object,
                "object {case}"
            );
            assert_eq!(
                fingerprint(slots.iter().flat_map(|s| s.view).flat_map(i32::to_le_bytes)),
                v.view,
                "view {case}"
            );
            assert_eq!(
                fingerprint(
                    slots
                        .iter()
                        .flat_map(|s| s.screen)
                        .flat_map(i16::to_le_bytes)
                ),
                v.screen,
                "screen {case}"
            );
            assert_eq!(
                fingerprint(slots.iter().map(|s| s.stamp).flat_map(u32::to_le_bytes)),
                v.stamps,
                "stamps {case}"
            );
            count += 1;
        }
    }
    assert!(lines.next().is_none());
    assert_eq!(count, 8192);
}
