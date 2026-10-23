//! The planet terrain engine's query path, ported from `ffe.asm` (F1816-F1834,
//! F1846, F1849-F1867, F1875, F1876 in distance mode). A planet is an
//! icosahedron whose faces are subdivided edge by edge: each edge's midpoint node
//! is made by one of nineteen routines (F1849-F1867), chosen by the planet's
//! type, which place the new vertex and its height. Nodes and edge records come
//! from two fixed arenas whose overflow aborts the whole query. Verified against
//! the executed original (`docs/notes/terrain.md`).

use crate::model::multiply;
use crate::model_math::table_root;

/// Bytes of a node (`0x48`).
pub const NODE_BYTES: usize = 0x48;
/// Nodes the arena holds before the original aborts (F1818 aborts at 0x5dc
/// outstanding).
pub const NODE_LIMIT: u32 = 0x5dc;
/// Edge records the arena holds before the original aborts (F1820 aborts at 0xbb8).
pub const EDGE_LIMIT: u32 = 0xbb8;
/// Nodes the query builds on its own for the icosahedron's twelve corners.
pub const LOCAL_NODES: usize = 12;
/// A null node or edge handle.
pub const NIL: usize = usize::MAX;

/// Offsets into the cache's terrain table block (`DATA_007836`).
pub mod tables {
    /// `DATA_007839`: the displacement scale per level (dwords).
    pub const SCALE: usize = 79;
    /// `DATA_007840`: the edge-vector scale per level (dwords).
    pub const EDGE_SCALE: usize = 207;
    /// `DATA_007841`: the feature types (12 bytes each: height map, multiplier map,
    /// number of levels).
    pub const FEATURES: usize = 335;
    /// `DATA_007843`: the planet types (48 bytes each).
    pub const TYPES: usize = 503;
    /// `DATA_007854`: the midpoint corner adjustments, 32 bytes per routine.
    pub const ADJUST: usize = 5995;
    /// `DATA_007844`: the midpoint routine ranks (dwords, 32 slots).
    pub const MIDPOINTS: usize = 1175;
    /// `DATA_007892`: the icosahedron's twelve corner directions (12 bytes each).
    pub const CORNERS: usize = 6603;
    /// `DATA_007893`: the twenty faces (24 bytes each: corner and edge indexes).
    pub const FACES: usize = 6747;
    /// `DATA_007894`: the thirty edges (8 bytes each: the two corner indexes).
    pub const EDGES: usize = 7227;
}

/// An abort of the whole query (the original's `longjmp`): the code is 1 when the
/// node arena ran out and 2 for the edge records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Abort(pub u8);

/// A vertex of the subdivision (`0x48` bytes), kept as the original's bytes so
/// every field keeps its offset: `+0` and `+2` the map coordinates (words), `+4`,
/// `+8`, `+0xc` the position, `+0x24` and `+0x26` the quadrant words, `+0x28`,
/// `+0x2c`, `+0x30` the edge vector, `+0x34`..`+0x3a` the two layers' map
/// coordinates, `+0x3c` the height, `+0x40` the clamped height and `+0x44` the
/// detail level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    pub bytes: [u8; NODE_BYTES],
}

impl Default for Node {
    fn default() -> Self {
        Self {
            bytes: [0; NODE_BYTES],
        }
    }
}

impl Node {
    pub fn dword(&self, at: usize) -> i32 {
        i32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap())
    }
    pub fn set_dword(&mut self, at: usize, value: i32) {
        self.bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }
    pub fn word(&self, at: usize) -> i16 {
        i16::from_le_bytes(self.bytes[at..at + 2].try_into().unwrap())
    }
    pub fn set_word(&mut self, at: usize, value: i16) {
        self.bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    pub fn x(&self) -> i32 {
        self.dword(4)
    }
    pub fn y(&self) -> i32 {
        self.dword(8)
    }
    pub fn z(&self) -> i32 {
        self.dword(0xc)
    }
    pub fn height(&self) -> i32 {
        self.dword(0x3c)
    }
    pub fn clamped(&self) -> i32 {
        self.dword(0x40)
    }
    pub fn level(&self) -> i32 {
        self.dword(0x44)
    }
}

/// An edge record (F1820, sixteen bytes): the node at its head, the links to the
/// two halves (each another record whose head is the half's far end) and the node
/// made at its midpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    pub head: usize,
    pub first: usize,
    pub mid: usize,
    pub second: usize,
}

/// A feature layer's maps at one level: the height map and the multiplier map as
/// the cache numbers them (0 none, 1 `DATA_007637`, 2 `DATA_007638`).
pub type LayerEntry = [u8; 2];

