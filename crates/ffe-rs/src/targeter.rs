//! The combat targeter: `FUNC_001113`/`FUNC_001114`'s targeting tunnels and
//! `FUNC_001111`'s auto-targetter cursor (`ffe.asm:81554-82200`).
//!
//! `F1100` draws four nested squares from the view centre toward the locked
//! target while `DATA_009024` ("Show targeting tunnels") is set, the target's
//! screen words are inside ±1000, and — with `DATA_009025` off — the target has
//! no statistics block (i.e. is not a ship). `F1113` places each square on the
//! line from (160,79) to the target, and `F1114` draws its four edges clipped to
//! the 320x158 view.
//!
//! `F1100` also owns the range text: its value is in hundredths, the string's
//! FF30 index 7 writes two decimals, and the on-view suffix has no leading
//! space (`5.40AU`). [`range_text`] builds the exact on-view, Behind, Out of
//! Range and arrow forms.
//!
//! `F1111` (the gunsight routine) draws a blinking cursor while one of the
//! original's auto-targetter flags is set: `DATA_007246` is the navigation
//! targetter (a 4-half-size box at (160,79), colour 0x11) and `DATA_007247` the
//! combat targetter (a circle of radius 6 at (159,79), colour 0x10). Both blink
//! with `DATA_007252`, which is the parity of the game second (`DATA_008799 /
//! 0xc22e`).
//!
//! `F1054`'s `T` (action 0x74) and `Shift+T` toggle the two flags, but only
//! with the Auto targetter fitted (`ship+0xc8 & 0x80`, record 0x17,
//! `ffe.asm:78737-78768`). While a flag is set, `F1010` calls `F1062`/`F1064`
//! every frame: `F1061`/`F1063` walk the live objects in F989's order and pick
//! the first whose screen position is within a six-pixel Manhattan distance of
//! the view centre; the acquisition writes the target (`player+0xec` /
//! `player+0xed`) and disarms. The targeters also skip the current target and
//! the one captured when they were armed (`DATA_007248`/`DATA_007249`).
//!
//! The recovered listing has no call to `F1010`; the port runs the acquisition
//! from its flight step so the armed cursor is not inert (`docs/notes/combat.md`
//! §8.4).

/// F1100/F1111's targetter colours: navigation 0x11, combat 0x10.
///
/// These are COLPAL palette indices, not F1326 colour words: F1587 uses the
/// value directly as a video index. The fixed low palette (`F1641`'s upload of
/// `DATA_007769`) makes 0x11 green `(0,60,0)` six-bit and 0x10 red
/// `(60,0,0)`. `F1327` reaches the same entries through the fixed map
/// (`0x0f0` -> 17, `0xf00` -> 16); the dynamic upper half is not involved.
pub const NAVIGATION_COLOUR: u8 = 0x11;
pub const COMBAT_COLOUR: u8 = 0x10;

/// The missile view's combat-target colour (`F1099` adds 0x4f to 0x10 when
/// `ship+0xea == 5`). The port has no missile view yet; kept so a caller does
/// not have to invent the value.
pub const MISSILE_VIEW_COLOUR: u8 = 0x5f;

/// F1100's three fixed text forms (besides the arrow forms).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeForm {
    /// The on-view number: F1100's 0x8604/05. The original string starts with
    /// the (ignored) 0x81 marker and a newline, so the text sits 10 rows below
    /// the target's `-5` origin.
    OnView,
    /// 0x8606/07: `Target <n> km/AU Behind`.
    Behind,
    /// 0x8608/09: `Target Out of Range (<n> km/AU)`.
    OutOfRange,
    /// F1100's off-view arrow forms (0x860a..0x8611).
    Arrow(RangeArrow),
}

/// The four off-view arrow strings. The bytes are the original font codes:
/// 0x7d/0x7e are the left/right arrows, 0x7f/0x80 their below-target variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeArrow {
    /// 0x860a/b: `\x7d Target <n> km/AU` (target above/level, left).
    Left,
    /// 0x860c/d: `Target <n> km/AU \x7e` (above/level, right).
    Right,
    /// 0x860e/f: `\x7f Target <n> km/AU` (target below, left).
    BelowLeft,
    /// 0x8610/11: `Target <n> km/AU \x80` (target below, right).
    BelowRight,
}

