//! The planet surface in draw mode, ported from `ffe.asm`: F1876 with its mode
//! argument 0 (as F1874 calls it), F1833's set-up, F1834's face preparation, and
//! the recursive subdivision F1868 (and F1869, its twin for planets with a sea)
//! which hands every triangle fine enough, or far enough off, to the emitter
//! (`DATA_009270`, F1840). Verified against the executed original
//! (`docs/notes/terrain.md`).
//!
//! The subdivision shares the midpoint routines and the arenas with the distance
//! query (`terrain.rs`); what differs is that the nodes carry screen coordinates
//! (`Projection::Flat`) and the recursion culls against the view.

use crate::model::multiply;
use crate::model_depth;
use crate::model_instance::project;
use crate::model_math::table_root;
use crate::terrain::{
    extent, find_msb, hash, row_length, tables, Abort, Fault, Node, Projection, Slot, Stop,
    Terrain, Tree, LOCAL_NODES, NIL,
};

/// Everything F1874 hands F1876 besides the feature records.
#[derive(Debug, Clone, Copy)]
pub struct Scene {
    /// The object's `+0xa0`.
    pub seed: u32,
    /// The planet centre in view space (record units).
    pub origin: [i32; 3],
    /// The three axis vectors (nine dwords).
    pub axes: [i32; 9],
    /// The planet type (record word `+6`).
    pub kind: usize,
    /// The game's detail setting (`DATA_008812`).
    pub detail: i32,
    /// The instance's `+0x130`, the scale of F1777's depth keys.
    pub zoom: i32,
}

/// What a draw did besides emitting triangles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawn {
    /// Bytes of the stream the records took.
    pub consumed: usize,
    /// The arena that overflowed and ended the drawing early, if any.
    pub aborted: Option<u8>,
}

/// One triangle handed to the emitter.
pub struct Emission {
    /// The three nodes (handles into the terrain).
    pub nodes: [usize; 3],
    /// The normal, scaled to 15 bits (F1827).
    pub normal: [i32; 3],
    /// F1777's depth key of the first node, pushed forward by the slack.
    pub key: u32,
    /// The subdivision level (`DATA_009286`).
    pub level: i32,
}

/// F1826: the vector reduced to about 14 bits.
fn reduce(v: [i32; 3]) -> [i32; 3] {
    let shift = extent(v) - 13;
    if shift >= 0 {
        v.map(|c| c >> shift)
    } else {
        v
    }
}

/// F1465: the length of a vector (the sum of its squares through F1467).
fn length(v: [i32; 3]) -> i32 {
    let sum = v
        .iter()
        .fold(0i32, |sum, c| sum.wrapping_add(c.wrapping_mul(*c)));
    table_root(sum as u32) as i32
}

/// F1827: the unit normal (15 bits) of the triangle of three view positions.
fn normal(a: [i32; 3], b: [i32; 3], c: [i32; 3]) -> Result<[i32; 3], Fault> {
    let mut first: [i32; 3] = std::array::from_fn(|k| b[k].wrapping_sub(a[k]));
    let mut second: [i32; 3] = std::array::from_fn(|k| c[k].wrapping_sub(a[k]));
    for v in [&mut first, &mut second] {
        let shift = extent(*v) - 6;
        if shift > 0 {
            *v = v.map(|c| c >> shift);
        }
    }
    let cross = [
        first[1]
            .wrapping_mul(second[2])
            .wrapping_sub(first[2].wrapping_mul(second[1])),
        first[2]
            .wrapping_mul(second[0])
            .wrapping_sub(first[0].wrapping_mul(second[2])),
        first[0]
            .wrapping_mul(second[1])
            .wrapping_sub(first[1].wrapping_mul(second[0])),
    ];
    let divisor = length(cross).wrapping_add(1);
    let mut out = [0; 3];
    for k in 0..3 {
        let numerator = i64::from(cross[k].wrapping_shl(15));
        if divisor == 0 {
            return Err(Fault::Division);
        }
        let quotient = numerator / i64::from(divisor);
        if quotient != i64::from(quotient as i32) {
            return Err(Fault::Division);
        }
        out[k] = quotient as i32;
    }
    Ok(out)
}

/// What the recursion keeps beyond the terrain itself.
struct Pass<'t, 'e> {
    tree: &'t Tree,
    /// The planet has a sea (F1869 rather than F1868).
    sea: bool,
    /// `DATA_009264`: the depth added to a triangle's key position.
    slack: i32,
    zoom: i32,
    /// The midpoint routines for levels below five and from five on.
    coarse: usize,
    fine: usize,
    emit: &'e mut dyn FnMut(&Terrain<'_>, Emission),
}

