//! The original's intro text writer: F1399/F1397/F1398 (expand and wrap) and
//! F1587/F1589 (draw with a shadow), for the raw SysMenu credit strings.
//!
//! The strings keep the original's layout codes: `0x0d` starts a new line,
//! `0xfd XX` sets x to `2 * XX`, `0xfe XX YY` sets x to `2 * XX` and y to `YY`,
//! and `0x01 XX` selects a colour. The 18 credits in `base:text/intro-credits`
//! use `0x0d`, `0xfd` and `0xfe`; the colour code is implemented for
//! completeness because the shared writer handles it.
//!
//! Wrap follows F1398's observable rule: when the next glyph would pass the
//! wrap width, the last space on the line becomes a newline (F1398 writes
//! `0x0d` there) and a word that never fits is broken at the width. Layout
//! codes reset the line start, as F1587's `0x1f` does.

use crate::front_end::IntroLine;
use crate::indexed::BitmapFont;

/// The raw credits and the character-width table from the cache (v27).
pub struct IntroText {
    strings: Vec<Vec<u8>>,
    widths: [i8; 256],
}

impl IntroText {
    pub fn new(cache: &ffe_content::VerifiedCache) -> Result<Self, String> {
        let credits = cache
            .asset("base:text/intro-credits")
            .ok_or("Cache lacks base:text/intro-credits (intro credits, cache v27).")?;
        let record = cache
            .manifest()
            .files
            .iter()
            .find(|record| record.id == "base:text/intro-credits")
            .ok_or("Cache lacks the intro credit descriptor.")?;
        let count = match record.asset {
            ffe_content::AssetKind::IntroStrings { count, .. } => count as usize,
            _ => return Err("Invalid intro credit descriptor.".to_owned()),
        };
        let mut strings: Vec<Vec<u8>> = credits
            .split(|byte| *byte == 0)
            .take(count)
            .map(<[u8]>::to_vec)
            .collect();
        strings.retain(|string| !string.is_empty());
        if strings.len() != count {
            return Err(format!(
                "The intro credits hold {} non-empty strings, not {count}.",
                strings.len()
            ));
        }
        let widths = cache
            .asset("base:text/char-widths")
            .ok_or("Cache lacks base:text/char-widths (character widths, cache v27).")?;
        if widths.len() != 256 {
            return Err("The character-width table is not 256 bytes.".to_owned());
        }
        let widths: [i8; 256] = std::array::from_fn(|index| widths[index] as i8);
        Ok(Self { strings, widths })
    }

    /// The raw bytes of one credit line. `IntroLine::string` indexes the
    /// original SysMenu table order (`0x941b..0x942c`).
    pub fn bytes(&self, line: &IntroLine) -> Result<&[u8], String> {
        self.strings
            .get(line.string)
            .map(Vec::as_slice)
            .ok_or_else(|| format!("Intro credit {} has no string.", line.string))
    }

