//! F6 model viewer: every imported model executed by the verified port and drawn
//! in the original render order. A development view, not the faithful renderer:
//! the camera, light, flat colour mapping and fill are diagnostic choices, and
//! curved surfaces, text and labels are not drawn. Thruster glows (F1738) are
//! the original's queued F1805 quads drawn by the ported textured rasteriser.
//! Textured faces follow the original F1708/F1712 entry and F1644/F1647
//! rasterisers with the cache v8 descriptor table (verified, see
//! `docs/notes/textures.md`); without v8 they show as a lit grey stand-in.
use crate::ships::ShipStats;
use crate::textures::TextureSet;
use ffe_content::{AssetKind, VerifiedCache};
use ffe_core::model::{evaluate, VertexContext, JITTER_SEED};
use ffe_core::model_animation::{
    door_step, time_word, top_level_locals, START_TICS, TICS_PER_FRAME,
};
use ffe_core::model_commands::{material, VisibilityContext};
use ffe_core::model_execute::{
    execute_live, execute_scene, resolve_colour, ChildEnvironment, ChildModel, ChildModels,
    Execution, InstanceContext, Primitive,
};
use ffe_core::model_fill::near_outline;
use ffe_core::model_instance::{Vertices, NEAR_SENTINEL};
use ffe_core::model_lighting::{default_lighting, pack, Lighting};
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::{self, Normal, NormalContext};
use ffe_core::model_texture::{
    draw_quad, draw_triangle, flat_colour, quad_uv, textured_colours, textured_path, triangle_uv,
};
use ffe_core::model_thruster::{line_colour, Shape as ThrusterShape};
use std::cell::Cell;

/// One screen-space primitive in draw order (logical 320x158 view coordinates).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Fill(Vec<(i32, i32)>, [u8; 3]),
    /// An on-view flat triangle or quad (F1804/F1809 entry, points in face
    /// order), drawn by F1604 or F1607.
    Flat(Vec<(i32, i32)>, [u8; 3]),
    /// A polygon entry's F1560 edges in order (`model_polygon::edges`), filled
    /// by F1468/F1560/F1613.
    Polygon(Vec<[[i16; 2]; 2]>, [u8; 3]),
    Line((i32, i32), (i32, i32), [u8; 3]),
    /// An F1811 curve entry's four points in entry order, drawn by F1546.
    Curve([[i16; 2]; 4], [u8; 3]),
    Point((i32, i32), [u8; 3]),
    Textured(Textured),
    /// A thruster glow (F1738's F1805 entry): drawn like [`Shape::Textured`] (F1647
    /// over bitmap 0x123 or 0x65), but left out of the viewer's fit so that the
    /// model keeps its size as the plume grows with thrust.
    Glow(Textured),
    /// F1812: a model-name-table string in the original bitmap font at the
    /// label's queued screen point (colour 15, clipped, unshadowed).
    Label((i32, i32), Vec<u8>),
}

/// A textured F1807/F1805 queue entry: screen points in queue order (the face's
/// vertices v0..), the descriptor's bitmap dword and the slot colours that
/// `DATA_009252` receives (RGB of the F1326 colours).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Textured {
    pub points: Vec<(i32, i32)>,
    pub bitmap: u32,
    pub slots: Vec<[u8; 3]>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Frame {
    pub shapes: Vec<Shape>,
    /// Primitives the viewer does not draw yet: labels whose string is missing
    /// from the cache, and every curved surface (F1876 is not ported).
    pub skipped: usize,
    /// Opcode 10 text records the executor ran but the viewer does not draw yet
    /// (the glyph programs are not imported or executed).
    pub texts: usize,
    /// Why child models were left out of this frame, if they were.
    pub children_error: Option<String>,
    /// The F1476 state after this frame's jittered vertices: the next frame's
    /// [`AnimationInputs::jitter`] continues the original's single stream.
    pub jitter: [u32; 2],
}

pub struct ViewerModel {
    pub index: u32,
    pub vertex_count: u32,
    pub scale: u32,
    vertices: Vec<u8>,
    commands: Vec<u8>,
    normals: Vec<u8>,
    normal_count: u32,
    /// Largest object-space vertex distance at input 0, or the evaluation error.
    radius: Result<f64, String>,
}

/// The global words and locals a top-level model runs with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationInputs {
    pub globals: [u16; 64],
    pub locals: [i32; 64],
    /// The F1476 state jittered vertices (opcodes 7/8) draw from, such as the
    /// flickering far end of a thruster plume. The original keeps one stream
    /// running across frames; pass the previous [`Frame::jitter`] to animate.
    pub jitter: [u32; 2],
}

impl AnimationInputs {
    /// What the original passes a drawn object whose other words are zero
    /// (`ffe_core::model_animation`, `docs/notes/animation.md`): global 0 is
    /// `GameTics >> 10` (F1534), global 1 the gear/door word; local 0 is 0 and
    /// locals 12..17 hold the shades (F1680). Locals 1..11 are stale stack data
    /// in the original; zero here.
    pub fn original(tics: u32, state_word: u16, shades: &[[u8; 3]; 8]) -> Self {
        let mut globals = [0u16; 64];
        globals[0] = time_word(tics);
        globals[1] = state_word;
        Self {
            globals,
            locals: top_level_locals(shades).0,
            jitter: JITTER_SEED,
        }
    }

    /// The diagnostic test input: every global and local holds `value`, except
    /// global word 4 (opcode 30 light points, whose tables are not imported).
    pub fn test(value: u16) -> Self {
        let mut globals = [value; 64];
        globals[4] = 0;
        Self {
            globals,
            locals: [i32::from(value); 64],
            jitter: JITTER_SEED,
        }
    }
}

/// Time-acceleration multipliers the viewer cycles through (diagnostic; the
/// original's own acceleration levels are not modelled here).
const RATES: [u32; 4] = [1, 10, 100, 1000];