/// The COLPAL index for the navigation target's green. The fixed map's
/// `0x0f0` word selects the same low entry (`DATA_007770[0x0f0] == 17`), so the
/// colour is the imported palette, not a hard-coded RGB.
pub const NAVIGATION_WORD: u16 = 0x0f0;
/// The fixed-map word for the combat target's low entry 16.
pub const COMBAT_WORD: u16 = 0xf00;

/// F1111's cursor box origin (0xa0, 0x4f) and half size.
pub const CURSOR_BOX: ((i32, i32), i32) = ((0xa0, 0x4f), 4);
/// F1111's cursor circle centre (0x9f, 0x4f) and radius (`0xc >> 1`).
pub const CURSOR_CIRCLE: ((i32, i32), i32) = ((0x9f, 0x4f), 6);

/// F1061/F1063's crosshair: an object's screen position within this Manhattan
/// distance of the view centre is acquired. F989 passes the object's x+3/y-4 to
/// the callbacks, which compare against (0xa3,0x4b), so the object's own point
/// is (0xa0,0x4f) — the view centre.
pub const CROSSHAIR: (i32, i32) = (0xa0, 0x4f);
/// F1061/F1063's `cmp ebx,6; jg` window.
pub const ACQUIRE_DISTANCE: i32 = 6;

/// F989's live-object screen window before it calls a targeter callback:
/// `x > 0`, `x + 3 < 0x13c`, `y > 0` and `y - 4 < 0x95` (`ffe.asm:73373`).
pub fn targetable(x: i32, y: i32) -> bool {
    x > 0 && x + 3 < 0x13c && y > 0 && y - 4 < 0x95
}

/// F1061/F1063: the first object of `entries` (F989's order: highest slot
/// first) within [`ACQUIRE_DISTANCE`] of [`CROSSHAIR`], skipping the current
/// target and the target captured when the cursor was armed. `entries` are
/// `(object id, screen x, screen y)`.
pub fn acquire(
    entries: &[(u32, i32, i32)],
    current: Option<u32>,
    previous: Option<u32>,
) -> Option<u32> {
    entries
        .iter()
        .find(|(id, x, y)| {
            Some(*id) != current
                && Some(*id) != previous
                && targetable(*x, *y)
                && (*x - CROSSHAIR.0).abs() + (*y - CROSSHAIR.1).abs() <= ACQUIRE_DISTANCE
        })
        .map(|(id, _, _)| *id)
}

/// Which auto-targetter `DATA_007246`/`DATA_007247` is armed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    /// `DATA_007246`: the navigation targetter's box.
    Navigation,
    /// `DATA_007247`: the combat targetter's circle.
    Combat,
}

/// `F1054`'s targetter bytes: the two armed flags and the skip bytes F1061/
/// F1063 read (`DATA_007248`/`DATA_007249`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Arming {
    /// `DATA_007246`: the navigation targetter is armed.
    pub navigation: bool,
    /// `DATA_007247`: the combat targetter is armed.
    pub combat: bool,
    /// `DATA_007248`: the navigation target at the moment `T` armed it.
    pub previous_navigation: Option<u32>,
    /// `DATA_007249`: the combat target at the moment `Shift+T` armed it.
    pub previous_combat: Option<u32>,
}

impl Arming {
    /// `T`: toggle `DATA_007246`; arming stores the current target in
    /// `DATA_007248` so the first acquisition does not re-pick it. Returns the
    /// new armed state.
    pub fn toggle_navigation(&mut self, current: Option<u32>) -> bool {
        if self.navigation {
            self.navigation = false;
        } else {
            self.navigation = true;
            self.previous_navigation = current;
        }
        self.navigation
    }

