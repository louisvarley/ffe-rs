//! The original's runtime upper palette: F1326/F1327/F1330/F1331.
//!
//! The low 128 entries are the fixed palette imported as `base:palette/low`
//! (`DATA_007769`, uploaded by `F1641`, `ffe.asm:98136-98161`). Entries
//! 128..255 are rebuilt at run time: a frame registers the 12-bit material
//! colours it uses with [`RuntimePalette::register`] (`F1326_DPal32to16`,
//! `ffe.asm:82761-82781`), [`RuntimePalette::build`] reproduces
//! `F1330_DPalBuild` (`ffe.asm:82962-83394`) and [`RuntimePalette::resolve`]
//! reproduces `F1327_DPal16toPal` (`ffe.asm:82785-82803`).
//!
//! # Colour words
//!
//! A colour word is 12 bits, four per component: `r << 8 | g << 4 | b`
//! (`F1326`). The palette itself stores six-bit components; a request is
//! quantised to a 4-bit cube cell (`component >> 2`) and a box representative
//! is an odd six-bit value, `2 * (lo + hi - 1) + 1` for the tightened box
//! bounds, or `4 * cell + 1` for a single-cell box. The capture of the
//! original's default space colour is entry 182, six-bit `(3, 3, 19)`.
//!
//! # The builder
//!
//! `F1330` builds one 64-entry bank per call and toggles the destination bank
//! (`DATA_007634 ^= 0x40`), so the 128 upper entries are two banks: video
//! entries `0x80..0xbf` and `0xc0..0xff`. The occupancy cube (`DATA_009139`) is
//! not cleared between builds during a screen, only when a palette file is
//! loaded (`F1328_DPalClear`); it therefore accumulates the screen's colours.
//! With more than 64 occupied cells the builder median-cuts the occupied
//! extent into 64 boxes, splitting the heap root on its longest axis at the
//! midpoint; with at most 64 it makes one box per occupied cell. Each box's
//! cells then map to the box's palette index (`DATA_009139 = box + 1`,
//! `DATA_009140[-1 + index] = palette index`).
//!
//! The previous bank's entries are used for slot identity: they claim the
//! boxes/cells that contain them (`DATA_009140`) before the new boxes take the
//! remaining slots from the end of a list in reverse order. This is why an
//! object can keep a palette index across frames while the palette is rebuilt.
//!
//! The port resolves colours to RGB for its 32-bit framebuffer; it does not
//! upload a DAC. [`RuntimePalette::changed`] exposes the entries a build would
//! have uploaded (`F1331_DPalSet`, `ffe.asm:83398-83427`).

/// The 16x16x16 request cube.
pub const CUBE: usize = 16;
/// Cells in the cube (`DATA_009139`).
pub const CELLS: usize = CUBE * CUBE * CUBE;
/// Dynamic entries per build.
pub const BOXES: usize = 64;
/// Upper palette entries (two banks).
pub const UPPER_ENTRIES: usize = 2 * BOXES;
/// Entries in the fixed low palette.
pub const LOW_ENTRIES: usize = 128;
/// Bytes in the fixed 12-bit-colour to low-palette map (`DATA_007770`).
pub const FIXED_ENTRIES: usize = CELLS;

/// A half-open box over cube cells, with `hi` exclusive. An empty box is the
/// original's `F1329` empty result: `lo = 16`, `hi = 0` on every axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cut {
    lo: [i32; 3],
    hi: [i32; 3],
    count: i32,
}

impl Cut {
    fn empty() -> Self {
        Self {
            lo: [CUBE as i32; 3],
            hi: [0; 3],
            count: 0,
        }
    }

    fn cells(self) -> impl Iterator<Item = [i32; 3]> {
        (self.lo[0]..self.hi[0]).flat_map(move |r| {
            (self.lo[1]..self.hi[1])
                .flat_map(move |g| (self.lo[2]..self.hi[2]).map(move |b| [r, g, b]))
        })
    }
}

