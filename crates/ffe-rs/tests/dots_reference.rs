//! Local-only comparison of the particle rasteriser and the unlit point dots with the executed
//! original: `--particle` (F1610/F1611) and `--dots` (F1699, F1700, F1701, F1800, F1801).
//! Run through `tools/reference/verify-dots.py`.
use ffe_core::model::VertexContext;
use ffe_core::model_dot::{particle_clipped, point_dot, DotTables, Span};
use ffe_core::model_math::angle_operand;
use serde_json::Value;

fn int(value: &Value) -> i64 {
    value
        .as_i64()
        .unwrap_or_else(|| panic!("not an integer: {value}"))
}

fn fnv64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(14695981039346656037u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(1099511628211)
    })
}

fn plot(buffer: &mut [u8], span: &Span, value: u8) {
    for x in span.x..span.x + span.width {
        if (0..320).contains(&x) && (0..200).contains(&span.y) {
            buffer[span.y as usize * 320 + x as usize] = value;
        }
    }
}

#[test]
#[ignore = "Local-only particle comparison; use tools/reference/verify-dots.py"]
fn executed_reference_particles() {
    let text = std::fs::read_to_string(std::env::var_os("FFE_PARTICLE_VECTORS").unwrap()).unwrap();
    assert!(text.len() < 16 << 20);
    let (mut cases, mut drawn, mut large) = (0, 0, 0);
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let centre = case["centre"].as_array().unwrap();
        let (x, y) = (int(&centre[0]) as i16, int(&centre[1]) as i16);
        let size = int(&case["size"]) as i32;
        let mut buffer = vec![0xffu8; 320 * 200];
        for span in particle_clipped(x, y, size) {
            plot(&mut buffer, &span, 7);
        }
        let written = buffer.iter().filter(|b| **b != 0xff).count() as i64;
        assert_eq!(written, int(&case["written"]), "{line}");
        assert_eq!(
            format!("{:016x}", fnv64(&buffer)),
            case["pixels_fnv1a64"].as_str().unwrap(),
            "{line}"
        );
        cases += 1;
        drawn += usize::from(written > 0);
        large += usize::from(size >= 6);
    }
    eprintln!("{cases} particles, {drawn} drawn, {large} large");
    assert!(cases >= 6000 && drawn > 4000 && large > 2500);
}

fn fnv32_positions(start: u32, indices: impl Iterator<Item = usize>) -> u32 {
    indices.fold(start, |hash, i| (hash ^ i as u32).wrapping_mul(16_777_619))
}

#[test]
#[ignore = "Local-only dot comparison; use tools/reference/verify-dots.py"]
fn executed_reference_dots() {
    let cache =
        ffe_content::read(&std::fs::read(std::env::var_os("FFE_DOT_CACHE").unwrap()).unwrap())
            .unwrap();
    let tables = DotTables::new(cache.asset("base:world/dot-tables").unwrap()).unwrap();
    let text = std::fs::read_to_string(std::env::var_os("FFE_DOT_VECTORS").unwrap()).unwrap();
    assert!(text.len() < 16 << 20);
    let (mut cases, mut drawn, mut near, mut pairs, mut singles) = (0, 0, 0, 0, 0);
    for line in text.lines() {
        let case: Value = serde_json::from_str(line).unwrap();
        let screen = case["screen"].as_array().unwrap();
        let screen = [int(&screen[0]) as i16, int(&screen[1]) as i16];
        let instance = case["instance"].as_array().unwrap();
        let instance = [
            int(&instance[0]) as u8,
            int(&instance[1]) as u8,
            int(&instance[2]) as u8,
        ];
        let angle = angle_operand(int(&case["angle"]) as i16, &VertexContext::default());
        let dot = point_dot(
            int(&case["material"]) as u16,
            angle,
            int(&case["scale"]) as i32,
            instance,
            screen,
            [int(&case["view"][0]) as i32, int(&case["view"][1]) as i32],
            int(&case["depth"]) as i32,
            &tables,
            [0; 3],
        );
        cases += 1;
        let dot = match dot {
            Ok(dot) => dot,
            Err(_) => {
                // The near path (an ellipse) is not ported.
                near += 1;
                continue;
            }
        };
        let mut union = vec![0xffu8; 320 * 200];
        let mut core = vec![0xffu8; 320 * 200];
        if let Some(dot) = dot {
            match dot.shape {
                ffe_core::model_dot::DotShape::One { .. } => singles += 1,
                ffe_core::model_dot::DotShape::Two { .. } => pairs += 1,
            }
            let last = dot.spans().len();
            let core_from = match dot.shape {
                ffe_core::model_dot::DotShape::One { .. } => 0,
                ffe_core::model_dot::DotShape::Two { core_size, .. } => {
                    last - ffe_core::model_dot::particle_clipped(dot.x, dot.y, core_size).len()
                }
            };
            for (index, (span, _)) in dot.spans().iter().enumerate() {
                plot(&mut union, span, 1);
                if index >= core_from {
                    plot(&mut core, span, 1);
                }
            }
        }
        let written = union.iter().filter(|b| **b != 0xff).count() as i64;
        assert_eq!(written, int(&case["written"]), "{line}");
        let all = fnv32_positions(
            0x8422_2325,
            union
                .iter()
                .enumerate()
                .filter(|(_, b)| **b != 0xff)
                .map(|(i, _)| i),
        );
        assert_eq!(i64::from(all), int(&case["positions"]), "{line}");
        let core_positions = fnv32_positions(
            0,
            core.iter()
                .enumerate()
                .filter(|(_, b)| **b != 0xff)
                .map(|(i, _)| i),
        );
        let core_count = core.iter().filter(|b| **b != 0xff).count() as i64;
        let wanted_count = int(&case["centre_written"]);
        let wanted_hash = int(&case["centre_positions"]);
        let centre = (screen[1] as usize)
            .wrapping_mul(320)
            .wrapping_add(screen[0] as usize);
        let centre_drawn =
            (0..320).contains(&screen[0]) && (0..200).contains(&screen[1]) && union[centre] != 0xff;
        if !centre_drawn {
            assert_eq!((wanted_count, wanted_hash), (0, 0), "{line}");
        } else {
            // The pixels sharing the centre's colour are the core, or all of them when the halo
            // took the same palette entry.
            let all_hash = i64::from(fnv32_positions(
                0,
                union
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| **b != 0xff)
                    .map(|(i, _)| i),
            ));
            assert!(
                (wanted_count == core_count && wanted_hash == i64::from(core_positions))
                    || (wanted_count == written && wanted_hash == all_hash),
                "centre class differs: {line}"
            );
        }
        drawn += usize::from(written > 0);
    }
    eprintln!("{cases} points, {drawn} drawn, {near} near, {pairs} two-tone, {singles} single");
    assert!(cases >= 6000 && drawn > 3000 && pairs > 500 && singles > 500 && near < 1500);
}
