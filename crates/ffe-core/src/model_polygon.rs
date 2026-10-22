//! F1721 complex polygons: outline segments, the segment handlers' queue entries and
//! the draw routine's outline.
//!
//! The polygon record (opcode 5) holds a list of 16-bit segment words. `word >> 9`
//! indexes `DATA_007817` (0 terminator, 1 F1734, 2 F1724, 3 F1730, 4 F1733,
//! 5 F1723, 6 F1736); the low byte is the segment's first operand and some forms
//! take two more bytes. The handlers project vertices (F1471), clip edges against
//! the near plane z = 0x40 (F1564), turn circles into two cubic curves
//! (F1554/F1555/F1556/F1557) and append the original queue entries after the
//! polygon's F1794 header: F1795 edges, F1796 "edge from the previous point"
//! entries and F1797 curves. A normal end appends the F1799 fill; the flag in byte
//! +2 bit 7 instead abandons a polygon that reaches behind the near plane (F1726),
//! leaving only an F1813 entry.
//!
//! The draw routines hand every edge to F1560 (span insertion); curves are
//! subdivided by F1547/F1549 forward differencing. [`edges`] reproduces that edge
//! sequence; the span rasteriser itself (F1560/F1559/F1613) is not ported.
use crate::model::multiply;
use crate::model_instance::Vertex;
use crate::model_math::table_root;

/// One outline segment (`DATA_007817` handler) with the vertex/byte operands it reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Segment {
    /// Index 1, F1734 (6 bytes): start a contour at `start` (byte +3) with a curve
    /// through `controls` (bytes +2, +5) to `end` (byte +4). The word's low byte is
    /// not read by F1734.
    CurveStart {
        start: u8,
        controls: [u8; 2],
        end: u8,
    },
    /// Index 2, F1724 (4 bytes): start a contour at `start` (low byte) with an edge
    /// to `end` (byte +2). Byte +3 is not read.
    LineStart { start: u8, end: u8 },
    /// Index 3, F1730 (2 bytes): edge to `vertex` (low byte).
    Line { vertex: u8 },
    /// Index 4, F1733 (4 bytes): curve through `controls` (low byte, byte +3) to
    /// `end` (byte +2).
    Curve { controls: [u8; 2], end: u8 },
    /// Index 5, F1723 (2 bytes): close the contour back to its start.
    Close,
    /// Index 6, F1736 (4 bytes): circle around `centre` (low byte), facing `normal`
    /// (byte +2), with radius byte +3 shifted by the instance scale (+0x148).
    Circle { centre: u8, normal: u8, radius: u8 },
}

/// Bytes after the segment word for each `DATA_007817` index; the same sizes as the
/// skip table `DATA_007819` that F1726 uses.
const DATA_SIZES: [usize; 7] = [0, 4, 2, 0, 2, 0, 2];

fn segment_index(word: u16) -> Result<usize, String> {
    // `movsx` then `shr 9`: a negative word indexes far outside the table.
    let index = ((word as i16 as i32) as u32) >> 9;
    if index > 6 {
        return Err(format!(
            "Polygon segment word {word:#06x} indexes outside DATA_007817."
        ));
    }
    Ok(index as usize)
}

/// Decode a polygon's segment bytes (byte +3 gives their length L). The list must end
/// with its terminator word exactly at L: the original returns past the terminator
/// when it draws the polygon but past L when it skips it, so any other layout would
/// make the record size depend on visibility. Every vertex operand must be a model
/// vertex (the handlers read them sign-extended; negative ones are external).
pub fn decode_segments(bytes: &[u8], vertex_count: u32) -> Result<Vec<Segment>, String> {
    let mut segments = Vec::new();
    let mut position = 0;
    loop {
        let word = bytes
            .get(position..position + 2)
            .map(|w| u16::from_le_bytes([w[0], w[1]]))
            .ok_or("Polygon segments run past their declared length.")?;
        let index = segment_index(word)?;
        if index == 0 {
            if position + 2 != bytes.len() {
                return Err(
                    "Polygon segment terminator does not end the declared length.".to_owned(),
                );
            }
            return Ok(segments);
        }
        let data = bytes
            .get(position + 2..position + 2 + DATA_SIZES[index])
            .ok_or("Polygon segments run past their declared length.")?;
        let low = word as u8;
        let segment = match index {
            1 => Segment::CurveStart {
                start: data[1],
                controls: [data[0], data[3]],
                end: data[2],
            },
            2 => Segment::LineStart {
                start: low,
                end: data[0],
            },
            3 => Segment::Line { vertex: low },
            4 => Segment::Curve {
                controls: [low, data[1]],
                end: data[0],
            },
            5 => Segment::Close,
            _ => Segment::Circle {
                centre: low,
                normal: data[0],
                radius: data[1],
            },
        };
        let vertices: &[u8] = match &segment {
            Segment::CurveStart {
                start,
                controls,
                end,
            } => &[*start, controls[0], controls[1], *end],
            Segment::LineStart { start, end } => &[*start, *end],
            Segment::Line { vertex } => &[*vertex],
            Segment::Curve { controls, end } => &[controls[0], controls[1], *end],
            Segment::Close => &[],
            Segment::Circle { centre, .. } => &[*centre],
        };
        if vertices
            .iter()
            .any(|v| *v >= 0x80 || u32::from(*v) >= vertex_count)
        {
            return Err("Polygon segment references external/out-of-range vertex.".to_owned());
        }
        segments.push(segment);
        position += 2 + DATA_SIZES[index];
    }
}