/// The runtime palette: the fixed low half, the fixed request map and the
/// dynamic upper half with its persistent cube.
#[derive(Clone)]
pub struct RuntimePalette {
    /// `DATA_007770`: 0, or a low palette index + 1. Fixed colours never enter
    /// the dynamic cube; they resolve straight to the low palette.
    fixed: Vec<u8>,
    /// `base:palette/low`, 128 six-bit RGB entries.
    low: Vec<u8>,
    /// `DATA_009139`: 0 unused, 1 registered (box 0 pending), box + 1 built.
    cells: [u8; CELLS],
    /// `DATA_009141` (`DATA_009140` is one byte earlier so `box + 1` indexes
    /// this array): the palette index (128..255) per box.
    slots: [u8; BOXES],
    /// `DATA_009144`: 128 six-bit RGB entries, two banks of 64.
    colours: [[u8; 3]; UPPER_ENTRIES],
    /// `DATA_009142`: entries whose colour changed in the last build.
    changed: Vec<u8>,
    /// `DATA_007634`: 0 or 64, the bank this build wrote and the next one reads.
    bank: usize,
    built: bool,
}

impl RuntimePalette {
    /// A palette with the imported fixed map and low palette. Missing assets
    /// leave the corresponding lookups empty (all colours are dynamic / black).
    pub fn new(fixed: Vec<u8>, low: Vec<u8>) -> Self {
        let fixed = if fixed.len() == FIXED_ENTRIES {
            fixed
        } else {
            Vec::new()
        };
        let low = if low.len() == LOW_ENTRIES * 3 {
            low
        } else {
            Vec::new()
        };
        Self {
            fixed,
            low,
            cells: [0; CELLS],
            slots: [0; BOXES],
            colours: [[0; 3]; UPPER_ENTRIES],
            changed: Vec::new(),
            bank: 0,
            built: false,
        }
    }

    /// `F1326_DPal32to16` (`ffe.asm:82761`): mark a requested colour in the
    /// cube. Fixed colours are skipped. The mark is `1`, which the next build
    /// replaces with the box index + 1.
    pub fn register(&mut self, key: u16) {
        let key = usize::from(key & 0xfff);
        if self.fixed.get(key).copied().unwrap_or(0) == 0 {
            self.cells[key] = 1;
        }
    }

    /// Whether anything is in the cube.
    pub fn is_empty(&self) -> bool {
        self.cells.iter().all(|cell| *cell == 0)
    }

    /// `F1330_DPalBuild` (`ffe.asm:82962`): build the current bank from the
    /// cube and toggle the bank. The caller resolves the colours it wants with
    /// [`Self::resolve`] afterwards.
    pub fn build(&mut self) {
        self.changed.clear();
        let boxes = self.make_boxes();
        self.slots = [0; BOXES];

        // The map and the zeroed slots are written before the previous bank
        // claims slots (`F1330` 83160-83223).
        for (index, cut) in boxes.iter().enumerate() {
            let slot = (index + 1) as u8;
            for [r, g, b] in cut.cells() {
                let key = (r as usize) << 8 | (g as usize) << 4 | b as usize;
                self.cells[key] = slot;
            }
        }

        // The previous bank's colours claim the boxes that contain them; the
        // rest are available in reverse order (83274-83310).
        let mut valid = Vec::new();
        for entry in 0..BOXES {
            let colour = self.colours[self.bank + entry];
            let key = usize::from(colour[0] >> 2) << 8
                | usize::from(colour[1] >> 2) << 4
                | usize::from(colour[2] >> 2);
            let cell = usize::from(self.cells[key]);
            if cell == 0 || self.slots[cell - 1] != 0 {
                valid.push(entry as u8);
            } else {
                self.slots[cell - 1] = (0x80 + self.bank + entry) as u8;
            }
        }

        // Assign and write each box's representative (83314-83382).
        for (index, cut) in boxes.iter().enumerate() {
            if self.slots[index] == 0 {
                let entry = match valid.pop() {
                    Some(entry) => entry as usize,
                    // The original reads below its valid-list buffer; there is
                    // no such colour, so reuse the box's own ordinal.
                    None => index,
                };
                self.slots[index] = (0x80 + self.bank + entry) as u8;
            }
            let slot = usize::from(self.slots[index] - 0x80);
            let colour = representative(*cut);
            if self.colours[slot] != colour {
                self.changed.push(slot as u8);
                self.colours[slot] = colour;
            }
        }

        self.bank ^= 0x40;
        self.built = true;
    }

    /// `F1327_DPal16toPal` (`ffe.asm:82785`): the palette index for a colour
    /// word, or `None` before a build has mapped the cell.
    pub fn index(&self, key: u16) -> Option<u8> {
        let key = usize::from(key & 0xfff);
        if let Some(fixed) = self.fixed.get(key).copied().filter(|value| *value != 0) {
            return Some(fixed - 1);
        }
        if !self.built {
            return None;
        }
        let cell = usize::from(self.cells[key]);
        if cell == 0 {
            return None;
        }
        let slot = self.slots[cell - 1];
        (slot != 0).then_some(slot)
    }

