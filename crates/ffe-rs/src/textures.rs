//! Textured-face resources from the content cache: the original bitmaps by F1620
//! id (primary 0..96, then secondary), the v8 descriptor table and the 12-bit
//! colour to low-palette map. Used by the diagnostic F6 viewer and the texture
//! reference tests. See `docs/notes/textures.md`.
use ffe_content::{AssetKind, VerifiedCache};
use ffe_core::model_texture::{fixed_palette_index, Descriptors, TextureMap, BITMAP_IDS};

struct Bitmap {
    width: u32,
    masked_zero: bool,
    texels: Vec<u8>,
}

#[derive(Default)]
pub struct TextureSet {
    bitmaps: Vec<Option<Bitmap>>,
    descriptors: Option<Descriptors>,
    colour_map: Option<Vec<u8>>,
    low_palette: Option<Vec<u8>>,
}

impl TextureSet {
    /// Everything optional: a cache before v8 has bitmaps but no descriptor table,
    /// and textured faces then fall back to the grey stand-in.
    pub fn from_cache(cache: &VerifiedCache) -> Self {
        let bitmaps = (0..BITMAP_IDS)
            .map(|id| {
                let (table, index) = if id < 96 {
                    ("primary", id)
                } else {
                    ("secondary", id - 96)
                };
                let asset = format!("base:bitmap/{table}/{index:03}");
                let record = cache.manifest().files.iter().find(|r| r.id == asset)?;
                let (width, masked_zero) = match record.asset {
                    AssetKind::IndexedBitmap {
                        width, masked_zero, ..
                    } => (width, masked_zero),
                    // The two 64 x 512 blocks have header flags 0 (not masked).
                    AssetKind::TextureBlock { width, .. } => (width, false),
                    _ => return None,
                };
                Some(Bitmap {
                    width,
                    masked_zero,
                    texels: cache.asset(&asset)?.to_vec(),
                })
            })
            .collect();
        Self {
            bitmaps,
            descriptors: cache
                .asset("base:texture/descriptors")
                .and_then(|bytes| Descriptors::from_bytes(bytes).ok()),
            colour_map: cache.asset("base:palette/fixed-map").map(<[u8]>::to_vec),
            low_palette: cache.asset("base:palette/low").map(<[u8]>::to_vec),
        }
    }

    pub fn descriptors(&self) -> Option<&Descriptors> {
        self.descriptors.as_ref()
    }

    /// The bitmap F1620 returns for this id (`dword & 0xfff`).
    pub fn map(&self, id: u32) -> Option<TextureMap<'_>> {
        let bitmap = self.bitmaps.get(id as usize)?.as_ref()?;
        Some(TextureMap {
            width: bitmap.width,
            masked_zero: bitmap.masked_zero,
            texels: &bitmap.texels,
        })
    }

    /// RGB for an F1326 12-bit colour. Colours `DATA_007770` maps to the fixed low
    /// palette use that entry (F1327's fixed path, exact). Others are allocated at
    /// run time in the upper palette, which is not modelled: they are shown as
    /// each nibble times 4 in six-bit VGA units, the scale the fixed entries follow
    /// (approximation).
    pub fn rgb(&self, colour: u16) -> [u8; 3] {
        let fixed = self
            .colour_map
            .as_deref()
            .and_then(|map| fixed_palette_index(map, colour))
            .zip(self.low_palette.as_deref())
            .and_then(|(index, palette)| {
                palette.get(3 * usize::from(index)..3 * usize::from(index) + 3)
            });
        match fixed {
            Some(rgb6) => [rgb6[0] << 2, rgb6[1] << 2, rgb6[2] << 2],
            None => [colour >> 8, colour >> 4, colour].map(|n| ((n & 15) as u8 * 4) << 2),
        }
    }
}