    /// `Shift+T`: toggle `DATA_007247` and store `DATA_007249`.
    pub fn toggle_combat(&mut self, current: Option<u32>) -> bool {
        if self.combat {
            self.combat = false;
        } else {
            self.combat = true;
            self.previous_combat = current;
        }
        self.combat
    }

    /// F1062: an acquisition disarms the navigation cursor.
    pub fn acquired_navigation(&mut self) {
        self.navigation = false;
    }

    /// F1064: an acquisition (or a failed scan that clears the target) disarms
    /// the combat cursor.
    pub fn acquired_combat(&mut self) {
        self.combat = false;
    }
}

/// The locked target's tunnel parameters (`F1100`'s call to `F1113`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tunnels {
    /// The target's screen position (`object+0x98/+0x9a`), before F1100's -1.
    pub x: i32,
    pub y: i32,
    /// F1100's half-size base: `0x40 + ((-mantissa & 0x7ff) >> 5)`.
    pub base: i32,
    /// 0x11 for the navigation target, 0x10 (0x5f in missile view) for combat.
    pub colour: u8,
}

/// F1100's tunnel half size from the range ffp mantissa (`DATA_008882`).
pub fn tunnel_base(mantissa: i16) -> i32 {
    0x40 + (((-i32::from(mantissa)) & 0x7ff) >> 5)
}

/// F1100's number: the km/AU value is in hundredths (`laser::range_value`), and
/// the string's FF30 index 7 is `FUNC_001381_FF30Write2dpFracV1`, so the text
/// is always `<int>.<two digits><unit>` with no space before the suffix
/// (`ffedat.asm:18889-18891`; `ffe.asm:81263-81281`). The trailing zeros are
/// kept: 540 -> `5.40AU`, 5 -> `0.05AU`.
pub fn range_number(value: i32, au: bool) -> String {
    let unit = if au { "AU" } else { "km" };
    let sign = if value < 0 { "-" } else { "" };
    let value = value.unsigned_abs();
    format!("{sign}{}.{:02}{unit}", value / 100, value % 100)
}

/// F1100's off-view arrow pick (`ffe.asm:81299-81323`): row 79 and below uses
/// the below-target variants; the horizontal split is column 160.
pub fn off_view_arrow(x: i32, y: i32) -> RangeArrow {
    if y > 79 {
        if x >= 160 {
            RangeArrow::BelowRight
        } else {
            RangeArrow::BelowLeft
        }
    } else if x >= 160 {
        RangeArrow::Right
    } else {
        RangeArrow::Left
    }
}

/// F1100's text form for a target. On-view bounds are `x-5 < 0x138` and
/// `y-5 < 0x8e` with no negative values (the original's unsigned compares).
pub fn range_form(behind: bool, offscreen: bool, x: i32, y: i32) -> RangeForm {
    if behind {
        RangeForm::Behind
    } else if offscreen {
        RangeForm::OutOfRange
    } else if (0..0x138).contains(&(x - 5)) && (0..0x8e).contains(&(y - 5)) {
        RangeForm::OnView
    } else {
        RangeForm::Arrow(off_view_arrow(x, y))
    }
}

/// The expanded F1100 range string for `form` (`ffedat.asm:18889-18915`).
/// `RangeForm::OnView` returns the number only; the caller applies F1100's
/// leading-newline offset (10 rows) when it draws it.
pub fn range_text(form: RangeForm, value: i32, au: bool) -> Vec<u8> {
    let number = range_number(value, au).into_bytes();
    let number = number.as_slice();
    match form {
        RangeForm::OnView => number.to_vec(),
        RangeForm::Behind => [b"Target ".as_slice(), number, b" Behind"].concat(),
        RangeForm::OutOfRange => [b"Target Out of Range (".as_slice(), number, b")"].concat(),
        RangeForm::Arrow(RangeArrow::Left) => [b"} Target ".as_slice(), number].concat(),
        RangeForm::Arrow(RangeArrow::Right) => [b"Target ".as_slice(), number, b" ~"].concat(),
        RangeForm::Arrow(RangeArrow::BelowLeft) => [b"\x7f Target ".as_slice(), number].concat(),
        RangeForm::Arrow(RangeArrow::BelowRight) => {
            [b"Target ".as_slice(), number, b" \x80"].concat()
        }
    }
}