    /// The resolved RGB for a colour word, expanded from six-bit like
    /// [`crate::indexed::palette_rgb6`] (`component << 2`, max 252).
    pub fn resolve(&self, key: u16) -> Option<[u8; 3]> {
        let index = self.index(key)?;
        self.entry(index)
    }

    /// The RGB of a palette entry (0..255), expanded from six-bit. Entries the
    /// current screen has not written are black, like the original's cleared
    /// upper half.
    pub fn entry(&self, index: u8) -> Option<[u8; 3]> {
        if index < LOW_ENTRIES as u8 {
            let at = 3 * usize::from(index);
            let rgb = self.low.get(at..at + 3)?;
            Some([rgb[0] << 2, rgb[1] << 2, rgb[2] << 2])
        } else {
            let colour = self.colours[usize::from(index) - LOW_ENTRIES];
            Some([colour[0] << 2, colour[1] << 2, colour[2] << 2])
        }
    }

    /// The six-bit palette entry (what the original uploads), or `None` for an
    /// out-of-range index / missing low palette.
    pub fn entry6(&self, index: u8) -> Option<[u8; 3]> {
        if index < LOW_ENTRIES as u8 {
            let at = 3 * usize::from(index);
            let rgb = self.low.get(at..at + 3)?;
            Some([rgb[0], rgb[1], rgb[2]])
        } else {
            Some(self.colours[usize::from(index) - LOW_ENTRIES])
        }
    }

    /// `F1331_DPalSet`'s changed-entry list from the last build (slots 0..127).
    pub fn changed(&self) -> &[u8] {
        &self.changed
    }

    /// The bank the next build writes (`DATA_007634` after `F1331` toggles it):
    /// 0 or 64. The last build wrote the other one.
    pub fn bank(&self) -> usize {
        self.bank
    }

    /// The current 128 upper entries as six-bit RGB (bank order, not video
    /// order: entry 0 here is video 0x80, entry 64 is 0xc0).
    pub fn upper6(&self) -> &[[u8; 3]; UPPER_ENTRIES] {
        &self.colours
    }

    /// Clear the cube like `F1328_DPalClear`; the low half and the colour
    /// banks stay. The viewer calls it before each frame's registration so the
    /// 64 boxes are cut around the colours that frame actually queues.
    pub fn clear(&mut self) {
        self.cells = [0; CELLS];
    }

    /// Tighten a box to its occupied cells (`F1329`, `ffe.asm:82822-82958`).
    fn tighten(&self, lo: [i32; 3], hi: [i32; 3]) -> Cut {
        let mut cut = Cut::empty();
        for [r, g, b] in (lo[0]..hi[0])
            .flat_map(|r| (lo[1]..hi[1]).flat_map(move |g| (lo[2]..hi[2]).map(move |b| [r, g, b])))
        {
            let key = (r as usize) << 8 | (g as usize) << 4 | b as usize;
            if self.cells[key] != 0 {
                cut.count += 1;
                for (axis, value) in [r, g, b].into_iter().enumerate() {
                    cut.lo[axis] = cut.lo[axis].min(value);
                    cut.hi[axis] = cut.hi[axis].max(value + 1);
                }
            }
        }
        cut
    }

    fn longest(cut: Cut) -> usize {
        let mut axis = 0;
        let mut range = cut.hi[0] - cut.lo[0];
        for next in 1..3 {
            let extent = cut.hi[next] - cut.lo[next];
            if extent > range {
                range = extent;
                axis = next;
            }
        }
        axis
    }

    /// `F1329`'s longest-axis extent (`box + 0xc`), the value the original's
    /// heap compares. An empty box is -16 (`hi = 0`, `lo = 16`), so it sinks
    /// below every real box.
    fn extent(cut: Cut) -> i32 {
        (0..3)
            .map(|axis| cut.hi[axis] - cut.lo[axis])
            .max()
            .unwrap_or(0)
    }