pub struct ModelViewer {
    models: Vec<ViewerModel>,
    table: SineTable,
    selected: usize,
    yaw: i32,
    pitch: i32,
    zoom: i32,
    /// `Some(value)`: the constant test input; `None`: the original inputs over
    /// the viewer's own game clock.
    test: Option<u16>,
    /// The viewer's `GameTics` (day fraction), advanced at the original rate.
    tics: u32,
    rate: usize,
    /// Global word 1 and its F865 direction byte.
    door: u16,
    door_state: i8,
    textures: TextureSet,
    lighting: Lighting,
    /// The terrain tables and height maps (cache v17), for curved surfaces.
    terrain: Option<(Vec<u8>, Vec<u8>)>,
    /// Descriptor +0x18 and +0x1c per table slot (cache v10); empty without it.
    descriptors: Vec<[i32; 2]>,
    ship_stats: Vec<Option<ShipStats>>,
    /// `DATA_004682`, split into its NUL-terminated entries (cache v11).
    model_strings: Vec<Vec<u8>>,
    /// Font model slots 0..2's glyph programs (cache v14).
    glyphs: Vec<Option<GlyphSet>>,
    /// The object string F1747's `0x3016`/`0x3022` expand (the drawn object's
    /// word at +0x124, its registration). Empty falls back to the model's own
    /// name, so the viewer shows something instead of nothing.
    registration: Vec<u8>,
    /// Global word 15 (the ship's forward thrust, object +0xba); `None` leaves it
    /// to the animation input. See [`ModelViewer::set_thrust`].
    thrust: Option<u16>,
    /// The F1476 state the next frame draws from, and the state the last
    /// rendered frame ended with (taken over when the clock advances).
    jitter: [u32; 2],
    jitter_next: Cell<[u32; 2]>,
}

/// The engine settings the viewer's E key steps through: off (the animation
/// input), half and full plume (4095, where ship models stop lengthening it).
const THRUST_STEPS: [Option<u16>; 3] = [None, Some(2048), Some(4095)];

/// One font model's per-character command programs (cache v14): F1747 executes
/// the program for each character and reads its final return word for the next
/// pen vertex. Starts are relative to `bytes`; characters may alias a program.
#[derive(Debug, Clone)]
pub struct GlyphSet {
    vertex_count: u32,
    scale: u32,
    starts: Vec<u32>,
    bytes: Vec<u8>,
}

impl GlyphSet {
    /// Decode a `base:model-glyphs/NNN` payload: `count` u32 starts, then the
    /// program block.
    pub fn from_payload(payload: &[u8], count: u32, vertex_count: u32, scale: u32) -> Option<Self> {
        let offsets = count as usize * 4;
        if payload.len() <= offsets || count == 0 {
            return None;
        }
        Some(Self {
            vertex_count,
            scale,
            starts: payload[..offsets]
                .chunks_exact(4)
                .map(|entry| u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]))
                .collect(),
            bytes: payload[offsets..].to_vec(),
        })
    }

    /// The program for `code`, when the set defines it.
    pub fn program(&self, code: u8) -> Option<&[u8]> {
        let index = usize::from(code.checked_sub(ffe_core::model_commands::GLYPH_FIRST_CHAR)?);
        let start = *self.starts.get(index)? as usize;
        let end = self
            .starts
            .iter()
            .map(|offset| *offset as usize)
            .filter(|offset| *offset > start)
            .min()
            .unwrap_or(self.bytes.len());
        self.bytes.get(start..end)
    }
}

/// The model global that ship models read as their forward thrust: global word 15
/// is object word `+0x9c + 2 * 15` = `+0xba` (F1473), the z word of the vector F598
/// clamps to the ship's per-axis limits and stores at +0xb6/+0xb8/+0xba. The ships'
/// thruster preamble skips the glows when this word is zero or has bit 15 set, sets
/// local 1 to `min(thrust, 4095) << 4` (zero below 5120) and interpolates each
/// plume end (vertex B, curve vertex opcode 19) by local 1. See
/// `docs/notes/thrusters.md`.
pub const THRUST_GLOBAL: usize = 15;

/// The imported models as child calls see the model table.
struct ChildTable<'a>(&'a ModelViewer);

impl ChildModels for ChildTable<'_> {
    fn model(&self, index: u16) -> Result<ChildModel<'_>, String> {
        let viewer = self.0;
        let model = viewer
            .find(u32::from(index))
            .ok_or_else(|| format!("Child model {index} is not in the cache."))?;
        let [scale_offset, radius] = *viewer
            .descriptors
            .get(usize::from(index))
            .ok_or_else(|| format!("Child model {index} has no descriptor."))?;
        Ok(ChildModel {
            commands: &model.commands,
            vertices: &model.vertices,
            vertex_count: model.vertex_count,
            normals: &model.normals,
            normal_count: model.normal_count,
            scale: model.scale as i32,
            scale_offset,
            radius,
        })
    }
}

const GENERATION: u32 = 2;

/// Without the v9 lighting tables: a diagnostic light and a plain 7..0 ramp.
const FALLBACK_LIGHTING: Lighting = Lighting {
    light: [13107, -16384, 24576],
    shades: [
        [7; 3], [6; 3], [5; 3], [4; 3], [3; 3], [2; 3], [1; 3], [0; 3],
    ],
};

