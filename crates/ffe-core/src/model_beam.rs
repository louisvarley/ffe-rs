//! F1760/F1761: opcode 17's beam, a tube between two vertices (a ship's landing-gear
//! legs). F1761 builds an ellipse at each end (F1553, then F1555's six projections),
//! keys the beam by its midpoint, and queues one complex-polygon entry: the tube's side,
//! split along its length into an unlit and a lit half when the material is lit and the
//! light runs across the beam, else filled in one colour; then each end whose cap normal
//! faces the viewer as a filled ellipse. The entry ops are the F1794 family the complex
//! polygons use (F1795 edge, F1796 edge from the last point, F1797 curve, F1798 move,
//! F1791 fill and continue, F1799 fill), so the outlines here are [`Entry`] lists whose
//! edges [`crate::model_polygon::edges`] gives F1560. See `docs/notes/beams.md`.
//!
//! Colours are left as [`BeamColour`] specifications: F1691 needs the texture descriptor
//! table, which the caller holds.
use crate::model::multiply;
use crate::model_commands::{material, Beam};
use crate::model_instance::Vertex;
use crate::model_math::shift_down;
use crate::model_normals::Normal;
use crate::model_polygon::{circle_points, Entry};

/// What F1761 reads from the instance besides the two vertices.
pub struct BeamInputs<'a> {
    /// The vertex frame at +0x14, which turns the beam's axis normal.
    pub matrix: [i32; 9],
    /// The instance scale at +0x148, which shifts the radii.
    pub scale: i32,
    /// The F1777 key scale.
    pub key_scale: i32,
    /// The model's normal records (+0x154): six bytes per mirrored pair.
    pub normal_records: &'a [u8],
    /// The instance light vector at +0x134.
    pub light: [i32; 3],
    /// The evaluated normals (`DATA_009201`), for the end caps.
    pub normals: &'a [Normal],
}

/// How the caller colours one fill (F1694/F1693, then F1691 and F1326).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeamColour {
    /// The beam's material through F1694 and F1691 (no normal colour); `shaded` adds
    /// the instance's shade 7 (`+0x84 + 21`) byte by byte, the lit half.
    Side { shaded: bool },
    /// An end cap: its own material through F1693 with the cap normal's colour, then F1691.
    Cap { material: u16, normal: [u8; 3] },
}

/// One F1791/F1799 fill: its outline entries and its colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeamFill {
    pub entries: Vec<Entry>,
    pub colour: BeamColour,
}

/// The queue entry F1761 leaves for one beam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BeamOutline {
    /// F1777 of the midpoint between the two vertices' views.
    pub key: u32,
    /// Fills in queue order.
    pub fills: Vec<BeamFill>,
    /// The two ends' views and their ellipse vectors (`u`, `v`), for the modern engine.
    pub ends: [BeamEnd; 2],
}

/// One end of the tube in view space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeamEnd {
    pub centre: [i32; 3],
    pub u: [i32; 3],
    pub v: [i32; 3],
}

/// F1551: the cross product `a × b`, each component's 32-bit difference shifted right 15.
fn cross(a: [i32; 3], b: [i32; 3]) -> [i32; 3] {
    [
        a[1].wrapping_mul(b[2])
            .wrapping_sub(a[2].wrapping_mul(b[1]))
            >> 15,
        a[2].wrapping_mul(b[0])
            .wrapping_sub(a[0].wrapping_mul(b[2]))
            >> 15,
        a[0].wrapping_mul(b[1])
            .wrapping_sub(a[1].wrapping_mul(b[0]))
            >> 15,
    ]
}

/// The 32-bit (wrapping) dot product of the `imul` sums.
fn dot(a: [i32; 3], b: [i32; 3]) -> i32 {
    a[0].wrapping_mul(b[0])
        .wrapping_add(a[1].wrapping_mul(b[1]))
        .wrapping_add(a[2].wrapping_mul(b[2]))
}

/// F1465: a vector's length, the square root (F1467) of the 32-bit sum of squares.
fn length(v: [i32; 3]) -> i32 {
    crate::model_math::table_root(dot(v, v) as u32) as i32
}

