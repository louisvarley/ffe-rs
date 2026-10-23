//! The planet terrain engine's query path, ported from `ffe.asm` (F1816-F1834,
//! F1846, F1849-F1867, F1875, F1876 in distance mode). A planet is an
//! icosahedron whose faces are subdivided edge by edge: each edge's midpoint node
//! is made by one of nineteen routines (F1849-F1867), chosen by the planet's
//! type, which place the new vertex and its height. Nodes and edge records come
//! from two fixed arenas whose overflow aborts the whole query. Verified against
//! the executed original (`docs/notes/terrain.md`).

use crate::model::multiply;

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