/// One queue entry after the polygon's F1794 header, as the original stores it.
/// Points are screen words (x, y) in the logical 320x158 view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Entry {
    /// F1795 (built by F1719): an edge between two points.
    Edge([[i16; 2]; 2]),
    /// F1796 (built by F1718): an edge from the previous entry's last point.
    To([i16; 2]),
    /// F1797 (built by F1720): a cubic curve through four control points.
    Curve([[i16; 2]; 4]),
}

/// The queue contents F1721 leaves for one drawn polygon.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outline {
    /// Entries in queue order. Empty when abandoned.
    pub entries: Vec<Entry>,
    /// The flag abandoned the polygon (F1726): the header holds only an F1813 entry
    /// and no F1799 fill follows.
    pub abandoned: bool,
}

/// F1564: binary-search the segment from `behind` (z < 0x40) toward `front` for the
/// near plane, then project at z = 0x40 (a shift by 2 instead of a division).
pub fn near_point(behind: [i32; 3], front: [i32; 3]) -> [i16; 2] {
    let mut point = behind;
    let mut step = [
        front[0].wrapping_sub(behind[0]) >> 1,
        front[1].wrapping_sub(behind[1]) >> 1,
        (front[2].wrapping_sub(behind[2]) as u32 >> 1) as i32,
    ];
    while step[2] != 0 {
        if point[2] <= 0x40 {
            point = std::array::from_fn(|i| point[i].wrapping_add(step[i]));
        } else {
            point = std::array::from_fn(|i| point[i].wrapping_sub(step[i]));
        }
        step = [step[0] >> 1, step[1] >> 1, (step[2] as u32 >> 1) as i32];
    }
    let [mut x, mut y, _] = point;
    if matches!(x >> 29, 0 | -1) && matches!(y >> 29, 0 | -1) {
        x = x.wrapping_shl(2);
        y = y.wrapping_shl(2);
    }
    if x < 0 {
        while x <= -0x1f00 {
            x >>= 1;
            y >>= 1;
        }
    } else {
        while x >= 0x1f00 {
            x >>= 1;
            y >>= 1;
        }
    }
    if y < 0 {
        while y <= -0x1f00 {
            x >>= 1;
            y >>= 1;
        }
    } else {
        while y >= 0x1f00 {
            x >>= 1;
            y >>= 1;
        }
    }
    [
        (x as i16).wrapping_add(0xa0),
        0x4f_i16.wrapping_sub(y as i16),
    ]
}

/// F1557: project a view point for the circle routine; `None` when z < 0x40 or the
/// scaled z reaches zero.
pub fn circle_point([x, y, z]: [i32; 3]) -> Option<[i16; 2]> {
    if z < 0x40 {
        return None;
    }
    let (mut x, mut y, mut z) = (x, y, z);
    let mut magnitude = x.wrapping_abs() | y.wrapping_abs();
    while magnitude > 0x80_0000 {
        magnitude >>= 1;
        x >>= 1;
        y >>= 1;
        z >>= 1;
    }
    if z == 0 {
        return None;
    }
    let mut sx = x.wrapping_shl(8).wrapping_div(z);
    let mut sy = y.wrapping_shl(8).wrapping_div(z);
    while !(-0x3fff..0x4000).contains(&sx) || !(-0x3fff..0x4000).contains(&sy) {
        sx >>= 1;
        sy >>= 1;
    }
    Some([
        (sx as i16).wrapping_add(0xa0),
        0x4f_i16.wrapping_sub(sy as i16),
    ])
}

