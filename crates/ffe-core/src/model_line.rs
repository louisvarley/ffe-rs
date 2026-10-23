//! The original line rasteriser: F1616 (DrawLine) with F1601 (DrawHLine), as
//! F1803 calls it for the line entries F1704 queues (opcode 2 lines and narrow
//! thrusters). F1704 has already moved both end points onto the 320x158 view,
//! so F1616 itself does no clipping. F1811's curves (opcode 22) go through
//! F1546, which draws F1549's forward-difference steps with F1616. Verified
//! against the executed original (`docs/notes/lines.md`).
use crate::model_polygon::curve_edges;
use crate::model_thruster::clip_toward;

/// F1601: `length` pixels from (x, y) rightwards; nothing when `length <= 0`.
fn horizontal(x: i32, y: i32, length: i32, plot: &mut impl FnMut(i32, i32)) {
    for i in 0..length.max(0) {
        plot(x + i, y);
    }
}

/// F1616: draw the line from `a` to `b`, calling `plot(x, y)` for each pixel in
/// the original's order. Rows are walked top to bottom with a 16.16 x step
/// (`(dx << 16) / dy`, truncated); each row is one F1601 run. A step of at most
/// one pixel per row (a steep line) draws single pixels; a longer one draws the
/// run to the next row's x, and the last row runs to the end point. A shallow line
/// leaning left starts with a single pixel at the top point and then draws each
/// row one step ahead (the original's asymmetry, kept).
pub fn draw_line(a: [i16; 2], b: [i16; 2], mut plot: impl FnMut(i32, i32)) {
    let [ax, ay] = a.map(i32::from);
    let [bx, by] = b.map(i32::from);
    if ay == by {
        if ax <= bx {
            horizontal(ax, ay, bx - ax + 1, &mut plot);
        } else {
            horizontal(bx, by, ax - bx + 1, &mut plot);
        }
        return;
    }
    // The upper point (smaller y) first.
    let ((top_x, top_y), (end_x, end_y)) = if ay > by {
        ((bx, by), (ax, ay))
    } else {
        ((ax, ay), (bx, by))
    };
    let dy = end_y - top_y;
    let dx = end_x - top_x;
    let slope = if dx >= 0 {
        dx.wrapping_shl(16) / dy
    } else {
        -((-dx).wrapping_shl(16) / dy)
    };
    let mut x = top_x.wrapping_shl(16);
    if (-0x10000..=0x10000).contains(&slope) {
        for y in top_y..end_y {
            horizontal(x >> 16, y, 1, &mut plot);
            x = x.wrapping_add(slope);
        }
        horizontal(end_x, end_y, 1, &mut plot);
    } else if slope > 0 {
        for y in top_y..end_y {
            let next = x.wrapping_add(slope);
            horizontal(x >> 16, y, (next >> 16) - (x >> 16), &mut plot);
            x = next;
        }
        horizontal(x >> 16, end_y, end_x - (x >> 16) + 1, &mut plot);
    } else {
        horizontal(top_x, top_y, 1, &mut plot);
        for y in top_y + 1..=end_y {
            let next = x.wrapping_add(slope);
            horizontal(next >> 16, y, (x >> 16) - (next >> 16), &mut plot);
            x = next;
        }
    }
}

/// F1546, as F1811 calls it for a curve entry's four points (entry order): the
/// 2^n forward-difference steps of F1549 (the same steps F1547 takes, without
/// its closing edge to the end point), each drawn with F1616. A step with both
/// ends off the view is skipped; with one end off, that end is first moved
/// toward the other by F1561, which can leave it off the view (the original
/// then draws outside the view, so `plot` receives such pixels as they come).
/// Errors where F1562/F1563's division would fault.
pub fn draw_curve(points: [[i16; 2]; 4], mut plot: impl FnMut(i32, i32)) -> Result<(), String> {
    let on_view = |[x, y]: [i16; 2]| (i32::from(x) as u32) < 0x140 && (i32::from(y) as u32) < 0x9e;
    let edges = curve_edges(points);
    for &[mut from, mut to] in &edges[..edges.len() - 1] {
        match (on_view(from), on_view(to)) {
            (false, false) => continue,
            (false, true) => clip_toward(&mut from, to)?,
            (true, false) => clip_toward(&mut to, from)?,
            (true, true) => {}
        }
        draw_line(from, to, &mut plot);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{draw_curve, draw_line};

    fn pixels(a: [i16; 2], b: [i16; 2]) -> Vec<(i32, i32)> {
        let mut out = Vec::new();
        draw_line(a, b, |x, y| out.push((x, y)));
        out
    }

    #[test]
    fn horizontal_vertical_and_single_points() {
        assert_eq!(pixels([5, 3], [2, 3]), [(2, 3), (3, 3), (4, 3), (5, 3)]);
        assert_eq!(pixels([4, 4], [4, 4]), [(4, 4)]);
        assert_eq!(pixels([1, 2], [1, 0]), [(1, 0), (1, 1), (1, 2)]);
    }

    #[test]
    fn shallow_lines_run_each_row_and_lean_left_one_row_ahead() {
        // Right: rows of (dx << 16) / dy, the last row runs to the end point.
        assert_eq!(
            pixels([0, 0], [5, 1]),
            [(0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 1)]
        );
        // Left: a single pixel at the top point, then each row from the next x.
        assert_eq!(
            pixels([5, 0], [0, 1]),
            [(5, 0), (0, 1), (1, 1), (2, 1), (3, 1), (4, 1)]
        );
    }

    #[test]
    fn curves_skip_steps_off_the_view_and_clip_one_end() {
        let curve = |points: [[i16; 2]; 4]| {
            let mut out = Vec::new();
            draw_curve(points, |x, y| out.push((x, y))).unwrap();
            out
        };
        // Every step off the view (left of it): nothing drawn.
        assert!(curve([[-50, 10], [-40, 20], [-30, 30], [-20, 40]]).is_empty());
        // A point curve: 2^2 one-pixel steps at that point.
        assert_eq!(curve([[7, 9]; 4]), vec![(7, 9); 4]);
        // Crossing the left edge: the off-view end moves to x 0 first.
        let crossing = curve([[-10, 5], [-3, 5], [3, 5], [10, 5]]);
        assert_eq!(crossing.first(), Some(&(0, 5)));
        assert!(crossing
            .iter()
            .all(|&(x, y)| (0..=10).contains(&x) && y == 5));
    }
}