/// The query's arenas and globals.
pub struct Terrain<'a> {
    table: &'a [u8],
    maps: &'a [u8],
    nodes: Vec<Node>,
    free_nodes: Vec<usize>,
    nodes_out: u32,
    edges: Vec<Edge>,
    free_edges: Vec<usize>,
    edges_out: u32,
    /// `DATA_009269`: the detail level base F1829 counts down from.
    pub level_base: i32,
    /// `DATA_009260`: per layer and level, the maps of the feature at hand.
    pub layers: [[LayerEntry; 32]; 2],
}

/// How a midpoint routine differs from the family's pattern.
#[derive(Debug, Clone, Copy)]
struct Variant {
    /// The edge vector is the sum of the ends' scaled (F1849 family), not copied.
    scaled: bool,
    height: Height,
    /// The detail test uses one level less when the two heights straddle one.
    signed: bool,
    /// The displacement also takes the new height's deviation from the mean.
    deviation: bool,
    /// The height has a clamped copy at `+0x40`.
    clamp: bool,
    /// The routine also carries the two feature layers' map coordinates.
    layers: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Height {
    None,
    Mean,
    Noise,
}

const fn variant(
    scaled: bool,
    height: Height,
    signed: bool,
    deviation: bool,
    clamp: bool,
    layers: bool,
) -> Variant {
    Variant {
        scaled,
        height,
        signed,
        deviation,
        clamp,
        layers,
    }
}

/// F1849 to F1867 in order.
const VARIANTS: [Variant; 19] = [
    variant(true, Height::None, false, false, false, false),
    variant(false, Height::Noise, false, false, false, false),
    variant(false, Height::Noise, true, false, false, false),
    variant(false, Height::Noise, false, true, false, false),
    variant(false, Height::Noise, true, true, true, false),
    variant(true, Height::Noise, false, false, false, false),
    variant(true, Height::Noise, true, false, false, false),
    variant(true, Height::Noise, false, true, false, false),
    variant(true, Height::Noise, true, true, true, false),
    variant(false, Height::Noise, false, false, false, true),
    variant(false, Height::Noise, true, false, false, true),
    variant(false, Height::Noise, false, true, false, true),
    variant(false, Height::Noise, true, true, true, true),
    variant(true, Height::Noise, false, false, false, true),
    variant(true, Height::Noise, true, false, false, true),
    variant(true, Height::Noise, false, true, false, true),
    variant(true, Height::Noise, true, true, true, true),
    variant(false, Height::Mean, false, false, false, false),
    variant(true, Height::Mean, false, false, false, false),
];

/// F1824: a hash of a word.
pub fn hash(value: u32) -> u32 {
    let rotated = value.rotate_left(5).wrapping_add(value);
    value.wrapping_add(rotated.rotate_left(16))
}

/// F1846: the height noise for an edge from the heights at its ends.
pub fn noise(a: i32, b: i32, level: i32) -> i32 {
    if level <= 1 {
        hash((a ^ b) as u32) as i32 >> 4
    } else {
        let spread = (a.wrapping_sub(b)).wrapping_abs();
        multiply(hash((a ^ b) as u32) as i32, spread) >> 1
    }
}

/// F1831 as the distance mode installs it (`DATA_009280`): the quadrant word of a
/// map u coordinate.
pub fn quadrant_u(value: i32) -> i16 {
    quadrant(value, 0xa0, 8)
}

/// F1832 (`DATA_009281`): the same for a v coordinate.
pub fn quadrant_v(value: i32) -> i16 {
    quadrant(value, 0x4f, 8)
}

fn quadrant(value: i32, middle: i32, special: i32) -> i16 {
    if value == -0x7ffe {
        special as i16
    } else if value == middle {
        2
    } else if value >= middle {
        4
    } else {
        1
    }
}

/// F1848 (`DATA_009282`): the quadrant words of a node from the signs of its x
/// and y.
pub fn quadrant_node(node: &mut Node) {
    for (from, to) in [(4, 0x24), (8, 0x26)] {
        let value = node.dword(from);
        node.set_word(
            to,
            if value == 0 {
                2
            } else if value >= 0 {
                4
            } else {
                1
            },
        );
    }
}

fn msb(value: u32) -> i32 {
    31 - value.max(1).leading_zeros() as i32
}

impl<'a> Terrain<'a> {
    /// `table` is the cache's terrain table block and `maps` the two height maps.
    pub fn new(table: &'a [u8], maps: &'a [u8]) -> Self {
        let mut terrain = Self {
            table,
            maps,
            nodes: Vec::new(),
            free_nodes: Vec::new(),
            nodes_out: 0,
            edges: Vec::new(),
            free_edges: Vec::new(),
            edges_out: 0,
            level_base: 0,
            layers: [[[0; 2]; 32]; 2],
        };
        terrain.reset();
        terrain
    }