/// F1554 with F1555/F1556: a circle around `centre` (view space) facing `axis`, as
/// two cubic curves `[P0, P0 + v, P1 + v, P1]` and `[P0, P0 - v, P1 - v, P1]` with
/// P0/P1 = centre ± u. `None` when any of the six points fails F1557.
pub fn circle(centre: [i32; 3], axis: [i32; 3], radius: i32) -> Option<[[[i16; 2]; 4]; 2]> {
    let [ax, ay, az] = axis;
    let sum = ax.wrapping_mul(ax).wrapping_add(ay.wrapping_mul(ay));
    let (u, v) = if sum < 9 {
        // F1556: the axis is (nearly) the view direction.
        ([radius, 0, 0], [0, radius.wrapping_mul(0xaaaa) >> 15, 0])
    } else {
        let length = table_root(sum as u32) as i32;
        let scaled = |value: i32| value.wrapping_shl(15).wrapping_div(length);
        let k = radius.wrapping_shl(10).wrapping_div(3);
        (
            [
                scaled(ay)
                    .wrapping_mul(radius)
                    .wrapping_shr(15)
                    .wrapping_neg(),
                scaled(ax).wrapping_mul(radius) >> 15,
                0,
            ],
            [
                ax.wrapping_mul(az).wrapping_div(length).wrapping_mul(k) >> 15,
                ay.wrapping_mul(az).wrapping_div(length).wrapping_mul(k) >> 15,
                length.wrapping_neg().wrapping_mul(k) >> 15,
            ],
        )
    };
    let add =
        |a: [i32; 3], b: [i32; 3]| -> [i32; 3] { std::array::from_fn(|i| a[i].wrapping_add(b[i])) };
    let sub =
        |a: [i32; 3], b: [i32; 3]| -> [i32; 3] { std::array::from_fn(|i| a[i].wrapping_sub(b[i])) };
    let p0 = add(centre, u);
    let first = circle_point(p0)?;
    let first_plus = circle_point(add(p0, v))?;
    let first_minus = circle_point(sub(p0, v))?;
    let p1 = sub(centre, u);
    let last = circle_point(p1)?;
    let last_plus = circle_point(add(p1, v))?;
    let last_minus = circle_point(sub(p1, v))?;
    Some([
        [first, first_plus, last_plus, last],
        [first, first_minus, last_minus, last],
    ])
}

/// F1547/F1549 as called by F1797: forward-difference the cubic in 2^n steps
/// (n = 2..5 from the Manhattan distance between the first point and the control
/// points' mean), each step an edge, then a last edge to the end point.
pub fn curve_edges(points: [[i16; 2]; 4]) -> Vec<[[i16; 2]; 2]> {
    let [a, b, c, d] = points.map(|p| p.map(i32::from));
    let distance: i32 = (0..2)
        .map(|k| ((a[k] + b[k] + c[k] + d[k]) / 4 - a[k]).wrapping_abs())
        .sum();
    let n = match distance as u32 {
        0..=3 => 2,
        4..=29 => 3,
        30..=24999 => 4,
        _ => 5,
    };
    let cubic: [i32; 2] =
        std::array::from_fn(|k| (d[k] - a[k] + 3 * b[k] - 3 * c[k]).wrapping_shl(16 - 3 * n));
    let linear: [i32; 2] = std::array::from_fn(|k| (3 * b[k] - 3 * a[k]).wrapping_shl(16 - n));
    let square: [i32; 2] =
        std::array::from_fn(|k| (3 * c[k] - 6 * b[k] + 3 * a[k]).wrapping_shl(16 - 2 * n));
    let mut position: [i32; 2] = std::array::from_fn(|k| a[k].wrapping_shl(16));
    let mut first: [i32; 2] =
        std::array::from_fn(|k| cubic[k].wrapping_add(linear[k]).wrapping_add(square[k]));
    let mut second: [i32; 2] = std::array::from_fn(|k| {
        square[k]
            .wrapping_mul(2)
            .wrapping_add(cubic[k].wrapping_mul(6))
    });
    let third: [i32; 2] = std::array::from_fn(|k| cubic[k].wrapping_mul(6));
    let point = |p: [i32; 2]| p.map(|v| (v >> 16) as i16);
    let mut current = point(position);
    let mut edges = Vec::with_capacity((1 << n) + 1);
    for _ in 0..1 << n {
        let previous = current;
        for k in 0..2 {
            position[k] = position[k].wrapping_add(first[k]);
            first[k] = first[k].wrapping_add(second[k]);
            second[k] = second[k].wrapping_add(third[k]);
        }
        current = point(position);
        edges.push([previous, current]);
    }
    edges.push([current, points[3]]);
    edges
}

