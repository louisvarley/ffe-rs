use ffe_core::model::VertexContext;
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::execute;
use ffe_core::model_normals::Normal;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    program: usize,
    instance: usize,
    local: i32,
    colour: [u8; 3],
    normal0: [u8; 3],
    lines: Vec<[u32; 3]>,
    alive: u32,
}

/// Same synthetic programs and inputs as the adapter's `--colour` mode.
const PROGRAMS: [&[u16]; 7] = [
    &[26, 0x0123, 0x0002, 2, 0x0456, 0x0201, 0],
    &[26, 0x1abc, 0x0001, 2, 0x0456, 0x0201, 0],
    &[26, 0x3abc, 0x0003, 2, 0x0456, 0x0201, 0],
    &[26, 0x5005, 0x0002, 2, 0x0456, 0x0201, 0],
    &[
        26, 0x1111, 0xc802, 0x0222, 0x0333, 0x0444, 0x0555, 0x0666, 0x0777, 0x1888, 2, 0x0456,
        0x0201, 0,
    ],
    &[26, 0x1123, 0x0082, 26, 0x0010, 0x0000, 2, 0x0456, 0x0201, 0],
    &[26, 0x1fff, 0x0007, 2, 0x0456, 0x0201, 0],
];
const COLOURS: [[u8; 3]; 2] = [[0, 0, 0], [5, 9, 250]];

#[test]
#[ignore = "Local-only colour command results; use tools/reference/verify-colour.py"]
fn executed_reference_colour_commands() {
    let input = std::fs::read_to_string(std::env::var_os("FFE_COLOUR_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 65536);
    let cases: Vec<Case> = input
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 32);
    let normals: Vec<Normal> = (0..8u8)
        .map(|i| Normal {
            culled: false,
            colour: [i.wrapping_mul(40).wrapping_add(7), i * 3, 255 - i],
        })
        .collect();
    let visibility = VisibilityContext {
        depth: 100,
        scale: 0,
        detail: [0, 0],
        normals: &normals,
    };
    let mut expected_order = Vec::new();
    for program in 0..7 {
        for instance in 0..2 {
            for local in 0..if program == 4 { 10 } else { 1 } {
                expected_order.push((program, instance, local));
            }
        }
    }
    for (case, (program, instance, local)) in cases.iter().zip(expected_order) {
        assert_eq!(
            (case.program, case.instance, case.local),
            (program, instance, local)
        );
        let stream: Vec<u8> = PROGRAMS[program]
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .collect();
        let mut context = VertexContext::default();
        context.locals[8] = local;
        context.colour = COLOURS[instance];
        let result = execute(&stream, 8, &mut context, &visibility, None, 16).unwrap();
        let label = format!("program {program} instance {instance} local {local}");
        assert_eq!(context.colour, case.colour, "{label}");
        assert_eq!(
            result.normal_zero_colour.unwrap_or(normals[0].colour),
            case.normal0,
            "{label}"
        );
        let lines: Vec<[u32; 3]> = result
            .lines
            .iter()
            .map(|l| {
                [
                    l.material.into(),
                    l.vertices[0].into(),
                    l.vertices[1].into(),
                ]
            })
            .collect();
        assert_eq!(lines, case.lines, "{label}");
        assert_eq!(case.alive, 1, "{label}");
    }
}
