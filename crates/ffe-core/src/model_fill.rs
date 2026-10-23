//! The original flat-face rasterisers: F1604 (DrawTriangle) for the F1804
//! entries F1709 queues and F1607/F1608 (DrawQuad, with F1602 DrawPolySegment,
//! F1603 CalcLineGrad and F1601 DrawHLine) for the F1809 entries F1712 queues.
//! F1709/F1712 queue these only when every point is on the 320x158 view; faces
//! reaching off it take the polygon path instead. Verified against the
//! executed original (`docs/notes/fills.md`).

/// The 16.16 x step `(dx << 16) / dy` as F1603 (and F1604 inline) computes it:
/// the magnitude shifted and divided (truncating, 32-bit wrapping), then the
/// sign applied; `dy == 0` gives `0x7fffffff`, or `0x80000001` for `dx < 0`.
pub fn gradient(dx: i32, dy: i32) -> i32 {
    let divide = |value: i32, by: i32| value.checked_div(by).unwrap_or(value);
    if dy == 0 {
        if dx < 0 {
            i32::MIN + 1
        } else {
            i32::MAX
        }
    } else if dy > 0 {
        if dx >= 0 {
            divide(dx.wrapping_shl(16), dy)
        } else {
            divide(dx.wrapping_neg().wrapping_shl(16), dy).wrapping_neg()
        }
    } else if dx >= 0 {
        divide(dx.wrapping_shl(16), dy.wrapping_neg()).wrapping_neg()
    } else {
        divide(dx.wrapping_shl(16).wrapping_neg(), dy.wrapping_neg())
    }
}

/// F1601: `length` pixels from (x, y) rightwards; nothing when `length <= 0`.
fn horizontal(x: i32, y: i32, length: i32, plot: &mut impl FnMut(i32, i32)) {
    for i in 0..length.max(0) {
        plot(x + i, y);
    }
}

/// F1604's row: on rows 0..0x9e (unsigned), x from `left >> 16` (at least 0)
/// up to `right >> 16` (at most 0x140), exclusive.
fn triangle_row(y: i32, left: i32, right: i32, plot: &mut impl FnMut(i32, i32)) {
    if (y as u32) < 0x9e {
        let from = (left >> 16).max(0);
        let to = (right >> 16).min(0x140);
        horizontal(from, y, to - from, plot);
    }
}

/// F1604: the triangle's rows from the top point's row up to, not including,
/// the bottom point's row, each from the left edge to the right edge
/// (exclusive), clipped to rows 0..158 and x 0..320. Calls `plot(x, y)`.
pub fn fill_triangle(points: [[i16; 2]; 3], mut plot: impl FnMut(i32, i32)) {
    let [mut p1, mut p2, mut p3] = points.map(|p| p.map(i32::from));
    if p1[1] > p2[1] {
        std::mem::swap(&mut p1, &mut p2);
    }
    if p1[1] > p3[1] {
        std::mem::swap(&mut p1, &mut p3);
    }
    if p2[1] > p3[1] {
        std::mem::swap(&mut p2, &mut p3);
    }
    let slope = |a: [i32; 2], b: [i32; 2], y: i32| gradient(b[0] - a[0], b[1] - y);
    let mut y = p1[1];
    let rows = |y: &mut i32,
                end: i32,
                left: &mut i32,
                right: &mut i32,
                steps: (i32, i32),
                plot: &mut dyn FnMut(i32, i32)| {
        let mut plot = plot;
        while *y < end {
            triangle_row(*y, *left, *right, &mut plot);
            *left = left.wrapping_add(steps.0);
            *right = right.wrapping_add(steps.1);
            *y += 1;
        }
    };
    if p1[1] < p2[1] {
        let s12 = slope(p1, p2, y);
        let s13 = slope(p1, p3, y);
        let mut left = p1[0].wrapping_shl(16);
        let mut right = left;
        if s12 < s13 {
            rows(&mut y, p2[1], &mut left, &mut right, (s12, s13), &mut plot);
            if y < p3[1] {
                left = p2[0].wrapping_shl(16);
                let s23 = slope(p2, p3, y);
                rows(&mut y, p3[1], &mut left, &mut right, (s23, s13), &mut plot);
            }
        } else {
            rows(&mut y, p2[1], &mut left, &mut right, (s13, s12), &mut plot);
            if y < p3[1] {
                right = p2[0].wrapping_shl(16);
                let s23 = slope(p2, p3, y);
                rows(&mut y, p3[1], &mut left, &mut right, (s13, s23), &mut plot);
            }
        }
    } else if y < p3[1] {
        // Flat top: the two upper points share a row.
        let s23 = slope(p2, p3, y);
        let s13 = slope(p1, p3, y);
        let (mut left, mut right, steps) = if p1[0] < p2[0] {
            (p1[0].wrapping_shl(16), p2[0].wrapping_shl(16), (s13, s23))
        } else {
            (p2[0].wrapping_shl(16), p1[0].wrapping_shl(16), (s23, s13))
        };
        rows(&mut y, p3[1], &mut left, &mut right, steps, &mut plot);
    }
}

/// One F1603 edge: the 16.16 x, its step and the height still to draw.
#[derive(Clone, Copy)]
struct Edge {
    x: i32,
    step: i32,
    height: i32,
}

/// F1603: the edge from `a` to `b` (x starts at `a.x << 16`).
fn edge(a: [i32; 2], b: [i32; 2]) -> Edge {
    Edge {
        x: a[0].wrapping_shl(16),
        step: gradient(b[0] - a[0], b[1] - a[1]),
        height: b[1] - a[1],
    }
}