/// F1553 with F1556 and F1555: an ellipse around `centre` (view space) on the beam's
/// `axis`. `u` lies across both the axis and the line of sight, scaled to `radius`;
/// `v = (u × axis) * 0xaaaa >> 15`, the cubic control offset. A zero `u` takes F1556's
/// fixed vectors. `None` when a projection fails.
pub fn ellipse(
    centre: [i32; 3],
    axis: [i32; 3],
    radius: i32,
) -> Option<(BeamEnd, [[[i16; 2]; 4]; 2])> {
    let (towards, _) = shift_down(centre);
    let across = cross(towards, axis);
    let size = length(across);
    let (u, v) = if size == 0 {
        ([radius, 0, 0], [0, radius.wrapping_mul(0xaaaa) >> 15, 0])
    } else {
        let scale = radius.wrapping_shl(15).checked_div(size)?;
        let u = across.map(|c| c.wrapping_mul(scale) >> 15);
        let v = cross(u, axis).map(|c| c.wrapping_mul(0xaaaa) >> 15);
        (u, v)
    };
    let curves = circle_points(centre, u, v)?;
    Some((BeamEnd { centre, u, v }, curves))
}

/// The word F1761 reads at `DATA_007976 + 2 * index`: the 128-entry arc cosine table
/// (`acos(x) * 32767 / pi` for x = i / 64, i = 0..63, and x = (i - 128) / 64 above),
/// or for a negative index the arc tangent table just before it
/// (`atan(i / 128) * 32767 / pi`). Both are computed: they match the supplied
/// executable word for word (`tests::arc_tables_match_the_original`), except the one
/// entry the original stores as 32766 (acos(-1)).
pub fn arc_word(index: i32) -> Option<i16> {
    let scale = 32767.0 / std::f64::consts::PI;
    match index {
        -128..=-1 => {
            let i = f64::from(index + 128);
            Some(((i / 128.0).atan() * scale).floor() as i16)
        }
        64 => Some(32766),
        0..=127 => {
            let x = f64::from(if index < 64 { index } else { index - 128 }) / 64.0;
            Some((x.acos() * scale).floor() as i16)
        }
        _ => None,
    }
}

/// F1549's forward-difference state over a cubic, and F1550's emission.
struct Curve {
    position: [i32; 2],
    first: [i32; 2],
    second: [i32; 2],
    third: [i32; 2],
    counter: i32,
    shift: u32,
}

impl Curve {
    /// F1549: `2^n` steps (n = 2..5 from the Manhattan distance between the first point
    /// and the four points' mean), the counter at `2^n - 1`.
    fn new(points: [[i16; 2]; 4]) -> Self {
        let [a, b, c, d] = points.map(|p| p.map(i32::from));
        let distance = (0..2)
            .map(|k| ((a[k] + b[k] + c[k] + d[k]) / 4 - a[k]).wrapping_abs())
            .fold(0i32, i32::wrapping_add) as u32;
        let n = match distance {
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
        Self {
            position: a.map(|v| v.wrapping_shl(16)),
            first: std::array::from_fn(|k| {
                cubic[k].wrapping_add(linear[k]).wrapping_add(square[k])
            }),
            second: std::array::from_fn(|k| {
                square[k]
                    .wrapping_mul(2)
                    .wrapping_add(cubic[k].wrapping_mul(6))
            }),
            third: cubic.map(|v| v.wrapping_mul(6)),
            counter: (1 << n) - 1,
            shift: 16 - n,
        }
    }

    /// The current point, as F1550 and F1761 store it (`>> 16`, a word).
    fn point(&self) -> [i16; 2] {
        self.position.map(|v| (v >> 16) as i16)
    }

    /// F1761's split `limit` in this curve's counter units (`sar` by `16 - n`).
    fn limit(&self, split: i32) -> i32 {
        split >> self.shift
    }

    /// F1550: step and emit a point (F1796) at least once, while the counter, decremented
    /// after each step, stays at or above `limit`; nothing when the counter is spent.
    fn emit(&mut self, limit: i32, out: &mut Outline) {
        if self.counter < 0 {
            return;
        }
        loop {
            for k in 0..2 {
                self.position[k] = self.position[k].wrapping_add(self.first[k]);
                self.first[k] = self.first[k].wrapping_add(self.second[k]);
                self.second[k] = self.second[k].wrapping_add(self.third[k]);
            }
            out.to(self.point());
            self.counter -= 1;
            if self.counter < limit {
                break;
            }
        }
    }
}

/// The entries of one fill being built, with F1798's pending move.
#[derive(Default)]
struct Outline {
    entries: Vec<Entry>,
    moved: Option<[i16; 2]>,
}

impl Outline {
    fn edge(&mut self, a: [i16; 2], b: [i16; 2]) {
        self.moved = None;
        self.entries.push(Entry::Edge([a, b]));
    }