    /// F1816: empty both arenas. The nodes keep no history (the original's keep
    /// their old bytes; nothing reads a field before writing it).
    pub fn reset(&mut self) {
        self.nodes = vec![Node::default(); LOCAL_NODES + NODE_LIMIT as usize];
        self.free_nodes = (0..NODE_LIMIT as usize)
            .rev()
            .map(|i| LOCAL_NODES + i)
            .collect();
        self.nodes_out = 0;
        self.edges = vec![
            Edge {
                head: NIL,
                first: NIL,
                mid: NIL,
                second: NIL
            };
            EDGE_LIMIT as usize
        ];
        self.free_edges = (0..EDGE_LIMIT as usize).rev().collect();
        self.edges_out = 0;
    }

    /// F1818: a node from the arena.
    pub fn alloc_node(&mut self) -> Result<usize, Abort> {
        self.nodes_out += 1;
        if self.nodes_out >= NODE_LIMIT {
            return Err(Abort(1));
        }
        Ok(self.free_nodes.pop().expect("arena counted"))
    }

    /// F1819: give a node back.
    pub fn free_node(&mut self, node: usize) {
        self.nodes_out -= 1;
        self.free_nodes.push(node);
    }

    /// F1820: an edge record with no links and no midpoint.
    pub fn alloc_edge(&mut self) -> Result<usize, Abort> {
        self.edges_out += 1;
        if self.edges_out >= EDGE_LIMIT {
            return Err(Abort(2));
        }
        let edge = self.free_edges.pop().expect("arena counted");
        let head = self.edges[edge].head;
        self.edges[edge] = Edge {
            head,
            first: NIL,
            mid: NIL,
            second: NIL,
        };
        Ok(edge)
    }

    /// F1821: give an edge record, the chain behind it and everything below back.
    pub fn free_edge(&mut self, edge: usize) {
        let mut current = edge;
        while current != NIL {
            let record = self.edges[current];
            if record.first != NIL {
                self.free_edge(record.first);
            }
            if record.mid != NIL {
                self.free_node(record.mid);
            }
            self.edges_out -= 1;
            self.free_edges.push(current);
            current = record.second;
        }
    }

    pub fn node(&self, handle: usize) -> &Node {
        &self.nodes[handle]
    }

    pub fn node_mut(&mut self, handle: usize) -> &mut Node {
        &mut self.nodes[handle]
    }

    pub fn edge(&self, handle: usize) -> &Edge {
        &self.edges[handle]
    }

    pub fn edge_mut(&mut self, handle: usize) -> &mut Edge {
        &mut self.edges[handle]
    }

    pub fn nodes_out(&self) -> u32 {
        self.nodes_out
    }

    pub fn edges_out(&self) -> u32 {
        self.edges_out
    }

    fn table_dword(&self, at: usize) -> i32 {
        i32::from_le_bytes(self.table[at..at + 4].try_into().unwrap())
    }

    /// F1829: the detail level a node's distance calls for.
    pub fn set_level(&mut self, node: usize) {
        let distance = self.nodes[node].z().max(0x400);
        let level = self.level_base - msb(distance as u32);
        self.nodes[node].set_dword(0x44, if level - 1 < 0 { 1 } else { level });
    }

    /// The map byte a layer reads at a map coordinate.
    fn map_byte(&self, map: u8, index: usize) -> u8 {
        self.maps[(usize::from(map) - 1) * 0x4000 + index]
    }

