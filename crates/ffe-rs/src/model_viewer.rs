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
use ffe_core::model_dot::{particle_clipped, point_dot, DotShape, DotTables};
use ffe_core::model_execute::{
    execute_live, execute_scene, resolve_colour, ChildEnvironment, ChildModel, ChildModels,
    ChildOutcome, ChildRun, Execution, InstanceContext, Primitive,
};
use ffe_core::model_fill::near_outline;
use ffe_core::model_instance::{Vertices, NEAR_SENTINEL};
use ffe_core::model_lighting::{default_lighting, pack, Lighting};
use ffe_core::model_math::SineTable;
use ffe_core::model_normals::{self, NormalContext};
use ffe_core::model_texture::{
    draw_quad, draw_triangle, flat_colour, quad_uv, textured_colours, textured_path, triangle_uv,
};
use ffe_core::model_thruster::{line_colour, Shape as ThrusterShape};
use ffe_core::star_points::{StarField, StarTables};
use ffe_core::terrain_emit::{GROUP_CLOSE, GROUP_OPEN};
use std::cell::Cell;

/// One screen-space primitive in draw order (logical 320x158 view coordinates).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shape {
    Fill(Vec<(i32, i32)>, [u8; 3]),
    /// A triangle shaded from its corners' colours (the planet surface's detail
    /// entries).
    Gouraud(Vec<(i32, i32)>, Vec<[u8; 3]>),
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
    /// An F1808/F1806 textured entry with explicit per-vertex coordinates
    /// (F1645/F1648) rather than the descriptor's corner table.
    TexturedUv(Textured, Vec<[i16; 2]>),
    /// A thruster glow (F1738's F1805 entry): drawn like [`Shape::Textured`] (F1647
    /// over bitmap 0x123 or 0x65), but left out of the viewer's fit so that the
    /// model keeps its size as the plume grows with thrust.
    Glow(Textured),
    /// F1812: a model-name-table string in the original bitmap font at the
    /// label's queued screen point (colour 15, clipped, unshadowed).
    Label((i32, i32), Vec<u8>),
    /// A particle or dot (F1611): horizontal runs `[x, y, width]` in one colour.
    Spans(Vec<[i32; 3]>, [u8; 3]),
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
    /// The F1777 depth key of each shape, aligned with [`Self::shapes`], in the
    /// original's far-to-near order. Curved-surface entries (F1840/F1841) are
    /// merged into this order, so a ship draws over a planet it is in front of.
    pub keys: Vec<u32>,
    /// Primitives the viewer does not draw yet: labels whose string is missing
    /// from the cache, and surfaces whose bitmap is absent from it.
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

#[derive(Clone)]
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
    /// Largest object-space z of a vertex at input 0 (a station's docking face).
    front: Option<f64>,
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

/// The model the galaxy map draws a sector's stars with.
pub const STAR_MODEL: u32 = 0xab;

/// Time-acceleration multipliers the viewer cycles through (diagnostic; the
/// original's own acceleration levels are not modelled here).
const RATES: [u32; 4] = [1, 10, 100, 1000];

/// The words a planet surface's order queue may hold in the flight view. The
/// original's buffer is 50,000 words and the game exits when one more is asked
/// for; a planet at a few radii completely fills that queue, which would leave a
/// large hole in the surface, so the viewer draws it with room to spare instead
/// (EXP-009 in `docs/DEVIATIONS.md`). The emitters' reference oracles still use
/// the original 50,000-word limit.
pub const SURFACE_QUEUE_WORDS: usize = 0x2_0000;

/// A generated surface queue held between registration and drawing: the words
/// F1876/F1840 queued and their far-to-near order.
struct SurfaceBatch {
    words: Vec<u32>,
    order: Vec<(u32, usize)>,
}

/// One textured surface entry before palette resolution: its screen points, the
/// F1620 bitmap dword, and the slot table's raw F1326 colour words.
struct TerrainTexture {
    /// F1777 key of the queue entry's header (the group's key for ring entries).
    key: u32,
    points: Vec<(i32, i32)>,
    bitmap: u32,
    slots: Vec<(usize, u16)>,
    uv: Option<Vec<[i16; 2]>>,
}

/// One colour word from the 22-step terrain ramp (F1840's slot table).
fn ramp_word(w: &[u32], at: usize, band: u32, first: usize) -> u16 {
    let step = ((band.min(511) * 22) >> 9).min(21) as usize;
    let word = w[at + 2 + first + step / 2];
    let value = if step % 2 == 0 { word } else { word >> 16 };
    (value & 0xfff) as u16
}

/// Every curved surface in this execution and its executed children, in the
/// order the children ran. Child primitives join the root `ordered` list, but a
/// child's surfaces stay in its own [`Execution`]; the planet in the intro's
/// model-176 scene is one (model 449).
fn all_surfaces<'a>(
    result: &'a Execution,
    out: &mut Vec<&'a ffe_core::model_execute::SurfaceEmission>,
) {
    out.extend(result.surfaces.iter());
    for run in &result.children {
        if let ChildOutcome::Executed(child) = &run.outcome {
            all_surfaces(child, out);
        }
    }
}

/// The first child (or descendant) that could not run, in command order.
fn child_error(runs: &[ChildRun]) -> Option<String> {
    for run in runs {
        match &run.outcome {
            ffe_core::model_execute::ChildOutcome::Failed(error) => {
                return Some(format!(
                    "Child model {} at {:#x}: {error}",
                    run.model, run.offset
                ))
            }
            ffe_core::model_execute::ChildOutcome::Executed(child) => {
                if let Some(error) = child_error(&child.children) {
                    return Some(format!(
                        "Child model {} at {:#x}: {error}",
                        run.model, run.offset
                    ));
                }
            }
            _ => {}
        }
    }
    None
}

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
    /// The seed (`+0xa0`) of the surface being drawn, set by the caller.
    pub surface_seed: std::cell::Cell<u32>,
    /// `DATA_008812`, the "Level of Shape Detail" value F1876 reads (the
    /// executed boot's default is 1; see [`Self::set_surface_detail`]).
    surface_detail: std::cell::Cell<i32>,
    /// F1877's `DATA_008814..8817` from the last planet surface drawn: the
    /// background flag and colour the original clears the next upper view to.
    /// [`ModelViewer::clear_haze_sky`] begins a frame's scene.
    pub haze_sky: std::cell::Cell<Option<[u8; 3]>>,
    /// The longest axis vector of the last surface drawn: the planet's radius in
    /// the model's units.
    pub surface_extent: std::cell::Cell<f64>,
    /// Descriptor +0x18 and +0x1c per table slot (cache v10); empty without it.
    descriptors: Vec<[i32; 2]>,
    ship_stats: Vec<Option<ShipStats>>,
    /// `DATA_004682`, split into its NUL-terminated entries (cache v11).
    model_strings: Vec<Vec<u8>>,
    /// Font model slots 0..2's glyph programs (cache v14).
    glyphs: Vec<Option<GlyphSet>>,
    /// The object string F1747's `0x3016`/`0x3022` expand (the drawn object's
    /// word at +0x124, its registration). Empty object strings remain empty.
    registration: Vec<u8>,
    /// The tombstone's F1344 expansion for FF9C00..`03`: the commander name and
    /// game date FFE's game-over models engrave on model 172/175.
    commander: Vec<u8>,
    game_date: Vec<u8>,
    /// When drawing a tombstone, only this face's text records are drawn. FFE
    /// selects them with the text record's normal (2 = RIP/obituary, 4 =
    /// GAME OVER); the port's glyph normals are degenerate, so the model's own
    /// spin angle decides the face instead (see `screen::draw_game_over`).
    tombstone_face: Option<u8>,
    /// Global word 15 (the ship's forward thrust, object +0xba); `None` leaves it
    /// to the animation input. See [`ModelViewer::set_thrust`].
    thrust: Option<u16>,
    /// The F1476 state the next frame draws from, and the state the last
    /// rendered frame ended with (taken over when the clock advances).
    jitter: [u32; 2],
    jitter_next: Cell<[u32; 2]>,
    /// The galaxy map's star light-point tables (cache v23).
    stars: Option<StarTables>,
    /// The small-dot size tables (cache v25) and the ambient colour their halos mix with.
    dots: Option<DotTables>,
    sky: [u8; 3],
    /// The background colour word registered with the runtime palette before a
    /// scene (F1330's first `F1326` call on `DATA_009217`). `Screen` sets it
    /// from the space/haze colour; the intro from F961's F1575 argument.
    background: Cell<u16>,
    /// The galaxy tables and system strings (cache v21, v22) that name a star's label.
    names: Option<(Vec<u8>, Vec<u8>)>,
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
    /// The font vertex a control code (a new line) moves the pen to (cache v24).
    newline: Option<u32>,
    starts: Vec<u32>,
    bytes: Vec<u8>,
}

