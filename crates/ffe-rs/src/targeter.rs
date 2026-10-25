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
//! `F1111` (the gunsight routine) draws a blinking cursor while one of the
//! original's auto-targetter flags is set: `DATA_007246` is the navigation
//! targetter (a 4-half-size box at (160,79), colour 0x11) and `DATA_007247` the
//! combat targetter (a circle of radius 6 at (159,79), colour 0x10). Both blink
//! with `DATA_007252`, which is the parity of the game second (`DATA_008799 /
//! 0xc22e`). The port does not arm the auto-targetter (its `T` cycles locks); the
//! draw path is exposed for the caller and for frames.

/// F1100/F1111's targetter colours: navigation 0x11, combat 0x10.
pub const NAVIGATION_COLOUR: u8 = 0x11;
pub const COMBAT_COLOUR: u8 = 0x10;

/// F1111's cursor box origin (0xa0, 0x4f) and half size.
pub const CURSOR_BOX: ((i32, i32), i32) = ((0xa0, 0x4f), 4);
/// F1111's cursor circle centre (0x9f, 0x4f) and radius (`0xc >> 1`).
pub const CURSOR_CIRCLE: ((i32, i32), i32) = ((0x9f, 0x4f), 6);

/// Which auto-targetter `DATA_007246`/`DATA_007247` is armed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    /// `DATA_007246`: the navigation targetter's box.
    Navigation,
    /// `DATA_007247`: the combat targetter's circle.
    Combat,
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
    fn the_cursor_circle_is_an_outline_of_radius_six() {
        let pixels = circle(CURSOR_CIRCLE.0, CURSOR_CIRCLE.1);
        assert!(pixels.contains(&(159, 73)));
        assert!(pixels.contains(&(159, 85)));
        assert!(pixels.contains(&(153, 79)));
        assert!(pixels.contains(&(165, 79)));
        assert!(!pixels.contains(&(159, 79)), "the centre is not filled");
        assert!(pixels.iter().all(|(_, y)| (0..0x9e).contains(y)));
    }
}
