use ffe_content::AssetKind;
use ffe_core::{
    model::VertexContext, model_commands::arithmetic_with_table, model_math::SineTable,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Vector {
    Sine {
        angle: i32,
        value: i32,
    },
    Rotate {
        matrix: usize,
        axis: u8,
        angle: i32,
        hash: String,
    },
    Math {
        op: u16,
        a: i32,
        angle: i32,
        value: u16,
    },
}

#[test]
#[ignore = "Local-only executed JJFFE transform comparison; use tools/reference/verify-transforms.py"]
fn executed_reference_integer_sine_rotations_and_trig_arithmetic() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_TRANSFORM_CACHE").unwrap()).unwrap(),
    )
    .unwrap();
    let id = "base:math/sine";
    assert!(cache
        .manifest()
        .files
        .iter()
        .any(|r| r.id == id && matches!(r.asset, AssetKind::SineTable { .. })));
    let table = SineTable::from_bytes(cache.asset(id).unwrap()).unwrap();
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_TRANSFORM_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 8 * 1024 * 1024);
    let angles = [
        0,
        1,
        0x3fff,
        0x4000,
        0x4001,
        0x7fff,
        0x8000,
        0xbfff,
        0xc000,
        0xffff,
        -1,
        -65536,
        65536,
        i32::MIN,
        i32::MAX,
    ];
    let matrices = [
        [i32::MAX, 0, 0, 0, i32::MAX, 0, 0, 0, i32::MAX],
        [
            i32::MIN,
            i32::MAX,
            -1,
            1,
            -65536,
            32768,
            123456789,
            -987654321,
            0,
        ],
        [123, 456, 789, -1011, 1213, -1415, 1617, -1819, 2021],
    ];
    let amplitudes = [i32::MIN, -65536, -1, 0, 1, 65535, i32::MAX];
    let mut counts = [0; 3];
    for (i, line) in input.lines().enumerate() {
        let v: Vector = serde_json::from_str(line).unwrap();
        match v {
            Vector::Sine { angle, value } => {
                let n = counts[0];
                assert_eq!(i, n);
                assert!(n < 65541);
                assert_eq!(
                    angle,
                    if n < 65536 {
                        n as i32
                    } else {
                        angles[n - 65536 + 10]
                    }
                );
                assert_eq!(table.sin32(angle), value, "sine angle {angle}");
                counts[0] += 1;
            }
            Vector::Rotate {
                matrix,
                axis,
                angle,
                hash,
            } => {
                let n = counts[1];
                assert_eq!(i, 65541 + n);
                assert!(n < 135);
                assert_eq!(
                    (matrix, axis, angle),
                    (n / 45, (n / 15 % 3) as u8, angles[n % 15])
                );
                let mut result = matrices[matrix];
                table.rotate(&mut result, axis, angle).unwrap();
                let actual = result
                    .into_iter()
                    .flat_map(i32::to_le_bytes)
                    .fold(14695981039346656037u64, |h, b| {
                        (h ^ u64::from(b)).wrapping_mul(1099511628211)
                    });
                assert_eq!(format!("{actual:016x}"), hash, "rotation {n}");
                counts[1] += 1;
            }
            Vector::Math {
                op,
                a,
                angle,
                value,
            } => {
                let n = counts[2];
                assert_eq!(i, 65541 + 135 + n);
                assert!(n < 210);
                assert_eq!(
                    (op, a, angle),
                    (
                        13 + (n / 105) as u16,
                        amplitudes[n / 15 % 7],
                        angles[n % 15]
                    )
                );
                let mut context = VertexContext::default();
                context.locals[8] = a;
                context.locals[9] = angle;
                assert_eq!(
                    arithmetic_with_table(op << 4 | 13, [0xc8, 0xc9], &mut context, Some(&table))
                        .unwrap(),
                    value,
                    "trig arithmetic {n}"
                );
                assert_eq!(context.locals[0], i32::from(value));
                counts[2] += 1;
            }
        }
    }
    assert_eq!(counts, [65541, 135, 210]);
}