/// The edges the draw routines pass to F1560, in order: F1795 its two points, F1796
/// the previous entry's last point and its own, F1797 the F1547 subdivision.
pub fn edges(entries: &[Entry]) -> Result<Vec<[[i16; 2]; 2]>, String> {
    let mut result = Vec::new();
    let mut last: Option<[i16; 2]> = None;
    for entry in entries {
        match entry {
            Entry::Edge([a, b]) => {
                result.push([*a, *b]);
                last = Some(*b);
            }
            Entry::To(point) => {
                let previous =
                    last.ok_or("A polygon edge continues from its queue header, not a point.")?;
                result.push([previous, *point]);
                last = Some(*point);
            }
            Entry::Curve(points) => {
                result.extend(curve_edges(*points));
                last = Some(points[3]);
            }
        }
    }
    Ok(result)
}

/// A single closed path for an even-odd fill of the outline: each chain of
/// connected edges, bridged to and from the first chain's start so that every
/// bridge is traversed twice and cancels. A presentation helper, not original code.
pub fn fill_path(entries: &[Entry]) -> Result<Vec<[i16; 2]>, String> {
    let mut chains: Vec<Vec<[i16; 2]>> = Vec::new();
    for [a, b] in edges(entries)? {
        match chains.last_mut() {
            Some(chain) if chain.last() == Some(&a) => chain.push(b),
            _ => chains.push(vec![a, b]),
        }
    }
    let mut path = Vec::new();
    let Some(anchor) = chains.first().map(|chain| chain[0]) else {
        return Ok(path);
    };
    for (i, chain) in chains.iter().enumerate() {
        path.extend_from_slice(chain);
        path.push(chain[0]);
        if i > 0 {
            path.push(anchor);
        }
    }
    Ok(path)
}

/// Inputs the segment handlers read from the instance.
pub struct Inputs<'a> {
    /// Vertex frame at +0x14 (rotates circle normals).
    pub matrix: [i32; 9],
    /// Instance scale at +0x148 (shifts circle radii).
    pub scale: i32,
    /// The model's normal records (+0x154): six bytes per mirrored pair.
    pub normal_records: &'a [u8],
}

enum Flow {
    Continue,
    /// F1726 from an aligned cursor: the rest of the list is skipped.
    Abandon,
}

/// The handlers' instance fields. `None` marks a field this polygon has not set:
/// the original would read whatever an earlier polygon left there, which is an
/// explicit error here.
#[derive(Default)]
struct State {
    /// +0x104: the contour needs closing.
    open: Option<bool>,
    /// +0x108: where the contour entered from behind the near plane.
    entry: Option<[i16; 2]>,
    /// +0x10c: the previous point is behind the near plane.
    clipped: Option<bool>,
    /// +0x110: the contour's start vertex.
    start: Option<Vertex>,
    /// +0x114: the previous vertex.
    previous: Option<Vertex>,
    /// +0x118/+0x11c: the last circle normal byte and its view-space axis.
    axis: Option<(u8, [i32; 3])>,
    /// +0x128 bit 7 (from record byte +2): abandon instead of clipping.
    flag: bool,
    entries: Vec<Entry>,
}

fn unset<T: Copy>(value: Option<T>, field: &str) -> Result<T, String> {
    value.ok_or_else(|| {
        format!("Polygon segment reads {field} before this polygon set it (left by earlier code).")
    })
}

fn z(vertex: &Vertex) -> i32 {
    vertex.view[2]
}

