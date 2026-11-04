//! The ship library's issue page (`ffe.asm` F71/F78-F81): F78 copies each
//! shown article into one text buffer with the original's layout codes,
//! F1398 wraps it at 300 pixels, F80 draws the journal's masthead and F79
//! writes the text with F1585 (F1587/F1598) at (10, y) clipped to
//! `0x28..0x8c`, with F81 moving `y` (`DATA_008580`) two pixels a call while
//! a scroll is under way. See `docs/notes/journal.md`.
//!
//! The text keeps F1587's codes: `0x0d` a new line (back to x 10, ten
//! pixels down), `0x1e x` the pen at `2 * x`, `0x01 c` a colour, `0x02 n` a
//! picture and `0x03` F1398's break beside one. The archive's articles use
//! neither pictures nor colours (no `[Gr`), so the picture and colour codes
//! are carried but not drawn.

/// F79's text position and clip (F1585's x, ymin and ymax).
pub const TEXT_X: i32 = 10;
pub const TOP: i32 = 0x28;
pub const BOTTOM: i32 = 0x8c;
/// F1585's default line height.
pub const LINE: i32 = 10;
/// F78's F1398 width.
pub const WRAP: i32 = 0x12c;
/// F79's text colour.
pub const COLOUR: u8 = 0x0f;
/// F80's masthead row.
pub const MASTHEAD_Y: i32 = 2;
/// F80's up arrow (hot area 5) and F79's down arrow (hot area 6): bitmap
/// and place.
pub const UP_ARROW: (u32, i32, i32) = (0x2f, 0x88, 0x92);
pub const DOWN_ARROW: (u32, i32, i32) = (0x30, 0x7c, 0x92);
/// F79/F80's `0xa601` ("More ...") with either arrow.
pub const MORE: (u16, i32, i32) = (0xa601, 0x9a, 0x92);
/// F78's byline message, `0xa602` ("By:  ").
pub const BYLINE: u16 = 0xa602;

/// F1587 stops drawing glyphs once the pen reaches this x.
const PEN_LIMIT: i32 = 0x13c;
/// F1587 returns at a newline below this y.
const LAST_LINE: i32 = 0xbf;

/// One article as F78 reads it: the publication line (copied as it is) and
/// the rest, headline then body, already through F78's copy pass (`0x0e` to
/// `0x83`, `&` to `+`, `\n` to `0x0d`, `0xd5` to `'`, `0x7f` dropped), its
/// `[` macros expanded, without the last byte F78's count leaves out.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Source {
    pub publication: Vec<u8>,
    pub text: Vec<u8>,
}

/// F78's copy pass over an article's raw text after its publication line:
/// the character changes before F76 sees a `[`, and the count that stops
/// four bytes before the article's block ends (the block is the text and
/// its NUL, so the text's last byte is not copied).
pub fn copy_pass(raw: &[u8]) -> Vec<u8> {
    let kept = &raw[..raw.len().saturating_sub(1)];
    kept.iter()
        .filter(|byte| **byte != 0x7f)
        .map(|byte| match *byte {
            0x0e => 0x83,
            b'&' => b'+',
            b'\n' => 0x0d,
            0xd5 => b'\'',
            other => other,
        })
        .collect()
}

/// F78 for every article of the issue (F71's order), each wrapped by F1398
/// on its own: the headline upper-cased and centred with F1597's advances
/// (`advance`), the byline (`0x1e` and F84's `byline`, a space and the
/// publication line) under its right end, and five spaces after every new
/// line. `widths` is F1398's table (`DATA_007648`).
pub fn compose(
    articles: &[Source],
    byline: &[u8],
    advance: impl Fn(u8) -> i32,
    widths: &[i8; 256],
) -> Vec<u8> {
    let mut out = Vec::new();
    for article in articles {
        let start = out.len();
        out.extend([0x0d, 0x1e, 0x0a]);
        let mut first = true;
        for &byte in &article.text {
            out.push(byte);
            if byte != 0x0d {
                continue;
            }
            if first {
                first = false;
                let end = out.len() - 1;
                let mut width = 0i32;
                for byte in &mut out[start + 2..end] {
                    *byte = byte.to_ascii_uppercase();
                    width += advance(*byte);
                }
                let room = 0x140 - width;
                out[start + 2] = (room / 4) as u8;
                out.push(0x1e);
                out.push(((room / 2 + width - 0x28) / 2) as u8);
                out.extend_from_slice(byline);
                out.push(b' ');
                out.extend_from_slice(&article.publication);
                out.push(0x0d);
            }
            out.extend_from_slice(b"     ");
        }
        let mut segment = out.split_off(start);
        wrap(&mut segment, WRAP, widths, |_| (0, 0));
        out.extend(segment);
    }
    out
}