/// One of F1113's four nested boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TunnelBox {
    pub x: i32,
    pub y: i32,
    /// F1114's half size: `base >> shift`.
    pub half: i32,
}

/// F1113: the box at `shift` (0..=3). The squares move from the view centre
/// (shift 0, the largest) three quarters of the way to the target (shift 3, the
/// smallest). `x` is the caller's target x minus one, as F1100 passes it.
pub fn tunnel(base: i32, x: i32, y: i32, shift: u32) -> TunnelBox {
    let shift = shift.min(3) as i32;
    let weight = shift << 5;
    let rest = 0x80 - weight;
    TunnelBox {
        x: (weight * x + rest * 0xa0) >> 7,
        y: (weight * y + rest * 0x4f) >> 7,
        half: base >> shift,
    }
}

/// The four tunnel boxes in F1100's call order (shift 0..=3).
pub fn tunnels(tunnels: Tunnels) -> [TunnelBox; 4] {
    std::array::from_fn(|shift| tunnel(tunnels.base, tunnels.x - 1, tunnels.y, shift as u32))
}

/// F1114's edges: F1114 clips the box to x 0..=0x13f and y 0..=0x9d and draws an
/// edge only when its own coordinate is inside the view. The returned segments
/// are the original's four DrawLine calls, in its order (left, right, top,
/// bottom) with the drawing conditions applied.
pub fn box_edges(x: i32, y: i32, half: i32) -> Vec<((i32, i32), (i32, i32))> {
    let left = (x - half).max(0);
    let top = (y - half).max(0);
    let right = if x + half >= 320 { 0x13f } else { x + half };
    let bottom = if y + half >= 0x9e { 0x9d } else { y + half };
    if left > 0x140 || top > 0x9e || right < 0 || bottom < 0 {
        return Vec::new();
    }
    let mut edges = Vec::with_capacity(4);
    if (0..0x140).contains(&(x - half)) {
        edges.push(((left, top), (left, bottom)));
    }
    if (0..0x140).contains(&(x + half)) {
        edges.push(((right, top), (right, bottom)));
    }
    if (0..0x9e).contains(&(y - half)) {
        edges.push(((left, top), (right, top)));
    }
    if (0..0x9e).contains(&(y + half)) {
        edges.push(((left, bottom), (right, bottom)));
    }
    edges
}