    /// F1398: wrap a raw credit string to `width` pixels.
    pub fn wrap(&self, text: &[u8], width: i32) -> Vec<u8> {
        if width <= 0 {
            return text.to_vec();
        }
        let mut out = Vec::with_capacity(text.len());
        let mut x = 0i32;
        let mut last_space: Option<usize> = None;
        let mut index = 0;
        while index < text.len() {
            let byte = text[index];
            match byte {
                0x00 => break,
                // Colour: the operand is data, not a glyph.
                0x01 => {
                    out.push(byte);
                    if let Some(value) = text.get(index + 1) {
                        out.push(*value);
                    }
                    index += 2;
                }
                // Newline: F1587 adds the height and returns to the line start.
                0x0d => {
                    out.push(byte);
                    x = 0;
                    last_space = None;
                    index += 1;
                }
                // Set x (F1587's 0x1e, the expanded 0xfd).
                0xfd | 0x1e => {
                    out.push(byte);
                    let Some(value) = text.get(index + 1) else {
                        break;
                    };
                    out.push(*value);
                    x = i32::from(*value) * 2;
                    last_space = None;
                    index += 2;
                }
                // Set x and y (F1587's 0x1f, the expanded 0xfe).
                0xfe | 0x1f => {
                    out.push(byte);
                    let (Some(x_value), Some(y_value)) = (text.get(index + 1), text.get(index + 2))
                    else {
                        break;
                    };
                    out.push(*x_value);
                    out.push(*y_value);
                    x = i32::from(*x_value) * 2;
                    last_space = None;
                    index += 3;
                }
                // Already-inserted wrap markers.
                0x03 => {
                    out.push(byte);
                    x = 0;
                    last_space = None;
                    index += 1;
                }
                byte if byte >= 0x20 => {
                    let advance = i32::from(self.widths[usize::from(byte)]);
                    let advance = if advance <= 0 { 1 } else { advance };
                    if x > 0 && x + advance > width {
                        if let Some(space) = last_space {
                            out[space] = 0x0d;
                            x = self.measure(&out[space + 1..]);
                            last_space = None;
                        } else {
                            out.push(0x0d);
                            x = 0;
                        }
                    }
                    out.push(byte);
                    x += advance;
                    if byte == b' ' {
                        last_space = Some(out.len() - 1);
                    }
                    index += 1;
                }
                _ => {
                    out.push(byte);
                    index += 1;
                }
            }
        }
        out
    }

    /// F1587/F1589: draw one credit with a one-pixel black shadow, clipped to
    /// the 320x200 display. `x`/`y` are the call's start position.
    pub fn draw(
        &self,
        line: &IntroLine,
        font: &BitmapFont,
        palette: &[u32; 128],
        frame: &mut [u32],
    ) -> Result<(), String> {
        let text = self.wrap(self.bytes(line)?, line.wrap);
        let mut x = line.x;
        let mut y = line.y;
        let colour = u8::try_from(line.colour).unwrap_or(15);
        let mut index = 0;
        while index < text.len() {
            let byte = text[index];
            match byte {
                0x00 => break,
                0x01 => {
                    if let Some(value) = text.get(index + 1) {
                        // DATA_007776 is not imported; the credits' only colour
                        // code is absent, so keep the call's colour.
                        let _ = value;
                    }
                    index += 2;
                }
                0x0d => {
                    y += 10;
                    x = line.x;
                    index += 1;
                }
                0x03 => {
                    x = line.x;
                    index += 1;
                }
                0xfd | 0x1e => {
                    let Some(value) = text.get(index + 1) else {
                        break;
                    };
                    x = i32::from(*value) * 2;
                    index += 2;
                }
                0xfe | 0x1f => {
                    let (Some(x_value), Some(y_value)) = (text.get(index + 1), text.get(index + 2))
                    else {
                        break;
                    };
                    x = i32::from(*x_value) * 2;
                    y = i32::from(*y_value);
                    index += 3;
                }
                byte if byte >= 0x20 => {
                    if x < 316 {
                        if let Some(glyph) = font.glyph(byte) {
                            // F1589 draws the shadow first at (+1,+1), then the
                            // glyph, so the glyph wins where they overlap.
                            for (col, row) in glyph.pixels() {
                                let sx = x + col as i32 + 1;
                                let sy = y + row as i32 + 1;
                                if (0..320).contains(&sx) && (0..200).contains(&sy) {
                                    frame[sy as usize * 320 + sx as usize] = palette[0];
                                }
                            }
                            for (col, row) in glyph.pixels() {
                                let px = x + col as i32;
                                let py = y + row as i32;
                                if (0..320).contains(&px) && (0..200).contains(&py) {
                                    frame[py as usize * 320 + px as usize] =
                                        palette[usize::from(colour)];
                                }
                            }
                            x += glyph.advance as i32;
                        }
                    }
                    index += 1;
                }
                _ => index += 1,
            }
        }
        Ok(())
    }