/// F71/F78: an issue's text for F79, each article composed and wrapped as
/// F78 does with F1597's advances (`advance`, none below 0x20 or from
/// 0xa0), F1398's width table (`base:text/char-widths`) and `0xa602`'s
/// byline. Empty without the width table.
pub fn issue_text(
    articles: &[crate::scripts::Article],
    messages: &ffe_core::message_strings::Messages<'_>,
    cache: &ffe_content::VerifiedCache,
    advance: impl Fn(u8) -> Option<i32>,
) -> Vec<u8> {
    let Some(widths) = cache
        .asset("base:text/char-widths")
        .filter(|table| table.len() == 256)
    else {
        return Vec::new();
    };
    let widths: [i8; 256] = std::array::from_fn(|index| widths[index] as i8);
    let byline = messages.get(BYLINE).unwrap_or_default();
    let sources: Vec<Source> = articles
        .iter()
        .map(|article| article.source.clone())
        .collect();
    compose(
        &sources,
        byline,
        |code| {
            if (0x20..0xa0).contains(&code) {
                advance(code).unwrap_or(0)
            } else {
                0
            }
        },
        &widths,
    )
}

/// F1398: wrap `text` in place at `width` pixels with `widths`
/// (`DATA_007648`; a negative width is a new line). An overflowing line
/// breaks at the last space before it (that space becomes `0x0d`, or `0x03`
/// beside a picture); with no space back to the start it breaks at the
/// first space after it. `picture` gives a picture's width and height.
pub fn wrap(
    text: &mut Vec<u8>,
    width: i32,
    widths: &[i8; 256],
    picture: impl Fn(u8) -> (i32, i32),
) {
    text.push(0);
    let at = |text: &[u8], i: usize| text.get(i).copied().unwrap_or(0);
    let mut i = 0usize;
    let mut room = width;
    let mut rows = 0i32;
    let mut beside = false;
    let mut saved = 0i32;
    let mut narrowed = 0i32;
    'text: loop {
        let byte = at(text, i);
        i += 1;
        if byte == 0 {
            break;
        }
        match byte {
            1 => {
                i += 1;
                continue;
            }
            2 => {
                let (w, h) = picture(at(text, i));
                i += 1;
                rows = h;
                saved = room;
                room -= w + 10;
                narrowed = room;
                beside = false;
                continue;
            }
            3 => continue,
            _ => {}
        }
        let advance = i32::from(widths[usize::from(byte)]);
        if advance < 0 {
            room = width;
            beside = true;
            if rows > 0 {
                rows -= 10;
            }
            continue;
        }
        room -= advance;
        if room <= saved && beside && rows > 0 {
            // Beside a picture: break at the last space with 0x03.
            loop {
                i -= 1;
                if text[i] == b' ' {
                    break;
                }
                if i == 0 {
                    break 'text;
                }
            }
            text[i] = 3;
            i += 1;
            room = narrowed;
            beside = false;
        }
        if room > 0 || byte == 0x0d {
            continue;
        }
        loop {
            i -= 1;
            if text[i] == b' ' {
                text[i] = 0x0d;
                i += 1;
                room = width;
                if rows > 0 {
                    rows -= 10;
                }
                beside = true;
                continue 'text;
            }
            if i == 0 {
                loop {
                    let byte = at(text, i);
                    i += 1;
                    if byte == b' ' || at(text, i) == 0 {
                        break;
                    }
                }
                if at(text, i) == 0 {
                    break 'text;
                }
            }
        }
    }
    text.pop();
}

/// One glyph F1587 hands to F1598.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Glyph {
    pub x: i32,
    pub y: i32,
    pub code: u8,
}

/// What F79 writes at `y`: the glyphs and F1598's `DATA_009221`, set when a
/// glyph reaches the bottom clip (more text below the page).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Page {
    pub glyphs: Vec<Glyph>,
    pub more_below: bool,
}