    /// The original's heap removal (83025-83159): the root's saved pointer is
    /// replaced by the larger child until a leaf is reached, and the shrunken
    /// root is written into that hole. It does not stop early or sift the root
    /// back up, so this is a literal port rather than a general sift-down.
    fn sink_root(heap: &mut [usize], boxes: &[Cut]) {
        let size = heap.len() - 1;
        let root = heap[1];
        let mut at = 1;
        loop {
            let left = at * 2;
            if left > size {
                break;
            }
            let child = if left < size
                && Self::extent(boxes[heap[left]]) < Self::extent(boxes[heap[left + 1]])
            {
                left + 1
            } else {
                left
            };
            heap[at] = heap[child];
            at = child;
        }
        heap[at] = root;
    }

    /// The boxes `F1330` builds: 64 median-cut boxes when the occupied cube has
    /// more than 64 cells, otherwise one per occupied cell.
    fn make_boxes(&self) -> Vec<Cut> {
        let initial = self.tighten([0; 3], [CUBE as i32; 3]);
        if initial.count <= BOXES as i32 {
            return (0..CELLS)
                .filter(|index| self.cells[*index] != 0)
                .map(|index| {
                    let cell = [
                        (index >> 8) as i32,
                        ((index >> 4) & 0xf) as i32,
                        (index & 0xf) as i32,
                    ];
                    Cut {
                        lo: cell,
                        hi: cell.map(|value| value + 1),
                        count: 1,
                    }
                })
                .collect();
        }
        let mut boxes = vec![initial];
        // A max-heap of box indices, 1-based as in the original. The key is
        // `F1329`'s longest-axis extent (`box + 0xc`), not the cell count.
        let mut heap = vec![usize::MAX, 0];
        while boxes.len() < BOXES {
            let root = heap[1];
            let axis = Self::longest(boxes[root]);
            let old_lo = boxes[root].lo;
            let old_hi = boxes[root].hi;
            let mid = (old_lo[axis] + old_hi[axis]) / 2;
            // The left box keeps the root's slot; the right box is new.
            boxes[root].hi[axis] = mid;
            boxes[root] = self.tighten(old_lo, boxes[root].hi);
            let mut right = Cut {
                lo: old_lo,
                hi: old_hi,
                count: 0,
            };
            right.lo[axis] = mid;
            // Drop the shrunken root down the larger-child path (the original
            // does this unconditionally, 83025-83159).
            Self::sink_root(&mut heap, &boxes);
            let index = boxes.len();
            boxes.push(self.tighten(right.lo, right.hi));
            // Append the right box and sift it up by extent.
            heap.push(index);
            let mut at = heap.len() - 1;
            while at > 1 {
                let parent = at / 2;
                if Self::extent(boxes[heap[parent]]) < Self::extent(boxes[heap[at]]) {
                    heap.swap(parent, at);
                    at = parent;
                } else {
                    break;
                }
            }
        }
        boxes
    }
}

