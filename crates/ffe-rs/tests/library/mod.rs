//! Shared by the reference tests that need the models' collision data.

use ffe_core::surface::{Mesh, Shape, Shapes};

pub struct Library {
    pub cache: ffe_content::VerifiedCache,
}

impl Library {
    pub fn load() -> Self {
        Self {
            cache: ffe_content::read(
                &std::fs::read(std::env::var_os("FFE_WORLD_CACHE").unwrap()).unwrap(),
            )
            .unwrap(),
        }
    }
}

impl Shapes for Library {
    fn model(&self, model: u32) -> Option<ffe_core::objects::ModelInfo> {
        ffe_core::objects::ModelInfo::read(
            self.cache.asset("base:model/records")?,
            self.cache.asset("base:ship/stats")?,
            model,
        )
    }

    fn shape(&self, model: u32) -> Option<Shape<'_>> {
        // A model whose vertices did not import still serves as the moving body: only
        // its scale and region chain are read.
        let vertices = self
            .cache
            .asset(&format!("base:model-vertices/{model:03}"))
            .unwrap_or(&[]);
        let normals = self
            .cache
            .asset(&format!("base:model-normals/{model:03}"))
            .unwrap_or(&[]);
        let records = self.cache.asset("base:model/records")?;
        let record = &records[20 * model as usize..20 * model as usize + 20];
        let table = self.cache.asset("base:model/collision")?;
        let entry = &table[12 * model as usize..12 * model as usize + 12];
        let word = |at: usize| u32::from_le_bytes(entry[at..at + 4].try_into().unwrap()) as usize;
        let (offset, length, back) = (word(0), word(4), word(8));
        if length == 0 {
            return None;
        }
        let data = &table[12 * 466 + offset..];
        Some(Shape {
            mesh: Mesh {
                vertices,
                normals,
                scale: i32::from_le_bytes(record[0..4].try_into().unwrap()),
                offset: i32::from_le_bytes(record[4..8].try_into().unwrap()),
            },
            stream: &data[back..back + length],
            back: &data[..back],
        })
    }
}