impl ModelViewer {
    pub fn new(cache: &VerifiedCache) -> Result<Self, String> {
        let table = SineTable::from_bytes(
            cache
                .asset("base:math/sine")
                .ok_or("The model viewer needs cache v6 or later (sine table).")?,
        )?;
        let mut models = Vec::new();
        for record in &cache.manifest().files {
            let AssetKind::ModelCommands {
                index,
                vertex_count,
                ..
            } = record.asset
            else {
                continue;
            };
            let find = |kind: &str| {
                let id = format!("base:model-{kind}/{index:03}");
                let record = cache.manifest().files.iter().find(|r| r.id == id);
                Some((record?, cache.asset(&id)?))
            };
            let (Some((vertex_record, vertices)), Some((normal_record, normals))) =
                (find("vertices"), find("normals"))
            else {
                continue;
            };
            let (
                AssetKind::ModelVertices { scale, .. },
                AssetKind::ModelNormals { normal_count, .. },
            ) = (&vertex_record.asset, &normal_record.asset)
            else {
                continue;
            };
            models.push(ViewerModel::new(
                index,
                vertex_count,
                *scale,
                vertices,
                cache.asset(&record.id).ok_or("Missing command payload.")?,
                normals,
                *normal_count,
            ));
        }
        let mut viewer = Self::from_models(models, table)?;
        viewer.textures = TextureSet::from_cache(cache);
        viewer.terrain = cache
            .asset("base:terrain/tables")
            .zip(cache.asset("base:terrain/maps"))
            .map(|(t, m)| (t.to_vec(), m.to_vec()));
        if let Some(lighting) = cache
            .asset("base:lighting/default")
            .and_then(|bytes| default_lighting(bytes).ok())
        {
            viewer.lighting = lighting;
        }
        if let Some(table) = cache.asset("base:model/descriptors") {
            viewer.descriptors = table
                .chunks_exact(8)
                .map(|entry| {
                    std::array::from_fn(|i| {
                        i32::from_le_bytes([
                            entry[4 * i],
                            entry[4 * i + 1],
                            entry[4 * i + 2],
                            entry[4 * i + 3],
                        ])
                    })
                })
                .collect();
        }
        if let Some(table) = cache.asset("base:ship/stats") {
            viewer.ship_stats = table.chunks_exact(48).map(ShipStats::from_block).collect();
        }
        if let Some(table) = cache.asset("base:text/model-names") {
            let mut strings: Vec<Vec<u8>> =
                table.split(|byte| *byte == 0).map(<[u8]>::to_vec).collect();
            if table.last() == Some(&0) {
                strings.pop();
            }
            viewer.model_strings = strings;
        }
        viewer.glyphs = (0..3)
            .map(|index| {
                let id = format!("base:model-glyphs/{index:03}");
                let record = cache.manifest().files.iter().find(|r| r.id == id)?;
                let AssetKind::GlyphPrograms {
                    count,
                    vertex_count,
                    scale,
                    ..
                } = record.asset
                else {
                    return None;
                };
                GlyphSet::from_payload(cache.asset(&id)?, count, vertex_count, scale)
            })
            .collect();
        Ok(viewer)
    }

    /// The model's original ship statistics (cache v12), if it has any.
    pub fn ship_stats(&self, index: u32) -> Option<ShipStats> {
        self.ship_stats.get(index as usize).copied().flatten()
    }

    /// Use these textures (bitmaps, descriptor table, colour map).
    pub fn set_textures(&mut self, textures: TextureSet) {
        self.textures = textures;
    }

    /// Rasterise a textured shape with the original affine mapping (F1644/F1647
    /// and the F1650-F1653 spans), calling `plot` with each pixel's RGB.
    /// Texels without a slot colour (7 and above, never written by face entries;
    /// stale `DATA_009252` contents in the original) are drawn black, the zeroed
    /// start-up value. Returns false when the bitmap is missing.
    pub fn rasterise(&self, shape: &Textured, mut plot: impl FnMut(i32, i32, [u8; 3])) -> bool {
        let Some(map) = self.textures.map(shape.bitmap & 0xfff) else {
            return false;
        };
        let point = |i: usize| [shape.points[i].0 as i16, shape.points[i].1 as i16];
        let colour = |texel: u8| {
            shape
                .slots
                .get(usize::from(texel))
                .copied()
                .unwrap_or([0; 3])
        };
        let plot = |x, y, texel| plot(x, y, colour(texel));
        match shape.points.len() {
            3 => draw_triangle(
                [point(0), point(1), point(2)],
                triangle_uv(map.width),
                map,
                plot,
            ),
            4 => draw_quad(
                [point(0), point(1), point(2), point(3)],
                quad_uv(map.width, (shape.bitmap >> 12 & 7) as u8),
                map,
                plot,
            ),
            _ => return false,
        };
        true
    }

    /// A nibble colour as the original shows it: packed by F1326 (no clamp), then
    /// the fixed low palette where `DATA_007770` maps it, else 4-bit VGA levels.
    /// The 12-bit mask only guards the table lookup against an out-of-range sum.
    fn rgb(&self, colour: [u8; 3]) -> [u8; 3] {
        self.textures.rgb(pack(colour) & 0xfff)
    }