    /// F1796: an edge from the last point; after an F1798 move, from the moved-to point.
    fn to(&mut self, point: [i16; 2]) {
        match self.moved.take() {
            Some(from) => self.entries.push(Entry::Edge([from, point])),
            None => self.entries.push(Entry::To(point)),
        }
    }

    /// F1798: the next F1796 starts from `point`, with no edge to it.
    fn move_to(&mut self, point: [i16; 2]) {
        self.moved = Some(point);
    }

    fn curve(&mut self, points: [[i16; 2]; 4]) {
        self.moved = None;
        self.entries.push(Entry::Curve(points));
    }

    fn fill(&mut self, colour: BeamColour, fills: &mut Vec<BeamFill>) {
        self.moved = None;
        fills.push(BeamFill {
            entries: std::mem::take(&mut self.entries),
            colour,
        });
    }
}

/// F1761 for `beam` between the projected vertices `a` and `b`. `Ok(None)` when either
/// end is not in front (view z <= 0x40) or an ellipse fails to project: nothing is
/// queued. Errors mark reads the original makes outside the data the port models.
pub fn run(
    beam: &Beam,
    a: &Vertex,
    b: &Vertex,
    inputs: &BeamInputs<'_>,
) -> Result<Option<BeamOutline>, String> {
    if a.view[2] <= 0x40 || b.view[2] <= 0x40 {
        return Ok(None);
    }
    let [first, second, cap_a, cap_b] = beam.parameters.map(i32::from);
    // Radii: the word's high byte, shifted by the instance scale (`shr` then `shl cl`).
    let radius = |word: i32| ((word as u32) >> 8).wrapping_shl(inputs.scale as u32 & 31) as i32;
    // The axis: the first word's normal record `(w & 0x7e) >> 1`, bytes +2..+4 shifted
    // left 8 (x negated for an odd word), turned by the vertex frame (F1669).
    let pair = ((first & 0x7e) >> 1) as usize;
    let record = pair
        .checked_sub(1)
        .and_then(|pair| inputs.normal_records.get(pair * 6 + 2..pair * 6 + 5))
        .ok_or_else(|| format!("Beam axis normal {pair} is not a model normal record."))?;
    let mut vector = [0, 1, 2].map(|i| i32::from(record[i] as i8) << 8);
    if first & 1 != 0 {
        vector[0] = vector[0].wrapping_neg();
    }
    let m = inputs.matrix;
    let axis: [i32; 3] = std::array::from_fn(|j| {
        multiply(vector[0], m[j])
            .wrapping_add(multiply(vector[1], m[3 + j]))
            .wrapping_add(multiply(vector[2], m[6 + j]))
    });
    let radius_a = radius(first);
    let Some((end_a, [near_a, far_a])) = ellipse(a.view, axis, radius_a) else {
        return Ok(None);
    };
    let Some((end_b, [near_b, far_b])) = ellipse(b.view, axis, radius(second)) else {
        return Ok(None);
    };
    let middle: [i32; 3] = std::array::from_fn(|k| a.view[k].wrapping_add(b.view[k]) / 2);
    let key = crate::model_depth::key(middle, inputs.key_scale);

    let decoded = material(beam.material);
    // The light across the beam: lit materials with |axis × light| under 0x1000 split
    // the side into two shades.
    let across = if decoded.unlit {
        None
    } else {
        Some(length(cross(axis, inputs.light))).filter(|size| *size < 0x1000)
    };
    let mut fills = Vec::new();
    let mut out = Outline::default();
    let [a0, _, _, _] = near_a;
    let [b0, _, _, b1] = near_b;
    match across {
        Some(size) => {
            let (mut colours, mut split) = ([false, true], {
                let denominator = (size.wrapping_mul(radius_a) >> 15).wrapping_add(3);
                let ratio = dot(axis, end_a.u)
                    .checked_div(denominator)
                    .ok_or("Beam split divides by zero.")?;
                let word = arc_word(ratio >> 9).ok_or_else(|| {
                    format!("Beam split reads outside the arc tables ({}).", ratio >> 9)
                })?;
                i32::from(word) * 2
            });
            if dot(end_a.u, inputs.light) < 0 {
                split = split.wrapping_neg();
                colours.swap(0, 1);
            }
            let mut curve_b = Curve::new(near_b);
            let mut curve_a = Curve::new(near_a);
            // The first half: A.P0 -> B.P0, B's curve to the split, then from A.P0 along
            // A's curve to the split and across to B's split point.
            out.edge(a0, b0);
            let limit = curve_b.limit(split);
            curve_b.emit(limit, &mut out);
            out.move_to(a0);
            let limit = curve_a.limit(split);
            curve_a.emit(limit, &mut out);
            out.to(curve_b.point());
            out.fill(BeamColour::Side { shaded: colours[0] }, &mut fills);
            // The second half: across the split, the rest of A's curve, A.P1 -> B.P1,
            // then from B's split point along the rest of B's curve.
            out.edge(curve_b.point(), curve_a.point());
            curve_a.emit(0, &mut out);
            out.to(b1);
            out.move_to(curve_b.point());
            curve_b.emit(0, &mut out);
            out.fill(BeamColour::Side { shaded: colours[1] }, &mut fills);
        }
        None => {
            // One fill: A's curve, to B.P1, B's curve backwards, back to A.P0.
            let [p0, p1, p2, p3] = near_b;
            out.curve(near_a);
            out.to(b1);
            out.curve([p3, p2, p1, p0]);
            out.to(a0);
            out.fill(BeamColour::Side { shaded: false }, &mut fills);
        }
    }
    // The caps: an end whose word has bit 7 and whose normal (`w & 0x7f`) is not culled
    // gets its whole ellipse, in the cap material with the normal's colour.
    for (word, material, near, far) in [
        (first, cap_a, near_a, far_a),
        (second, cap_b, near_b, far_b),
    ] {
        if word & 0x80 == 0 {
            continue;
        }
        let normal = inputs
            .normals
            .get((word & 0x7f) as usize)
            .ok_or_else(|| format!("Beam cap normal {} is out of range.", word & 0x7f))?;
        if normal.culled {
            continue;
        }
        out.curve(near);
        out.curve(far);
        out.fill(
            BeamColour::Cap {
                material: material as u16,
                normal: normal.colour,
            },
            &mut fills,
        );
    }
    Ok(Some(BeamOutline {
        key,
        fills,
        ends: [end_a, end_b],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arc_tables_match_the_original() {
        // FNV-1a over the 256 words F1761 can read (atan then acos), as the supplied
        // executable stores them (`DATA_007976` and the table before it).
        let mut hash = 0x811c_9dc5u32;
        for index in -128..128 {
            for byte in arc_word(index).unwrap().to_le_bytes() {
                hash = (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193);
            }
        }
        assert_eq!(arc_word(0), Some(16383));
        assert_eq!(arc_word(64), Some(32766));
        assert_eq!(arc_word(127), Some(16546));
        assert_eq!(arc_word(-1), Some(0x1fd6));
        assert_eq!(arc_word(-128), Some(0));
        assert_eq!(arc_word(128), None);
        assert_eq!(hash, ARC_TABLES_FNV);
    }

    /// FNV-1a of the original's 256 words (computed from the supplied data).
    const ARC_TABLES_FNV: u32 = 0x411a_900b;

    #[test]
    fn the_split_curves_emit_as_f1550_counts() {
        let curve_points = [[0, 0], [10, 0], [20, 0], [30, 0]];
        let mut curve = Curve::new(curve_points);
        // Mean 15 from the first point: n = 3, eight steps.
        assert_eq!(curve.counter, 7);
        let mut out = Outline::default();
        // A limit of 4 emits 4 points (counter 7..4), the rest emits the other 4.
        curve.emit(4, &mut out);
        assert_eq!(out.entries.len(), 4);
        curve.emit(0, &mut out);
        assert_eq!(out.entries.len(), 8);
        assert_eq!(curve.point(), [30, 0]);
        // A spent curve emits nothing.
        curve.emit(0, &mut out);
        assert_eq!(out.entries.len(), 8);
    }

    #[test]
    fn a_move_starts_the_next_edge_at_its_point() {
        let mut out = Outline::default();
        out.edge([0, 0], [1, 1]);
        out.move_to([5, 5]);
        out.to([6, 6]);
        out.to([7, 7]);
        assert_eq!(
            out.entries,
            [
                Entry::Edge([[0, 0], [1, 1]]),
                Entry::Edge([[5, 5], [6, 6]]),
                Entry::To([7, 7])
            ]
        );
    }
}