impl GlyphSet {
    /// Decode a `base:model-glyphs/NNN` payload: `count` u32 starts, then the
    /// program block.
    pub fn from_payload(
        payload: &[u8],
        count: u32,
        vertex_count: u32,
        scale: u32,
        newline: Option<u32>,
    ) -> Option<Self> {
        let offsets = count as usize * 4;
        if payload.len() <= offsets || count == 0 {
            return None;
        }
        Some(Self {
            vertex_count,
            scale,
            newline,
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
        viewer.stars = cache
            .asset("base:world/star-points")
            .and_then(|bytes| StarTables::new(bytes).ok());
        viewer.dots = cache
            .asset("base:world/dot-tables")
            .and_then(|bytes| DotTables::new(bytes).ok());
        viewer.names = cache
            .asset("base:world/galaxy-tables")
            .zip(cache.asset("base:text/system-strings"))
            .map(|(tables, strings)| (tables.to_vec(), strings.to_vec()));
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
                    newline_vertex,
                    ..
                } = record.asset
                else {
                    return None;
                };
                GlyphSet::from_payload(
                    cache.asset(&id)?,
                    count,
                    vertex_count,
                    scale,
                    newline_vertex,
                )
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
    pub fn rasterise(&self, shape: &Textured, plot: impl FnMut(i32, i32, [u8; 3])) -> bool {
        self.rasterise_with(shape, None, plot)
    }

    /// F1645/F1648: [`Self::rasterise`] with the entry's own texture coordinates
    /// instead of F1643's descriptor corners.
    pub fn rasterise_uv(
        &self,
        shape: &Textured,
        uv: &[[i16; 2]],
        plot: impl FnMut(i32, i32, [u8; 3]),
    ) -> bool {
        self.rasterise_with(shape, Some(uv), plot)
    }

    fn rasterise_with(
        &self,
        shape: &Textured,
        uv: Option<&[[i16; 2]]>,
        mut plot: impl FnMut(i32, i32, [u8; 3]),
    ) -> bool {
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
                uv.and_then(|uv| uv.get(..3))
                    .map(|uv| [uv[0], uv[1], uv[2]])
                    .unwrap_or_else(|| triangle_uv(map.width)),
                map,
                plot,
            ),
            4 => draw_quad(
                [point(0), point(1), point(2), point(3)],
                uv.and_then(|uv| uv.get(..4))
                    .map(|uv| [uv[0], uv[1], uv[2], uv[3]])
                    .unwrap_or_else(|| quad_uv(map.width, (shape.bitmap >> 12 & 7) as u8)),
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
            surface_seed: std::cell::Cell::new(0),
            surface_detail: std::cell::Cell::new(1),
            haze_sky: std::cell::Cell::new(None),
            surface_extent: std::cell::Cell::new(0.0),
            descriptors: Vec::new(),
            ship_stats: Vec::new(),
            model_strings: Vec::new(),
            glyphs: Vec::new(),
            registration: Vec::new(),
            commander: Vec::new(),
            game_date: Vec::new(),
            tombstone_face: None,
            thrust: None,
            jitter: JITTER_SEED,
            jitter_next: Cell::new(JITTER_SEED),
            stars: None,
            dots: None,
            sky: [0; 3],
            background: Cell::new(0),
            names: None,
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
    pub fn lift_rows(&self, angle: u16) -> [[f64; 3]; 3] {
        let matrix = ffe_core::model_animation::build_yzt(&self.table, 0, i32::from(angle));
        std::array::from_fn(|k| {
            std::array::from_fn(|j| f64::from(matrix[3 * k + j]) / 2_147_483_648.0)
        })
    }

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

    /// Set `DATA_008812`, the "Level of Shape Detail" F1876 reads. The original
    /// stores the chosen row less one: the executed boot's default "High" is 1,
    /// and the game clamps a negative value to 0 (`F917`). The viewer starts at
    /// 1, the intro's value; callers with a preference update it.
    pub fn set_surface_detail(&self, detail: i32) {
        self.surface_detail.set(detail.max(0));
    }

    /// The current `DATA_008812` value.
    pub fn surface_detail(&self) -> i32 {
        self.surface_detail.get()
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
    /// `0x3016`/`0x3022` expand. Empty object strings remain empty.
    pub fn set_registration(&mut self, text: &[u8]) {
        self.registration = text.to_vec();
    }

    /// The commander name and short date FFE's game-over tombstone models
    /// (172/175) engrave through FF9C00..`03`. Model 172 shows "GAME OVER" and
    /// the obituary; model 175 shows the underlined name/date variant.
    pub fn set_game_over_text(&mut self, commander: &[u8], date: &[u8]) {
        self.commander = commander.to_vec();
        self.game_date = date.to_vec();
    }

    /// Draw only the tombstone face whose text records name `normal` (2 for the
    /// RIP/obituary face, 4 for the GAME OVER face). `None` draws every record
    /// and applies the ordinary culling.
    pub fn set_tombstone_face(&mut self, normal: Option<u8>) {
        self.tombstone_face = normal;
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
                Shape::Textured(textured) | Shape::TexturedUv(textured, _) => {
                    textured.points.clone()
                }
                Shape::Gouraud(points, _) => points.clone(),
                Shape::Glow(_) => Vec::new(),
                Shape::Label(_, _) => Vec::new(),
                Shape::Spans(spans, _) => spans.iter().map(|s| (s[0], s[1])).collect(),
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
        self.render_scene_with_scale(index, None, rows, position, light_view, inputs)
    }

    /// [`render_scene_with`] with F1536's object scale override: the original
    /// draws model 0x9d at the *hit ship's* scale (`+0x7e` from the ship
    /// statistics' `+0x14`), not the flash model's own zero scale.
    pub fn render_scene_with_scale(
        &self,
        index: u32,
        scale: Option<u32>,
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
        let scaled;
        let model = match scale {
            Some(scale) => {
                scaled = model.with_scale(scale);
                &scaled
            }
            None => model,
        };
        self.render_with(model, rows, position.map(|v| v as i32), light_view, inputs)
    }

    /// The matrix F1675 builds: yaw `a` about the vertical, then pitch `b` (binary angles).
    pub fn matrix_yzt(&self, a: i32, b: i32) -> [i32; 9] {
        ffe_core::model_animation::build_yzt(&self.table, a, b)
    }

    /// F1686's cull radius of a model: its descriptor's radius shifted by its scale.
    pub fn model_radius(&self, index: u32) -> Option<i32> {
        let model = self.find(index)?;
        let [_, radius] = *self.descriptors.get(index as usize)?;
        Some(radius.wrapping_shl(model.scale))
    }

    /// F830: the name of a system code (cache v21 and v22).
    pub fn system_name(&self, code: u32) -> Option<Vec<u8>> {
        let (tables, strings) = self.names.as_ref()?;
        let tables = ffe_core::sector::Tables::new(tables).ok()?;
        let strings = ffe_core::sector::Strings::new(strings).ok()?;
        tables.system_name(&strings, code)
    }

    /// One of the system strings (cache v22) by its F830 selector (`0x84xx`).
    pub fn system_string(&self, selector: u32) -> Option<Vec<u8>> {
        let (_, strings) = self.names.as_ref()?;
        let strings = ffe_core::sector::Strings::new(strings).ok()?;
        let index = (selector & 0x1ff).checked_sub(1)? as usize;
        strings.get(index).map(<[u8]>::to_vec)
    }

    /// The ambient colour a dot's halo mixes with (`DATA_008815`).
    pub fn set_sky(&mut self, colour: [u8; 3]) {
        self.sky = colour;
    }

    /// The background colour word `F1330` registers before building
    /// (`F1575`/`F1326` on `DATA_009217`).
    pub fn set_background(&self, colour: u16) {
        self.background.set(colour & 0xfff);
    }

    /// The background word last set.
    pub fn background(&self) -> u16 {
        self.background.get()
    }

    /// Register the background and build the palette; for a frame with no
    /// model (the intro's cleared loop point).
    pub fn prepare_palette(&self) {
        self.textures.register_colour(self.background.get());
        self.textures.build_palette();
    }

    /// The resolved RGB of a colour word through the runtime upper palette
    /// (or the low palette when the fixed map names it).
    pub fn palette_rgb(&self, colour: u16) -> Option<[u8; 3]> {
        self.textures.palette_rgb(colour)
    }

    /// One palette entry by video index (0..255), expanded by four.
    pub fn palette_entry(&self, index: u8) -> Option<[u8; 3]> {
        self.textures.palette_entry(index)
    }

    /// The current 128 upper entries as six-bit RGB (bank order).
    pub fn palette_upper6(&self) -> Option<Vec<[u8; 3]>> {
        self.textures.palette_upper6()
    }

    /// Begin a frame's scene: F1052 sets `DATA_008814` to 0 before F1536 draws
    /// the objects, and F1877 sets it again for a planet whose haze holds the eye.
    pub fn clear_haze_sky(&self) {
        self.haze_sky.set(None);
    }

    /// F1877's background colour from the scene just drawn, when a planet's
    /// atmosphere set it (`DATA_008814` non-zero).
    pub fn haze_sky(&self) -> Option<[u8; 3]> {
        self.haze_sky.get()
    }

    /// The star light-point tables, when the cache has them (v23).
    pub fn star_tables(&self) -> Option<&StarTables> {
        self.stars.as_ref()
    }

    /// Run the galaxy map's star model (0xab, opcode 30 `0x801e`) over one sector's star
    /// records (`records`, six bytes per star as F841 writes them): the stalk lines, points
    /// and labels of F1769 in the original's depth order. `matrix` is the object-to-view
    /// rotation (rows in Q31), `position` the object's origin in view space.
    pub fn execute_stars(
        &self,
        field: &StarField,
        records: &[u8],
        matrix: [i32; 9],
        position: [i32; 3],
    ) -> Result<Execution, String> {
        let tables = self
            .stars
            .as_ref()
            .ok_or("The star tables need cache v23.")?;
        let model = self
            .find(STAR_MODEL)
            .ok_or("The star model is not in the cache.")?;
        if field.types.len() > records.len() / 6 || records.len() > model.vertices.len() {
            return Err("The star records do not match the model.".to_owned());
        }
        let mut padded = records.to_vec();
        padded.resize(model.vertices.len(), 0);
        let mut vertices = Vertices::new(&padded, model.vertex_count)?.with_stars(tables, field);
        let mut context = VertexContext {
            matrix,
            basis: matrix,
            scale: model.scale,
            ..Default::default()
        };
        context.globals[1] = if field.stalks { 0x20 } else { 0 };
        context.globals[2] = field.code as u16;
        context.globals[3] = (field.code >> 16) as u16;
        context.globals[4] = field.types.len() as u16;
        let instance = InstanceContext {
            position,
            generation: GENERATION,
            key_scale: 0,
            operand_scale: model.scale as i32,
            textures: false,
        };
        let visibility = VisibilityContext {
            depth: 1,
            scale: 0,
            detail: [0, 0],
            normals: &[],
        };
        execute_live(
            &model.commands,
            &mut vertices,
            &mut context,
            &visibility,
            Some(&self.table),
            instance,
            4096,
        )
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
        self.render_matrix(model, matrix, rows, position, light_view, inputs, None)
    }

    /// Run `model` with an exact object matrix (rows in Q31, as the original keeps it) and
    /// draw what it queues. `rows` are the same axes as unit vectors (for the observer and the
    /// light). `stars` replaces the model's vertex records with a sector's star records and
    /// gives its light points their tables (the galaxy map's model 0xab).
    #[allow(clippy::too_many_arguments)]
    pub fn render_matrix(
        &self,
        model: &ViewerModel,
        matrix: [i32; 9],
        rows: [[f64; 3]; 3],
        position: [i32; 3],
        light_view: [f64; 3],
        inputs: &AnimationInputs,
        stars: Option<(&StarField, &[u8])>,
    ) -> Result<Frame, String> {
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
        let padded;
        if std::env::var_os("FFE_NORMAL_DEBUG").is_some() && model.index == 172 {
            let culled: Vec<usize> = normals
                .iter()
                .enumerate()
                .filter(|(_, normal)| normal.culled)
                .map(|(i, _)| i)
                .collect();
            eprintln!(
                "normals172 count {} observer {observer:?} culled {culled:?}",
                model.normal_count
            );
        }
        let mut vertices = match (stars, &self.stars) {
            (Some((field, records)), Some(tables)) => {
                if field.types.len() > records.len() / 6 || records.len() > model.vertices.len() {
                    return Err("The star records do not match the model.".to_owned());
                }
                let mut bytes = records.to_vec();
                bytes.resize(model.vertices.len(), 0);
                padded = bytes;
                Vertices::new(&padded, model.vertex_count)?
                    .with_normal_records(&model.normals)
                    .with_stars(tables, field)
            }
            (Some(_), None) => return Err("The star tables need cache v23.".to_owned()),
            (None, _) => Vertices::new(&model.vertices, model.vertex_count)?
                .with_normal_records(&model.normals),
        };
        vertices.set_jitter(inputs.jitter);
        let mut context = VertexContext {
            matrix,
            basis: matrix,
            scale: model.scale,
            globals: inputs.globals,
            locals: inputs.locals,
            ..Default::default()
        };
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
        if children_error.is_none() {
            children_error = child_error(&result.children);
        }
        // F1326 registration happens while the original queues its primitives
        // (model faces, text glyphs and the curved-surface entries), before
        // F1330 builds. Text glyphs are executed and surface queues generated
        // here so every colour they use is in the cube for this build. Start
        // from an empty cube so the boxes are cut around this frame's colours.
        self.textures.clear_palette_cube();
        self.register_execution_colours(&result);
        let mut frame = Frame {
            children_error,
            jitter: vertices.jitter(),
            texts: result.texts.iter().filter(|text| !text.culled).count(),
            ..Frame::default()
        };
        let mut text_runs = Vec::new();
        self.collect_texts(
            &result,
            model.index,
            &mut vertices,
            instance,
            light_view,
            &mut frame,
            &mut text_runs,
        )?;
        self.collect_child_texts(&result.children, light_view, &mut frame, &mut text_runs)?;
        // Text glyph colours are F1326-registered by F1747 before the build. The
        // visible glyph faces use the font's fixed low-palette materials, and the
        // text instance colour (the GAME!TEK cyan) is only added to local faces;
        // registering it would put a dynamic cell in model 177's empty upper
        // palette, which the executed original does not have.
        let (surfaces, skipped, haze) = self.collect_surfaces(&result, light);
        frame.skipped += skipped;
        for batch in &surfaces {
            self.register_surface_colours(batch);
        }
        // F1330 runs after the scene has registered its colours and before it
        // draws. Every model render receives its own build; the cube and the
        // two banks persist on the viewer, as in the original.
        self.prepare_palette();
        if let Some(word) = haze {
            self.haze_sky.set(Some(self.textures.rgb(word)));
        }
        self.draw_primitives(&result, &mut frame)?;
        for run in &text_runs {
            self.draw_primitives(run, &mut frame)?;
        }
        // F1840/F1841 queue their entries into the same F1778 list as the model's
        // faces; merge them by key so the planet is not painted over a nearer ship.
        let surface_shapes = self.surface_shapes(&surfaces);
        Self::merge_surface_shapes(&mut frame, surface_shapes);
        Ok(frame)
    }

    /// One F1805/F1807/F1806/F1808 queue entry as a texture recipe. `at` is the
    /// entry's header word (`at + 2` is its handler); `explicit` selects the
    /// F1806/F1808 form with its own u/v pairs and a slot table written at a
    /// base offset (`tex >> 16`), as F1645/F1648 and their entry handlers read
    /// them. `None` when the words are short or the bitmap is not in the cache.
    ///
    /// The recipe keeps the slot colour words (F1326) rather than RGB, so the
    /// caller can register them with the runtime palette before F1330 builds.
    fn terrain_texture(
        &self,
        w: &[u32],
        at: usize,
        key: u32,
        corners: usize,
        explicit: bool,
    ) -> Option<TerrainTexture> {
        let entry = at + 2;
        let words = match (corners, explicit) {
            (3, false) => 12,
            (4, false) => 13,
            (3, true) => 20,
            (4, true) => 22,
            _ => return None,
        };
        w.get(entry..entry + words)?;
        let (tex_at, uv_at, count_at, colours_at) = match (corners, explicit) {
            (3, false) => (4, None, 5, 5),
            (4, false) => (5, None, 6, 6),
            (3, true) => (7, Some(4), 8, 9),
            (4, true) => (9, Some(5), 10, 11),
            _ => return None,
        };
        let points = (0..corners)
            .map(|k| {
                let word = w[entry + 1 + k];
                (i32::from(word as i16), i32::from((word >> 16) as i16))
            })
            .collect::<Vec<_>>();
        let tex = w[entry + tex_at];
        let id = tex & 0xfff;
        // F1620/F1643: a missing bitmap has nothing to draw.
        self.textures.map(id)?;
        let slots: Vec<(usize, u16)> = if explicit {
            let base = ((tex >> 16) as u16) as usize;
            let count = (w[entry + count_at] & 0xffff) as usize;
            let mut slots = Vec::with_capacity(count);
            for k in 0..count {
                let word = *w.get(entry + colours_at + k / 2)?;
                let colour = if k % 2 == 0 { word } else { word >> 16 };
                slots.push((base + k, (colour & 0xffff) as u16));
            }
            slots
        } else {
            (0..7)
                .map(|k| (k, (w[entry + colours_at + k] & 0xffff) as u16))
                .collect()
        };
        let uv: Option<Vec<[i16; 2]>> = uv_at.map(|first| {
            (0..corners)
                .map(|k| {
                    let word = w[entry + first + k];
                    [word as i16, (word >> 16) as i16]
                })
                .collect()
        });
        Some(TerrainTexture {
            key,
            points,
            bitmap: if explicit { id } else { tex },
            slots,
            uv,
        })
    }

    /// Resolve a surface recipe's slot colours through the built palette.
    fn texture_shape(&self, recipe: &TerrainTexture) -> Shape {
        let length = recipe
            .slots
            .iter()
            .map(|(index, _)| index + 1)
            .max()
            .unwrap_or(0);
        let mut slots = vec![[0u8; 3]; length];
        for (index, word) in &recipe.slots {
            slots[*index] = self.textures.rgb(*word);
        }
        let textured = Textured {
            points: recipe.points.clone(),
            bitmap: recipe.bitmap,
            slots,
        };
        match &recipe.uv {
            Some(uv) => Shape::TexturedUv(textured, uv.clone()),
            None => Shape::Textured(textured),
        }
    }

    /// Opcode 31: generate the curved surfaces through the ported F1876 and its
    /// emitter and keep the queue, far-to-near order, for drawing after F1330.
    /// The skip count and F1877's raw haze colour word are returned as well.
    fn collect_surfaces(
        &self,
        result: &Execution,
        light: [i32; 3],
    ) -> (Vec<SurfaceBatch>, usize, Option<u16>) {
        use ffe_core::terrain::Terrain;
        use ffe_core::terrain_draw::Scene;
        use ffe_core::terrain_emit::{draw_planet, Queue, Shading};
        use ffe_core::terrain_haze::{Haze, Sky};
        let mut batches = Vec::new();
        let mut skipped = 0usize;
        let mut haze = None;
        let mut emissions = Vec::new();
        all_surfaces(result, &mut emissions);
        for emission in emissions {
            let (Some((table, maps)), Some(origin), Some(vectors)) =
                (&self.terrain, emission.origin, emission.axes)
            else {
                skipped += 1;
                continue;
            };
            let surface = &emission.surface;
            self.surface_extent.set(
                vectors
                    .iter()
                    .map(|v| v.iter().map(|c| f64::from(*c).powi(2)).sum::<f64>().sqrt())
                    .fold(0.0, f64::max),
            );
            let mut stream: Vec<u8> = surface.entries.iter().flatten().copied().collect();
            stream.extend([0, 0]);
            let scene = Scene {
                seed: self.surface_seed.get(),
                origin,
                axes: std::array::from_fn(|k| vectors[k / 3][k % 3]),
                kind: surface.parameters[0].max(0) as usize,
                detail: self.surface_detail.get(),
                zoom: 0,
            };
            // A scaled-down matrix (distant planets) shortens the light vector.
            let length = light
                .iter()
                .map(|c| f64::from(*c).powi(2))
                .sum::<f64>()
                .sqrt();
            let light = if length > 0.0 {
                light.map(|c| (f64::from(c) / length * 32_768.0) as i32)
            } else {
                light
            };
            let shading = Shading {
                light,
                shades: self.lighting.shades,
                detail: scene.detail,
                atmosphere: None,
                material: [8, 8, 8],
            };
            let mut terrain = Terrain::new(table, maps);
            let mut queue = Queue::with_limit(SURFACE_QUEUE_WORDS);
            // An overflow of the original's arenas or queue ends the drawing early;
            // what was queued stands.
            let word0 = i32::from(surface.word);
            let material = [
                ((word0 >> 8) & 15) as u8,
                ((word0 >> 4) & 15) as u8,
                (word0 & 15) as u8,
            ];
            let extent = self.surface_extent.get();
            let eye_squared = origin
                .iter()
                .map(|v| f64::from(*v) * f64::from(*v))
                .sum::<f64>();
            let eye_inside = extent > 0.0 && eye_squared < extent * extent;
            let record = Haze {
                origin,
                axes: scene.axes,
                material,
                // F1877's haze ring and sky are for an eye outside the planet.
                // The landed view sits on the terrain, inside the base sphere;
                // the original clears to the space colour there, so the ring is
                // left out rather than painted over the sky and ground.
                strength: if eye_inside {
                    0
                } else {
                    i32::from(surface.parameters[1]).wrapping_shl(16)
                },
                scale: i32::from(surface.parameters[2]),
                light,
                shades: self.lighting.shades,
            };
            let shading = Shading {
                material,
                ..shading
            };
            let mut sky = Sky::default();
            let outcome = draw_planet(
                &mut terrain,
                &scene,
                &stream,
                &shading,
                &record,
                emission.context.scale as i32,
                &self.table,
                &mut sky,
                &mut queue,
            );
            // A failed patch leaves the rest of the planet undrawn; the caller
            // reports the frame's skipped count rather than logging every frame.
            let _ = &outcome;
            // F1877 writes the background flag and colour when the eye is in
            // the atmosphere: the original's F1052 clears the next upper view to
            // it (`DATA_008814..8817`), and only draws stars when the flag is 0.
            if sky.flag != 0 {
                let [red, green, blue] = sky.colour;
                haze = Some(u16::from(red) << 8 | u16::from(green) << 4 | u16::from(blue));
            }
            let mut order = queue.headers();
            order.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
            batches.push(SurfaceBatch {
                words: queue.words,
                order,
            });
        }
        (batches, skipped, haze)
    }

    /// Register every colour a surface queue will resolve (F1326 while F1840
    /// queues, before F1330 builds).
    fn register_surface_colours(&self, batch: &SurfaceBatch) {
        let w = &batch.words;
        for (key, at) in &batch.order {
            let handler = w[*at + 2];
            if handler == GROUP_OPEN {
                let mut pos = *at + 3;
                while pos < w.len() && w[pos] != GROUP_CLOSE {
                    match w[pos] & 0xff {
                        5 => {
                            // F1879's textured ring quad (bitmap 0x43 and its
                            // seven slot colours) is drawn through the palette.
                            if let Some(recipe) = self.terrain_texture(w, pos - 2, *key, 4, false) {
                                for (_, word) in &recipe.slots {
                                    self.textures.register_colour(*word);
                                }
                            } else {
                                for k in 6..=12 {
                                    self.textures.register_colour((w[pos + k] & 0xfff) as u16);
                                }
                            }
                            pos += 13;
                        }
                        10 => {
                            self.textures.register_colour((w[pos + 5] & 0xfff) as u16);
                            pos += 6;
                        }
                        _ => break,
                    }
                }
                continue;
            }
            let corners = match handler & 0xff {
                4 | 7 | 8 => 3,
                5 | 6 | 9 => 4,
                _ => continue,
            };
            let explicit = matches!(handler & 0xff, 6 | 8);
            if let Some(recipe) = self.terrain_texture(w, *at, *key, corners, explicit) {
                for (_, word) in &recipe.slots {
                    self.textures.register_colour(*word);
                }
                continue;
            }
            for word in self.surface_fallback_colours(w, *at, handler & 0xff) {
                self.textures.register_colour(word);
            }
        }
    }

    /// The colour words the flat/Gouraud fallback for an entry resolves.
    fn surface_fallback_colours(&self, w: &[u32], at: usize, handler: u32) -> Vec<u16> {
        match handler {
            8 => (0..3)
                .map(|k| ramp_word(w, at, w[at + 6 + k] >> 16, 9))
                .collect(),
            6 => vec![
                ramp_word(w, at, w[at + 7] >> 16, 11),
                ramp_word(w, at, w[at + 8] >> 16, 11),
                ramp_word(w, at, 0, 11),
                ramp_word(w, at, 0, 11),
            ],
            other => vec![match other {
                4 => (w[at + 6] & 0xfff) as u16,
                7 => (w[at + 10] & 0xfff) as u16,
                _ => (w[at + 11] & 0xfff) as u16,
            }],
        }
    }

    /// The surfaces' queues as depth-keyed shapes, far to near after the palette
    /// build. Flat entries are F1804 triangles; the rest are F1805/F1807
    /// descriptor textures or F1806/F1808 explicit-coordinate textures, drawn by
    /// F1644/F1647. F1877's haze ring entries keep the group's key.
    fn surface_shapes(&self, batches: &[SurfaceBatch]) -> Vec<(u32, Shape)> {
        let mut shapes = Vec::new();
        for batch in batches {
            let w = &batch.words;
            for &(key, at) in &batch.order {
                let handler = w[at + 2];
                if handler == GROUP_OPEN {
                    // The atmosphere ring: bare entries up to the close marker.
                    let mut pos = at + 3;
                    while pos < w.len() && w[pos] != GROUP_CLOSE {
                        let point = |k: usize| {
                            let word = w[pos + k];
                            (i32::from(word as i16), i32::from((word >> 16) as i16))
                        };
                        let colour = |value: u32| self.textures.rgb((value & 0xfff) as u16);
                        match w[pos] & 0xff {
                            5 => {
                                // F1879's textured ring quad. Its words after the
                                // handler are the regular F1805 layout: four
                                // points, the descriptor dword (bitmap 0x43), then
                                // the seven slot colours. Draw the real texture
                                // through the palette; the old two-colour Gouraud
                                // stand-in interpolated RGB and put colours on the
                                // planet's limb that no palette entry held.
                                if let Some(texture) =
                                    self.terrain_texture(w, pos - 2, key, 4, false)
                                {
                                    shapes.push((key, self.texture_shape(&texture)));
                                    pos += 13;
                                    continue;
                                }
                                // Corners (outer, inner, inner, outer); the seven colours
                                // run from the inner edge to the outer.
                                let (inner, outer) = (colour(w[pos + 6]), colour(w[pos + 12]));
                                let points = [point(1), point(2), point(3), point(4)];
                                let colours = [outer, inner, inner, outer];
                                shapes.push((
                                    key,
                                    Shape::Gouraud(
                                        vec![points[0], points[1], points[2]],
                                        vec![colours[0], colours[1], colours[2]],
                                    ),
                                ));
                                shapes.push((
                                    key,
                                    Shape::Gouraud(
                                        vec![points[0], points[2], points[3]],
                                        vec![colours[0], colours[2], colours[3]],
                                    ),
                                ));
                                pos += 13;
                            }
                            10 => {
                                let points = vec![point(1), point(2), point(3), point(4)];
                                shapes.push((key, Shape::Fill(points, colour(w[pos + 5]))));
                                pos += 6;
                            }
                            _ => break,
                        }
                    }
                    continue;
                }
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
                // F1805/F1807 (descriptor coordinates) and F1806/F1808 (explicit
                // coordinates, the planet surface) draw the real bitmap when the
                // cache carries it. Without it, the old flat/gouraud stand-in runs.
                let explicit = matches!(handler & 0xff, 6 | 8);
                if let Some(texture) = self.terrain_texture(w, at, key, corners, explicit) {
                    shapes.push((texture.key, self.texture_shape(&texture)));
                    continue;
                }
                // The detail entries (F1808, F1806) colour each corner from the 22-step
                // ramp by its height band and blend across the face; the textured
                // ones (F1807, F1805) take the middle of their seven slots, the flat
                // ones their colour.
                let colour = |word: u16| self.textures.rgb(word);
                match handler & 0xff {
                    8 => {
                        let colours = self
                            .surface_fallback_colours(w, at, 8)
                            .into_iter()
                            .map(colour)
                            .collect::<Vec<_>>();
                        shapes.push((key, Shape::Gouraud(points, colours)));
                    }
                    6 => {
                        let colours = self.surface_fallback_colours(w, at, 6);
                        shapes.push((
                            key,
                            Shape::Gouraud(
                                vec![points[0], points[1], points[2]],
                                vec![colour(colours[0]), colour(colours[1]), colour(colours[2])],
                            ),
                        ));
                        shapes.push((
                            key,
                            Shape::Gouraud(
                                vec![points[0], points[2], points[3]],
                                vec![colour(colours[0]), colour(colours[2]), colour(colours[3])],
                            ),
                        ));
                    }
                    other => {
                        let colours = self.surface_fallback_colours(w, at, other);
                        shapes.push((key, Shape::Fill(points, self.textures.rgb(colours[0]))));
                    }
                }
            }
        }
        shapes
    }

    /// Insert curved-surface shapes into a frame's far-to-near order (F1788's
    /// descending key). Primitives and glyph faces are already in that order;
    /// each surface entry goes just after every shape whose key is at least its
    /// own, so a near ship draws over a far planet as the original's one queue
    /// does. Equal keys keep the primitive before the surface, and surfaces in
    /// their queue order.
    fn merge_surface_shapes(frame: &mut Frame, shapes: Vec<(u32, Shape)>) {
        for (key, shape) in shapes {
            let at = frame
                .keys
                .iter()
                .position(|value| *value < key)
                .unwrap_or(frame.keys.len());
            frame.keys.insert(at, key);
            frame.shapes.insert(at, shape);
        }
    }

    /// Register every colour the executed primitives resolve to, before the
    /// build. The original calls F1326 while queueing its fills, i.e. after
    /// executing the model and before F1330 draws with the palette.
    fn register_execution_colours(&self, result: &Execution) {
        let descriptors = self.textures.descriptors();
        for primitive in &result.ordered {
            match primitive {
                Primitive::Face(emission) => {
                    let textured = material(emission.face.material).texture != 0;
                    let colour = emission.context.colour;
                    let rgb = match (textured, descriptors) {
                        (true, Some(table)) => {
                            flat_colour(emission.face.material, colour, emission.shade, table).ok()
                        }
                        (true, None) => Some(std::array::from_fn(|i| 5 + emission.shade[i])),
                        (false, _) => Some(resolve_colour(
                            emission.face.material,
                            colour,
                            emission.shade,
                        )),
                    };
                    if let Some(rgb) = rgb {
                        self.register_frame_colour(rgb);
                    }
                    if textured {
                        if let Some(table) = descriptors {
                            if let Ok(entry) = textured_colours(
                                emission.face.material,
                                colour,
                                emission.shade,
                                table,
                            ) {
                                for slot in entry.colours {
                                    self.textures.register_colour(slot);
                                }
                            }
                        }
                    }
                }
                Primitive::Line(line) => {
                    self.register_frame_colour(resolve_colour(
                        line.material,
                        line.context.colour,
                        [0; 3],
                    ));
                }
                Primitive::Point(point) => {
                    self.register_frame_colour(resolve_colour(
                        point.material,
                        point.context.colour,
                        [0; 3],
                    ));
                }
                Primitive::Spline(spline) => self.register_frame_colour(spline.colour),
                Primitive::Chain(chain) => self.register_frame_colour(resolve_colour(
                    chain.chain.material,
                    chain.context.colour,
                    [0; 3],
                )),
                Primitive::Polygon(polygon) => {
                    let colour = if material(polygon.polygon.material).texture != 0 {
                        [5; 3]
                    } else {
                        polygon.colour
                    };
                    self.register_frame_colour(colour);
                }
                Primitive::Thruster(entry) => match &entry.shape {
                    ThrusterShape::Glow { colours, .. } => {
                        for colour in colours {
                            self.register_frame_colour(*colour);
                        }
                    }
                    ThrusterShape::Line { colour, .. } => {
                        let colour = match descriptors {
                            Some(table) => {
                                line_colour(entry.material, *colour, table).unwrap_or(*colour)
                            }
                            None => *colour,
                        };
                        self.register_frame_colour(colour);
                    }
                },
                Primitive::Label(_) | Primitive::SystemLabel(_) => {}
            }
        }
    }

    fn register_frame_colour(&self, colour: [u8; 3]) {
        self.textures.register_colour(pack(colour) & 0xfff);
    }

    /// Turn one executed stream's render-ordered primitives into shapes.
    fn draw_primitives(&self, result: &Execution, frame: &mut Frame) -> Result<(), String> {
        let point =
            |p: [i16; 2]| (p != NEAR_SENTINEL).then_some((i32::from(p[0]), i32::from(p[1])));
        for (index, primitive) in result.ordered.iter().enumerate() {
            let key = result.ordered_keys.get(index).copied().unwrap_or_default();
            // Every shape keeps its key with it: a `continue` below must not
            // shift the alignment between `shapes` and `keys`.
            macro_rules! push {
                ($shape:expr) => {{
                    frame.shapes.push($shape);
                    frame.keys.push(key);
                }};
            }
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
                    let height = ffe_core::model_execute::view_height();
                    let on_view = !behind
                        && emission.screen.iter().all(|[x, y]| {
                            (0..320).contains(&i32::from(*x)) && (0..height).contains(&i32::from(*y))
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
                        if std::env::var_os("FFE_FACE_DEBUG").is_some() {
                            eprintln!(
                                "outline face material {:#x} colour {rgb:?} screen {:?} behind {behind}",
                                face.material, emission.screen
                            );
                        }
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
                            push!(Shape::Polygon(edges, rgb));
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
                            push!(Shape::Textured(Textured {
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
                    push!(if points.len() == 2 {
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
                        push!(Shape::Line(point(a), point(b), rgb));
                    }
                }
                Primitive::Point(emitted) => {
                    let rgb = self.rgb(resolve_colour(
                        emitted.material,
                        emitted.context.colour,
                        [0; 3],
                    ));
                    // An unlit point is a dot (F1699 to F1801): a halo and a core, or one
                    // particle. A lit one is a shaded sphere, not ported: a pixel.
                    let dot = match (&self.dots, emitted.screen, emitted.view) {
                        (Some(tables), Some(screen), Some(view))
                            if material(emitted.material).unlit =>
                        {
                            point_dot(
                                emitted.material,
                                emitted.angle,
                                emitted.scale,
                                emitted.context.colour,
                                screen,
                                [view[0], view[1]],
                                view[2],
                                tables,
                                self.sky,
                            )
                            .ok()
                        }
                        _ => None,
                    };
                    match dot {
                        Some(Some(dot)) => {
                            let (x, y) = (dot.x, dot.y);
                            let particles: Vec<(i32, [u8; 3])> = match dot.shape {
                                DotShape::One { size, colour } => vec![(size, colour)],
                                DotShape::Two {
                                    halo_size,
                                    halo,
                                    core_size,
                                    core,
                                } => vec![(halo_size, halo), (core_size, core)],
                            };
                            for (size, colour) in particles {
                                let spans = ffe_core::model_dot::particle_clipped(x, y, size)
                                    .into_iter()
                                    .map(|s| [s.x, s.y, s.width])
                                    .collect();
                                push!(Shape::Spans(spans, self.rgb(colour)));
                            }
                        }
                        // Off the view or too near: nothing.
                        Some(None) => {}
                        None => {
                            if let Some(p) = emitted.screen.and_then(point) {
                                push!(Shape::Point(p, rgb));
                            }
                        }
                    }
                }
                Primitive::Spline(spline) => {
                    // F1811: the entry's four points, drawn by F1546.
                    if let Some(points) = spline.points {
                        push!(Shape::Curve(points, self.rgb(spline.colour)));
                    }
                }
                Primitive::Chain(chain) => {
                    // F1814: one F1611 particle of the entry's size at each
                    // point F1766 projected (the 0x7f terminator never reaches
                    // `points`).
                    let rgb = resolve_colour(chain.chain.material, chain.context.colour, [0; 3]);
                    let colour = self.rgb(rgb);
                    for [x, y] in &chain.points {
                        let spans = particle_clipped(*x, *y, chain.size)
                            .into_iter()
                            .map(|s| [s.x, s.y, s.width])
                            .collect();
                        push!(Shape::Spans(spans, colour));
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
                        push!(Shape::Polygon(edges, self.rgb(colour)));
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
                        push!(Shape::Glow(Textured {
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
                        push!(Shape::Line(a, b, self.rgb(colour)));
                    }
                },
                // A star's system name (F1812 with the 0x84xx string), in the bitmap font.
                Primitive::SystemLabel(label) => match self.system_name(label.code) {
                    Some(name) if label.key.is_some() => push!(Shape::Label(
                        (i32::from(label.screen[0]), i32::from(label.screen[1])),
                        name,
                    )),
                    Some(_) => {}
                    None => frame.skipped += 1,
                },
                Primitive::Label(label) => match self.model_string(label.label.string) {
                    Ok(text) => push!(Shape::Label(
                        (i32::from(label.screen[0]), i32::from(label.screen[1])),
                        text.to_vec(),
                    )),
                    Err(_) => frame.skipped += 1,
                },
            }
        }
        Ok(())
    }

    /// The texts of child models (the lift's posters are child models of the lift
    /// and the bay): each executed child's texts are executed in its own instance,
    /// with a fresh vertex cache over its records and the externals its parent gave
    /// it, and their glyph primitives are queued for drawing after the palette
    /// build.
    fn collect_child_texts(
        &self,
        runs: &[ChildRun],
        light_view: [f64; 3],
        frame: &mut Frame,
        out: &mut Vec<Execution>,
    ) -> Result<(), String> {
        for run in runs {
            let (ChildOutcome::Executed(execution), Some(setup)) = (&run.outcome, &run.setup)
            else {
                continue;
            };
            let table = ChildTable(self);
            let child = table.model(run.model)?;
            let mut vertices =
                Vertices::child(child.vertices, child.vertex_count, setup.externals)?
                    .with_normal_records(child.normals);
            frame.texts += execution.texts.iter().filter(|text| !text.culled).count();
            self.collect_texts(
                execution,
                u32::from(run.model),
                &mut vertices,
                setup.instance,
                light_view,
                frame,
                out,
            )?;
            self.collect_child_texts(&execution.children, light_view, frame, out)?;
        }
        Ok(())
    }

    /// Opcode 10 text (F1747): run each character's glyph program through the
    /// executor in a frame at the text's position, and move the pen to the font
    /// vertex the program's final return word names. The glyph geometry is queued
    /// for drawing after the model's own primitives (the original queues it into
    /// the same depth order, which is not modelled yet). Control codes below
    /// 0x20 use the font's component anchors, which are not imported: a string
    /// stops there and is reported in [`Frame::skipped`] rather than guessed.
    #[allow(clippy::too_many_arguments)]
    fn collect_texts(
        &self,
        result: &Execution,
        model: u32,
        parent: &mut Vertices<'_>,
        instance: InstanceContext,
        light_view: [f64; 3],
        frame: &mut Frame,
        out: &mut Vec<Execution>,
    ) -> Result<(), String> {
        for emission in &result.texts {
            if std::env::var_os("FFE_TEXT_DEBUG").is_some() {
                eprintln!(
                    "text {:#06x} normal {} culled {} shade {:?} material {:#x} colour {:?} glyphs {} size {} vertex {}",
                    emission.text.string,
                    emission.text.normal,
                    emission.culled,
                    emission.shade,
                    emission.text.material,
                    emission.context.colour,
                    emission.text.glyphs,
                    emission.text.size,
                    emission.text.vertex
                );
            }
            let tombstone_text = matches!(emission.text.string, 0x9c00..=0x9c03 | 0x4085..=0x4089);
            if let Some(face) = self.tombstone_face {
                if tombstone_text && emission.text.normal != face {
                    continue;
                }
                // The selected face's glyph records must be drawn even when the
                // degenerate font normals cull them; the face was chosen by the
                // model's spin angle in `Screen::draw_game_over`.
                if !tombstone_text && emission.culled {
                    continue;
                }
            } else if emission.culled {
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
            // F1740 picks the text frame (mode 6 copies the parent basis) and
            // F1747 negates its first row when that frame is left-handed.
            let word = u32::from(text.frame);
            let (matrix, _) = ffe_core::model_math::text_frame(
                &emission.context.matrix,
                emission.context.base_handedness,
                &emission.context.basis,
                emission.context.handedness,
                word,
            )
            .map_err(|error| format!("{error} (text at {:#x})", text.offset))?;
            let mut pen = position.view;
            let mut line = pen;
            // F1747 runs the glyphs in a nested instance whose operand scale (+0x148) is the
            // record's size nibble, not the font model's own scale, and whose generation
            // (+0xf0) is a counter that starts at 2 and moves on at every pen move, so
            // each vertex of the font is evaluated afresh for every glyph.
            let size = u32::from(text.size);
            let mut glyph_vertices = Vertices::new(&font.vertices, font.vertex_count)?
                .with_normal_records(&font.normals);
            // F1747 resolves the text's own material through F1692/F1693/F1691
            // (material RGB + instance colour + the text normal's colour) and
            // gives that to the glyph instance as its colour. Without it the
            // font's black faces come out black (the GAME!TEK letters).
            let text_colour =
                resolve_colour(text.material, emission.context.colour, emission.shade);
            let mut glyph_context = VertexContext {
                matrix,
                basis: matrix,
                scale: size,
                globals: emission.context.globals,
                locals: emission.context.locals,
                colour: text_colour,
                ..Default::default()
            };
            // The glyphs are lit as any object: the font's normal records, with the camera and
            // the light in the text's frame (F1685, run once for the string).
            let axes: [[f64; 3]; 3] = std::array::from_fn(|k| {
                std::array::from_fn(|j| f64::from(matrix[3 * k + j]) / 2_147_483_648.0)
            });
            let unit = 2f64.powi(size as i32);
            let observer = axes.map(|row| {
                let along: f64 = row.iter().zip(pen).map(|(a, p)| a * f64::from(p)).sum();
                (-along / unit) as i32
            });
            let length = light_view
                .iter()
                .map(|v| v * v)
                .sum::<f64>()
                .sqrt()
                .max(1e-9);
            let light = axes.map(|row| {
                (row.iter().zip(light_view).map(|(a, b)| a * b).sum::<f64>() / length * 32_768.0)
                    as i32
            });
            let mut normals = model_normals::evaluate(
                &font.normals,
                font.normal_count,
                &font.vertices,
                font.vertex_count,
                &NormalContext {
                    scale: size,
                    observer_scale: size,
                    observer,
                    light,
                    shades: self.lighting.shades,
                },
            )?;
            // The fonts declare no normal records, yet some glyph faces name normals 2 and 3,
            // which the original reads from whatever its buffer held: visible and unlit here.
            normals.resize(
                256,
                ffe_core::model_normals::Normal {
                    culled: false,
                    colour: [0; 3],
                },
            );
            let visibility = VisibilityContext {
                depth: 1,
                scale: 0,
                detail: [0, 0],
                normals: &normals,
            };
            if std::env::var_os("FFE_TEXT_DEBUG").is_some() {
                let culled: Vec<usize> = normals
                    .iter()
                    .enumerate()
                    .filter(|(_, normal)| normal.culled)
                    .map(|(i, _)| i)
                    .collect();
                eprintln!(
                    "  glyphs {} size {size} font scale {} normals {} culled {:?} observer {observer:?} light {light:?}",
                    text.glyphs, font.scale, font.normal_count, culled
                );
            }
            let mut generation = 2u32;
            for &code in &string {
                if code < 0x20 {
                    // A control code starts a new line: the pen returns to the line's start
                    // and the font's newline vertex, seen from there, becomes the next
                    // line's start.
                    let Some(newline) = set.newline else {
                        frame.skipped += 1;
                        break;
                    };
                    pen = line;
                    generation += 1;
                    let next = glyph_vertices.resolve(
                        newline as i8,
                        &glyph_context,
                        pen,
                        generation,
                        false,
                    )?;
                    line = next.view;
                    pen = line;
                    generation += 1;
                    continue;
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
                    generation,
                    key_scale: 0,
                    operand_scale: size as i32,
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
                out.push(execution);
                let next =
                    glyph_vertices.resolve(anchor as i8, &glyph_context, pen, generation, false)?;
                pen = next.view;
                generation += 1;
            }
        }
        Ok(())
    }

    /// F1344 expansion for the families the models' text commands use: the model
    /// string table (`0x4000..0x8000`), the drawn object's own string
    /// (`0x3016`, and `0x3022` unwrapped — F1398's width table is not imported),
    /// and FFE's game-over templates `0x9c00..0x9c03` (models 172/175).
    fn expanded_text(&self, code: u16, _model: u32) -> Result<Vec<u8>, String> {
        match code {
            0x3016 | 0x3022 => {
                // F1344 reads the object's +0x124 string, not the model's
                // catalogue name. Intro objects have no supplied registration.
                Ok(self.registration.clone())
            }
            0x4000..=0x7fff => self.model_string(code).map(<[u8]>::to_vec),
            // DATA_005129..005132, the tombstone's four strings. 0x0d is the
            // glyph font's new line; the strings are otherwise literal.
            0x9c00 => Ok(b"\rGAME\rOVER".to_vec()),
            0x9c01 => {
                if self.commander.is_empty() || self.game_date.is_empty() {
                    return Err("Game-over text needs a commander and date.".to_owned());
                }
                let mut text = b"\r\r\r\r\r\r\r  COMMANDER\r   ".to_vec();
                text.extend_from_slice(&self.commander);
                text.extend_from_slice(b"\r     DIED\r  ");
                text.extend_from_slice(&self.game_date);
                Ok(text)
            }
            0x9c02 => {
                if self.commander.is_empty() {
                    return Err("Game-over text needs a commander.".to_owned());
                }
                let mut text = b"\r_".to_vec();
                text.extend_from_slice(&self.commander);
                Ok(text)
            }
            0x9c03 => {
                if self.game_date.is_empty() {
                    return Err("Game-over text needs a date.".to_owned());
                }
                let mut text = b"\r\r\r\r____".to_vec();
                text.extend_from_slice(&self.game_date);
                Ok(text)
            }
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
    /// The model with a different F1683 render scale (`+0x7e`). F1536 draws
    /// model 0x9d through the hit ship's scale, not the flash model's own.
    pub fn with_scale(&self, scale: u32) -> Self {
        Self {
            index: self.index,
            vertex_count: self.vertex_count,
            scale,
            vertices: self.vertices.clone(),
            commands: self.commands.clone(),
            normals: self.normals.clone(),
            normal_count: self.normal_count,
            radius: self.radius.clone(),
            front: self.front,
        }
    }

    /// Largest object-space vertex distance at input 0, when evaluable alone.
    pub fn radius(&self) -> Option<f64> {
        self.radius.as_ref().ok().copied()
    }

    /// Largest object-space z of a vertex at input 0, when evaluable alone: for a
    /// station, how far its docking face (the +z end) stands from the centre.
    pub fn front(&self) -> Option<f64> {
        self.front
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
        let points = evaluate(
            vertices,
            vertex_count,
            &VertexContext {
                scale,
                ..Default::default()
            },
        );
        let front = points.as_ref().ok().and_then(|points| {
            points
                .iter()
                .map(|p| f64::from(p[2]))
                .max_by(f64::total_cmp)
        });
        let radius = points.map(|points| {
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
            front,
        }
    }
}

#[cfg(test)]
mod object_text_tests {
    use super::*;

    #[test]
    fn object_text_does_not_substitute_the_catalogue_name() {
        let table = SineTable::from_bytes(&vec![0; 16384 * 4]).unwrap();
        let model = ViewerModel::new(0, 0, 0, &[], &[0, 0], &[], 0);
        let mut viewer = ModelViewer::from_models(vec![model], table).unwrap();
        viewer.set_model_strings(vec![b"LONG SHIP CATALOGUE NAME".to_vec()]);
        for code in [0x3016, 0x3022] {
            assert!(viewer.expanded_text(code, 0).unwrap().is_empty());
        }
        viewer.set_registration(b"AB-123");
        for code in [0x3016, 0x3022] {
            assert_eq!(viewer.expanded_text(code, 0).unwrap(), b"AB-123");
        }
    }
}
