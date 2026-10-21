use ffe_core::model::VertexContext;
use ffe_core::model_commands::VisibilityContext;
use ffe_core::model_execute::execute;
use ffe_core::model_normals::Normal;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    case: String,
    events: Vec<[u32; 4]>,
    lines: Vec<[u32; 3]>,
    alive: u32,
}

/// Same synthetic programs as the adapter's `--text-submodel` mode.
fn program(case: &str) -> &'static [u16] {
    match case.rsplit_once('-').unwrap().0 {
        "text" => &[10, 0x0123, 0x5a02, 0x8104, 0x1234, 2, 0x0456, 0x0201, 0],
        "submodel" => &[0x1c6e, 0x0305, 2, 0x0456, 0x0201, 0],
        "extended" => &[0x002e, 0x8306, 0x0201, 0x0403, 2, 0x0456, 0x0201, 0],
        "stopped" => &[0x002e, 0x8306, 0x7f01, 0x0403, 2, 0x0456, 0x0201, 0],
        other => panic!("unknown case {other}"),
    }
}

/// Rust operand events rendered as the probe's recorded nested calls.
fn expected(case: &str) -> (Vec<[u32; 4]>, Vec<[u32; 3]>) {
    let culled = case.ends_with("-culled");
    let normals: Vec<Normal> = (0..4)
        .map(|index| Normal {
            culled: culled && index == 2,
            colour: [0; 3],
        })
        .collect();
    let visibility = VisibilityContext {
        depth: 100,
        scale: 0,
        detail: [0, 0],
        normals: &normals,
    };
    let stream: Vec<u8> = program(case).iter().flat_map(|w| w.to_le_bytes()).collect();
    let result = execute(
        &stream,
        8,
        &mut VertexContext::default(),
        &visibility,
        None,
        16,
    )
    .unwrap();
    let mut events = Vec::new();
    for text in &result.texts {
        if text.culled {
            continue;
        }
        let text = &text.text;
        events.push([1, text.vertex.into(), 0, 0]);
        events.push([2, text.glyphs.into(), 0, 0]);
        events.push([3, text.frame.into(), 0, 0]);
        // The probe's synthetic glyph descriptor has no vertices or normals.
        events.push([4, 0, 0, 4]);
        events.push([5, text.string.into(), 0, 0]);
        events.push([6, 0, 0, 0]);
    }
    for call in &result.submodels {
        let call = &call.submodel;
        events.push([7, call.vertex.into(), 0, 0]);
        events.push([8, call.model.into(), 0, 0]);
        // Child culling is not evaluated in Rust; the probe case name supplies it.
        if !culled {
            events.extend(
                call.extra_vertices()
                    .into_iter()
                    .map(|v| [7, v.into(), 0, 0]),
            );
            events.push([9, call.frame.into(), 0, 0]);
            events.push([10, 0, 0, 0]);
        }
    }
    let lines = result
        .lines
        .iter()
        .map(|line| {
            [
                line.material.into(),
                line.vertices[0].into(),
                line.vertices[1].into(),
            ]
        })
        .collect();
    (events, lines)
}

#[test]
#[ignore = "Local-only text/submodel handler operands; use tools/reference/verify-text-submodel.py"]
fn executed_reference_text_submodel_operands() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_TEXT_SUBMODEL_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 8192);
    let cases: Vec<Case> = input
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let names: Vec<&str> = cases.iter().map(|c| c.case.as_str()).collect();
    assert_eq!(
        names,
        [
            "text-visible",
            "text-culled",
            "submodel-visible",
            "submodel-culled",
            "extended-visible",
            "extended-culled",
            "stopped-visible"
        ]
    );
    for case in &cases {
        let (events, lines) = expected(&case.case);
        assert_eq!(case.events, events, "{}", case.case);
        assert_eq!(case.lines, lines, "{}", case.case);
        assert_eq!(case.alive, 1, "{}", case.case);
    }
}
