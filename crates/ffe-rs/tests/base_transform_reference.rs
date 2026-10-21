use ffe_core::model::VertexContext;
use ffe_core::model_math::SineTable;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    base: usize,
    hand: usize,
    code: u16,
    operand: u16,
    advance: i32,
    matrix: [i32; 9],
    handedness: u32,
}

/// Same inputs as the adapter's `--base-transform` mode.
const BASES: [[i32; 9]; 2] = [
    [i32::MAX, 0, 0, 0, i32::MAX, 0, 0, 0, i32::MAX],
    [
        123456789,
        -987654321,
        5,
        -6,
        7,
        i32::MIN,
        i32::MAX,
        -1,
        65536,
    ],
];
const HANDS: [u32; 3] = [0, 0xffff_ffff, 0x1234_5678];
const OPERANDS: [u16; 3] = [0, 0x1234, 0xffff];

#[test]
#[ignore = "Local-only F1741 base-relative transforms; use tools/reference/verify-base-transform.py"]
fn executed_reference_base_transforms() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_BASE_TRANSFORM_CACHE").unwrap()).unwrap(),
    )
    .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_BASE_TRANSFORM_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4 << 20);
    let cases: Vec<Case> = input
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 6048);
    let mut expected = Vec::new();
    for base in 0..2 {
        for hand in 0..3 {
            for form in 0..2u16 {
                for sel in 0..7u16 {
                    for neg in 0..8u16 {
                        for axis in 0..3u16 {
                            for operand in OPERANDS {
                                let code = form << 14 | neg << 10 | sel << 7 | axis << 5 | 28;
                                expected.push((base, hand, code, operand));
                            }
                        }
                    }
                }
            }
        }
    }
    for (case, (base, hand, code, operand)) in cases.iter().zip(expected) {
        assert_eq!(
            (case.base, case.hand, case.code, case.operand),
            (base, hand, code, operand)
        );
        assert_eq!(case.advance, 2, "record is 4 bytes");
        let mut context = VertexContext {
            basis: std::array::from_fn(|i| (0xdead_0000u32 + i as u32) as i32),
            matrix: BASES[base],
            base_handedness: HANDS[hand],
            ..Default::default()
        };
        for i in 0..8 {
            context.locals[i] = (i as u32 * 0x0101_0101) as i32;
        }
        context.globals = std::array::from_fn(|i| (i * 257 + 19) as u16);
        table
            .rotate_command(&mut context, code, operand as i16)
            .unwrap();
        let label = format!("code {code:#06x} operand {operand:#06x} base {base} hand {hand}");
        assert_eq!(context.basis, case.matrix, "{label}");
        assert_eq!(context.handedness, case.handedness, "{label}");
    }
}
