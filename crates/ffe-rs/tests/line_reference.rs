use ffe_core::model_line::draw_line;
use serde::Deserialize;

fn fnv(bytes: &[u8]) -> String {
    let value = bytes.iter().fold(14695981039346656037u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(1099511628211)
    });
    format!("{value:016x}")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raster {
    case: usize,
    colour: u8,
    points: [[i16; 2]; 2],
    written: usize,
    bounds: [usize; 4],
    pixels_fnv1a64: String,
}

/// Original F1616/F1601 (`--line-raster`): 4096 synthetic on-screen lines (random,
/// horizontal, vertical, short, near the one-pixel-per-row slope and at the view's
/// edges). The whole 320x200 buffer must match.
#[test]
#[ignore = "Local-only original line rasteriser; use tools/reference/verify-lines.py"]
fn executed_reference_line_raster() {
    let input =
        std::fs::read_to_string(std::env::var_os("FFE_LINE_RASTER_VECTORS").unwrap()).unwrap();
    assert!(input.len() < 4 << 20);
    let mut cases = 0;
    for line in input.lines() {
        let case: Raster = serde_json::from_str(line).unwrap();
        assert_eq!(case.case, cases);
        let mut buffer = vec![0xffu8; 320 * 200];
        draw_line(case.points[0], case.points[1], |x, y| {
            buffer[y as usize * 320 + x as usize] = case.colour
        });
        let mut bounds = [usize::MAX, usize::MAX, 0, 0];
        let mut written = 0;
        for (i, _) in buffer.iter().enumerate().filter(|(_, p)| **p != 0xff) {
            written += 1;
            let (x, y) = (i % 320, i / 320);
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
            "line raster case {} {:?}",
            case.case,
            case.points
        );
        cases += 1;
    }
    eprintln!("compared {cases} line raster cases, 0 mismatched");
    assert_eq!(cases, 4096);
}
