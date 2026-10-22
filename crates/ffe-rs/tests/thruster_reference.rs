mod common;

use ffe_core::model::VertexContext;
use ffe_core::model_commands::Thruster;
use ffe_core::model_depth;
use ffe_core::model_execute::ThrusterEmission;
use ffe_core::model_instance::Vertex;
use ffe_core::model_texture::Descriptors;
use ffe_core::model_thruster::{self, Instance, Shape};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    case: u32,
    kind: u32,
    scale: i32,
    key_scale: i32,
    colour: [u8; 3],
    material: u16,
    width: i16,
    a: [i32; 5],
    b: [i32; 5],
    advance: u32,
    nested: Vec<[u32; 4]>,
    queue: Vec<[u32; 3]>,
}

fn vertex([x, y, vx, vy, vz]: [i32; 5]) -> Vertex {
    Vertex {
        object: [0; 3],
        view: [vx, vy, vz],
        screen: [x as i16, y as i16],
        stamp: 2,
    }
}

/// F1738 called directly on synthetic entries already stamped with the instance
/// generation (`--thruster-cases`), and F1704 on synthetic entry pairs: every key,
/// F1805 glow entry (points, texture word, ramp colours), F1704 call and F1803 line
/// entry, and every queue operation must match.
#[test]
#[ignore = "Local-only comparison; use tools/reference/verify-thrusters.py"]
fn executed_reference_thruster_cases() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_THRUSTER_CACHE").unwrap()).unwrap())
            .unwrap();
    let descriptors =
        Descriptors::from_bytes(cache.asset("base:texture/descriptors").unwrap()).unwrap();
    let input = std::fs::read_to_string(std::env::var_os("FFE_THRUSTER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 64 << 20);
    let (mut compared, mut mismatches) = (0usize, Vec::new());
    let (mut cores, mut shafts, mut lines, mut rejected, mut none) = (0, 0, 0, 0, 0);
    for row in input.lines() {
        let case: Case = serde_json::from_str(row).unwrap();
        let (a, b) = (vertex(case.a), vertex(case.b));
        let (mut nested, mut queue) = (Vec::new(), Vec::new());
        if case.kind == 0 {
            let thruster = Thruster {
                offset: 0,
                material: case.material,
                vertices: [0, 1],
                width: case.width,
            };
            let deep = a.view[2] > model_thruster::MIN_DEPTH;
            let queued = model_thruster::entries(
                &thruster,
                &a,
                deep.then_some(&b),
                Instance {
                    scale: case.scale,
                    key_scale: case.key_scale,
                    colour: case.colour,
                },
            )
            .unwrap();
            let base = model_thruster::base_colour(case.material, case.colour);
            let colour = model_thruster::line_colour(case.material, base, &descriptors).ok();
            let emission = ThrusterEmission {
                thruster,
                context: VertexContext::default(),
                screens: deep.then_some([a.screen, b.screen]),
                width: queued.width,
                entries: queued.entries,
            };
            common::thruster_events(&emission, colour, &mut nested);
            for entry in &emission.entries {
                match entry.shape {
                    Shape::Glow { texture, .. } => {
                        queue.push([7, 0, entry.key]);
                        if texture == model_thruster::CORE_TEXTURE {
                            cores += 1;
                        } else {
                            shafts += 1;
                        }
                    }
                    Shape::Line { .. } => {
                        queue.push([8, 0, entry.key]);
                        lines += 1;
                    }
                }
            }
            if emission.entries.is_empty() {
                none += 1;
            }
        } else {
            match model_thruster::line(&a, &b).unwrap() {
                Some(points) => {
                    let key = model_depth::key(a.view, case.key_scale);
                    common::line_events(key, points, &mut nested);
                    queue.push([8, 0, key]);
                    lines += 1;
                }
                None => rejected += 1,
            }
        }
        let mut theirs = case.nested.clone();
        common::mask_thruster_lines(&nested, &mut theirs);
        if nested != theirs || queue != case.queue || case.advance != 8 {
            if mismatches.len() < 10 {
                eprintln!(
                    "  case {}: rust {nested:?} {queue:?}\n    original {:?} {:?}",
                    case.case, case.nested, case.queue
                );
            }
            mismatches.push(case.case);
        }
        compared += 1;
    }
    eprintln!(
        "compared {compared} cases; {} mismatched; {cores} cores, {shafts} shafts, \
         {lines} lines queued, {rejected} direct lines rejected, {none} thrusters queued nothing",
        mismatches.len()
    );
    assert!(mismatches.is_empty(), "{} cases differ", mismatches.len());
    assert!(compared >= 12288, "only {compared} cases compared");
}