/// F1585/F1587 with F1598's clip flag: the text from (10, `y`) in
/// `LINE`-pixel lines, glyph advances from `advance` (`None` for a byte
/// without a glyph). Stops at a new line below y 0xbf or after a line that
/// reached the bottom clip.
pub fn layout(text: &[u8], y: i32, advance: impl Fn(u8) -> Option<i32>) -> Page {
    let mut page = Page::default();
    if y >= 0xc0 {
        return page;
    }
    let (mut x, mut y, mut start) = (TEXT_X, y, TEXT_X);
    let mut i = 0;
    while let Some(&byte) = text.get(i) {
        i += 1;
        match byte {
            0 => break,
            0x20.. => {
                if x >= PEN_LIMIT {
                    continue;
                }
                // F1598: nothing at or below the bottom; a glyph whose rows
                // reach it sets the flag.
                if BOTTOM <= y {
                    page.more_below = true;
                    continue;
                }
                if y + 9 >= BOTTOM {
                    page.more_below = true;
                }
                page.glyphs.push(Glyph { x, y, code: byte });
                x += advance(byte).unwrap_or(0);
            }
            0x01 | 0x02 => i += 1,
            0x0d => {
                y += LINE;
                if y > LAST_LINE || page.more_below {
                    break;
                }
                x = start;
            }
            0x1e => {
                x = i32::from(text.get(i).copied().unwrap_or(0)) * 2;
                i += 1;
            }
            0x1f => {
                x = i32::from(text.get(i).copied().unwrap_or(0)) * 2;
                y = i32::from(text.get(i + 1).copied().unwrap_or(0));
                start = x;
                i += 2;
            }
            _ => {}
        }
    }
    page
}

/// F81's scroll: the text's `y` (`DATA_008580`) and the pixels left to move
/// up (`DATA_002286`) or down (`DATA_002287`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scroll {
    pub y: i32,
    up: i32,
    down: i32,
}

impl Default for Scroll {
    fn default() -> Self {
        Self::open()
    }
}

/// F81's actions: the arrows' hot areas and the page steps.
pub const ACTION_LINE_UP: u8 = 5;
pub const ACTION_LINE_DOWN: u8 = 6;
pub const ACTION_PAGE_UP: u8 = 0x0e;
pub const ACTION_PAGE_DOWN: u8 = 0x0f;

impl Scroll {
    /// F81 opening a journal: the text at y 0x28.
    pub fn open() -> Self {
        Scroll {
            y: TOP,
            up: 0,
            down: 0,
        }
    }

    /// One F81 call. While a scroll is under way it moves the text two
    /// pixels (down only while F80's last page had more below,
    /// `more_below`) and the call's action is lost; otherwise the action
    /// starts one: a line (the arrows, 5 and 6) or a page of 60 pixels
    /// (0xe up, 0xf down), less the text's offset into its line.
    pub fn tick(&mut self, action: Option<u8>, more_below: bool) {
        if self.up != 0 {
            for _ in 0..2 {
                if self.y < TOP {
                    self.y += 1;
                } else {
                    self.up = 0;
                }
            }
            for _ in 0..2 {
                if self.up != 0 {
                    self.up -= 1;
                }
            }
            return;
        }
        if self.down != 0 {
            for _ in 0..2 {
                if more_below {
                    self.y -= 1;
                } else {
                    self.down = 0;
                }
            }
            for _ in 0..2 {
                if self.down != 0 {
                    self.down -= 1;
                }
            }
            return;
        }
        let offset = self.y % 10;
        match action {
            Some(ACTION_PAGE_DOWN) => (self.down, self.up) = (0x3c - offset, 0),
            Some(ACTION_PAGE_UP) => (self.up, self.down) = (0x3c - offset, 0),
            Some(ACTION_LINE_UP) => (self.up, self.down) = (0xa - offset, 0),
            Some(ACTION_LINE_DOWN) => (self.down, self.up) = (0xa - offset, 0),
            _ => {}
        }
    }

    /// F80: the up arrow while the text is scrolled.
    pub fn scrolled(&self) -> bool {
        self.y < TOP
    }

    pub fn busy(&self) -> bool {
        self.up != 0 || self.down != 0
    }
}

