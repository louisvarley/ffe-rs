use ffe_core::model_fill::{fill_polygon, fill_quad, fill_triangle};
use serde::Deserialize;

fn fnv(bytes: &[u8]) -> String {
    let value = bytes.iter().fold(14695981039346656037u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(1099511628211)
    });
    format!("{value:016x}")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fill {
    case: usize,
    kind: usize,
    colour: u8,
    points: Vec<[i16; 2]>,
    written: usize,
    bounds: [i64; 4],
    pixels_fnv1a64: String,
}

/// Original F1604 and F1607/F1608 (`--fill-raster`): 4096 synthetic flat
/// triangles and quads (random, small, convex, edge values; triangles also off
/// the view), drawn 300 rows into a 320x800 buffer. The whole buffer must match.
#[test]
#[ignore = "Local-only original fill rasterisers; use tools/reference/verify-fills.py"]
fn executed_reference_fill_raster() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_FILL_RASTER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4 << 20);
    let mut cases = 0;
    for line in input.lines() {
        let case: Fill = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        assert_eq!(case.points.len(), case.kind);
        let mut buffer = vec![0xffu8; 320 * 800];
        let plot = |x: i32, y: i32| {
            let index = (y + 300) * 320 + x;
            assert!(
                (0..320 * 800).contains(&index),
                "case {} outside",
                case.case
            );
            buffer[index as usize] = case.colour
        };
        if case.kind == 3 {
            fill_triangle([case.points[0], case.points[1], case.points[2]], plot);
        } else {
            let p = &case.points;
            fill_quad([p[0], p[1], p[2], p[3]], plot);
        }
        let mut bounds = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
        let mut written = 0;
        for (i, _) in buffer.iter().enumerate().filter(|(_, p)| **p != 0xff) {
            written += 1;
            let (x, y) = ((i % 320) as i64, (i / 320) as i64 - 300);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
        }
        if written == 0 {
            bounds = [0; 4];
        }
        assert_eq!(
            (written, bounds, fnv(&buffer)),
            (case.written, case.bounds, case.pixels_fnv1a64.clone()),
            "fill raster case {} kind {} {:?}",
            case.case,
            case.kind,
            case.points
        );
        cases += 1;
    }
    eprintln!("compared {cases} fill raster cases, 0 mismatched");
    assert_eq!(cases, 4096);
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Span {
    case: usize,
    colour: u8,
    points: Vec<[i16; 2]>,
    written: usize,
    bounds: [i64; 4],
    pixels_fnv1a64: String,
}

/// Original F1468 + F1560 per edge + F1613 (`--span-raster`): 4096 synthetic
/// closed polygons of 3..12 points (on and off the view, self-crossing), drawn
/// 300 rows into a 320x800 buffer. The whole buffer must match.
#[test]
#[ignore = "Local-only original span rasteriser; use tools/reference/verify-fills.py"]
fn executed_reference_span_raster() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_SPAN_RASTER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4 << 20);
    let mut cases = 0;
    for line in input.lines() {
        let case: Span = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let mut buffer = vec![0xffu8; 320 * 800];
        let sides = case.points.len();
        let edges: Vec<[[i16; 2]; 2]> = (0..sides)
            .map(|i| [case.points[i], case.points[(i + 1) % sides]])
            .collect();
        fill_polygon(&edges, |x, y| {
            let index = (y + 300) * 320 + x;
            assert!(
                (0..320 * 800).contains(&index),
                "case {} outside",
                case.case
            );
            buffer[index as usize] = case.colour
        })
        .unwrap();
        let mut bounds = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
        let mut written = 0;
        for (i, _) in buffer.iter().enumerate().filter(|(_, p)| **p != 0xff) {
            written += 1;
            let (x, y) = ((i % 320) as i64, (i / 320) as i64 - 300);
            bounds = [
                bounds[0].min(x),
                bounds[1].min(y),
                bounds[2].max(x),
                bounds[3].max(y),
            ];
        }
        if written == 0 {
            bounds = [0; 4];
        }
        assert_eq!(
            (written, bounds, fnv(&buffer)),
            (case.written, case.bounds, case.pixels_fnv1a64.clone()),
            "span raster case {} {:?}",
            case.case,
            case.points
        );
        cases += 1;
    }
    eprintln!("compared {cases} span raster cases, 0 mismatched");
    assert_eq!(cases, 4096);
}