impl Terrain<'_> {
    /// F1876 in draw mode: subdivide the planet for the view and hand every
    /// triangle to `emit`. `stream` holds the model's feature records (after the
    /// surface command's operands).
    pub fn draw(
        &mut self,
        scene: &Scene,
        stream: &[u8],
        emit: &mut dyn FnMut(&Terrain<'_>, Emission),
    ) -> Result<Drawn, Fault> {
        self.projection = Projection::Flat;
        let (tree, consumed) = Tree::build(stream)?;
        let rows = [
            [scene.axes[0], scene.axes[1], scene.axes[2]],
            [scene.axes[3], scene.axes[4], scene.axes[5]],
            [scene.axes[6], scene.axes[7], scene.axes[8]],
        ];
        let longest = rows.iter().map(|r| row_length(*r)).fold(i32::MIN, i32::max);
        let bound = multiply(longest, 0x5400_0000).wrapping_mul(2);
        // DATA_007837: the detail, one more for each step the view is away.
        let distance = row_length(scene.origin) as u32;
        let mut adjust = scene.detail - 1;
        if (longest.wrapping_add(longest >> 4) as u32) < distance {
            adjust += 1;
            if (longest.wrapping_add(longest >> 3) as u32) < distance {
                adjust += 1;
            }
        }
        self.level_base = find_msb(bound as u32) + adjust;
        self.reset();
        let mut seed = scene.seed;
        let polar = self.type_dword(scene.kind, 0) == 2;
        for corner in 0..LOCAL_NODES {
            let at = tables::CORNERS + 12 * corner;
            let direction = [
                self.table_dword(at),
                self.table_dword(at + 4),
                self.table_dword(at + 8),
            ];
            let mut node = Node::default();
            for axis in 0..3 {
                let position = (0..3).fold(0i32, |sum, row| {
                    sum.wrapping_add(multiply(direction[row], scene.axes[3 * row + axis]))
                });
                node.set_dword(0x28 + 4 * axis, position);
                node.set_dword(4 + 4 * axis, position.wrapping_add(scene.origin[axis]));
            }
            if let Some([x, y]) = project([node.x(), node.y(), node.z()]) {
                node.set_word(0, x);
                node.set_word(2, y);
            }
            seed = hash(seed);
            node.set_dword(
                0x3c,
                if polar {
                    (direction[1] >> 5).wrapping_add(0x400_0000)
                } else {
                    seed as i32 >> 4
                },
            );
            self.nodes[corner] = node;
        }
        let far = scene.origin[2] > longest.wrapping_mul(2);
        let span = self.type_dword(scene.kind, 8) as usize;
        let slack = 0x1dcd_6500_i32 >> (scene.zoom & 31);
        let mut aborted = None;
        let result = self.draw_faces(scene, &tree, far, span, bound, slack, emit);
        if let Err(stop) = result {
            match stop {
                Stop::Abort(Abort(code)) => aborted = Some(code),
                Stop::Fault(fault) => return Err(fault),
            }
        }
        Ok(Drawn { consumed, aborted })
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_faces(
        &mut self,
        scene: &Scene,
        tree: &Tree,
        far: bool,
        span: usize,
        bound: i32,
        slack: i32,
        emit: &mut dyn FnMut(&Terrain<'_>, Emission),
    ) -> Result<(), Stop> {
        let mut edges = [NIL; 30];
        for (k, edge) in edges.iter_mut().enumerate() {
            let record = self.alloc_edge()?;
            self.edges[record].head = self.table_dword(tables::EDGES + 8 * k) as usize;
            *edge = record;
        }
        for face in 0..20 {
            let entry = tables::FACES + 24 * face;
            let word = |t: &Self, k: usize| t.table_dword(entry + 4 * k) as usize;
            let corners = [word(self, 0), word(self, 1), word(self, 2)];
            let sides = [
                edges[word(self, 3)],
                edges[word(self, 4)],
                edges[word(self, 5)],
            ];
            let face_tree = Some(tree.nodes[0].children[face]).filter(|t| *t != NIL);
            // F1834.
            let slot = span + 8 * usize::from(face_tree.is_some()) + 2 * usize::from(!far);
            let routine =
                |t: &Self, k: usize| t.table_dword(tables::MIDPOINTS + 4 * (slot + k)) as usize - 1;
            let (coarse, fine) = (routine(self, 4), routine(self, 0));
            self.prepare_face(corners, face_tree, tree);
            let mut pass = Pass {
                tree,
                sea: span & 1 != 0 && span >= 8,
                slack,
                zoom: scene.zoom,
                coarse,
                fine,
                emit,
            };
            self.descend(&mut pass, corners, sides, 0, bound, face_tree)?;
        }
        Ok(())
    }

    /// F1868 (F1869 with a sea): one triangle of the subdivision.
    fn descend(
        &mut self,
        pass: &mut Pass,
        [a, b, c]: [usize; 3],
        [e3, e4, e5]: [usize; 3],
        level: i32,
        bound: i32,
        tree: Option<usize>,
    ) -> Result<(), Stop> {
        let pos = |t: &Self, n: usize| [t.nodes[n].x(), t.nodes[n].y(), t.nodes[n].z()];
        let (na, nb, nc) = (self.nodes[a], self.nodes[b], self.nodes[c]);
        // The slack: how far the surface can lie from the triangle.
        let slack = if level < 10 {
            bound
        } else {
            let (low, high) = {
                let (x, y) = (na.clamped(), nb.clamped());
                let (mut low, mut high) = if y > x { (x, y) } else { (y, x) };
                let z = nc.clamped();
                if z < low {
                    low = z;
                } else if high < z {
                    high = z;
                }
                (low, high)
            };
            let sum = na
                .dword(0x28)
                .wrapping_abs()
                .wrapping_add(na.dword(0x2c).wrapping_abs())
                .wrapping_add(na.dword(0x30).wrapping_abs());
            multiply(sum, high.wrapping_sub(low) >> 3).wrapping_add(bound)
        };
        let limit = na.z().wrapping_add(slack);
        if limit < 0x40
            || na.x().wrapping_abs().wrapping_sub(slack) > limit
            || na.y().wrapping_abs().wrapping_sub(slack) > limit
        {
            return Ok(());
        }
        // The sea level moves the detail one level coarser for triangles across it.
        let lv = if pass.sea {
            let below = [na, nb, nc].iter().filter(|n| n.height() <= 0).count();
            if below == 0 || below == 3 {
                level
            } else {
                level - 1
            }
        } else {
            level
        };
        let in_front = na.z() >= 0x40 && nb.z() >= 0x40 && nc.z() >= 0x40;
        if in_front {
            let screen = |n: &Node| (i32::from(n.word(0)), i32::from(n.word(2)));
            let ((ax, ay), (bx, by), (cx, cy)) = (screen(&na), screen(&nb), screen(&nc));
            let area = (cx - ax)
                .wrapping_mul(by - ay)
                .wrapping_sub((cy - ay).wrapping_mul(bx - ax));
            let fine_enough = level > 13
                || (lv > na.level() && lv > nb.level() && lv > nc.level())
                || ((0..0x96).contains(&area) && lv > 8);
            if fine_enough {
                // Emit unless outside the view or facing away.
                let outside = |at: usize| {
                    let code = na.word(at) | nb.word(at) | nc.word(at);
                    self.table_dword(tables::OUTCODES + 4 * (code as usize & 0xf)) != 0
                };
                if outside(0x24) || outside(0x26) {
                    return Ok(());
                }
                let across = pass.sea && {
                    let below = [na, nb, nc].iter().filter(|n| n.height() <= 0).count();
                    below == 1 || below == 2
                };
                if !across && area <= 0 {
                    return Ok(());
                }
                let key =
                    model_depth::key([na.x(), na.y(), na.z().wrapping_add(pass.slack)], pass.zoom);
                let vector =
                    normal(pos(self, a), pos(self, b), pos(self, c)).map_err(Stop::Fault)?;
                (pass.emit)(
                    self,
                    Emission {
                        nodes: [a, b, c],
                        normal: vector,
                        key,
                        level,
                    },
                );
                return Ok(());
            }
            if level < 2 {
                // A triangle wholly beyond the horizon is not drawn.
                let vector =
                    normal(pos(self, a), pos(self, b), pos(self, c)).map_err(Stop::Fault)?;
                let mut beyond = true;
                for n in [&na, &nb, &nc] {
                    let reduced = reduce([n.x(), n.y(), n.z()]);
                    let limit = (-0x5000_i32 >> level).wrapping_mul(length(reduced));
                    let dot = (0..3).fold(0i32, |sum, k| {
                        sum.wrapping_add(vector[k].wrapping_mul(reduced[k]))
                    });
                    if limit <= dot {
                        beyond = false;
                        break;
                    }
                }
                if beyond {
                    return Ok(());
                }
            }
        } else {
            if bound < 0x1f4 {
                return Ok(());
            }
            // Some corner is behind the view: only a triangle wholly to one side of
            // the view's pyramid is dropped.
            let behind = [na, nb, nc].map(|n| n.z().wrapping_abs());
            let beyond = |axis: usize| {
                let v = [na.dword(axis), nb.dword(axis), nc.dword(axis)];
                (behind[0] < v[0] && v[1] > behind[1] && behind[2] < v[2])
                    || (behind[0].wrapping_neg() > v[0]
                        && v[1] < behind[1].wrapping_neg()
                        && behind[2].wrapping_neg() > v[2])
            };
            if beyond(4) || beyond(8) {
                return Ok(());
            }
        }
        self.split(pass, [a, b, c], [e3, e4, e5], level, bound, tree)
    }

    /// The rest of F1868: put the feature layers in place, make the midpoints and
    /// recurse into the four sub-triangles.
    fn split(
        &mut self,
        pass: &mut Pass,
        [a, b, c]: [usize; 3],
        [e3, e4, e5]: [usize; 3],
        level: i32,
        bound: i32,
        tree: Option<usize>,
    ) -> Result<(), Stop> {
        let tree_data = pass.tree;
        let mut saved = [[0i32; 3]; 2];
        if let Some(t) = tree {
            for (layer, saved) in saved.iter_mut().enumerate() {
                let Slot::Record(r) = tree_data.nodes[t].slots[layer] else {
                    continue;
                };
                let record = tree_data.records[r];
                let feature = tables::FEATURES + 12 * record.kind as usize;
                let maps = [
                    self.table_dword(feature) as u8,
                    self.table_dword(feature + 4) as u8,
                ];
                let levels = self.table_dword(feature + 8);
                for l in level..level + levels {
                    if let Some(entry) = self.layers.get_mut(layer) {
                        if let Some(slot) = entry.get_mut(l as usize) {
                            *slot = maps;
                        }
                    }
                }
                for (k, node) in [c, b, a].into_iter().enumerate() {
                    saved[k] = self.nodes[node].dword(0x34 + 4 * layer);
                }
                for (node, [u, v]) in [a, b, c].into_iter().zip(record.corners) {
                    self.nodes[node].set_word(0x34 + 4 * layer, u);
                    self.nodes[node].set_word(0x36 + 4 * layer, v);
                }
            }
        }
        let which = if level < 5 { pass.coarse } else { pass.fine };
        self.midpoint(which, e4, b, c, a, level)?;
        self.midpoint(which, e5, c, a, b, level)?;
        self.midpoint(which, e3, a, b, c, level)?;
        let n_a = self.alloc_edge()?;
        self.edges[n_a].head = self.edges[e4].mid;
        let n_b = self.alloc_edge()?;
        self.edges[n_b].head = self.edges[e5].mid;
        let n_c = self.alloc_edge()?;
        self.edges[n_c].head = self.edges[e3].mid;
        let (m3, m4, m5) = (self.edges[e3].mid, self.edges[e4].mid, self.edges[e5].mid);
        let half = |t: &Self, e: usize, end: usize| {
            if t.edges[e].head == end {
                t.edges[e].first
            } else {
                t.edges[e].second
            }
        };
        let child = |k: usize| {
            tree.map(|t| tree_data.nodes[t].children[k])
                .filter(|c| *c != NIL)
        };
        let next = level + 1;
        let bound = bound >> 1;
        let calls = [
            (
                [a, m3, m5],
                [half(self, e3, a), n_c, half(self, e5, a)],
                child(0),
            ),
            (
                [b, m4, m3],
                [half(self, e4, b), n_a, half(self, e3, b)],
                child(1),
            ),
            (
                [c, m5, m4],
                [half(self, e5, c), n_b, half(self, e4, c)],
                child(2),
            ),
            ([m4, m5, m3], [n_b, n_c, n_a], child(3)),
        ];
        for (nodes, edges, sub) in calls {
            self.descend(pass, nodes, edges, next, bound, sub)?;
        }
        if let Some(t) = tree {
            for (layer, saved) in saved.iter().enumerate() {
                let Slot::Record(r) = tree_data.nodes[t].slots[layer] else {
                    continue;
                };
                let record = tree_data.records[r];
                let feature = tables::FEATURES + 12 * record.kind as usize;
                let levels = self.table_dword(feature + 8);
                for l in level..level + levels {
                    if let Some(slot) = self.layers[layer].get_mut(l as usize) {
                        *slot = [0, 0];
                    }
                }
                for (k, node) in [c, b, a].into_iter().enumerate() {
                    self.nodes[node].set_dword(0x34 + 4 * layer, saved[k]);
                }
            }
        }
        for edge in [n_a, n_b, n_c] {
            self.free_edge(edge);
        }
        Ok(())
    }
}