/// F81's hot areas 5 and 6: the arrow bitmaps' rectangles (`size` the
/// bitmap's width and height).
pub fn arrow_action(x: i32, y: i32, size: (i32, i32)) -> Option<u8> {
    [(UP_ARROW, ACTION_LINE_UP), (DOWN_ARROW, ACTION_LINE_DOWN)]
        .into_iter()
        .find(|((_, ax, ay), _)| {
            (*ax..*ax + size.0).contains(&x) && (*ay..*ay + size.1).contains(&y)
        })
        .map(|(_, action)| action)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widths() -> [i8; 256] {
        let mut table = [0i8; 256];
        table[0x20..0x80].fill(5);
        table[0x0d] = -1;
        table[0x1e] = -1;
        table
    }

    #[test]
    fn copy_pass_changes_characters_and_drops_the_last_byte() {
        assert_eq!(copy_pass(b"A&B\x7fC\x0e\xd5\n"), b"A+BC\x83'".to_vec());
        assert_eq!(copy_pass(b""), Vec::<u8>::new());
    }

    #[test]
    fn wrap_breaks_at_the_last_space() {
        // 5 pixels a character: 20 pixels hold four.
        let mut text = b"ab cd ef".to_vec();
        wrap(&mut text, 20, &widths(), |_| (0, 0));
        assert_eq!(text, b"ab\x0dcd\x0def".to_vec());
        // A new line resets the room.
        let mut text = b"abc\x0dabc".to_vec();
        wrap(&mut text, 20, &widths(), |_| (0, 0));
        assert_eq!(text, b"abc\x0dabc".to_vec());
        // A word too long for the line breaks at the next space.
        let mut text = b"abcdef gh".to_vec();
        wrap(&mut text, 20, &widths(), |_| (0, 0));
        assert_eq!(text, b"abcdef\x0dgh".to_vec());
    }

    #[test]
    fn compose_centres_the_headline_and_adds_the_byline() {
        let article = Source {
            publication: b"S.L.A.M".to_vec(),
            text: b"Big news\x0dBody".to_vec(),
        };
        let text = compose(
            &[article],
            b"By:  ",
            |c| if c >= 0x20 { 4 } else { 0 },
            &[0; 256],
        );
        // Eight characters of four pixels: 32 wide, centred at 144 (72 / 2).
        let mut expected = vec![0x0d, 0x1e, 72];
        expected.extend(b"BIG NEWS\x0d");
        expected.extend([0x1e, ((144 + 32 - 40) / 2) as u8]);
        expected.extend(b"By:   S.L.A.M\x0d     Body");
        assert_eq!(text, expected);
    }

    #[test]
    fn layout_flags_text_reaching_the_bottom() {
        let lines: Vec<u8> = (0..12).flat_map(|_| [b'x', 0x0d]).collect();
        let page = layout(&lines, TOP, |_| Some(5));
        assert!(page.more_below);
        // Lines at 40..130 are drawn; 131+ reaches 140 and stops the text.
        assert_eq!(page.glyphs.last().unwrap().y, 140 - 10);
        let short = layout(b"x\x0dx", TOP, |_| Some(5));
        assert!(!short.more_below);
        assert_eq!(
            short.glyphs,
            [
                Glyph {
                    x: 10,
                    y: 40,
                    code: b'x'
                },
                Glyph {
                    x: 10,
                    y: 50,
                    code: b'x'
                }
            ]
        );
        // 0x1e moves the pen; a new line returns to x 10.
        let moved = layout(b"\x1e\x20x\x0dy", TOP, |_| Some(5));
        assert_eq!(moved.glyphs[0].x, 64);
        assert_eq!(moved.glyphs[1].x, 10);
    }

    #[test]
    fn scroll_moves_two_pixels_a_call() {
        let mut scroll = Scroll::open();
        // Up at the top does nothing.
        scroll.tick(Some(ACTION_LINE_UP), true);
        scroll.tick(None, true);
        assert_eq!(scroll.y, TOP);
        // A line down: ten pixels in five calls, the action lost meanwhile.
        scroll.tick(Some(ACTION_LINE_DOWN), true);
        assert!(scroll.busy());
        for _ in 0..5 {
            scroll.tick(Some(ACTION_PAGE_DOWN), true);
        }
        assert_eq!(scroll.y, TOP - 10);
        assert!(!scroll.busy());
        // A page down stops when nothing is below.
        scroll.tick(Some(ACTION_PAGE_DOWN), true);
        scroll.tick(None, true);
        scroll.tick(None, false);
        assert_eq!(scroll.y, TOP - 12);
        assert!(!scroll.busy());
        // A page up from y 28 is 60 - 8 = 52 pixels, stopping at the top.
        scroll.tick(Some(ACTION_PAGE_UP), true);
        for _ in 0..40 {
            scroll.tick(None, true);
        }
        assert_eq!(scroll.y, TOP);
        assert!(!scroll.scrolled());
    }

    #[test]
    fn the_arrows_are_hot_areas_five_and_six() {
        assert_eq!(arrow_action(0x88, 0x92, (10, 10)), Some(ACTION_LINE_UP));
        assert_eq!(
            arrow_action(0x7c + 9, 0x92 + 9, (10, 10)),
            Some(ACTION_LINE_DOWN)
        );
        assert_eq!(arrow_action(0x7c + 10, 0x92, (10, 10)), None);
    }
}