    /// F1849-F1867 by their order (`which` 0 to 18): make the midpoint of the edge
    /// `edge` between nodes `a` and `b`, `c` being the face's third corner.
    pub fn midpoint(
        &mut self,
        which: usize,
        edge: usize,
        a: usize,
        b: usize,
        c: usize,
        level: i32,
    ) -> Result<(), Abort> {
        let variant = VARIANTS[which];
        let existing = self.edges[edge].mid;
        if existing != NIL {
            if variant.layers {
                self.average_layers(existing, a, b, level);
            }
            return Ok(());
        }
        let node = self.alloc_node()?;
        self.edges[edge].mid = node;
        let (first, second) = if a == self.edges[edge].head {
            (a, b)
        } else {
            (b, a)
        };
        let link = self.alloc_edge()?;
        self.edges[link].head = first;
        self.edges[edge].first = link;
        let link = self.alloc_edge()?;
        self.edges[link].head = second;
        self.edges[edge].second = link;
        if variant.layers {
            self.average_layers(node, a, b, level);
        }
        let (na, nb) = (self.nodes[a], self.nodes[b]);
        if variant.scaled {
            let factor = self.table_dword(tables::EDGE_SCALE + 4 * level as usize);
            for at in [0x28, 0x2c, 0x30] {
                let sum = na.dword(at).wrapping_add(nb.dword(at));
                self.nodes[node].set_dword(at, multiply(sum, factor));
            }
        } else {
            for at in [0x28, 0x2c, 0x30] {
                self.nodes[node].set_dword(at, na.dword(at));
            }
        }
        let tested =
            if variant.signed && (na.height().wrapping_sub(1) ^ nb.height().wrapping_sub(1)) < 0 {
                level - 1
            } else {
                level
            };
        let mean = |at: usize| na.dword(at).wrapping_add(nb.dword(at)) >> 1;
        let mut scale = if variant.scaled {
            self.table_dword(tables::SCALE + 4 * level as usize)
        } else {
            0
        };
        if tested > na.level() && tested > nb.level() {
            // Both ends are coarse: the midpoint of the ends.
            if variant.clamp {
                let m = &mut self.nodes[node];
                if na.clamped() != 0 || nb.clamped() != 0 {
                    let value = mean(0x40);
                    m.set_dword(0x40, value);
                    m.set_dword(0x3c, value);
                } else {
                    m.set_dword(0x3c, mean(0x3c));
                    m.set_dword(0x40, 0);
                }
            } else if variant.height != Height::None {
                self.nodes[node].set_dword(0x3c, mean(0x3c));
            }
            for at in [4, 8, 0xc] {
                self.nodes[node].set_dword(at, mean(at));
            }
            if na.z() < 0x40 || nb.z() < 0x40 {
                quadrant_node(&mut self.nodes[node]);
            } else {
                self.coarse_coordinates(which, node, a, b, c);
            }
        } else {
            // The ends carry detail: displace the midpoint.
            let m_mean = mean(0x3c);
            if variant.height != Height::None {
                let height = if variant.height == Height::Noise {
                    noise(na.height(), nb.height(), level).wrapping_add(m_mean)
                } else {
                    m_mean
                };
                self.nodes[node].set_dword(0x3c, height);
                if variant.layers {
                    self.apply_layers(node, level);
                }
                if variant.clamp {
                    let height = self.nodes[node].height();
                    let clamped = height.max(0);
                    self.nodes[node].set_dword(0x40, clamped);
                    let deviation = (clamped.wrapping_sub(mean(0x40)) >> 3).wrapping_add(scale);
                    scale = deviation;
                } else if variant.deviation {
                    let height = self.nodes[node].height();
                    scale = (height.wrapping_sub(m_mean) >> 3).wrapping_add(scale);
                }
            }
            for (at, from) in [(4, 0x28), (8, 0x2c), (0xc, 0x30)] {
                let displacement = multiply(self.nodes[node].dword(from), scale);
                self.nodes[node].set_dword(at, displacement.wrapping_add(mean(at)));
            }
            quadrant_node(&mut self.nodes[node]);
        }
        self.set_level(node);
        Ok(())
    }

    /// The layers' map coordinates of a node, halfway between its ends' where the
    /// layer has a map at this level.
    fn average_layers(&mut self, node: usize, a: usize, b: usize, level: i32) {
        let (na, nb) = (self.nodes[a], self.nodes[b]);
        for layer in 0..2 {
            if self.layers[layer][level as usize][0] == 0 {
                continue;
            }
            for half in 0..2 {
                let at = 0x34 + 4 * layer + 2 * half;
                let mean = (i32::from(na.word(at)) + i32::from(nb.word(at))) >> 1;
                self.nodes[node].set_word(at, mean as i16);
            }
        }
    }

    /// The coarse midpoint's map coordinates and quadrant words.
    fn coarse_coordinates(&mut self, which: usize, node: usize, a: usize, b: usize, c: usize) {
        let (na, nb, nc) = (self.nodes[a], self.nodes[b], self.nodes[c]);
        for at in [0, 2] {
            let mean = (i32::from(na.word(at)) + i32::from(nb.word(at))) >> 1;
            self.nodes[node].set_word(at, mean as i16);
        }
        if nc.z() > 0x40 {
            let shift_v = usize::from(na.word(2) > nb.word(2));
            let shift_u = usize::from(na.word(0) > nb.word(0));
            let first = i32::from(nc.word(0)) - i32::from(na.word(0));
            let second = i32::from(nb.word(2)) - i32::from(na.word(2));
            let third = i32::from(nb.word(0)) - i32::from(na.word(0));
            let fourth = i32::from(nc.word(2)) - i32::from(na.word(2));
            let index = usize::from(first.wrapping_mul(second) > third.wrapping_mul(fourth));
            let base = tables::ADJUST + 32 * which + shift_v * 16 + shift_u * 8 + index * 4;
            let du = i16::from_le_bytes(self.table[base..base + 2].try_into().unwrap());
            let dv = i16::from_le_bytes(self.table[base + 2..base + 4].try_into().unwrap());
            let m = &mut self.nodes[node];
            m.set_word(0, m.word(0).wrapping_add(du));
            m.set_word(2, m.word(2).wrapping_add(dv));
        }
        let m = &mut self.nodes[node];
        let (u, v) = (i32::from(m.word(0)), i32::from(m.word(2)));
        m.set_word(0x24, quadrant_u(u));
        m.set_word(0x26, quadrant_v(v));
    }

