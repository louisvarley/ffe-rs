//! Local-only check of the font models' per-character glyph programs.
//!
//! `FFE_GLYPH_EXE=game/FIRSTENC.EXE cargo test -p ffe-convert --test glyphs -- --ignored --nocapture`
use ffe_convert::embedded::{glyph_programs, GLYPH_FIRST_CHAR};

#[test]
#[ignore = "Local-only glyph program scan; needs the original executable"]
fn glyph_programs_decode_for_the_three_font_sets() {
    let exe = std::env::var_os("FFE_GLYPH_EXE").expect("FFE_GLYPH_EXE");
    let bytes = std::fs::read(exe).unwrap();
    for index in [0, 1, 2] {
        let programs = glyph_programs(&bytes, 0xd0cfc, index).unwrap();
        println!(
            "font set {index}: {} programs, {} bytes, {} vertices, scale {}",
            programs.starts.len(),
            programs.bytes.len(),
            programs.vertex_count,
            programs.scale
        );
        let mut total_faces = 0;
        let mut total_points = 0;
        for offset in 0..programs.starts.len() as u8 {
            let code = GLYPH_FIRST_CHAR + offset;
            let program = programs.program(code).unwrap();
            let decoded = ffe_core::model_commands::decode(program, programs.vertex_count);
            match &decoded {
                Ok(parsed) => {
                    total_faces += parsed.faces.len();
                    total_points += parsed.points.len();
                    if offset < 6 || code == b'A' || code == b'a' || code == b'~' {
                        println!(
                            "  char {code:?} ({} bytes): {} commands, {} faces, {} points",
                            program.len(),
                            parsed.commands.len(),
                            parsed.faces.len(),
                            parsed.points.len()
                        );
                    }
                }
                Err(error) => println!("  char {code:?} ERROR {error}"),
            }
        }
        println!("  totals: {total_faces} faces, {total_points} points");
    }
}
