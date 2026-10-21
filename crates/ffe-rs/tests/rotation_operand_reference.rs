use ffe_core::{
    model::VertexContext,
    model_math::{angle_operand, SineTable},
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Vector {
    Operand {
        encoded: u16,
        value: i32,
    },
    Command {
        matrix: usize,
        axis: u8,
        encoded: u16,
        hash: String,
    },
}

#[test]
#[ignore = "Local-only executed JJFFE rotation operands; use tools/reference/verify-rotation-operands.py"]
fn executed_reference_packed_angles_and_rotation_commands() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_ROTATION_CACHE").unwrap()).unwrap())
            .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_ROTATION_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 8 * 1024 * 1024);
    let globals = std::array::from_fn(|i| (i * 257 + 19) as u16);
    let base = VertexContext {
        globals,
        locals: std::array::from_fn(|i| ((i as u32 * 1234567) ^ 0x5a5a5a5a) as i32),
        ..Default::default()
    };
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
    let operands = [
        0, 1, 0x3f, 0x40, 0x7f, 0x1234, 0x3fff, 0x4000, 0x7fff, 0x8000, 0xffff, 0x80, 0xbf, 0xc8,
        0xc9,
    ];
    let mut counts = [0; 2];
    for (i, line) in input.lines().enumerate() {
        match serde_json::from_str::<Vector>(line).unwrap() {
            Vector::Operand { encoded, value } => {
                assert_eq!(i, counts[0]);
                assert!(i < 65536);
                assert_eq!(usize::from(encoded), i);
                assert_eq!(
                    angle_operand(encoded as i16, &base),
                    value,
                    "encoded {encoded:#06x}"
                );
                counts[0] += 1;
            }
            Vector::Command {
                matrix,
                axis,
                encoded,
                hash,
            } => {
                let n = counts[1];
                assert_eq!(i, 65536 + n);
                assert!(n < 135);
                assert_eq!(
                    (matrix, axis, encoded),
                    (n / 45, (n / 15 % 3) as u8, operands[n % 15])
                );
                let mut context = VertexContext {
                    matrix: matrices[matrix],
                    globals,
                    ..Default::default()
                };
                context.locals[8] = -65537;
                context.locals[9] = i32::MAX;
                table
                    .rotate_command(
                        &mut context,
                        0x801c | (u16::from(axis) << 5),
                        encoded as i16,
                    )
                    .unwrap();
                let actual = context
                    .matrix
                    .into_iter()
                    .flat_map(i32::to_le_bytes)
                    .fold(14695981039346656037u64, |h, b| {
                        (h ^ u64::from(b)).wrapping_mul(1099511628211)
                    });
                assert_eq!(format!("{actual:016x}"), hash, "command {n}");
                counts[1] += 1;
            }
        }
    }
    assert_eq!(counts, [65536, 135]);
}