/// F1602: `height` rows from `*y` between the two edges' x, each one F1601 run
/// from the smaller x of length `|right - left|` (rows 0..0x9e only; rows above
/// the view are stepped over). The edges and `*y` advance by `height` rows.
fn segment(
    left: &mut Edge,
    right: &mut Edge,
    height: i32,
    y: &mut i32,
    plot: &mut impl FnMut(i32, i32),
) {
    let mut rows = height;
    if rows < 0 || *y >= 0x9e || rows.wrapping_add(*y) <= 0 {
        left.x = left.x.wrapping_add(rows.wrapping_mul(left.step));
        right.x = right.x.wrapping_add(rows.wrapping_mul(right.step));
        *y = y.wrapping_add(rows);
        return;
    }
    let (mut l, mut r, mut row) = (left.x, right.x, *y);
    if row < 0 {
        l = l.wrapping_sub(row.wrapping_mul(left.step));
        r = r.wrapping_sub(row.wrapping_mul(right.step));
        rows += row;
        row = 0;
    }
    while rows != 0 && row < 0x9e {
        let (a, b) = (l >> 16, r >> 16);
        if b >= a {
            horizontal(a, row, b - a, plot);
        } else {
            horizontal(b, row, a - b, plot);
        }
        l = l.wrapping_add(left.step);
        r = r.wrapping_add(right.step);
        row += 1;
        rows -= 1;
    }
    if rows != 0 {
        l = l.wrapping_add(left.step.wrapping_mul(rows));
        r = r.wrapping_add(right.step.wrapping_mul(rows));
        row += rows;
    }
    left.x = l;
    right.x = r;
    *y = row;
}

/// F1607/F1608: the quad turned so its first point is the first (in order)
/// on the top row, then filled between the edge to the second point's side
/// and the edge to the fourth point's side, segment by segment as each side
/// reaches its next point (F1602 with F1601, so the run is `|right - left|`
/// long and x is not clipped). Calls `plot(x, y)`.
pub fn fill_quad(points: [[i16; 2]; 4], mut plot: impl FnMut(i32, i32)) {
    let mut v = points.map(|p| p.map(i32::from));
    let top = v.iter().map(|p| p[1]).fold(v[0][1], i32::min);
    while v[0][1] > top {
        v.rotate_left(1);
    }
    let [a, b, c, d] = v;
    let mut y = a[1];
    let mut left = edge(a, b);
    let mut right = edge(a, d);
    if d[1] > b[1] {
        let rows = left.height;
        segment(&mut left, &mut right, rows, &mut y, &mut plot);
        right.height -= left.height;
        left = edge(b, c);
        if d[1] > c[1] {
            let rows = left.height;
            segment(&mut left, &mut right, rows, &mut y, &mut plot);
            left = edge(c, d);
            let rows = left.height;
            segment(&mut left, &mut right, rows, &mut y, &mut plot);
        } else {
            let rows = right.height;
            segment(&mut left, &mut right, rows, &mut y, &mut plot);
            right = edge(d, c);
            let rows = right.height;
            segment(&mut left, &mut right, rows, &mut y, &mut plot);
        }
    } else {
        let rows = right.height;
        segment(&mut left, &mut right, rows, &mut y, &mut plot);
        left.height -= right.height;
        right = edge(d, c);
        if b[1] < c[1] {
            let rows = left.height;
            segment(&mut left, &mut right, rows, &mut y, &mut plot);
            left = edge(b, c);
            let rows = left.height;
            segment(&mut left, &mut right, rows, &mut y, &mut plot);
        } else {
            let rows = right.height;
            segment(&mut left, &mut right, rows, &mut y, &mut plot);
            right = edge(c, b);
            let rows = right.height;
            segment(&mut left, &mut right, rows, &mut y, &mut plot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fill_quad, fill_triangle, gradient};

    fn pixels(fill: impl FnOnce(&mut dyn FnMut(i32, i32))) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        fill(&mut |x, y| out.push((x, y)));
        out
    }

    #[test]
    fn gradients_truncate_and_mark_flat_edges() {
        assert_eq!(gradient(1, 3), 0x5555);
        assert_eq!(gradient(-1, 3), -0x5555);
        assert_eq!(gradient(5, 0), 0x7fff_ffff);
        assert_eq!(gradient(-5, 0), -0x7fff_ffff);
    }

    #[test]
    fn triangles_exclude_the_bottom_row_and_right_edge() {
        let filled = pixels(|plot| fill_triangle([[0, 0], [4, 0], [0, 4]], plot));
        assert_eq!(filled.len(), 4 + 3 + 2 + 1);
        assert!(filled.iter().all(|&(x, y)| x + y < 4 && y < 4));
        // A point or a line covers nothing.
        assert!(pixels(|plot| fill_triangle([[3, 3]; 3], plot)).is_empty());
        // Rows above the view are skipped, x is clipped to 0..320.
        let clipped = pixels(|plot| fill_triangle([[-10, -5], [400, -5], [-10, 3]], plot));
        assert!(clipped
            .iter()
            .all(|&(x, y)| (0..320).contains(&x) && (0..3).contains(&y)));
    }

    #[test]
    fn quads_fill_between_their_two_sides() {
        let square = pixels(|plot| fill_quad([[2, 2], [6, 2], [6, 6], [2, 6]], plot));
        assert_eq!(square.len(), 16);
        assert!(square
            .iter()
            .all(|&(x, y)| (2..6).contains(&x) && (2..6).contains(&y)));
        // Any starting corner gives the same square.
        let mut turned = pixels(|plot| fill_quad([[6, 6], [2, 6], [2, 2], [6, 2]], plot));
        let mut original = square.clone();
        turned.sort_unstable();
        original.sort_unstable();
        assert_eq!(turned, original);
    }
}