    pub fn from_models(mut models: Vec<ViewerModel>, table: SineTable) -> Result<Self, String> {
        if models.is_empty() {
            return Err("The cache holds no models. Build a v7 data.bin to view them.".to_owned());
        }
        models.sort_by_key(|m| m.index);
        Ok(Self {
            models,
            table,
            selected: 0,
            yaw: 30,
            pitch: -20,
            zoom: 0,
            test: None,
            tics: START_TICS,
            rate: 0,
            door: 0,
            door_state: 0,
            textures: TextureSet::default(),
            lighting: FALLBACK_LIGHTING,
            terrain: None,
            descriptors: Vec::new(),
            ship_stats: Vec::new(),
            model_strings: Vec::new(),
            glyphs: Vec::new(),
            registration: Vec::new(),
            thrust: None,
            jitter: JITTER_SEED,
            jitter_next: Cell::new(JITTER_SEED),
        })
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn model(&self) -> &ViewerModel {
        &self.models[self.selected]
    }

    /// Move through the model list, wrapping.
    pub fn step(&mut self, step: i32) {
        let count = self.models.len() as i32;
        self.selected = (self.selected as i32 + step).rem_euclid(count) as usize;
    }

    /// Rotate in 5-degree steps.
    pub fn turn(&mut self, yaw: i32, pitch: i32) {
        self.yaw = (self.yaw + 5 * yaw.clamp(-1, 1)).rem_euclid(360);
        self.pitch = (self.pitch + 5 * pitch.clamp(-1, 1)).rem_euclid(360);
    }

    /// Zoom in quarter-octave steps, bounded.
    pub fn zoom(&mut self, step: i32) {
        self.zoom = (self.zoom + step.clamp(-1, 1)).clamp(-8, 12);
    }

    /// Use the constant test input `value` instead of the clock.
    pub fn set_animation(&mut self, value: u16) {
        self.test = Some(value);
    }

    /// Back to the original inputs over the viewer's clock.
    pub fn use_clock(&mut self) {
        self.test = None;
    }

    /// The test input in use, if any.
    pub fn animation(&self) -> Option<u16> {
        self.test
    }

    /// The clock's `GameTics`, the gear/door word and the acceleration.
    pub fn clock(&self) -> (u32, u16, u32) {
        (self.tics, self.door, RATES[self.rate])
    }

    pub fn set_clock(&mut self, tics: u32) {
        self.tics = tics;
    }

    /// Advance the clock by real milliseconds: `timer_delta(ms, 994 * rate)` tics,
    /// as the original's accelerated counter, and move the gear/door word by
    /// F865 with that frame time. Runs in test mode too, so returning to the
    /// clock continues from where it is.
    pub fn advance(&mut self, elapsed_ms: u32) {
        let acceleration = TICS_PER_FRAME * RATES[self.rate];
        let Ok(delta) = ffe_core::timer_delta(elapsed_ms, acceleration) else {
            return;
        };
        self.tics = self.tics.wrapping_add(delta);
        (self.door, self.door_state) = door_step(self.door, self.door_state, delta);
        // A new frame of game time: jittered vertices continue the stream.
        if delta != 0 {
            self.jitter = self.jitter_next.get();
        }
    }

    /// Start moving the gear/door word the other way (F865's +0xc9 = 1 opens
    /// towards 0xffff, -1 closes towards 0).
    pub fn toggle_door(&mut self) {
        self.door_state = if self.door_state > 0 || (self.door_state == 0 && self.door == 0xffff) {
            -1
        } else {
            1
        };
    }

    pub fn cycle_rate(&mut self) {
        self.rate = (self.rate + 1) % RATES.len();
    }

    /// The inputs the selected model runs with now.
    pub fn inputs(&self) -> AnimationInputs {
        let mut inputs = match self.test {
            Some(value) => AnimationInputs::test(value),
            None => AnimationInputs::original(self.tics, self.door, &self.lighting.shades),
        };
        inputs.jitter = self.jitter;
        inputs
    }

    /// A generated station's orientation at game time `tics` (F866: rotation
    /// speed `speed`, orientation word 0): row k is station axis k in its
    /// parent's frame, as unit vectors. The station spins about its own z axis,
    /// the docking axis, by `((tics << (speed - 1)) >> 16)` / 65536 turns; model
    /// 74 is generated with speed 10 (one turn per 2^23 tics, about 169 s at
    /// acceleration 1). Identity for speed 0.
    pub fn station_rows(&self, tics: u32, speed: i8) -> [[f64; 3]; 3] {
        let rotation = ffe_core::model_animation::OrbitalRotation::station(speed);
        let matrix = rotation
            .matrix(&self.table, tics)
            .unwrap_or(ffe_core::model::IDENTITY);
        std::array::from_fn(|k| {
            std::array::from_fn(|j| f64::from(matrix[3 * k + j]) / 2_147_483_648.0)
        })
    }

    /// The shades every instance copies (locals 12..17).
    pub fn shades(&self) -> [[u8; 3]; 8] {
        self.lighting.shades
    }

    /// Set the forward thrust ships read from global word 15 ([`THRUST_GLOBAL`],
    /// object +0xba), or `None` to leave that word to the animation input. Ship
    /// thruster glows appear for 320..=0x7fff and lengthen with it up to 4095.
    pub fn set_thrust(&mut self, value: Option<u16>) {
        self.thrust = value;
    }

    pub fn thrust(&self) -> Option<u16> {
        self.thrust
    }

    /// Supply the F1345 (`0x40xx`) strings to a synthetic viewer.
    pub fn set_model_strings(&mut self, strings: Vec<Vec<u8>>) {
        self.model_strings = strings;
    }

    /// The drawn object's registration string (object +0x124), which F1747's
    /// `0x3016`/`0x3022` expand. Empty uses the model's own name.
    pub fn set_registration(&mut self, text: &[u8]) {
        self.registration = text.to_vec();
    }

    /// Supply the glyph sets for synthetic viewers (index = font model slot).
    pub fn set_glyph_sets(&mut self, sets: Vec<Option<GlyphSet>>) {
        self.glyphs = sets;
    }

    /// F1343/F1345's direct model-string case. Other FF-code families need
    /// their own handlers and are rejected rather than guessed.
    fn model_string(&self, code: u16) -> Result<&[u8], String> {
        if !(0x4000..0x8000).contains(&code) {
            return Err(format!("Unsupported model string code {code:#06x}."));
        }
        self.model_strings
            .get(usize::from(code & 0x3fff))
            .map(Vec::as_slice)
            .ok_or_else(|| format!("Model string {code:#06x} is not in the cache."))
    }

    /// Step the engines through off, half and full thrust ([`THRUST_STEPS`]).
    pub fn cycle_thrust(&mut self) {
        let position = THRUST_STEPS.iter().position(|t| *t == self.thrust);
        self.thrust = THRUST_STEPS[position.map_or(0, |p| (p + 1) % THRUST_STEPS.len())];
    }

    /// Execute the selected model and project its ordered primitives. A first pass
    /// at the bounding-radius distance measures the drawn extent; the second fits
    /// it to about 70 pixels from the centre, then applies the zoom.
    pub fn render(&self) -> Result<Frame, String> {
        let radius = self.model().radius.clone()?.max(1.0);
        let first = (radius * 256.0 / 60.0).max(128.0);
        let probe = self.render_at(first)?;
        let extent = probe
            .shapes
            .iter()
            .flat_map(|shape| match shape {
                Shape::Fill(points, _) | Shape::Flat(points, _) => points.clone(),
                Shape::Polygon(edges, _) => edges
                    .iter()
                    .flatten()
                    .map(|[x, y]| (i32::from(*x), i32::from(*y)))
                    .collect(),
                Shape::Line(a, b, _) => vec![*a, *b],
                Shape::Curve(points, _) => points
                    .iter()
                    .map(|[x, y]| (i32::from(*x), i32::from(*y)))
                    .collect(),
                Shape::Point(p, _) => vec![*p],
                Shape::Textured(textured) => textured.points.clone(),
                Shape::Glow(_) => Vec::new(),
                Shape::Label(_, _) => Vec::new(),
            })
            .map(|(x, y)| (x - 160).abs().max((y - 79).abs()))
            .max()
            .unwrap_or(0);
        let fitted = if extent > 0 {
            (first * f64::from(extent) / 70.0).max(128.0)
        } else {
            first
        };
        let frame = self.render_at(fitted * 2f64.powf(-f64::from(self.zoom) / 4.0))?;
        self.jitter_next.set(frame.jitter);
        Ok(frame)
    }

    /// The light direction as a unit vector: the original default light (F960,
    /// `DATA_007231`) from a v9 cache.
    pub fn light_direction(&self) -> [f64; 3] {
        let light = self.lighting.light.map(f64::from);
        let length = light.iter().map(|v| v * v).sum::<f64>().sqrt().max(1.0);
        light.map(|v| v / length)
    }

    /// FUNC_001110's closest external camera for a ship model: its descriptor
    /// radius word (+0x1c) shifted left by its scale (+0x14) + 8, in the camera's
    /// distance units. `None` without the v10 descriptor table.
    pub fn camera_minimum(&self, index: u32) -> Option<i32> {
        let model = self.find(index)?;
        let [_, radius] = *self.descriptors.get(index as usize)?;
        Some(((radius & 0xffff) as u32).wrapping_shl(model.scale + 8) as i32)
    }

    /// The model with this table index, if imported.
    pub fn find(&self, index: u32) -> Option<&ViewerModel> {
        self.models.iter().find(|m| m.index == index)
    }

    /// Render model `index` in a scene camera. `rows[k]` is object axis k in view
    /// space (unit vectors), `position` the model origin in view space, both in
    /// the model's own (post-scale) units; `light_view` the light direction in
    /// view space. Full detail, input 0.
    pub fn render_scene(
        &self,
        index: u32,
        rows: [[f64; 3]; 3],
        position: [f64; 3],
        light_view: [f64; 3],
    ) -> Result<Frame, String> {
        self.render_scene_with(index, rows, position, light_view, &AnimationInputs::test(0))
    }

    /// [`Self::render_scene`] with explicit animation inputs, e.g.
    /// `AnimationInputs::original(tics, 0, &viewer.shades())` for a model at a
    /// game time.
    pub fn render_scene_with(
        &self,
        index: u32,
        rows: [[f64; 3]; 3],
        position: [f64; 3],
        light_view: [f64; 3],
        inputs: &AnimationInputs,
    ) -> Result<Frame, String> {
        let model = self
            .find(index)
            .ok_or_else(|| format!("Model {index} is not in the cache."))?;
        if position.iter().any(|v| v.abs() > 1.5e9) {
            return Err("Model too far for the 32-bit view space.".to_owned());
        }
        self.render_with(model, rows, position.map(|v| v as i32), light_view, inputs)
    }

    fn render_at(&self, distance: f64) -> Result<Frame, String> {
        let distance = distance.max(128.0);
        let (yaw, pitch) = (
            f64::from(self.yaw).to_radians(),
            f64::from(self.pitch).to_radians(),
        );
        let rotate = |[x, y, z]: [f64; 3]| {
            let (rx, rz) = (x * yaw.cos() - z * yaw.sin(), x * yaw.sin() + z * yaw.cos());
            [
                rx,
                y * pitch.cos() - rz * pitch.sin(),
                y * pitch.sin() + rz * pitch.cos(),
            ]
        };
        // Row k is object axis k in view space, as vertex evaluation reads the frame.
        let rows = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]].map(rotate);
        if distance > 1.5e9 {
            return Err("Model too large for the viewer's 32-bit view space.".to_owned());
        }
        self.render_with(
            self.model(),
            rows,
            [0, 0, distance as i32],
            // The default light, taken as view-relative in this inspection view.
            self.light_direction(),
            &self.inputs(),
        )
    }

    fn render_with(
        &self,
        model: &ViewerModel,
        rows: [[f64; 3]; 3],
        position: [i32; 3],
        light_view: [f64; 3],
        inputs: &AnimationInputs,
    ) -> Result<Frame, String> {
        let mut matrix = [0i32; 9];
        for (k, row) in rows.iter().enumerate() {
            for (axis, value) in row.iter().enumerate() {
                matrix[3 * k + axis] = (value * 2_147_483_647.0).round() as i32;
            }
        }
        // Camera and light in object space, record units, for the original normal
        // test: the camera sits at -position in view space.
        let unit = 2f64.powi(model.scale as i32);
        let observer = rows.map(|row| {
            let along: f64 = row
                .iter()
                .zip(position)
                .map(|(a, p)| a * f64::from(p))
                .sum();
            (-along / unit) as i32
        });
        let length = light_view.iter().map(|v| v * v).sum::<f64>().sqrt();
        let light = rows.map(|row| {
            // F1518 normalises the light to length 0x8000.
            (row.iter().zip(light_view).map(|(a, b)| a * b).sum::<f64>() / length * 32_768.0) as i32
        });
        let normals = model_normals::evaluate(
            &model.normals,
            model.normal_count,
            &model.vertices,
            model.vertex_count,
            &NormalContext {
                scale: model.scale,
                observer_scale: model.scale,
                observer,
                light,
                shades: self.lighting.shades,
            },
        )?;
        let mut vertices =
            Vertices::new(&model.vertices, model.vertex_count)?.with_normal_records(&model.normals);
        vertices.set_jitter(inputs.jitter);
        let mut context = VertexContext {
            matrix,
            basis: matrix,
            scale: model.scale,
            globals: inputs.globals,
            locals: inputs.locals,
            ..Default::default()
        };
        // Light points (opcode 30 0x801e) count global word 4; keep it empty.
        context.globals[4] = 0;
        if let Some(thrust) = self.thrust {
            context.globals[THRUST_GLOBAL] = thrust;
        }
        // F1683 sets +0x148 from descriptor +0x14 (the model scale) and +0x130 to 0;
        // thruster widths (F1738) shift by it.
        let instance = InstanceContext {
            position,
            generation: GENERATION,
            key_scale: 0,
            operand_scale: model.scale as i32,
            textures: true,
        };
        // Detail branches see the nearest depth: the viewer wants full detail.
        let visibility = VisibilityContext {
            depth: 1,
            scale: 0,
            detail: [0, 0],
            normals: &normals,
        };
        // With the v10 descriptor table, child models run inside their parent
        // (`execute_scene`); without it, child calls are only recorded.
        let mut children_error = None;
        let scene = if self.descriptors.is_empty() {
            None
        } else {
            let environment = ChildEnvironment {
                models: &ChildTable(self),
                shades: self.lighting.shades,
                // The top-level light (+0x134) in the frame children are set up in.
                light: light_view.map(|v| (v / length * 32_768.0) as i32),
                arena: 0,
            };
            // Transactional: on an error, vertices and context are unchanged.
            match execute_scene(
                &model.commands,
                &mut vertices,
                &mut context,
                &visibility,
                Some(&self.table),
                instance,
                4096,
                &environment,
            ) {
                Ok(result) => Some(result),
                Err(error) => {
                    children_error = Some(error);
                    None
                }
            }
        };
        // Without children (no v10 table, or a child that cannot run yet).
        let result = match scene {
            Some(result) => result,
            None => execute_live(
                &model.commands,
                &mut vertices,
                &mut context,
                &visibility,
                Some(&self.table),
                instance,
                4096,
            )?,
        };
        let mut frame = Frame {
            children_error,
            jitter: vertices.jitter(),
            texts: result.texts.iter().filter(|text| !text.culled).count(),
            ..Frame::default()
        };
        self.draw_primitives(&result, &mut frame)?;
        self.draw_texts(&result, model.index, &mut vertices, instance, &mut frame)?;
        self.draw_surfaces(&result, light, &mut frame);
        Ok(frame)
    }

    /// Opcode 31: the curved surfaces through the ported F1876 and its emitter,
    /// the queue drawn far to near as flat triangles (the textured entries take
    /// their first colour; F1877's haze and the texture fills are not ported).
    fn draw_surfaces(&self, result: &Execution, light: [i32; 3], frame: &mut Frame) {
        use ffe_core::terrain::Terrain;
        use ffe_core::terrain_draw::Scene;
        use ffe_core::terrain_emit::{draw_surface, Queue, Shading};
        for emission in &result.surfaces {
            let (Some((table, maps)), Some(origin), Some(vectors)) =
                (&self.terrain, emission.origin, emission.axes)
            else {
                frame.skipped += 1;
                continue;
            };
            let surface = &emission.surface;
            let mut stream: Vec<u8> = surface.entries.iter().flatten().copied().collect();
            stream.extend([0, 0]);
            let scene = Scene {
                seed: 0,
                origin,
                axes: std::array::from_fn(|k| vectors[k / 3][k % 3]),
                kind: surface.parameters[0].max(0) as usize,
                detail: 2,
                zoom: 0,
            };
            let shading = Shading {
                light,
                shades: self.lighting.shades,
                detail: scene.detail,
                atmosphere: None,
                material: [8, 8, 8],
            };
            let mut terrain = Terrain::new(table, maps);
            let mut queue = Queue::default();
            // An overflow of the original's arenas or queue ends the drawing early;
            // what was queued stands.
            let _ = draw_surface(&mut terrain, &scene, &stream, &shading, &mut queue);
            let mut order = queue.headers();
            order.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
            for (_, at) in order {
                let w = &queue.words;
                let handler = w[at + 2];
                let corners = match handler & 0xff {
                    4 | 7 | 8 => 3,
                    5 | 6 | 9 => 4,
                    _ => continue,
                };
                let points: Vec<(i32, i32)> = (0..corners)
                    .map(|k| {
                        let word = w[at + 3 + k];
                        (i32::from(word as i16), i32::from((word >> 16) as i16))
                    })
                    .collect();
                // Where the colour sits: after the corners (flat), or in the slots.
                let colour = match handler & 0xff {
                    4 => w[at + 6],
                    7 => w[at + 7],
                    5 => w[at + 8],
                    _ => w[at + 7],
                } & 0xfff;
                frame
                    .shapes
                    .push(Shape::Flat(points, self.textures.rgb(colour as u16)));
            }
        }
    }

    /// Turn one executed stream's render-ordered primitives into shapes.
    fn draw_primitives(&self, result: &Execution, frame: &mut Frame) -> Result<(), String> {
        let point =
            |p: [i16; 2]| (p != NEAR_SENTINEL).then_some((i32::from(p[0]), i32::from(p[1])));
        for primitive in &result.ordered {
            match primitive {
                Primitive::Face(emission) => {
                    let face = &emission.face;
                    // The shade of the face's own (parent or child) normal table.
                    let shade = emission.shade;
                    let textured = material(face.material).texture != 0;
                    let descriptors = self.textures.descriptors();
                    let colour = emission.context.colour;
                    let rgb = self.rgb(match (textured, descriptors) {
                        // F1693/F1691 flat colour (the textured path is chosen below).
                        (true, Some(table)) => flat_colour(face.material, colour, shade, table)?,
                        // No v8 descriptor table: lit grey stand-in.
                        (true, None) => std::array::from_fn(|i| 5 + shade[i]),
                        (false, _) => resolve_colour(face.material, colour, shade),
                    });
                    let count = face.vertices.len();
                    if emission.screen.is_empty() || emission.screen.len() != count {
                        continue;
                    }
                    let behind = emission.screen.contains(&NEAR_SENTINEL);
                    let on_view = !behind
                        && emission.screen.iter().all(|[x, y]| {
                            (0..320).contains(&i32::from(*x)) && (0..158).contains(&i32::from(*y))
                        });
                    let textured_entry = descriptors.is_some()
                        && count >= 3
                        && !behind
                        && textured_path(
                            true,
                            face.material,
                            &emission
                                .screen
                                .iter()
                                .map(|p| p.map(i32::from))
                                .collect::<Vec<_>>(),
                        );
                    // F1714: a flat face reaching off the view (or behind the near
                    // plane) is queued as a polygon outline and span-filled.
                    if !textured_entry
                        && !on_view
                        && matches!(count, 3 | 4)
                        && emission.views.len() == count
                    {
                        let vertices: Vec<([i16; 2], [i32; 3])> = emission
                            .screen
                            .iter()
                            .copied()
                            .zip(emission.views.iter().copied())
                            .collect();
                        let outline = near_outline(&vertices);
                        let edges: Vec<[[i16; 2]; 2]> =
                            outline.windows(2).map(|pair| [pair[0], pair[1]]).collect();
                        if !edges.is_empty() {
                            frame.shapes.push(Shape::Polygon(edges, rgb));
                        }
                        continue;
                    }
                    let points: Vec<(i32, i32)> =
                        emission.screen.iter().filter_map(|p| point(*p)).collect();
                    if points.len() != count {
                        continue;
                    }
                    let screens: Vec<[i32; 2]> = points.iter().map(|&(x, y)| [x, y]).collect();
                    if let Some(table) = descriptors.filter(|_| points.len() >= 3) {
                        if textured_path(true, face.material, &screens) {
                            let entry = textured_colours(face.material, colour, shade, table)?;
                            frame.shapes.push(Shape::Textured(Textured {
                                points,
                                bitmap: entry.bitmap,
                                slots: entry
                                    .colours
                                    .iter()
                                    .map(|c| self.textures.rgb(*c))
                                    .collect(),
                            }));
                            continue;
                        }
                    }
                    frame.shapes.push(if points.len() == 2 {
                        Shape::Line(points[0], points[1], rgb)
                    } else if on_view && matches!(points.len(), 3 | 4) {
                        Shape::Flat(points, rgb)
                    } else {
                        Shape::Fill(points, rgb)
                    });
                }
                Primitive::Line(line) => {
                    let rgb = self.rgb(resolve_colour(line.material, line.context.colour, [0; 3]));
                    // F1803: F1704's clipped, stored end points, drawn by F1616.
                    if let Some([a, b]) = line.screen {
                        let point = |[x, y]: [i16; 2]| (i32::from(x), i32::from(y));
                        frame.shapes.push(Shape::Line(point(a), point(b), rgb));
                    }
                }
                Primitive::Point(emitted) => {
                    let rgb = self.rgb(resolve_colour(
                        emitted.material,
                        emitted.context.colour,
                        [0; 3],
                    ));
                    if let Some(p) = emitted.screen.and_then(point) {
                        frame.shapes.push(Shape::Point(p, rgb));
                    }
                }
                Primitive::Spline(spline) => {
                    // F1811: the entry's four points, drawn by F1546.
                    if let Some(points) = spline.points {
                        frame
                            .shapes
                            .push(Shape::Curve(points, self.rgb(spline.colour)));
                    }
                }
                Primitive::Polygon(polygon) => {
                    // The original's queued entries and F1547 edge subdivision, filled
                    // by its span rasteriser. Textured fills (F1691 texture colour not
                    // ported) show grey. An abandoned outline has no entries.
                    let edges = ffe_core::model_polygon::edges(&polygon.outline.entries)?;
                    if !edges.is_empty() {
                        let colour = if material(polygon.polygon.material).texture != 0 {
                            [5; 3]
                        } else {
                            polygon.colour
                        };
                        frame.shapes.push(Shape::Polygon(edges, self.rgb(colour)));
                    }
                }
                Primitive::Thruster(entry) => match &entry.shape {
                    // F1805: the slot colours through F1326/F1327, then the original
                    // textured quad (F1647) over bitmap 0x123 (core) or 0x65 (shaft).
                    // Both bitmaps skip texel 0, so slot 0, which F1738 never writes
                    // (stale queue memory in the original), is never drawn.
                    ThrusterShape::Glow {
                        points,
                        texture,
                        colours,
                    } => {
                        let mut slots = vec![[0; 3]];
                        slots.extend(colours.iter().map(|c| self.rgb(*c)));
                        frame.shapes.push(Shape::Glow(Textured {
                            points: points
                                .iter()
                                .map(|[x, y]| (i32::from(*x), i32::from(*y)))
                                .collect(),
                            bitmap: *texture,
                            slots,
                        }));
                    }
                    // F1803: the stored end points (x - 1 when positive) in the F1691
                    // colour; the line rasteriser (F1616) is the viewer's own.
                    ThrusterShape::Line { points, colour } => {
                        let colour = match self.textures.descriptors() {
                            Some(table) => line_colour(entry.material, *colour, table)?,
                            None => *colour,
                        };
                        let [a, b] = points.map(|[x, y]| (x, y));
                        frame.shapes.push(Shape::Line(a, b, self.rgb(colour)));
                    }
                },
                Primitive::Label(label) => match self.model_string(label.label.string) {
                    Ok(text) => frame.shapes.push(Shape::Label(
                        (i32::from(label.screen[0]), i32::from(label.screen[1])),
                        text.to_vec(),
                    )),
                    Err(_) => frame.skipped += 1,
                },
            }
        }
        Ok(())
    }

    /// Opcode 10 text (F1747): run each character's glyph program through the
    /// executor in a frame at the text's position, and move the pen to the font
    /// vertex the program's final return word names. The glyph geometry is drawn
    /// after the model's own primitives (the original queues it into the same
    /// depth order, which is not modelled yet). Control codes below 0x20 use the
    /// font's component anchors, which are not imported: a string stops there and
    /// is reported in [`Frame::skipped`] rather than guessed.
    fn draw_texts(
        &self,
        result: &Execution,
        model: u32,
        parent: &mut Vertices<'_>,
        instance: InstanceContext,
        frame: &mut Frame,
    ) -> Result<(), String> {
        for emission in &result.texts {
            if emission.culled {
                continue;
            }
            let text = &emission.text;
            let Some(set) = self
                .glyphs
                .get(usize::from(text.glyphs))
                .and_then(Option::as_ref)
            else {
                frame.skipped += 1;
                continue;
            };
            let Some(font) = self.find(u32::from(text.glyphs)) else {
                frame.skipped += 1;
                continue;
            };
            // The programs must belong to this font model: same vertex list and scale.
            if set.vertex_count != font.vertex_count || set.scale != font.scale {
                frame.skipped += 1;
                continue;
            }
            let Ok(string) = self.expanded_text(text.string, model) else {
                frame.skipped += 1;
                continue;
            };
            let position = parent.resolve(
                text.vertex as i8,
                &emission.context,
                instance.position,
                instance.generation,
                true,
            )?;
            // F1740: frame mode 6 copies the parent basis; otherwise the record's
            // word permutes it through F1739.
            let word = u32::from(text.frame);
            let matrix = if word >> 8 & 7 == 6 {
                emission.context.basis
            } else {
                ffe_core::model_math::frame_rows(
                    &emission.context.matrix,
                    emission.context.base_handedness,
                    word,
                )
                .map_err(|error| format!("{error} (text at {:#x})", text.offset))?
                .0
            };
            let mut pen = position.view;
            let mut glyph_vertices = Vertices::new(&font.vertices, font.vertex_count)?
                .with_normal_records(&font.normals);
            let mut glyph_context = VertexContext {
                matrix,
                basis: matrix,
                scale: font.scale,
                globals: emission.context.globals,
                locals: emission.context.locals,
                colour: emission.context.colour,
                ..Default::default()
            };
            // The glyph faces are flat decals: every normal is visible and lit only
            // by the material and instance colour (the font's own lighting is not
            // evaluated yet).
            let normals = vec![
                Normal {
                    culled: false,
                    colour: [0; 3],
                };
                32_768
            ];
            let visibility = VisibilityContext {
                depth: 1,
                scale: 0,
                detail: [0, 0],
                normals: &normals,
            };
            for &code in &string {
                if code < 0x20 {
                    frame.skipped += 1;
                    break;
                }
                let Some(program) = set.program(code) else {
                    frame.skipped += 1;
                    break;
                };
                let Some(anchor) = pen_vertex(program) else {
                    frame.skipped += 1;
                    break;
                };
                let glyph_instance = InstanceContext {
                    position: pen,
                    generation: instance.generation,
                    key_scale: 0,
                    operand_scale: font.scale as i32,
                    textures: true,
                };
                let execution = execute_live(
                    program,
                    &mut glyph_vertices,
                    &mut glyph_context,
                    &visibility,
                    Some(&self.table),
                    glyph_instance,
                    4096,
                )
                .map_err(|error| {
                    format!("{error} (glyph {code:?} of text at {:#x})", text.offset)
                })?;
                self.draw_primitives(&execution, frame)?;
                let next = glyph_vertices.resolve(
                    anchor as i8,
                    &glyph_context,
                    pen,
                    instance.generation,
                    true,
                )?;
                pen = next.view;
            }
        }
        Ok(())
    }

    /// F1344 expansion for the families the models' text commands use: the model
    /// string table (`0x4000..0x8000`) and the drawn object's own string
    /// (`0x3016`, and `0x3022` unwrapped — F1398's width table is not imported).
    fn expanded_text(&self, code: u16, model: u32) -> Result<Vec<u8>, String> {
        match code {
            0x3016 | 0x3022 => {
                if !self.registration.is_empty() {
                    return Ok(self.registration.clone());
                }
                self.model_strings
                    .get(model as usize)
                    .cloned()
                    .ok_or_else(|| format!("Model {model} has no name string."))
            }
            0x4000..=0x7fff => self.model_string(code).map(<[u8]>::to_vec),
            _ => Err(format!("Unsupported text code {code:#06x}.")),
        }
    }
}

/// The next pen vertex a glyph program names: bits 6.. of its final return word.
fn pen_vertex(program: &[u8]) -> Option<u8> {
    let last = program.chunks_exact(2).next_back()?;
    Some((u16::from_le_bytes([last[0], last[1]]) >> 6) as u8)
}

impl ViewerModel {
    /// Largest object-space vertex distance at input 0, when evaluable alone.
    pub fn radius(&self) -> Option<f64> {
        self.radius.as_ref().ok().copied()
    }

    pub fn new(
        index: u32,
        vertex_count: u32,
        scale: u32,
        vertices: &[u8],
        commands: &[u8],
        normals: &[u8],
        normal_count: u32,
    ) -> Self {
        let radius = evaluate(
            vertices,
            vertex_count,
            &VertexContext {
                scale,
                ..Default::default()
            },
        )
        .map(|points| {
            points
                .iter()
                .map(|p| p.iter().map(|c| f64::from(*c).powi(2)).sum::<f64>().sqrt())
                .fold(0.0, f64::max)
        });
        Self {
            index,
            vertex_count,
            scale,
            vertices: vertices.to_vec(),
            commands: commands.to_vec(),
            normals: normals.to_vec(),
            normal_count,
            radius,
        }
    }
}