impl State {
    /// F1725: edge from `a` to `b`, clipped at the near plane.
    fn edge(&mut self, a: Vertex, b: Vertex) -> Flow {
        self.previous = Some(b);
        self.open = Some(true);
        if z(&a) < 0x40 {
            if self.flag {
                return Flow::Abandon;
            }
            if z(&b) < 0x40 {
                self.open = Some(false);
                self.clipped = Some(true);
                return Flow::Continue;
            }
            let point = near_point(a.view, b.view);
            self.entry = Some(point);
            self.clipped = Some(false);
            self.entries.push(Entry::Edge([point, b.screen]));
        } else if z(&b) < 0x40 {
            if self.flag {
                return Flow::Abandon;
            }
            let point = near_point(b.view, a.view);
            self.clipped = Some(true);
            self.entries.push(Entry::Edge([a.screen, point]));
        } else {
            self.clipped = Some(false);
            self.entries.push(Entry::Edge([a.screen, b.screen]));
        }
        Flow::Continue
    }

    /// F1731: edge from the previous point to `point`.
    fn line_to(&mut self, point: Vertex) -> Result<Flow, String> {
        if !unset(self.clipped, "+0x10c")? {
            let previous = unset(self.previous, "+0x114")?;
            if z(&point) >= 0x40 {
                self.previous = Some(point);
                self.entries.push(Entry::To(point.screen));
            } else if self.flag {
                return Ok(Flow::Abandon);
            } else {
                let near = near_point(point.view, previous.view);
                self.previous = Some(point);
                self.clipped = Some(true);
                self.entries.push(Entry::To(near));
            }
        } else if z(&point) >= 0x40 {
            self.reenter(point)?;
        } else {
            self.previous = Some(point);
        }
        Ok(Flow::Continue)
    }

    /// F1732: the outline comes back in front of the near plane at `point`.
    fn reenter(&mut self, point: Vertex) -> Result<(), String> {
        let previous = unset(self.previous, "+0x114")?;
        self.previous = Some(point);
        self.clipped = Some(false);
        let near = near_point(previous.view, point.view);
        if unset(self.open, "+0x104")? {
            self.entries.push(Entry::To(near));
            self.entries.push(Entry::To(point.screen));
        } else {
            self.open = Some(true);
            self.entry = Some(near);
            self.entries.push(Entry::Edge([near, point.screen]));
        }
        Ok(())
    }

    /// F1735: curve from `from` through `first` and `second` to `end`; any of the
    /// four behind the near plane makes it an F1725 edge from `from` to `end`.
    fn curve(&mut self, from: Vertex, first: Vertex, second: Vertex, end: Vertex) -> Flow {
        if [&end, &from, &first, &second].iter().any(|v| z(v) < 0x40) {
            return self.edge(from, end);
        }
        self.previous = Some(end);
        self.clipped = Some(false);
        self.open = Some(true);
        self.entries.push(Entry::Curve([
            from.screen,
            first.screen,
            second.screen,
            end.screen,
        ]));
        Flow::Continue
    }

    /// F1723 and the end of F1721: close an open contour back to its start, then
    /// back to the point where it entered if it is still behind.
    fn close(&mut self) -> Result<Flow, String> {
        if unset(self.open, "+0x104")? {
            let start = unset(self.start, "+0x110")?;
            if let Flow::Abandon = self.line_to(start)? {
                return Ok(Flow::Abandon);
            }
            if unset(self.clipped, "+0x10c")? {
                let entry = unset(self.entry, "+0x108")?;
                self.entries.push(Entry::To(entry));
            }
        }
        Ok(Flow::Continue)
    }
}

/// F1726 from an arbitrary position: skip segment records by `DATA_007819` sizes
/// until a terminator; return its position.
fn skip_from(bytes: &[u8], mut position: usize) -> Result<usize, String> {
    loop {
        let word = bytes
            .get(position..position + 2)
            .map(|w| u16::from_le_bytes([w[0], w[1]]))
            .ok_or("Polygon abandonment skips past the record.")?;
        let index = segment_index(word)?;
        if index == 0 {
            return Ok(position);
        }
        position += 2 + DATA_SIZES[index];
    }
}

