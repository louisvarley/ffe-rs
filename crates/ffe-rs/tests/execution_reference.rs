use ffe_core::model::{VertexContext, IDENTITY};
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::{execute, execute_live, InstanceContext};
use ffe_core::model_instance::Vertices;
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::Normal;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vector {
    program: usize,
    count: i32,
    angle: i32,
    depth: i32,
    flags: u8,
    faces: usize,
    emissions: String,
    locals: String,
    matrix: String,
    alive: u32,
}

fn hash(bytes: impl IntoIterator<Item = u8>) -> String {
    let value = bytes.into_iter().fold(14695981039346656037u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1099511628211)
    });
    format!("{value:016x}")
}

#[test]
#[ignore = "Local-only executed JJFFE dispatcher; use tools/reference/verify-execution.py"]
fn executed_reference_composed_command_programs() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_EXECUTION_CACHE").unwrap()).unwrap(),
    )
    .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_EXECUTION_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 512 * 1024);
    let words: [Vec<u16>; 3] = [
        vec![
            13, 0x00c8, 7, 0x0123, 0x0200, 0x0402, 0x1d, 0x01c0, 0xff14, 0x00c0, 0,
        ],
        vec![
            0x8b, 0x8002, 3, 0x123, 0x0200, 0x0402, 0x803c, 0xc9, 8, 0x234, 0x0200, 0x0604, 2, 12,
            31, 0x1dd, 0xc9c8, 3, 0x345, 0x0402, 0x0603, 0,
        ],
        vec![
            13, 0x00c8, 0x805c, 0xc9, 0x2ed, 0xc9c8, 0xb3, 0x01c0, 8, 0x456, 0x0200, 0x0604, 2, 12,
            0x8003, 3, 0x567, 0x0200, 0x0402, 0,
        ],
    ];
    let angles = [0, 1, 0x4000, -65537, i32::MAX];
    let depths = [0, 31, 32, 100, i32::MAX];
    let mut count = 0;
    for (i, line) in input.lines().enumerate() {
        let v: Vector = serde_json::from_str(line).unwrap();
        assert!(i < 900);
        assert_eq!(
            (v.program, v.count, v.angle, v.depth, v.flags),
            (
                i / 300,
                (i / 100 % 3 + 1) as i32,
                angles[i / 20 % 5],
                depths[i / 4 % 5],
                (i % 4) as u8
            )
        );
        assert_eq!(v.alive, 1);
        let bytes: Vec<_> = words[v.program]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        let normals: Vec<_> = (0..4)
            .map(|n| Normal {
                culled: n >= 2 && v.flags & (1 << (n - 2)) != 0,
                colour: [0; 3],
            })
            .collect();
        let inputs = VisibilityContext {
            depth: v.depth,
            scale: 0,
            detail: [0, 0],
            normals: &normals,
        };
        let mut context = VertexContext {
            matrix: IDENTITY,
            ..Default::default()
        };
        for n in 0..8 {
            context.locals[n] = (n * 257 + 7) as i32;
        }
        context.locals[8] = v.count;
        context.locals[9] = v.angle;
        let result = execute(&bytes, 8, &mut context, &inputs, Some(&table), 64).unwrap();
        assert_eq!(result.emissions.len(), v.faces, "case {i}");
        let mut emitted = Vec::new();
        for event in result.emissions {
            let face = event.face;
            let mut args = vec![u32::from(face.material), u32::from(face.normal)];
            args.extend(face.vertices.into_iter().map(u32::from));
            if args.len() == 5 {
                args.push(255);
            }
            emitted.extend(args.into_iter().flat_map(u32::to_le_bytes));
            emitted.extend(
                event.context.locals[..8]
                    .iter()
                    .flat_map(|x| x.to_le_bytes()),
            );
            emitted.extend(event.context.matrix.into_iter().flat_map(i32::to_le_bytes));
        }
        assert_eq!(hash(emitted), v.emissions, "emission order/state case {i}");
        assert_eq!(
            hash(context.locals[..8].iter().flat_map(|x| x.to_le_bytes())),
            v.locals,
            "locals case {i}"
        );
        assert_eq!(
            hash(context.matrix.into_iter().flat_map(i32::to_le_bytes)),
            v.matrix,
            "matrix case {i}"
        );
        count += 1;
    }
    assert_eq!(count, 900);
    for (index, offset) in [(74, 0xc), (75, 0x8)] {
        let bytes = cache
            .asset(&format!("base:model-commands/{index:03}"))
            .unwrap();
        let vertex_count = if index == 74 { 54 } else { 88 };
        let mut context = VertexContext::default();
        let before = context.clone();
        let inputs = VisibilityContext {
            depth: 100,
            scale: 0,
            detail: [0, 0],
            normals: &[],
        };
        let error = execute(
            bytes,
            vertex_count,
            &mut context,
            &inputs,
            Some(&table),
            4096,
        )
        .unwrap_err();
        assert_eq!(
            error,
            format!("Unsupported executed command 0x0006 at {offset:#x}.")
        );
        assert_eq!(context, before);
    }
}

#[test]
#[ignore = "Local-only real station streams; needs FFE_EXECUTION_CACHE"]
fn executed_reference_station_depth_composition_advances() {
    let cache = ffe_content::read(
        &std::fs::read(std::env::var_os("FFE_EXECUTION_CACHE").unwrap()).unwrap(),
    )
    .unwrap();
    let table = SineTable::from_bytes(cache.asset("base:math/sine").unwrap()).unwrap();
    // Offsets: first depth command, then the records each model stopped at before v0.14/v0.15.
    for (index, offset, earlier, vertex_count) in [
        (74u32, 0xcusize, &["0x000a at 0xf4"][..], 54u32),
        (75, 0x8, &["0x1c6e at 0x1c8", "0x001a at 0x234"][..], 88),
    ] {
        let bytes = cache
            .asset(&format!("base:model-commands/{index:03}"))
            .unwrap();
        let records = cache
            .asset(&format!("base:model-vertices/{index:03}"))
            .unwrap();
        let normals = vec![
            Normal {
                culled: false,
                colour: [0; 3]
            };
            32768
        ];
        let inputs = VisibilityContext {
            depth: 100,
            scale: 0,
            detail: [0, 0],
            normals: &normals,
        };
        let mut context = VertexContext::default();
        let before = context.clone();
        let mut vertices = Vertices::new(records, vertex_count).unwrap();
        let instance = InstanceContext {
            position: [0; 3],
            generation: 2,
            key_scale: 0,
            operand_scale: 0,
        };
        if let Err(error) = execute_live(
            bytes,
            &mut vertices,
            &mut context,
            &inputs,
            Some(&table),
            instance,
            4096,
        ) {
            eprintln!("model {index}: {error}");
            assert!(
                !error.contains(&format!("0x0006 at {offset:#x}")),
                "model {index} still stopped at opcode 6: {error}"
            );
            for stop in earlier {
                assert!(
                    !error.contains(stop),
                    "model {index} still stopped at an earlier record: {error}"
                );
            }
            assert_eq!(context, before, "model {index} mutated context on error");
        }
    }
}