/// F1612's circle outline: for each row the two horizontal extremes, clipped to
/// the view (`y < 0x9e`). F1111 draws it with a radius of six.
pub fn circle(centre: (i32, i32), radius: i32) -> Vec<(i32, i32)> {
    let mut pixels = Vec::new();
    let radius = radius.max(0);
    for dy in -radius..=radius {
        let inside = radius * radius - dy * dy;
        let dx = (inside as f64).sqrt() as i32;
        let y = centre.1 + dy;
        if !(0..0x9e).contains(&y) {
            continue;
        }
        pixels.push((centre.0 - dx, y));
        if dx != 0 {
            pixels.push((centre.0 + dx, y));
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tunnel_starts_at_the_view_centre_with_the_largest_box() {
        let params = Tunnels {
            x: 160,
            y: 79,
            base: 64,
            colour: NAVIGATION_COLOUR,
        };
        let boxes = tunnels(params);
        // F1100 calls F1113 with the target's x minus one, so only shift 0 is
        // exactly on (160,79) for a dead-ahead target.
        assert_eq!((boxes[0].x, boxes[0].y), (160, 79));
        assert_eq!(boxes[0].half, 64);
        for (shift, boxed) in boxes.iter().enumerate().skip(1) {
            assert_eq!((boxed.x, boxed.y), (159, 79), "shift {shift}");
            assert_eq!(boxed.half, 64 >> shift);
        }
    }

    #[test]
    fn the_tunnel_leans_three_quarters_of_the_way_to_the_target() {
        let boxes = tunnels(Tunnels {
            x: 200,
            y: 119,
            base: 127,
            colour: COMBAT_COLOUR,
        });
        // F1113 is called with the target's x minus one.
        assert_eq!(
            boxes.map(|b| (b.x, b.y)),
            [(160, 79), (169, 89), (179, 99), (189, 109)]
        );
        assert_eq!(boxes.map(|b| b.half), [127, 63, 31, 15]);
    }

    #[test]
    fn the_base_size_comes_from_the_range_mantissa() {
        assert_eq!(tunnel_base(i16::MIN), 64);
        assert_eq!(tunnel_base(-1), 64);
        assert_eq!(tunnel_base(-0x7fff), 127);
        assert!(tunnel_base(0x7fff) >= 64 && tunnel_base(0x7fff) <= 127);
    }

    #[test]
    fn the_box_edges_clip_to_the_view() {
        let edges = box_edges(160, 79, 8);
        assert_eq!(edges.len(), 4);
        assert_eq!(edges[0], ((152, 71), (152, 87)));
        assert_eq!(edges[1], ((168, 71), (168, 87)));
        assert_eq!(edges[2], ((152, 71), (168, 71)));
        assert_eq!(edges[3], ((152, 87), (168, 87)));
        // A box wholly below the view is not drawn.
        assert!(box_edges(160, 200, 4).is_empty());
        // The bottom is clamped to 0x9d and the top stays inside.
        let edges = box_edges(160, 160, 20);
        assert_eq!(edges[1], ((180, 140), (180, 157)));
    }

    #[test]
    fn the_targeter_acquires_within_six_pixels_of_the_view_centre() {
        // F1061 compares the object's (x+3, y-4) to (0xa3, 0x4b): equivalently
        // the object's own point to (0xa0, 0x4f) with the same Manhattan sum.
        let entries = [
            (7, 0xa0, 0x4f),          // dead centre
            (8, 0xa0 + 6, 0x4f),      // exactly six
            (9, 0xa0 + 7, 0x4f),      // seven: out
            (10, 0xa0 + 3, 0x4f + 4), // 3 + 4 = 7: out
            (11, 0xa0 - 3, 0x4f - 3), // 3 + 3 = 6: in
        ];
        assert_eq!(acquire(&entries, None, None), Some(7));
        // The first acquisition skips the armed-time target and the current
        // one (F1061's two `cmp`).
        assert_eq!(acquire(&entries, Some(7), None), Some(8));
        assert_eq!(acquire(&entries, Some(7), Some(8)), Some(11));
        assert_eq!(acquire(&entries, Some(7), Some(11)), Some(8));
        // F989's window: x > 0, x + 3 < 0x13c, y > 0, y - 4 < 0x95.
        assert!(targetable(1, 5));
        assert!(!targetable(0, 79));
        assert!(!targetable(0x139, 79));
        assert!(!targetable(160, 0));
        assert!(targetable(160, 0x98));
        assert!(!targetable(160, 0x99));
    }

    #[test]
    fn arming_toggles_and_stores_the_skip_targets() {
        let mut arming = Arming::default();
        assert!(arming.toggle_navigation(Some(3)));
        assert_eq!(arming.previous_navigation, Some(3));
        assert!(!arming.toggle_navigation(Some(4)));
        assert_eq!(arming.previous_navigation, Some(3));
        // Re-arm for the cross-cursor test, then arm the combat cursor too.
        assert!(arming.toggle_navigation(Some(4)));
        assert!(arming.toggle_combat(Some(9)));
        assert_eq!(arming.previous_combat, Some(9));
        arming.acquired_combat();
        assert!(!arming.combat);
        assert!(arming.navigation, "the other cursor stays armed");
        arming.acquired_navigation();
        assert!(!arming.navigation);
    }

    #[test]
    fn the_cursor_circle_is_an_outline_of_radius_six() {
        let pixels = circle(CURSOR_CIRCLE.0, CURSOR_CIRCLE.1);
        assert!(pixels.contains(&(159, 73)));
        assert!(pixels.contains(&(159, 85)));
        assert!(pixels.contains(&(153, 79)));
        assert!(pixels.contains(&(165, 79)));
        assert!(!pixels.contains(&(159, 79)), "the centre is not filled");
        assert!(pixels.iter().all(|(_, y)| (0..0x9e).contains(y)));
    }

    #[test]
    fn the_range_is_hundredths_with_two_decimals_and_no_space() {
        assert_eq!(range_number(540, true), "5.40AU");
        assert_eq!(range_number(100, false), "1.00km");
        assert_eq!(range_number(5, true), "0.05AU");
        assert_eq!(range_number(0, false), "0.00km");
        assert_eq!(range_number(12_345, true), "123.45AU");
        // F1545 writes the sign before the integer, F1377 puts the point in.
        assert_eq!(range_number(-5, true), "-0.05AU");
    }

    #[test]
    fn the_range_forms_are_f1100s_strings() {
        assert_eq!(range_text(RangeForm::OnView, 540, true), b"5.40AU".to_vec());
        assert_eq!(
            range_text(RangeForm::Behind, 540, true),
            b"Target 5.40AU Behind".to_vec()
        );
        assert_eq!(
            range_text(RangeForm::OutOfRange, 540, true),
            b"Target Out of Range (5.40AU)".to_vec()
        );
        // The four off-view forms: 0x7d/0x7e left/right, 0x7f/0x80 below.
        assert_eq!(
            range_text(RangeForm::Arrow(RangeArrow::Left), 100, false),
            b"} Target 1.00km".to_vec()
        );
        assert_eq!(
            range_text(RangeForm::Arrow(RangeArrow::Right), 100, false),
            b"Target 1.00km ~".to_vec()
        );
        assert_eq!(
            range_text(RangeForm::Arrow(RangeArrow::BelowLeft), 540, true),
            b"\x7f Target 5.40AU".to_vec()
        );
        assert_eq!(
            range_text(RangeForm::Arrow(RangeArrow::BelowRight), 540, true),
            b"Target 5.40AU \x80".to_vec()
        );
    }

    #[test]
    fn the_off_view_arrows_follow_f1100s_row_and_column() {
        assert_eq!(off_view_arrow(100, 79), RangeArrow::Left);
        assert_eq!(off_view_arrow(159, 79), RangeArrow::Left);
        assert_eq!(off_view_arrow(160, 79), RangeArrow::Right);
        assert_eq!(off_view_arrow(100, 80), RangeArrow::BelowLeft);
        assert_eq!(off_view_arrow(159, 120), RangeArrow::BelowLeft);
        assert_eq!(off_view_arrow(160, 80), RangeArrow::BelowRight);
        assert_eq!(off_view_arrow(300, 158), RangeArrow::BelowRight);
    }

    #[test]
    fn the_range_form_prefers_behind_then_out_of_range_then_the_view_bounds() {
        assert_eq!(range_form(true, true, 160, 79), RangeForm::Behind);
        assert_eq!(range_form(false, true, 160, 79), RangeForm::OutOfRange);
        // On view: (x-5, y-5) in x 0..0x137, y 0..0x8d.
        assert_eq!(range_form(false, false, 5, 5), RangeForm::OnView);
        assert_eq!(
            range_form(false, false, 4, 5),
            RangeForm::Arrow(RangeArrow::Left)
        );
        assert_eq!(range_form(false, false, 316, 146), RangeForm::OnView);
        assert_eq!(
            range_form(false, false, 317, 79),
            RangeForm::Arrow(RangeArrow::Right)
        );
        assert_eq!(
            range_form(false, false, 160, 147),
            RangeForm::Arrow(RangeArrow::BelowRight)
        );
    }

    #[test]
    fn the_target_colours_are_the_fixed_low_palette_entries() {
        assert_eq!(NAVIGATION_COLOUR, 0x11);
        assert_eq!(COMBAT_COLOUR, 0x10);
        // The fixed map's pure-green/pure-red words select the same entries.
        assert_eq!(NAVIGATION_WORD, 0x0f0);
        assert_eq!(COMBAT_WORD, 0xf00);
    }
}