/// Run the segment handlers of one drawn (not culled) polygon after F1721 has keyed
/// and allocated it. `resolve` is F1471 behind the callers' cache check (it projects
/// and stamps). `bytes` are the raw segment bytes, needed for the original skip.
pub fn run(
    segments: &[Segment],
    bytes: &[u8],
    flag: bool,
    inputs: &Inputs<'_>,
    resolve: &mut dyn FnMut(u8) -> Result<Vertex, String>,
) -> Result<Outline, String> {
    let mut state = State {
        flag,
        ..State::default()
    };
    let mut position = 0;
    let mut abandoned = false;
    for segment in segments {
        let data = position + 2;
        let flow = match *segment {
            Segment::CurveStart {
                start,
                controls,
                end,
            } => {
                let from = resolve(start)?;
                state.start = Some(from);
                let first = resolve(controls[0])?;
                let second = resolve(controls[1])?;
                let end = resolve(end)?;
                state.curve(from, first, second, end)
            }
            Segment::LineStart { start, end } => {
                let from = resolve(start)?;
                state.start = Some(from);
                let end = resolve(end)?;
                state.edge(from, end)
            }
            Segment::Line { vertex } => {
                let point = resolve(vertex)?;
                state.line_to(point)?
            }
            Segment::Curve { controls, end } => {
                let from = unset(state.previous, "+0x114")?;
                if z(&from) >= 0x40 {
                    let first = resolve(controls[0])?;
                    let second = resolve(controls[1])?;
                    let end = resolve(end)?;
                    state.curve(from, first, second, end)
                } else {
                    // F1733 behind the near plane: only the first control point.
                    let first = resolve(controls[0])?;
                    if z(&first) < 0x40 {
                        state.previous = Some(first);
                    } else {
                        state.reenter(first)?;
                    }
                    Flow::Continue
                }
            }
            Segment::Close => state.close()?,
            Segment::Circle {
                centre,
                normal,
                radius,
            } => {
                let centre = resolve(centre)?;
                if z(&centre) <= 0x40 {
                    if state.flag {
                        // F1736 passes the cursor at its own data bytes to F1726.
                        if skip_from(bytes, data)? + 2 != bytes.len() {
                            return Err(
                                "Polygon circle abandonment skips to a misaligned terminator."
                                    .to_owned(),
                            );
                        }
                        abandoned = true;
                        break;
                    }
                } else {
                    let axis = match state.axis {
                        Some((cached, axis)) if cached == normal => axis,
                        _ => {
                            let axis = circle_axis(normal, inputs)?;
                            state.axis = Some((normal, axis));
                            axis
                        }
                    };
                    let radius = u32::from(radius).wrapping_shl(inputs.scale as u32 & 31) as i32;
                    if let Some([a, b]) = circle(centre.view, axis, radius) {
                        state.entries.push(Entry::Curve(a));
                        state.entries.push(Entry::Curve(b));
                    }
                }
                state.open = Some(false);
                Flow::Continue
            }
        };
        if let Flow::Abandon = flow {
            abandoned = true;
            break;
        }
        position = data + segment_size(segment);
    }
    if !abandoned {
        // After the terminator F1721 closes the contour like F1723, but an
        // abandonment there would make F1726 skip the following commands.
        if let Flow::Abandon = state.close()? {
            return Err(
                "Polygon close abandons past its terminator (F1726 would skip later commands)."
                    .to_owned(),
            );
        }
    }
    Ok(if abandoned {
        Outline {
            entries: Vec::new(),
            abandoned: true,
        }
    } else {
        Outline {
            entries: state.entries,
            abandoned: false,
        }
    })
}

fn segment_size(segment: &Segment) -> usize {
    match segment {
        Segment::CurveStart { .. } => 4,
        Segment::LineStart { .. } | Segment::Curve { .. } | Segment::Circle { .. } => 2,
        Segment::Line { .. } | Segment::Close => 0,
    }
}

/// F1736's axis: normal record `(n >> 1) - 1` bytes +2..+4 (x negated for odd n),
/// rotated by the transposed vertex frame with F1521 products.
fn circle_axis(normal: u8, inputs: &Inputs<'_>) -> Result<[i32; 3], String> {
    let record = usize::from(normal >> 1)
        .checked_sub(1)
        .and_then(|pair| inputs.normal_records.get(pair * 6..pair * 6 + 6))
        .ok_or_else(|| format!("Polygon circle normal {normal} is not a model normal record."))?;
    let mut vector = [2, 3, 4].map(|i| i32::from(record[i] as i8));
    if normal & 1 != 0 {
        vector[0] = vector[0].wrapping_neg();
    }
    let m = inputs.matrix;
    Ok(std::array::from_fn(|j| {
        multiply(m[j], vector[0])
            .wrapping_add(multiply(m[3 + j], vector[1]))
            .wrapping_add(multiply(m[6 + j], vector[2]))
    }))
}
