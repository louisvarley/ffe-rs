use ffe_core::model::VertexContext;
use ffe_core::model_commands::{self, VisibilityContext};
use ffe_core::model_depth::{decode, Context};
use ffe_core::model_execute::{execute_live, InstanceContext, Primitive};
use ffe_core::model_instance::Vertices;
use ffe_core::model_order::Order;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DepthVector {
    case: usize,
    scale: i32,
    shift: i32,
    point_set: usize,
    code: u16,
    initial: usize,
    consumed: usize,
    active: u32,
    words: usize,
    key: Option<u32>,
}

#[test]
#[ignore = "Local-only original depth commands; use tools/reference/verify-order.py"]
fn executed_reference_depth_commands_and_group_changes() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_DEPTH_COMMAND_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4 * 1024 * 1024);
    let scales = [
        i32::MIN,
        -33,
        -29,
        -1,
        0,
        3,
        4,
        31,
        32,
        34,
        35,
        36,
        i32::MAX,
    ];
    let shifts = [i32::MIN, -33, -1, 0, 1, 15, 31, 32, 33, i32::MAX];
    let codes = [
        0x0006u16, 0x1fe6, 0x2046, 0x2066, 0x4046, 0x6046, 0x6066, 0x8006, 0xa006, 0xc006, 0xe046,
        0xe066, 0xffa6, 0xffc6, 0xffe6,
    ];
    let depths = [1, 0x40000000, i32::MIN];
    let mut count = 0;
    for (i, line) in input.lines().enumerate() {
        assert!(i < 11700);
        let v: DepthVector = serde_json::from_str(line).unwrap();
        assert_eq!(v.case, i);
        assert_eq!(v.scale, scales[i / 900]);
        assert_eq!(v.shift, shifts[i / 90 % 10]);
        assert_eq!(v.point_set, i / 30 % 3);
        assert_eq!(v.code, codes[i / 2 % 15]);
        assert_eq!(v.initial, i % 2);
        let vertices: Vec<_> = (0..8)
            .map(|index| {
                [
                    if index & 1 != 0 {
                        i32::MIN
                    } else {
                        index as i32 * 8
                    },
                    -(index as i32) * 65536,
                    depths[(v.point_set + index) % 3],
                ]
            })
            .collect();
        let context = Context {
            vertices: &vertices,
            position: [i32::MIN, 65536, depths[v.point_set]],
            key_scale: v.scale,
            operand_scale: v.shift,
        };
        let mut bytes = v.code.to_le_bytes().to_vec();
        bytes.extend([4, 255, 6, 0]);
        let (command, size) = decode(&bytes).unwrap();
        let key = command.resolve(&context).unwrap();
        assert_eq!(v.consumed, size - 2, "consumption case {i}");
        assert_eq!(v.active, u32::from(key.is_some()), "group case {i}");
        assert_eq!(v.key, key, "key case {i}");
        assert_eq!(
            v.words,
            v.initial * 4 + usize::from(key.is_some()) * 3,
            "allocator case {i}"
        );
        bytes.truncate(size);
        bytes.extend([0, 0]);
        assert_eq!(
            model_commands::decode(&bytes, 8).unwrap().commands[0].size,
            size
        );
        count += 1;
    }
    assert_eq!(count, 11700);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrderVector {
    seed: u32,
    active: u32,
    state: u32,
    order: Vec<u32>,
}

#[test]
#[ignore = "Local-only original group sorting; use tools/reference/verify-order.py"]
fn executed_reference_depth_groups_and_sort_order() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_ORDER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 1024 * 1024);
    let keys = [0, 1, 0x7fffffff, 0x80000000, u32::MAX, 7];
    let mut count = 0;
    for (i, line) in input.lines().enumerate() {
        let v: OrderVector = serde_json::from_str(line).unwrap();
        assert!(i < 128);
        assert_eq!(v.seed, i as u32 + 1);
        assert_eq!(v.active, 0);
        assert_eq!(v.state, 0);
        let mut order = Order::new(65).unwrap();
        let mut state = v.seed;
        for id in 0..64 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            let kind = state >> 16 & 3;
            let key = if v.seed % 4 == 0 {
                7
            } else {
                keys[(state >> 24) as usize % 6]
            };
            match kind {
                0 => order.group(None).unwrap(),
                1 => order.group(Some(key)).unwrap(),
                _ => order.push(key, id).unwrap(),
            }
        }
        order.group(None).unwrap();
        assert_eq!(order.finish().unwrap(), v.order, "order seed {}", v.seed);
        count += 1;
    }
    assert_eq!(count, 128);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ComposedOrderVector {
    order: Vec<u32>,
    keys: Vec<u32>,
}

#[test]
#[ignore = "Local-only composed depth/order check; use tools/reference/verify-order.py"]
fn executed_reference_composed_order() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_COMPOSED_ORDER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4096);
    let v: ComposedOrderVector = serde_json::from_str(input.trim()).unwrap();

    fn triangle(material: u16, [a, b, c]: [u8; 3]) -> [u16; 4] {
        [
            3,
            material,
            u16::from_le_bytes([b, a]),
            u16::from_le_bytes([0, c]),
        ]
    }

    let mut words = Vec::new();
    words.push(0x4006); // depth vertex 0 -> group key 0
    words.extend_from_slice(&triangle(1, [1, 0, 2]));
    words.push(0x4046); // depth vertex 2 -> group key 9
    words.extend_from_slice(&triangle(2, [3, 2, 0]));
    words.push(0x0006); // close
    words.extend_from_slice(&triangle(3, [2, 3, 1])); // standalone key 9
    words.push(0);
    let stream: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();
    let records: [u8; 12] = [1, 0, 0, 0, 8, 0, 1, 0, 0, 0, 80, 0];
    let mut vertices = Vertices::new(&records, 4).unwrap();
    let mut context = VertexContext::default();
    let visibility = VisibilityContext {
        depth: 100,
        scale: 0,
        detail: [0, 0],
        // Every face uses normal 0 and keeps at least one vertex on screen, so the
        // F1708 queueing rule admits all three marker faces.
        normals: &[ffe_core::model_normals::Normal {
            culled: false,
            colour: [0; 3],
        }],
    };
    let instance = InstanceContext {
        position: [0; 3],
        generation: 2,
        key_scale: 0,
        operand_scale: 0,
        textures: true,
    };
    let result = execute_live(
        &stream,
        &mut vertices,
        &mut context,
        &visibility,
        None,
        instance,
        32,
    )
    .unwrap();
    fn face_material(primitive: &Primitive) -> u32 {
        match primitive {
            Primitive::Face(emission) => u32::from(emission.face.material),
            other => panic!("expected a face primitive, got {other:?}"),
        }
    }

    let materials: Vec<u32> = result.ordered.iter().map(face_material).collect();
    assert_eq!(materials, v.order);
    assert_eq!(result.ordered_keys, v.keys);
}
