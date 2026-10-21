use ffe_core::model::VertexContext;
use ffe_core::model_commands::{arithmetic, bit_branch, decode, material};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Vector {
    Math {
        op: u16,
        a: i32,
        b: i32,
        destination: u16,
        result: u16,
    },
    Branch {
        code: u16,
        a: i32,
        bit: u8,
        relative: i32,
    },
    Material {
        hash: String,
    },
    Face {
        code: u16,
        seed: u8,
        faces: Vec<[u32; 6]>,
    },
}

#[test]
#[ignore = "Local-only executed JJFFE command comparison; use tools/reference/verify-commands.py"]
fn executed_reference_command_arithmetic_branches_materials_and_faces() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_COMMAND_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 256 * 1024);
    let mut counts = [0; 4];
    let values = [i32::MIN, -65536, -1, 0, 1, 31, 32, 63, 65535, i32::MAX];
    let mut expected = Vec::new();
    for op in 0u16..16 {
        if matches!(op, 13 | 14) {
            continue;
        }
        for (ai, a) in values.into_iter().enumerate() {
            for (bi, b) in values.into_iter().enumerate() {
                if (op == 3 && a == i32::MIN && b == -1)
                    || (op == 8 && b == 0)
                    || (op == 10 && (a as u32).wrapping_add(b as u32) > 63)
                {
                    continue;
                }
                expected.push((op, a, b, ((ai + bi) % 8) as u16));
            }
        }
    }
    for line in input.lines() {
        match serde_json::from_str::<Vector>(line).unwrap() {
            Vector::Math {
                op,
                a,
                b,
                destination,
                result,
            } => {
                assert_eq!(counts[1..], [0, 0, 0]);
                assert_eq!((op, a, b, destination), expected[counts[0]]);
                let mut context = VertexContext::default();
                context.locals[8] = a;
                context.locals[9] = b;
                context.globals = std::array::from_fn(|i| (i * 257 + 19) as u16);
                let before = context.locals;
                let value = arithmetic(destination << 8 | op << 4 | 13, [0xc8, 0xc9], &mut context)
                    .unwrap();
                assert_eq!(value, result, "op {op}, a {a}, b {b}");
                for (i, previous) in before.into_iter().enumerate() {
                    assert_eq!(
                        context.locals[i],
                        if i == usize::from(destination) {
                            i32::from(result)
                        } else {
                            previous
                        }
                    );
                }
                counts[0] += 1;
            }
            Vector::Branch {
                code,
                a,
                bit,
                relative,
            } => {
                assert_eq!(counts[0], expected.len());
                assert_eq!(counts[2..], [0, 0]);
                let i = counts[1];
                assert_eq!(code, [19, 20, 0x93, 0x94, 0xffd3, 0xffd4][i / 60]);
                assert_eq!(a, values[i / 6 % 10]);
                assert_eq!(bit, [0, 1, 2, 32, 33, 255][i % 6]);
                let mut context = VertexContext::default();
                context.locals[8] = a;
                assert_eq!(
                    bit_branch(code, [0xc8, bit], &context)
                        .unwrap()
                        .unwrap_or(-999),
                    relative
                );
                counts[1] += 1;
            }
            Vector::Material { hash } => {
                assert_eq!(counts[1], 360);
                assert_eq!(counts[2..], [0, 0]);
                let bytes = (0..=u16::MAX).flat_map(|i| {
                    let m = material(i);
                    let mut data = [0u8; 16];
                    data[..3].copy_from_slice(&m.rgb);
                    data[4..8].copy_from_slice(&u32::from(m.texture).to_le_bytes());
                    data[8..12].copy_from_slice(&(u32::from(m.unlit) * 0x2000).to_le_bytes());
                    data[12..16]
                        .copy_from_slice(&(u32::from(m.local_colour) * 0x1000).to_le_bytes());
                    data
                });
                let actual = bytes.fold(14695981039346656037u64, |hash, b| {
                    (hash ^ u64::from(b)).wrapping_mul(1099511628211)
                });
                assert_eq!(format!("{actual:016x}"), hash);
                counts[2] += 1;
            }
            Vector::Face { code, seed, faces } => {
                assert_eq!(counts[2], 1);
                assert_eq!(code, [3, 4, 7, 8][counts[3] / 8]);
                assert_eq!(usize::from(seed), counts[3] % 8);
                let mut bytes = code.to_le_bytes().to_vec();
                bytes.extend_from_slice(&(0x1023 + u16::from(seed) * 0x202).to_le_bytes());
                bytes.extend_from_slice(&[seed * 2, seed * 2 + 2, seed * 2 + 4, seed * 2 + 6]);
                if matches!(code, 4 | 8) {
                    bytes.extend_from_slice(&[seed * 2, 0]);
                }
                bytes.extend_from_slice(&[0, 0]);
                let program = decode(&bytes, 128).unwrap();
                assert_eq!(program.faces.len(), faces.len());
                for (face, expected) in program.faces.iter().zip(faces) {
                    let mut actual = [
                        u32::from(face.material),
                        u32::from(face.normal),
                        255,
                        255,
                        255,
                        255,
                    ];
                    for (i, vertex) in face.vertices.iter().enumerate() {
                        actual[i + 2] = u32::from(*vertex);
                    }
                    assert_eq!(actual, expected, "code {code}, seed {seed}");
                }
                counts[3] += 1;
            }
        }
    }
    assert_eq!(counts, [expected.len(), 360, 1, 32]);
}