    /// The layers' feature heights: each layer with a map at this level turns the
    /// node's height by the map's byte at the node's coordinates there.
    fn apply_layers(&mut self, node: usize, level: i32) {
        for layer in 0..2 {
            let entry = self.layers[layer][level as usize];
            if entry[0] == 0 {
                continue;
            }
            let at = 0x34 + 4 * layer;
            let (u, v) = (self.nodes[node].word(at), self.nodes[node].word(at + 2));
            if u as u16 >= 0x80 || v as u16 >= 0x80 {
                continue;
            }
            let index = usize::from(u as u16) + (usize::from(v as u16) << 7);
            let height = self.nodes[node].height() >> 7;
            let height = if entry[1] != 0 {
                i32::from(self.map_byte(entry[1], index)).wrapping_mul(height)
            } else {
                height
            };
            let bump = i32::from(self.map_byte(entry[0], index) as i8) << 21;
            self.nodes[node].set_dword(0x3c, height.wrapping_add(bump));
        }
    }
}

/// A feature record of the stream: the feature type and the map coordinates of
/// the face's three corners (u, v words each).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Record {
    kind: i32,
    corners: [[i16; 2]; 3],
}

/// A node of F1823's tree of the stream's records: the path from the face down
/// to a record's place, two slots at each node (a record, or the count of levels
/// an enclosing record still covers) and four children (twenty at the root).
#[derive(Debug, Clone)]
struct TreeNode {
    slots: [Slot; 2],
    children: [usize; 20],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// Zero, or the levels an enclosing record still covers, as a non-positive count.
    Count(i32),
    Record(usize),
}

/// The stream's records as F1823 files them: the root and every node below it.
#[derive(Debug, Clone)]
struct Tree {
    nodes: Vec<TreeNode>,
    records: Vec<Record>,
}

/// Why a query could not be answered as the original would.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    /// A record was filed under another record's place, which the original
    /// reads as garbage.
    Overlap,
    /// The stream's records need more than the original's 4,096-byte pool.
    Pool,
    /// The original divides by zero (or overflows the quotient).
    Division,
}

/// Why the search stopped early.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Abort(Abort),
    Fault(Fault),
}

impl From<Abort> for Stop {
    fn from(abort: Abort) -> Self {
        Stop::Abort(abort)
    }
}

/// The result of a distance query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Distance {
    /// The distance along the query direction to the surface, or the limit.
    pub value: i32,
    /// The distance before it is limited (`DATA_009283`; `0x7fffffff` when no face
    /// held the point).
    pub raw: i32,
    /// Bytes of the stream the records took (including the terminating word).
    pub consumed: usize,
    /// The arena that overflowed and aborted the search, if any.
    pub aborted: Option<u8>,
    /// Whether a face was found that holds the query point.
    pub found: bool,
}

fn find_msb(value: u32) -> i32 {
    31 - value.max(1).leading_zeros() as i32
}

/// F1825: the highest bit of the largest component magnitude (OR'd).
fn extent(vector: [i32; 3]) -> i32 {
    find_msb(vector.iter().fold(0u32, |a, v| a | v.wrapping_abs() as u32))
}

/// The length of a matrix row (the part of F1876 before the corners).
fn row_length(row: [i32; 3]) -> i32 {
    let shift = extent(row) - 14;
    let square = |c: i32| c.wrapping_mul(c);
    if shift > 0 {
        let sum = row
            .iter()
            .fold(0i32, |a, c| a.wrapping_add(square(*c >> shift)));
        (table_root(sum as u32) << shift) as i32
    } else {
        let sum = row.iter().fold(0i32, |a, c| a.wrapping_add(square(*c)));
        table_root(sum as u32) as i32
    }
}

/// F1875: -1 if the cross product `p x q` of the two points' x and y is negative.
fn side(p: [i32; 3], q: [i32; 3]) -> i32 {
    let product = |a: i32, b: i32| i64::from(a) * i64::from(b);
    -i32::from(product(p[0], q[1]) - product(p[1], q[0]) < 0)
}

