//! The original flat-face rasterisers: F1604 (DrawTriangle) for the F1804
//! entries F1709 queues and F1607/F1608 (DrawQuad, with F1602 DrawPolySegment,
//! F1603 CalcLineGrad and F1601 DrawHLine) for the F1809 entries F1712 queues.
//! F1709/F1712 queue these only when every point is on the 320x158 view; faces
//! reaching off it take the polygon path instead: F1468's span table, an F1560
//! insertion per edge and F1613's span drawing ([`fill_polygon`]). Verified
//! against the executed original (`docs/notes/fills.md`).
use crate::model_polygon::near_point;
use crate::model_thruster::clip_axis;

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

/// Rows in F1468's span table (`DATA_009202`): one per view row.
const SPAN_ROWS: usize = 0x9e;
/// x values one row keeps (F1558 drops the rest).
const SPAN_LIMIT: usize = 10;

/// F1468's span table: per row a count and up to ten x values, the rows
/// touched (`+0x1b2c` lowest, `+0x1b30` one past the highest) and whether an
/// edge end lay off the view in x (`+0x1b28`, which selects F1600).
pub struct Spans {
    rows: Vec<(usize, [i32; SPAN_LIMIT])>,
    top: i32,
    bottom: i32,
    clipped: bool,
}

impl Default for Spans {
    /// F1468: every count zero, top 0x9e, bottom -1, not clipped.
    fn default() -> Self {
        Self {
            rows: vec![(0, [0; SPAN_LIMIT]); SPAN_ROWS],
            top: SPAN_ROWS as i32,
            bottom: -1,
            clipped: false,
        }
    }
}

impl Spans {
    /// F1560: insert one edge. Ordered top first; dropped when it lies wholly at
    /// or below row 0x9e or at or above row 0; its ends clipped to rows 0 and
    /// 0x9e along the edge (F1563); then F1559 adds `x >> 16` to every row from
    /// the top end's down to, not including, the bottom end's, stepping x by the
    /// edge's 16.16 gradient (F1558 keeps the first ten per row).
    pub fn edge(&mut self, a: [i16; 2], b: [i16; 2]) -> Result<(), String> {
        let (mut top, mut bottom) = if b[1] < a[1] { (b, a) } else { (a, b) };
        if top[1] >= 0x9e || bottom[1] <= 0 {
            return Ok(());
        }
        if bottom[1] > 0x9e {
            clip_axis(&mut bottom, top, 1, 0x9e)?;
        }
        if top[1] < 0 {
            clip_axis(&mut top, bottom, 1, 0x9e)?;
        }
        let off = |x: i16| (i32::from(x) as u32) >= 0x140;
        if off(top[0]) || off(bottom[0]) {
            self.clipped = true;
        }
        self.bottom = self.bottom.max(i32::from(bottom[1]));
        self.top = self.top.min(i32::from(top[1]));
        // F1559.
        let rows = i32::from(bottom[1]) - i32::from(top[1]);
        if rows == 0 {
            return Ok(());
        }
        let dx = i32::from(bottom[0]) - i32::from(top[0]);
        let step = if dx > 0 {
            dx.wrapping_shl(16) / rows
        } else {
            (dx.wrapping_shl(16).wrapping_neg() / rows).wrapping_neg()
        };
        let mut x = i32::from(top[0]).wrapping_shl(16);
        for row in i32::from(top[1])..i32::from(top[1]) + rows {
            // Rows outside the table are F1558 writes past it; the clipping
            // above keeps every row of a real edge in 0..0x9e.
            if let Some((count, xs)) = self.rows.get_mut(row as usize) {
                if *count < SPAN_LIMIT {
                    xs[*count] = x >> 16;
                    *count += 1;
                }
            }
            x = x.wrapping_add(step);
        }
        Ok(())
    }

    /// F1613: from row `bottom - 1` up to `top`, each row's x values sorted
    /// (bubble sort, signed) and drawn in pairs from the last, `F1601(x[i], y,
    /// x[i + 1] - x[i])` (an odd first value is left out); with an edge off the
    /// view in x, through F1600, which clips each run to x 0..0x140.
    pub fn draw(&self, mut plot: impl FnMut(i32, i32)) {
        if self.bottom < self.top {
            return;
        }
        let mut y = self.bottom - 1;
        while y >= self.top {
            if let Some((count, xs)) = self.rows.get(y as usize) {
                let mut xs = xs[..*count].to_vec();
                for end in (2..=xs.len()).rev() {
                    for i in 1..end {
                        if xs[i - 1] > xs[i] {
                            xs.swap(i - 1, i);
                        }
                    }
                }
                let mut i = xs.len() as isize - 2;
                while i >= 0 {
                    let (x, length) = (xs[i as usize], xs[i as usize + 1] - xs[i as usize]);
                    if self.clipped {
                        clipped_horizontal(x, y, length, &mut plot);
                    } else {
                        horizontal(x, y, length, &mut plot);
                    }
                    i -= 2;
                }
            }
            y -= 1;
        }
    }
}

/// F1600: F1601 with the run clipped to x 0..0x140.
fn clipped_horizontal(mut x: i32, y: i32, mut length: i32, plot: &mut impl FnMut(i32, i32)) {
    if x < 0 {
        if length + x <= 0 {
            return;
        }
        length += x;
        x = 0;
    }
    if length + x > 0x140 {
        length = 0x140 - x;
        if length <= 0 {
            return;
        }
    }
    horizontal(x, y, length, plot);
}