    fn measure(&self, text: &[u8]) -> i32 {
        let mut x = 0;
        let mut index = 0;
        while index < text.len() {
            match text[index] {
                0x00 => break,
                0x01 => index += 2,
                0x0d | 0x03 => return 0,
                0xfd | 0x1e => {
                    if let Some(value) = text.get(index + 1) {
                        x = i32::from(*value) * 2;
                    }
                    index += 2;
                }
                0xfe | 0x1f => {
                    if let (Some(x_value), _) = (text.get(index + 1), text.get(index + 2)) {
                        x = i32::from(*x_value) * 2;
                    }
                    index += 3;
                }
                byte if byte >= 0x20 => {
                    let width = i32::from(self.widths[usize::from(byte)]).max(1);
                    x += width;
                    index += 1;
                }
                _ => index += 1,
            }
        }
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths(value: i8) -> [i8; 256] {
        std::array::from_fn(|_| value)
    }

    fn text(strings: Vec<&[u8]>, widths: [i8; 256]) -> IntroText {
        IntroText {
            strings: strings.into_iter().map(<[u8]>::to_vec).collect(),
            widths,
        }
    }

    fn font() -> BitmapFont {
        let mut bytes = vec![0u8; 1160];
        for glyph in bytes.chunks_exact_mut(10) {
            glyph[0] = 0x80;
            glyph[9] = 5;
        }
        BitmapFont::new(&bytes).unwrap()
    }

    fn line(string: usize, x: i32, y: i32, wrap: i32) -> IntroLine {
        IntroLine {
            code: 0x941b,
            string,
            first: 0,
            ends: None,
            colour: 15,
            x,
            y,
            wrap,
        }
    }

    #[test]
    fn wrap_breaks_at_the_last_space_and_keeps_the_width() {
        let intro = text(vec![b"aaaa bbbb cccc"], widths(4));
        let wrapped = intro.wrap(b"aaaa bbbb cccc", 20);
        // The spaces become newlines and no line exceeds 20 px.
        assert_eq!(wrapped, b"aaaa\x0dbbbb\x0dcccc");
        for part in wrapped.split(|byte| *byte == 0x0d) {
            assert!(intro.measure(part) <= 20, "{part:?} exceeds the wrap");
        }
    }

    #[test]
    fn a_word_longer_than_the_wrap_is_broken() {
        let intro = text(vec![b"abcdefgh"], widths(4));
        let wrapped = intro.wrap(b"abcdefgh", 12);
        assert_eq!(wrapped, b"abc\x0ddef\x0dgh");
        let joined: Vec<u8> = wrapped
            .iter()
            .copied()
            .filter(|byte| *byte != 0x0d)
            .collect();
        assert_eq!(joined, b"abcdefgh");
        for part in wrapped.split(|byte| *byte == 0x0d) {
            assert!(intro.measure(part) <= 12, "{part:?} exceeds the wrap");
        }
    }

    #[test]
    fn layout_codes_set_the_pen_and_draw_a_shadow() {
        // "A", then 0xfe 0x26 0x7e (x=76, y=126), then "B".
        let raw = b"A\xfe\x26\x7eB";
        let intro = text(vec![raw], widths(5));
        let mut frame = vec![0u32; 320 * 200];
        let palette: [u32; 128] = std::array::from_fn(|index| index as u32 + 1);
        intro
            .draw(&line(0, 10, 20, 0), &font(), &palette, &mut frame)
            .unwrap();
        // A at (10,20) and its shadow at (11,21); B at (76,126).
        assert_eq!(frame[20 * 320 + 10], palette[15]);
        assert_eq!(frame[21 * 320 + 11], palette[0]);
        assert_eq!(frame[126 * 320 + 76], palette[15]);
    }

    #[test]
    fn the_version_bytes_have_no_layout_codes() {
        let intro = text(vec![b"English Release 1.02 - 21/04/95"], widths(4));
        let wrapped = intro.wrap(intro.strings[0].as_slice(), 310);
        assert_eq!(wrapped, b"English Release 1.02 - 21/04/95");
    }
}