impl Tree {
    /// F1823: file the stream's records (16 bytes each: a key, the feature type,
    /// six words) until the zero key; returns the tree and the bytes consumed.
    fn build(stream: &[u8]) -> Result<(Self, usize), Fault> {
        let mut tree = Self {
            nodes: vec![TreeNode {
                slots: [Slot::Count(0); 2],
                children: [NIL; 20],
            }],
            records: Vec::new(),
        };
        let word = |at: usize| i16::from_le_bytes([stream[at], stream[at + 1]]);
        let mut at = 0;
        // The original's pool: the root takes 0x58 bytes, a node 0x18, a record 0x10.
        let mut pool = 0x58;
        loop {
            let key = i32::from(word(at));
            if key == 0 {
                return Ok((tree, at + 2));
            }
            let kind = i32::from(word(at + 2));
            let top = find_msb(key as u32);
            let mut path = (key.wrapping_shl((16 - top) as u32 & 31) & 0xffff) as u32;
            let mut node = 0;
            let mut depth = (top - 3) / 2;
            while depth > 0 {
                let index = (path >> 11) as usize;
                let existing = tree.nodes[node].children[index];
                if existing == NIL {
                    pool += 0x18;
                    let mut slots = [Slot::Count(0); 2];
                    for (slot, parent) in slots.iter_mut().zip(tree.nodes[node].slots) {
                        *slot = match parent {
                            Slot::Record(_) => return Err(Fault::Overlap),
                            Slot::Count(n) if n < 0 => Slot::Count(n + 1),
                            Slot::Count(_) => Slot::Count(0),
                        };
                    }
                    tree.nodes.push(TreeNode {
                        slots,
                        children: [NIL; 20],
                    });
                    let made = tree.nodes.len() - 1;
                    tree.nodes[node].children[index] = made;
                    node = made;
                } else {
                    node = existing;
                }
                path = (path << 2) & 0x1fff;
                depth -= 1;
            }
            if let Some(free) = tree.nodes[node]
                .slots
                .iter()
                .position(|s| *s == Slot::Count(0))
            {
                pool += 0x10;
                let corners = std::array::from_fn(|k| [word(at + 4 + 4 * k), word(at + 6 + 4 * k)]);
                tree.records.push(Record { kind, corners });
                tree.nodes[node].slots[free] = Slot::Record(tree.records.len() - 1);
            }
            if pool > 4096 {
                return Err(Fault::Pool);
            }
            at += 16;
        }
    }
}