/// F1794 -> F1795/F1796/F1797 -> F1799: a fresh span table (F1468), every edge
/// of the entry inserted (F1560; `model_polygon::edges` gives the sequence) and
/// the spans drawn (F1613).
pub fn fill_polygon(edges: &[[[i16; 2]; 2]], plot: impl FnMut(i32, i32)) -> Result<(), String> {
    let mut spans = Spans::default();
    for &[a, b] in edges {
        spans.edge(a, b)?;
    }
    spans.draw(plot);
    Ok(())
}

/// F1714: the closed outline of a flat face reaching off the view, from its
/// vertices' screen points and views in face order (a triangle repeats its last
/// vertex, as F1709 does). Turned until the first vertex is in front of the
/// near plane (z >= 0x40); then each vertex in front contributes its screen
/// point and each step across the near plane F1564's crossing (from the vertex
/// behind toward the one in front), ending back at the first point. Empty when
/// every vertex is behind (F1713 rejects that before F1714 is reached).
pub fn near_outline(vertices: &[([i16; 2], [i32; 3])]) -> Vec<[i16; 2]> {
    let mut v: Vec<([i16; 2], [i32; 3])> = vertices.to_vec();
    if v.len() == 3 {
        v.push(v[2]);
    }
    if v.len() != 4 || v.iter().all(|(_, view)| view[2] < 0x40) {
        return Vec::new();
    }
    while v[0].1[2] < 0x40 {
        v.rotate_left(1);
    }
    let front = |i: usize| v[i].1[2] >= 0x40;
    let near = |behind: usize, ahead: usize| near_point(v[behind].1, v[ahead].1);
    let mut points = vec![v[0].0];
    // Whether the last vertex walked is in front.
    let mut ahead = if !front(1) {
        points.push(near(1, 0));
        if front(2) {
            points.push(near(1, 2));
            true
        } else {
            false
        }
    } else {
        points.push(v[1].0);
        if !front(2) {
            points.push(near(2, 1));
            false
        } else {
            true
        }
    };
    ahead = if !ahead {
        if front(3) {
            points.push(near(2, 3));
            true
        } else {
            false
        }
    } else {
        points.push(v[2].0);
        if !front(3) {
            points.push(near(3, 2));
            false
        } else {
            true
        }
    };
    if ahead {
        points.push(v[3].0);
    } else {
        points.push(near(3, 0));
    }
    points.push(v[0].0);
    points
}

#[cfg(test)]
mod tests {
    use super::{fill_polygon, fill_quad, fill_triangle, gradient, near_outline};
    use crate::model_polygon::near_point;

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

    #[test]
    fn outlines_walk_the_face_and_cross_the_near_plane() {
        let front = |x: i16, y: i16| ([x, y], [i32::from(x), i32::from(y), 0x100]);
        let behind = ([-32766, -32766], [5, 5, 0x10]);
        // All in front: the four points and back to the first.
        let quad = [front(1, 1), front(9, 1), front(9, 9), front(1, 9)];
        assert_eq!(
            near_outline(&quad),
            [[1, 1], [9, 1], [9, 9], [1, 9], [1, 1]]
        );
        // A triangle repeats its last point, as F1709 passes it.
        assert_eq!(
            near_outline(&quad[..3]),
            [[1, 1], [9, 1], [9, 9], [9, 9], [1, 1]]
        );
        // One vertex behind: the crossings toward its two neighbours replace it.
        let crossing = [front(1, 1), behind, front(9, 9), front(1, 9)];
        let (b, a, c) = (behind.1, crossing[0].1, crossing[2].1);
        assert_eq!(
            near_outline(&crossing),
            [
                [1, 1],
                near_point(b, a),
                near_point(b, c),
                [9, 9],
                [1, 9],
                [1, 1]
            ]
        );
        // The first vertex behind: turned to start at the next one in front.
        let turned = near_outline(&[behind, front(9, 1), front(9, 9), front(1, 9)]);
        assert_eq!(turned.first(), Some(&[9, 1]));
        assert!(near_outline(&[behind; 4]).is_empty());
    }

    #[test]
    fn polygons_fill_their_span_pairs() {
        let square = [
            [[2, 2], [6, 2]],
            [[6, 2], [6, 6]],
            [[6, 6], [2, 6]],
            [[2, 6], [2, 2]],
        ];
        let mut filled = Vec::new();
        fill_polygon(&square, |x, y| filled.push((x, y))).unwrap();
        assert_eq!(filled.len(), 16);
        assert!(filled
            .iter()
            .all(|&(x, y)| (2..6).contains(&x) && (2..6).contains(&y)));
        // Off the view: rows clipped to 0..158, x to 0..320 (F1600).
        let mut clipped = Vec::new();
        let huge = [
            [[-50, -50], [400, -50]],
            [[400, -50], [400, 200]],
            [[400, 200], [-50, 200]],
            [[-50, 200], [-50, -50]],
        ];
        fill_polygon(&huge, |x, y| clipped.push((x, y))).unwrap();
        assert_eq!(clipped.len(), 320 * 158);
    }
}