/// The box's representative six-bit colour (`F1330` 83331-83355).
fn representative(cut: Cut) -> [u8; 3] {
    std::array::from_fn(|axis| (2 * (cut.lo[axis] + cut.hi[axis] - 1) + 1) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A low palette whose entry `i` is `(i % 64)` on every channel (six-bit).
    fn low() -> Vec<u8> {
        (0..LOW_ENTRIES)
            .flat_map(|index| {
                let value = (index % 64) as u8;
                [value, value, value]
            })
            .collect()
    }

    fn palette() -> RuntimePalette {
        let mut fixed = vec![0; FIXED_ENTRIES];
        fixed[0] = 1; // black is the only fixed colour of the real map at 0
        RuntimePalette::new(fixed, low())
    }

    #[test]
    fn fixed_colours_use_the_low_palette_and_are_not_registered() {
        let mut fixed = vec![0; FIXED_ENTRIES];
        fixed[0x000] = 1; // entry 0, black
        fixed[0x111] = 18; // entry 17, the low grey ramp
        let mut palette = RuntimePalette::new(fixed, low());
        palette.register(0x000);
        palette.register(0x111);
        assert!(
            palette.is_empty(),
            "fixed colours must not enter the dynamic cube"
        );
        assert_eq!(palette.resolve(0x000), Some([0, 0, 0]));
        // Low entry 17 is (17,17,17) six-bit, expanded by four.
        assert_eq!(palette.resolve(0x111), Some([68, 68, 68]));

        let mut fixed = vec![0; FIXED_ENTRIES];
        fixed[0x004] = 96; // entry 95 (the target HUD's low colour)
        let mut palette = RuntimePalette::new(fixed, low());
        palette.register(0x004);
        assert!(palette.is_empty());
        // Synthetic entry 95 is 95 % 64 = 31 on every channel.
        assert_eq!(palette.resolve(0x004), Some([124, 124, 124]));
    }

    #[test]
    fn a_few_colours_build_one_box_each_at_odd_six_bit_values() {
        let mut palette = palette();
        palette.register(0x004);
        palette.register(0x005);
        palette.build();
        // Single-cell boxes: component = 4 * cell + 1.
        // The first build's empty source bank offers slots 63, 62, ... so the
        // cells are written in reverse box order, but each resolves to its own
        // colour.
        assert_eq!(palette.resolve(0x004), Some([4, 4, 17 << 2]));
        assert_eq!(palette.resolve(0x005), Some([4, 4, 21 << 2]));
        assert_eq!(palette.changed().len(), 2);
        assert_eq!(palette.bank(), 64);
    }

    #[test]
    fn the_second_bank_reuses_the_first_banks_slots() {
        let mut palette = palette();
        palette.register(0x004);
        palette.build();
        let first = palette.index(0x004).unwrap();
        palette.register(0x004);
        palette.build();
        // After registering only the same colour, its box is claimed by the
        // previous bank's entry, so the index shifts to the other bank but the
        // colour is the same.
        let second = palette.index(0x004).unwrap();
        assert_eq!(first, 0x80 + 63);
        assert_eq!(second, 0xc0 + 63);
        assert_eq!(palette.resolve(0x004), Some([4, 4, 17 << 2]));
    }

    #[test]
    fn a_uniform_cube_cuts_into_sixty_four_non_empty_boxes() {
        // The heap must take the largest-extent box and re-sift the shrunken
        // root (F1330 83025-83159). Splitting one box repeatedly instead left
        // most of the 64 boxes empty and wrote their 31,31,31 representative
        // into the bank, which the intro and the surfaces showed as speckle.
        let mut palette = palette();
        for r in 0..8u16 {
            for g in 0..8u16 {
                for b in 0..8u16 {
                    palette.register(r << 8 | g << 4 | b);
                }
            }
        }
        let boxes = palette.make_boxes();
        assert_eq!(boxes.len(), BOXES);
        assert!(
            boxes.iter().all(|cut| cut.count > 0),
            "empty boxes: {:?}",
            boxes
                .iter()
                .filter(|cut| cut.count == 0)
                .collect::<Vec<_>>()
        );
        palette.build();
        let grey = palette
            .upper6()
            .iter()
            .filter(|colour| **colour == [31, 31, 31])
            .count();
        assert_eq!(grey, 0, "empty-box representatives reached the bank");
    }

    #[test]
    fn more_than_sixty_four_cells_make_exactly_sixty_four_boxes() {
        let mut palette = palette();
        // A 4x4x4 block of 64 cells plus one more; the builder must cut it.
        for r in 0..4 {
            for g in 0..4 {
                for b in 0..4 {
                    palette.register((r << 8) | (g << 4) | b);
                }
            }
        }
        palette.register(4 << 8);
        palette.build();
        let upper = palette.upper6();
        let used: std::collections::BTreeSet<[u8; 3]> = upper
            .iter()
            .copied()
            .filter(|colour| *colour != [0, 0, 0])
            .collect();
        assert!(used.len() > 1, "{used:?}");
        // Representatives are always odd six-bit values in 1..=61.
        assert!(used
            .iter()
            .flatten()
            .all(|value| { *value % 2 == 1 && (1..=61).contains(value) }));
    }

    #[test]
    fn resolve_before_a_build_is_none() {
        let mut palette = palette();
        palette.register(0x004);
        assert_eq!(palette.resolve(0x004), None);
        palette.build();
        assert!(palette.resolve(0x004).is_some());
    }

    #[test]
    fn clearing_between_frames_keeps_the_current_colours_representative() {
        // A screen's previous frames may have registered hundreds of cells. The
        // viewer clears the cube before each frame's registration, so a lone
        // colour is its own single-cell box rather than a box median-cut around
        // old colours. This is the surface-colour stability guard.
        let mut palette = palette();
        for r in 0..8u16 {
            for g in 0..8u16 {
                for b in 0..8u16 {
                    palette.register(r << 8 | g << 4 | b);
                }
            }
        }
        palette.build();
        palette.clear();
        palette.register(0x0f0);
        palette.build();
        // Single-cell (0,15,0): 4 * cell + 1 per component.
        assert_eq!(palette.resolve(0x0f0), Some([4, 244, 4]));
    }
}