impl Terrain<'_> {
    /// The planet type's third dword (what F1833 returns), the table entry for
    /// `kind`.
    fn type_dword(&self, kind: usize, field: usize) -> i32 {
        self.table_dword(tables::TYPES + 48 * kind + field)
    }

    /// F1834 for the distance mode: the detail level and quadrant words of a
    /// face's corners, and the feature records' effect on their heights.
    fn prepare_face(&mut self, corners: [usize; 3], tree: Option<usize>, tree_data: &Tree) {
        for node in corners {
            self.set_level(node);
            let (u, v) = (
                i32::from(self.nodes[node].word(0)),
                i32::from(self.nodes[node].word(2)),
            );
            self.nodes[node].set_word(0x24, quadrant_u(u));
            self.nodes[node].set_word(0x26, quadrant_v(v));
        }
        if let Some(tree) = tree {
            for slot in tree_data.nodes[tree].slots {
                let Slot::Record(record) = slot else { continue };
                let record = tree_data.records[record];
                let feature = tables::FEATURES + 12 * record.kind as usize;
                let heights = self.table_dword(feature) as u8;
                let multiplier = self.table_dword(feature + 4) as u8;
                for (node, [u, v]) in corners.into_iter().zip(record.corners) {
                    if u as u16 >= 0x80 || v as u16 >= 0x80 {
                        continue;
                    }
                    let index = usize::from(u as u16) + (usize::from(v as u16) << 7);
                    let n = &mut self.nodes[node];
                    let mut height = n.height() >> 7;
                    if multiplier != 0 {
                        height =
                            i32::from(self.maps[(usize::from(multiplier) - 1) * 0x4000 + index])
                                .wrapping_mul(height);
                    }
                    let bump =
                        i32::from(self.maps[(usize::from(heights) - 1) * 0x4000 + index] as i8)
                            << 21;
                    n.set_dword(0x3c, height.wrapping_add(bump));
                }
            }
        }
        for node in corners {
            let height = self.nodes[node].height();
            self.nodes[node].set_dword(0x40, height.max(0));
        }
    }

    /// F1876 in distance mode: how far along its z axis the point `translation`
    /// (with the planet's orientation `matrix`, rows of three) is from the planet's
    /// surface, searching down to the limit `max`. `stream` is the model's
    /// collision stream after the terrain record's six-byte header, `kind` that
    /// header's planet type, `seed` the object's `+0xa0` and `detail` the game's
    /// detail setting (`DATA_008812`).
    #[allow(clippy::too_many_arguments)]
    pub fn distance(
        &mut self,
        seed: u32,
        max: i32,
        stream: &[u8],
        translation: [i32; 3],
        matrix: [i32; 9],
        kind: usize,
        detail: i32,
    ) -> Result<Distance, Fault> {
        let (tree, consumed) = Tree::build(stream)?;
        let rows = [
            [matrix[0], matrix[1], matrix[2]],
            [matrix[3], matrix[4], matrix[5]],
            [matrix[6], matrix[7], matrix[8]],
        ];
        let longest = rows.iter().map(|r| row_length(*r)).fold(i32::MIN, i32::max);
        let bound = multiply(longest, 0x5400_0000).wrapping_mul(2);
        self.level_base = find_msb(bound as u32) + detail - 1;
        // F1816, then the twelve corners.
        self.reset();
        let mut seed = seed;
        let polar = self.type_dword(kind, 0) == 2;
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
                    sum.wrapping_add(multiply(direction[row], matrix[3 * row + axis]))
                });
                node.set_dword(0x28 + 4 * axis, position);
                node.set_dword(4 + 4 * axis, position.wrapping_add(translation[axis]));
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
        let far = translation[2] > longest.wrapping_mul(2);
        let (value, raw, found, aborted) = match self.search(&tree, translation, kind, far) {
            Ok(Some(value)) => {
                // The layer tables are cleared only by a search that finds a face.
                self.layers = [[[0; 2]; 32]; 2];
                let limited = if value == i32::MAX || value > max {
                    max
                } else {
                    value
                };
                (limited, value, true, None)
            }
            Ok(None) => (max, i32::MAX, false, None),
            Err(Stop::Abort(Abort(code))) => (max, i32::MAX, false, Some(code)),
            Err(Stop::Fault(fault)) => return Err(fault),
        };
        Ok(Distance {
            value,
            raw,
            consumed,
            aborted,
            found,
        })
    }

    fn search(
        &mut self,
        tree: &Tree,
        translation: [i32; 3],
        kind: usize,
        far: bool,
    ) -> Result<Option<i32>, Stop> {
        let mut edges = [NIL; 30];
        for (k, edge) in edges.iter_mut().enumerate() {
            let record = self.alloc_edge()?;
            self.edges[record].head = self.table_dword(tables::EDGES + 8 * k) as usize;
            *edge = record;
        }
        let span = self.type_dword(kind, 8) as usize;
        let pos = |t: &Self, n: usize| [t.nodes[n].x(), t.nodes[n].y(), t.nodes[n].z()];
        let relative = |p: [i32; 3]| std::array::from_fn(|k| p[k].wrapping_sub(translation[k]));
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
            // F1834: the midpoint routines for this face, and its corners.
            let slot = span + 8 * usize::from(face_tree.is_some()) + 2 * usize::from(!far);
            let routine =
                |t: &Self, k: usize| t.table_dword(tables::MIDPOINTS + 4 * (slot + k)) as usize - 1;
            // `DATA_009278` (the first four levels) reads from `DATA_007845`, four slots on.
            let (coarse, fine) = (routine(self, 4), routine(self, 0));
            self.prepare_face(corners, face_tree, tree);
            let [mut a, mut b, mut c] = corners;
            let inside = {
                let (p1, p2, p3) = (
                    relative(pos(self, a)),
                    relative(pos(self, b)),
                    relative(pos(self, c)),
                );
                side(p1, p2) >= 0 && side(p2, p3) >= 0 && side(p3, p1) >= 0
            };
            if !inside {
                continue;
            }
            let [mut e3, mut e4, mut e5] = sides;
            let mut cursor = face_tree;
            let mut level = 0;
            while level <= 9 && [a, b, c].iter().any(|n| self.nodes[*n].level() >= level) {
                if let Some(t) = cursor {
                    for layer in 0..2 {
                        let Slot::Record(r) = tree.nodes[t].slots[layer] else {
                            continue;
                        };
                        let record = tree.records[r];
                        let feature = tables::FEATURES + 12 * record.kind as usize;
                        let maps = [
                            self.table_dword(feature) as u8,
                            self.table_dword(feature + 4) as u8,
                        ];
                        let levels = self.table_dword(feature + 8);
                        for l in level..level + levels {
                            if let Some(entry) = self.layers.get_mut(layer + l as usize / 32) {
                                entry[l as usize % 32] = maps;
                            }
                        }
                        for (node, [u, v]) in [a, b, c].into_iter().zip(record.corners) {
                            self.nodes[node].set_word(0x34 + 4 * layer, u);
                            self.nodes[node].set_word(0x36 + 4 * layer, v);
                        }
                    }
                }
                let which = if level < 5 { coarse } else { fine };
                self.midpoint(which, e4, b, c, a, level)?;
                self.midpoint(which, e5, c, a, b, level)?;
                self.midpoint(which, e3, a, b, c, level)?;
                let (m3, m4, m5) = (self.edges[e3].mid, self.edges[e4].mid, self.edges[e5].mid);
                let (q3, q4, q5) = (
                    relative(pos(self, m3)),
                    relative(pos(self, m4)),
                    relative(pos(self, m5)),
                );
                let half = |t: &Self, e: usize, end: usize| {
                    if t.edges[e].head == end {
                        t.edges[e].first
                    } else {
                        t.edges[e].second
                    }
                };
                let branch;
                let (s, n, t);
                if side(q3, q5) >= 0 {
                    branch = 0;
                    s = half(self, e3, a);
                    n = self.alloc_edge()?;
                    self.edges[n].head = m3;
                    t = half(self, e5, a);
                    (b, c) = (m3, m5);
                } else if side(q4, q3) >= 0 {
                    branch = 1;
                    s = half(self, e4, b);
                    n = self.alloc_edge()?;
                    self.edges[n].head = m4;
                    t = half(self, e3, b);
                    (a, b, c) = (b, m4, m3);
                } else if side(q5, q4) >= 0 {
                    branch = 2;
                    s = half(self, e5, c);
                    n = self.alloc_edge()?;
                    self.edges[n].head = m5;
                    t = half(self, e4, c);
                    (a, b, c) = (c, m5, m4);
                } else {
                    branch = 3;
                    s = self.alloc_edge()?;
                    self.edges[s].head = m5;
                    n = self.alloc_edge()?;
                    self.edges[n].head = m3;
                    t = self.alloc_edge()?;
                    self.edges[t].head = m4;
                    (a, b, c) = (m4, m5, m3);
                }
                (e3, e4, e5) = (s, n, t);
                cursor = cursor
                    .map(|node| tree.nodes[node].children[branch])
                    .filter(|child| *child != NIL);
                level += 1;
            }
            return self.interpolate([a, b, c]).map(Some).map_err(Stop::Fault);
        }
        Ok(None)
    }
}

