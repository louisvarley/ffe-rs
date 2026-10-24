//! Bounded live vertex slots: object memoization, view translation and original projection.
use crate::model::{CachedVertex, ExternalVertices, ObjectCache, VertexContext};
use crate::star_points::{StarField, StarTables};

pub const NEAR_SENTINEL: [i16; 2] = [-32766; 2];

/// F1479. Coordinates are logical 320x158 viewport values, not window pixels.
pub fn project([x, y, z]: [i32; 3]) -> Option<[i16; 2]> {
    if z < 64 {
        return None;
    }
    let divide = |value: i32| {
        if z > 65536 {
            value / (z >> 8)
        } else {
            value.wrapping_shl(8) / z
        }
    };
    let mut screen_x = divide(x).wrapping_add(160);
    let mut screen_y = 79i32.wrapping_sub(divide(y));
    let mut magnitude = screen_x.wrapping_abs() | screen_y.wrapping_abs();
    while magnitude > 0x1f00 {
        screen_x >>= 1;
        screen_y >>= 1;
        magnitude >>= 1;
    }
    Some([screen_x as i16, screen_y as i16])
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Vertex {
    pub object: [i32; 3],
    pub view: [i32; 3],
    pub screen: [i16; 2],
    pub stamp: u32,
}

impl From<CachedVertex> for Vertex {
    fn from(value: CachedVertex) -> Self {
        Self {
            object: value.object,
            view: value.view,
            screen: value.screen,
            stamp: value.stamp,
        }
    }
}

#[derive(Clone)]
pub struct Vertices<'a> {
    objects: ObjectCache<'a>,
    normal_records: &'a [u8],
    stars: Option<(&'a StarTables, &'a StarField)>,
}

impl<'a> Vertices<'a> {
    pub fn new(records: &'a [u8], count: u32) -> Result<Self, String> {
        Ok(Self {
            objects: ObjectCache::new(records, count)?,
            normal_records: &[],
            stars: None,
        })
    }

    /// Attach the galaxy map's star tables and the object's stars, which opcode 30
    /// `0x801e` (F1769) draws.
    pub fn with_stars(mut self, tables: &'a StarTables, field: &'a StarField) -> Self {
        self.stars = Some((tables, field));
        self
    }

    pub fn stars(&self) -> Option<(&'a StarTables, &'a StarField)> {
        self.stars
    }

    /// Attach the model's normal records (instance +0x154, six bytes per mirrored
    /// pair), which polygon circles (F1736) read. Without them a circle is an error.
    pub fn with_normal_records(mut self, records: &'a [u8]) -> Self {
        self.normal_records = records;
        self
    }

    pub fn normal_records(&self) -> &'a [u8] {
        self.normal_records
    }

    /// A child instance's fresh cache (F1689/F1690 zero every stamp, including the six
    /// external slots) whose negative vertices resolve from `externals`.
    pub fn child(
        records: &'a [u8],
        count: u32,
        externals: ExternalVertices,
    ) -> Result<Self, String> {
        let mut objects = ObjectCache::new(records, count)?;
        objects.externals = Some(externals);
        Ok(Self {
            objects,
            normal_records: &[],
            stars: None,
        })
    }

    /// Begin another invocation explicitly; context changes do not invalidate legacy slots.
    pub fn clear(&mut self) {
        self.objects.slots.fill(CachedVertex::default());
        self.objects.external_slots.fill(CachedVertex::default());
    }

    pub fn slots(&self) -> Vec<Vertex> {
        self.objects
            .slots
            .iter()
            .copied()
            .map(Vertex::from)
            .collect()
    }

    /// F1476 state that jittered vertices (opcodes 7/8) will draw from next.
    pub fn jitter(&self) -> [u32; 2] {
        self.objects.jitter
    }

    /// Set the F1476 state, e.g. from the original's `DATA_008818`/`DATA_008820`.
    pub fn set_jitter(&mut self, state: [u32; 2]) {
        self.objects.jitter = state;
    }

    /// Expanded vertex count (including mirrored partners).
    pub fn vertex_count(&self) -> usize {
        self.objects.slots.len()
    }

    /// F1472 behind the callers' cache check: the object-space position, evaluated
    /// (and stamped 1) only on a zero stamp. Transactional like [`Self::resolve`].
    pub fn object(&mut self, index: i8, context: &VertexContext) -> Result<[i32; 3], String> {
        if index < 0 {
            let mut working = self.objects.clone();
            let object = working.external(index as u8)?.object;
            self.objects = working;
            return Ok(object);
        }
        let slot =
            usize::try_from(index).map_err(|_| "External negative vertex index.".to_owned())?;
        if slot >= self.objects.slots.len() || context.scale > 31 {
            return Err("Invalid live vertex index or scale.".to_owned());
        }
        let mut working = self.objects.clone();
        let object = working.vertex(index as u8, context, 0)?;
        self.objects = working;
        Ok(object)
    }

    /// F1470 translates every call without updating the stamp; F1471 projects and stamps.
    /// Only a zero stamp re-evaluates the object, exactly as F1478 dependencies do.
    /// A matching generation bypasses both, as original command callers do.
    /// On failure, preserve all slots, including dependencies calculated during the request.
    pub fn resolve(
        &mut self,
        index: i8,
        context: &VertexContext,
        position: [i32; 3],
        generation: u32,
        projected: bool,
    ) -> Result<Vertex, String> {
        if index < 0 {
            // F1478's external path stamps the copied slot with the instance generation,
            // so F1470/F1471 reuse it; F1470's recomputed view is the copied one.
            if generation == 0 {
                return Err("Invalid live vertex generation.".to_owned());
            }
            let mut working = self.objects.clone();
            let slot = working.external(index as u8)?;
            self.objects = working;
            return Ok(slot.into());
        }
        let slot =
            usize::try_from(index).map_err(|_| "External negative vertex index.".to_owned())?;
        if slot >= self.objects.slots.len() || generation == 0 || context.scale > 31 {
            return Err("Invalid live vertex index, generation or scale.".to_owned());
        }
        if self.objects.slots[slot].stamp == generation {
            return Ok(self.objects.slots[slot].into());
        }
        let mut working = self.objects.clone();
        let object = working.vertex(index as u8, context, 0)?;
        let value = &mut working.slots[slot];
        if value.screen_space {
            // The original writes these vertices' screen position itself.
            value.stamp = generation;
        } else {
            value.view = std::array::from_fn(|axis| object[axis].wrapping_add(position[axis]));
            if projected {
                value.screen = project(value.view).unwrap_or(NEAR_SENTINEL);
                value.stamp = generation;
            }
        }
        let result = (*value).into();
        self.objects = working;
        Ok(result)
    }
}