impl Terrain<'_> {
    /// The end of F1876: the height of the final triangle's plane at the query
    /// point, measured along z.
    fn interpolate(&mut self, corners: [usize; 3]) -> Result<i32, Fault> {
        let mut points = corners.map(|n| [self.nodes[n].x(), self.nodes[n].y(), self.nodes[n].z()]);
        let all = points
            .iter()
            .flatten()
            .fold(0u32, |a, v| a | v.wrapping_abs() as u32);
        let shift = (find_msb(all) - 13).max(0);
        for point in &mut points {
            for v in point.iter_mut() {
                *v >>= shift;
            }
        }
        let [[x1, y1, z1], [x2, y2, z2], [x3, y3, z3]] = points;
        let m = i32::wrapping_mul;
        let a = i32::wrapping_add;
        let d = i32::wrapping_sub;
        let t70 = a(a(m(d(x1, x3), y2), m(d(x2, x1), y3)), m(d(x3, x2), y1));
        let t74 = a(a(m(d(x1, x2), y3), m(d(x3, x1), y2)), m(d(x2, x3), y1));
        let t78 = d(m(x1, y2), m(x2, y1));
        let t7c = d(m(x1, y3), m(x3, y1));
        let ratio = |numerator: i32, mut divisor: i32| -> Result<i32, Fault> {
            let bits = (0x1e - find_msb(numerator.wrapping_abs() as u32)).clamp(0, 15);
            if bits < 15 {
                divisor >>= 15 - bits;
                if divisor == 0 {
                    divisor = 1;
                }
            }
            numerator
                .wrapping_shl(bits as u32)
                .checked_div(divisor)
                .ok_or(Fault::Division)
        };
        let q1 = ratio(t7c, t74)?;
        let q2 = ratio(t78, t70)?;
        let plane = a(m(d(z2, z1), q1), m(d(z3, z1), q2));
        let base = self.nodes[corners[0]].z();
        Ok(if shift < 15 {
            a(plane >> (15 - shift), base)
        } else {
            a(plane.wrapping_shl((shift - 15) as u32), base)
        })
    }
}
