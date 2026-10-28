//! Bounded command control flow and operand emissions, not geometry evaluation or rasterization.
//!
//! `execute` runs without live vertex state and keeps the earlier behaviour: a reached
//! depth-order command fails explicitly. `execute_live` composes the verified F1696
//! depth commands and F1780/F1788 group ordering with the live view-space slots in
//! [`crate::model_instance`], and returns a render-ordered face stream in
//! [`Execution::ordered`]. Both variants are transactional: an error leaves the
//! caller's context and vertex cache unchanged.
use crate::model::VertexContext;
use crate::model::{variable, ExternalSource, ExternalVertices};
use crate::model_commands::{
    arithmetic_with_table, bit_branch, decode, material, visibility_branch, ChildScale, Colour,
    Face, Label, Polygon, Spline, Submodel, Surface, Text, Thruster, VisibilityContext,
};
use crate::model_depth::{self, Command as DepthCommand};
use crate::model_instance::Vertices;
use crate::model_math::{angle_operand, SineTable};
use crate::model_order::Order;
use crate::model_polygon::{self, Outline};
use crate::star_points::StarTables;
use std::cell::Cell;

thread_local! {
    /// The logical view height F1715/F1716 and the on-screen test use. The flight
    /// view is 158 rows; F917's tombstone covers the full 320x200 logical screen
    /// and temporarily raises this to 200.
    static VIEW_HEIGHT: Cell<i32> = const { Cell::new(158) };
}

/// Set the view height used by face queueing for the current thread. Callers
/// must restore 158; [`ViewHeightGuard`] does that on drop.
pub fn set_view_height(height: i32) {
    VIEW_HEIGHT.with(|value| value.set(height));
}

pub fn view_height() -> i32 {
    VIEW_HEIGHT.with(Cell::get)
}

/// Restore the flight view's 158-row clip after a full-screen model pass.
pub struct ViewHeightGuard;

impl ViewHeightGuard {
    pub fn full_screen() -> Self {
        set_view_height(200);
        crate::model_texture::set_raster_height(200);
        Self
    }
}

impl Drop for ViewHeightGuard {
    fn drop(&mut self) {
        set_view_height(158);
        crate::model_texture::set_raster_height(158);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emission {
    pub face: Face,
    pub context: VertexContext,
    /// Screen points F1708/F1712 projected, in `face.vertices` order; empty unless
    /// the face was queued. Child faces carry their own (child-cache) points.
    pub screen: Vec<[i16; 2]>,
    /// The view positions of the same vertices (what F1714 reads for a face
    /// reaching off the view); empty unless the face was queued.
    pub views: Vec<[i32; 3]>,
    /// The face normal's colour in the evaluating instance's normal table (zero
    /// when culled): what F1693 adds to a lit material.
    pub shade: [u8; 3],
}

/// A point-source operand event. This is the original handler's decoded input,
/// not a projected point or a rasterized pixel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointEmission {
    pub offset: usize,
    pub material: u16,
    /// Raw F1474 packed-angle operand.
    pub encoded_angle: i16,
    /// Resolved angle (`angle_operand`), already verified separately.
    pub angle: i32,
    pub flag: bool,
    pub vertex: u8,
    pub context: VertexContext,
    /// F1479 projection of the vertex's view when queued (`None` if near or not
    /// queued); computed from the resolved view, not by the original handler.
    pub screen: Option<[i16; 2]>,
    /// The vertex's view-space position when queued (what F1699 reads from the slot).
    pub view: Option<[i32; 3]>,
    /// The instance's operand scale (`+0x148`) F1699 shifts the radius by.
    pub scale: i32,
}

/// An F1766 vertex-pair chain (opcode 24) as F1814 queues it: one material
/// colour, an F1474 size scaled by the instance and the depth, and the
/// projected screen points (x - 1, y - 1) of every vertex that reaches the
/// view. F1814 draws one F1611 particle at each point.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainEmission {
    pub chain: crate::model_commands::Chain,
    pub context: VertexContext,
    /// The particle size after F1474, the instance scale and F1666's divide.
    pub size: i32,
    /// Screen points in F1766's pair order (high byte then low byte).
    pub points: Vec<[i16; 2]>,
}

/// A line operand event. This is the original handler's decoded input, not a
/// clipped/rasterized line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineEmission {
    pub offset: usize,
    pub material: u16,
    pub vertices: [u8; 2],
    pub context: VertexContext,
    /// The F1803 entry's two points when queued: F1704's clipped ends, x stored
    /// as `x - 1` when positive (`None` when F1704 rejects the line). The second
    /// vertex is resolved on a copy of the cache, so the original's cache state
    /// is unchanged.
    pub screen: Option<[[i16; 2]; 2]>,
    /// Not the original's: both ends in view space when they resolved, for a renderer that
    /// draws the line itself (the port's modern engine).
    pub views: Option<[[i32; 3]; 2]>,
}

/// A text operand event (F1747). `culled` is the original normal-flag skip; the
/// glyph programs, string expansion and their nested primitives are not executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEmission {
    pub text: Text,
    pub culled: bool,
    pub context: VertexContext,
    /// The colour of the text's normal entry. F1747's F1692/F1693/F1691 chain
    /// adds it before the colour becomes the glyph instance colour.
    pub shade: [u8; 3],
    /// The [`Primitive::Text`] place this text took in the depth order, when
    /// the execution queues one.
    pub queued: Option<usize>,
}

/// A child-model call operand event (F1749/F1750). The child's culling, frame and
/// program are not evaluated, so its nested primitives are absent from `ordered`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmodelEmission {
    pub submodel: Submodel,
    pub context: VertexContext,
}

/// A complex polygon (F1721). `key` is the depth key it was queued with, or `None`
/// when its normal is culled (the original then skips it and queues nothing).
/// `outline` holds the entries its segment handlers queued after the F1794 header
/// (see [`crate::model_polygon`]); [`crate::model_polygon::edges`] turns them into
/// the draw routine's edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolygonEmission {
    pub polygon: Polygon,
    pub key: Option<u32>,
    pub context: VertexContext,
    pub outline: Outline,
    /// F1692/F1693 colour, as F1326 receives it for the F1799 fill; zero when culled
    /// (not computed). For a textured material (`material(..).texture != 0`) F1721
    /// also runs F1691, which adds the first colour of the texture's descriptor
    /// (`DATA_007811 + 30 * texture + 8`); the executor has no descriptor table,
    /// so the caller completes that colour from [`Self::shade`]
    /// (`model_texture::flat_colour`).
    pub colour: [u8; 3],
    /// The normal's colour F1693 adds (zero when culled).
    pub shade: [u8; 3],
}

/// A sound request (opcode 15, F1759): sample `code >> 5 & 15` at volume
/// 63 - (|x| + |y| + |z|) / 0x20000 of the instance position, only when that is
/// positive. Playback (F1908) is not ported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundEmission {
    pub offset: usize,
    pub sample: u8,
    pub volume: u8,
}

/// A label (opcode 18, F1763/F1764). The vertex is projected (F1471); `key` is
/// `None` unless its screen point lies within x 2..=317, y 4..=149, when F1764
/// queues a 4-word F1812 text entry. F1764's pointer pick test is not modelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabelEmission {
    pub label: Label,
    pub key: Option<u32>,
    pub screen: [i16; 2],
}

/// A star's label (F1769 through F1764): the vertex is projected (F1471); `key` is `None`
/// unless its screen point lies within x 2..=317, y 4..=149, when F1764 queues a text entry
/// of `selector` (0x8400, a system name) and `code` (the sector code with the star number
/// in bits 26..31). F1764's pointer pick test is not modelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemLabelEmission {
    pub vertex: u8,
    pub selector: u16,
    pub code: u32,
    pub key: Option<u32>,
    pub screen: [i16; 2],
}

/// A curve queued by F1737 (opcode 22). `points` are the screen points of vertex
/// bytes +4, +2, +5, +3 in entry order; `key` is +4's depth key. Both are `None`
/// when the normal is culled or any of the four is nearer than z = 0x40, and the
/// original queues nothing. F1811's drawing is not ported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SplineEmission {
    pub spline: Spline,
    pub key: Option<u32>,
    pub points: Option<[[i16; 2]; 4]>,
    /// Not the original's: the four points in view space, in the same order, when queued.
    pub views: Option<[[i32; 3]; 4]>,
    /// F1692/F1693/F1691 colour; zero when culled (not computed).
    pub colour: [u8; 3],
}

/// A thruster (F1738). [`execute_live`] resolves A (and B when A is deeper than
/// 0x200) and queues what the original queues (see [`crate::model_thruster`]):
/// nothing, an F1704 line, or one or two F1805 glow quads, each also in
/// [`Execution::ordered`] as a [`Primitive::Thruster`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThrusterEmission {
    pub thruster: Thruster,
    pub context: VertexContext,
    /// The entries queued, in queue order (empty for [`execute`]).
    pub entries: Vec<crate::model_thruster::Entry>,
    /// A's and B's screen points when both were resolved (A deeper than 0x200).
    pub screens: Option<[[i16; 2]; 2]>,
    /// The scaled width when F1738 computed it (both vertices deeper than 0x200).
    pub width: Option<i32>,
}

/// A curved-surface operand event (F1874). F1876's patch rendering and its own
/// queueing are not ported, so it is absent from `ordered`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceEmission {
    pub surface: Surface,
    pub context: VertexContext,
    /// F1874's resolved origin (F1470, view space) when live vertices exist.
    pub origin: Option<[i32; 3]>,
    /// F1874's three axis vectors, each `axis.object - origin.object` in record
    /// units, in the record's byte +3..+5 order (`None` without live vertices).
    /// F1876 turns these into the patch; its rendering is not ported.
    pub axes: Option<[[i32; 3]; 3]>,
}

/// An opcode 21 nested depth block boundary (F1697): `Some(key)` starts a block
/// keyed by its vertex's depth (F1781), `None` ends it (F1782).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockEmission {
    pub offset: usize,
    pub key: Option<u32>,
}

/// Where an opcode 26 colour was written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourTarget {
    /// Instance colour (+0xf4..+0xf6), kept in [`VertexContext::colour`].
    Instance,
    /// Normal entry 0's colour, seen by later lighting through normal 0.
    NormalZero,
}

/// An executed F1762 colour command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColourEmission {
    pub colour: Colour,
    /// The selected material word.
    pub material: u16,
    pub target: ColourTarget,
    pub rgb: [u8; 3],
}

/// F1692/F1693 colour resolution: nibble RGB (zero when textured), plus the instance
/// colour for local materials, plus the normal's colour unless unlit or textured.
pub fn resolve_colour(value: u16, instance: [u8; 3], normal: [u8; 3]) -> [u8; 3] {
    let decoded = material(value);
    let mut rgb = decoded.rgb;
    if decoded.local_colour {
        rgb = std::array::from_fn(|i| rgb[i].wrapping_add(instance[i]));
    }
    if !decoded.unlit && decoded.texture == 0 {
        rgb = std::array::from_fn(|i| rgb[i].wrapping_add(normal[i]));
    }
    rgb
}

/// One depth-queue operation in program order: the original F1780 group changes
/// and F1778 primitive allocations. Sorting these is [`crate::model_order::Order`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueEvent {
    /// Close the current group and open one with this key, or none.
    Group(Option<u32>),
    /// Start (`Some`) or end (`None`) a nested block; each also closes the group.
    Nested(Option<u32>),
    Face(u32),
    Polygon(u32),
    Point(u32),
    Line(u32),
    Spline(u32),
    Label(u32),
    /// An F1805 thruster glow quad (F1738).
    Thruster(u32),
    /// An F1803 line queued by F1704 for a narrow thruster.
    ThrusterLine(u32),
    /// An F1814 particle chain (F1766).
    Chain(u32),
}

/// One render-order entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Primitive {
    Face(Emission),
    Polygon(PolygonEmission),
    Point(PointEmission),
    Line(LineEmission),
    Spline(SplineEmission),
    Label(LabelEmission),
    SystemLabel(SystemLabelEmission),
    Thruster(crate::model_thruster::Entry),
    Chain(ChainEmission),
    /// F1747's glyph primitives join the depth list where the text command
    /// runs: inside an active group they stay with its members. The number
    /// names the text ([`TextEmission::queued`]); the caller draws its glyphs
    /// here.
    Text(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Execution {
    /// Visited command offsets in execution order.
    pub commands: Vec<usize>,
    /// Face emissions in command order. Kept stable for diagnostics and earlier checks.
    pub emissions: Vec<Emission>,
    /// Point emissions in command order.
    pub points: Vec<PointEmission>,
    /// Line emissions in command order.
    pub lines: Vec<LineEmission>,
    /// Vertex-pair chain emissions (F1766) in command order.
    pub chains: Vec<ChainEmission>,
    /// Text operand events in command order; never in [`Self::ordered`].
    pub texts: Vec<TextEmission>,
    /// Child-model call operand events in command order; never in [`Self::ordered`].
    pub submodels: Vec<SubmodelEmission>,
    /// Executed colour commands in command order.
    pub colours: Vec<ColourEmission>,
    /// Complex polygons in command order ([`execute_live`] only).
    pub polygons: Vec<PolygonEmission>,
    /// Thrusters in command order; their queued entries are also in [`Self::ordered`].
    pub thrusters: Vec<ThrusterEmission>,
    /// Curved surfaces in command order; never in [`Self::ordered`].
    pub surfaces: Vec<SurfaceEmission>,
    /// Curves in command order ([`execute_live`] only).
    pub splines: Vec<SplineEmission>,
    /// Sound requests in command order; never in [`Self::ordered`].
    pub sounds: Vec<SoundEmission>,
    /// Labels in command order ([`execute_live`] only).
    pub labels: Vec<LabelEmission>,
    /// Star labels of light points (opcode 30 `0x801e`) in star order ([`execute_live`] only).
    pub system_labels: Vec<SystemLabelEmission>,
    /// Nested depth block boundaries in command order ([`execute_live`] only).
    pub blocks: Vec<BlockEmission>,
    /// Child-model calls in command order ([`execute_scene`] only): culled, skipped for
    /// lack of arena room, or executed with the child's own command-order record.
    pub children: Vec<ChildRun>,
    /// Final normal entry 0 colour if a colour command wrote it during this run.
    pub normal_zero_colour: Option<[u8; 3]>,
    /// Depth-queue operations in program order ([`execute_live`] only), excluding the
    /// top-level close after return. Executed children add theirs here, interleaved
    /// where they run; a child's own [`Execution::queue`] stays empty.
    pub queue: Vec<QueueEvent>,
    /// All emitted primitives in original render order. Populated only by
    /// [`execute_live`]; empty for [`execute`], which has no live vertex keys.
    pub ordered: Vec<Primitive>,
    /// Effective group/standalone keys aligned with [`Self::ordered`].
    pub ordered_keys: Vec<u32>,
}

/// The descriptor of a model a child call names (F1538 table entry): its three blocks
/// and the dwords F1750 reads at +0x14, +0x18 and +0x1c.
#[derive(Debug, Clone, Copy)]
pub struct ChildModel<'a> {
    pub commands: &'a [u8],
    /// Six-byte vertex pair records (+0x4) and the expanded count (+0x8).
    pub vertices: &'a [u8],
    pub vertex_count: u32,
    /// Normal pair records (+0xc) and the count (+0x10), including the implicit pair.
    pub normals: &'a [u8],
    pub normal_count: u32,
    /// +0x14, added to the child scale.
    pub scale: i32,
    /// +0x18, also added to the child scale.
    pub scale_offset: i32,
    /// +0x1c, the F1687 cull radius before the scale shift.
    pub radius: i32,
}

/// The model table as child calls see it. Unknown or undecodable slots are errors.
pub trait ChildModels {
    fn model(&self, index: u16) -> Result<ChildModel<'_>, String>;
}

/// Global state a child call reads besides its parent instance.
pub struct ChildEnvironment<'a> {
    pub models: &'a dyn ChildModels,
    /// `DATA_008831`: eight RGB shades every instance copies to +0x84 (child locals
    /// 12..17 and the F1480 colours of normals 0/1).
    pub shades: [[u8; 3]; 8],
    /// The top-level instance's light vector at +0x134 (`DATA_008821..8823` in F1683);
    /// F1740 copies it to each child, and F1685 turns it into the child's +0x38.
    pub light: [i32; 3],
    /// Bytes of the 0x8000-byte F1745 arena in use when the top-level program starts
    /// (the game's F1774 has pushed the top-level frame; the adapter runs F1772
    /// directly, with 0).
    pub arena: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChildOutcome {
    /// F1687 culled the call: nothing was resolved or run.
    Culled,
    /// F1745 found no arena room; F1774 returned without running the child.
    NoRoom,
    /// The child's own command-order record (its primitives are in the root
    /// [`Execution::ordered`]; its queue events in the root [`Execution::queue`]).
    Executed(Box<Execution>),
    /// The child (or a descendant) could not run. The original would draw
    /// whatever its stale stack held; the port leaves this child out but keeps
    /// its already-executed siblings, so one bad subtree does not discard the
    /// whole scene.
    Failed(String),
}

/// The instance F1750/F1740/F1685/F1774 built for a child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildSetup {
    pub instance: InstanceContext,
    /// Initial context. `basis`/`handedness` are placeholders (the original leaves them
    /// undefined), as are locals in `undefined_locals`.
    pub context: VertexContext,
    /// F1685/F1774 observer scale (+0x44) and position (+0x48) when the frame word
    /// lacks 0x4000; `None` when the original leaves them undefined.
    pub observer: Option<(u32, [i32; 3])>,
    pub externals: ExternalVertices,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChildRun {
    /// The calling command's offset in the parent program.
    pub offset: usize,
    pub model: u16,
    /// The F1687 radius: descriptor +0x1c shifted left by the child scale.
    pub radius: i32,
    pub outcome: ChildOutcome,
    pub setup: Option<ChildSetup>,
}

/// Instance fields needed to resolve live vertices and depth keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InstanceContext {
    /// Instance position at +0x4.
    pub position: [i32; 3],
    /// Instance generation at +0xf0. Zero is invalid.
    pub generation: u32,
    /// Depth key scale at +0x130.
    pub key_scale: i32,
    /// Depth operand scale at +0x148.
    pub operand_scale: i32,
    /// Texture rendering enabled (the original's global at `DATA_008812` is >= 0,
    /// its default). Textured faces then skip the off-screen rejection.
    pub textures: bool,
}

impl Default for InstanceContext {
    fn default() -> Self {
        Self {
            position: [0; 3],
            generation: 1,
            key_scale: 0,
            operand_scale: 0,
            textures: true,
        }
    }
}

struct InnerInput<'a, 'v> {
    bytes: &'a [u8],
    vertex_count: u32,
    visibility: &'v VisibilityContext<'v>,
    table: Option<&'a SineTable>,
    instance: InstanceContext,
    budget: usize,
}

fn selector(value: u8, context: &VertexContext) -> Result<(), String> {
    if value >= 0xd2 {
        return Err(
            "Execution cannot read local slots 18..63, which alias legacy instance fields."
                .to_owned(),
        );
    }
    crate::model::checked_variable(value, context).map(|_| ())
}

/// Execute without a live vertex cache. A reached depth-order command fails explicitly;
/// use [`execute_live`] for the composed F1696/F1780 path.
pub fn execute(
    bytes: &[u8],
    vertex_count: u32,
    context: &mut VertexContext,
    visibility: &VisibilityContext<'_>,
    table: Option<&SineTable>,
    budget: usize,
) -> Result<Execution, String> {
    execute_inner(
        InnerInput {
            bytes,
            vertex_count,
            visibility,
            table,
            instance: InstanceContext::default(),
            budget,
        },
        None,
        context,
        None,
    )
}

/// Execute with a caller-owned live vertex cache. The cache is committed only after a
/// real return, so view-space slots can be reused across invocations by generation.
/// Child-model calls are recorded as operand events; see [`execute_scene`] to run them.
pub fn execute_live(
    bytes: &[u8],
    vertices: &mut Vertices<'_>,
    context: &mut VertexContext,
    visibility: &VisibilityContext<'_>,
    table: Option<&SineTable>,
    instance: InstanceContext,
    budget: usize,
) -> Result<Execution, String> {
    let vertex_count = u32::try_from(vertices.vertex_count())
        .map_err(|_| "Live vertex count exceeds u32.".to_owned())?;
    execute_inner(
        InnerInput {
            bytes,
            vertex_count,
            visibility,
            table,
            instance,
            budget,
        },
        Some(vertices),
        context,
        None,
    )
}

/// [`execute_live`] that also runs every child model a call does not cull (F1750),
/// recursively, as the original does: each child has its own instance, vertex cache,
/// external-vertex table and normals, shares the depth queue, the command budget and
/// the F1476 jitter state, and its primitives join [`Execution::ordered`] where the
/// original queues them. Child runs are in [`Execution::children`]. Transactional.
#[allow(clippy::too_many_arguments)]
pub fn execute_scene(
    bytes: &[u8],
    vertices: &mut Vertices<'_>,
    context: &mut VertexContext,
    visibility: &VisibilityContext<'_>,
    table: Option<&SineTable>,
    instance: InstanceContext,
    budget: usize,
    children: &ChildEnvironment<'_>,
) -> Result<Execution, String> {
    let vertex_count = u32::try_from(vertices.vertex_count())
        .map_err(|_| "Live vertex count exceeds u32.".to_owned())?;
    execute_inner(
        InnerInput {
            bytes,
            vertex_count,
            visibility,
            table,
            instance,
            budget,
        },
        Some(vertices),
        context,
        Some(children),
    )
}

fn view_point(
    vertices: &mut Vertices<'_>,
    index: i8,
    context: &VertexContext,
    instance: &InstanceContext,
) -> Result<[i32; 3], String> {
    Ok(vertices
        .resolve(
            index,
            context,
            instance.position,
            instance.generation,
            false,
        )?
        .view)
}

/// A queued face's depth key, its projected screen points and their views.
type QueuedFace = (u32, Vec<[i16; 2]>, Vec<[i32; 3]>);

/// F1708/F1712 face queueing. A culled normal queues nothing. Otherwise every vertex is
/// projected (F1471) in the original order: b, c, (d,) then a. A textured face whose
/// screen points are all within the guard range and whose screen perimeter is at
/// least 6 is queued; any other face is queued unless every point is off screen and
/// the F1713 outcodes put them all left, all right (or above/below) or all behind
/// the near plane. The key is always
/// F1777 of vertex a's view.
fn face_queue_key(
    face: &Face,
    live: &mut Vertices<'_>,
    context: &VertexContext,
    instance: &InstanceContext,
    visibility: &VisibilityContext<'_>,
) -> Result<Option<QueuedFace>, String> {
    let culled = visibility
        .normals
        .get(usize::from(face.normal))
        .ok_or_else(|| {
            format!(
                "Face at {:#x} references an out-of-range normal.",
                face.offset
            )
        })?
        .culled;
    if culled {
        return Ok(None);
    }
    let count = face.vertices.len();
    let mut points = vec![crate::model_instance::Vertex::default(); count];
    for slot in (1..count).chain(std::iter::once(0)) {
        points[slot] = live.resolve(
            face.vertices[slot] as i8,
            context,
            instance.position,
            instance.generation,
            true,
        )?;
    }
    let screens: Vec<[i32; 2]> = points
        .iter()
        .map(|p| [i32::from(p.screen[0]), i32::from(p.screen[1])])
        .collect();
    let decoded = material(face.material);
    let guarded = screens
        .iter()
        .all(|[x, y]| (-2000..=2320).contains(x) && (-2000..=2158).contains(y));
    let perimeter: i32 = (0..count)
        .map(|i| {
            let [ax, ay] = screens[i];
            let [bx, by] = screens[(i + 1) % count];
            (ax - bx).abs() + (ay - by).abs()
        })
        .sum();
    let key = model_depth::key(points[0].view, instance.key_scale);
    let projected = points.iter().map(|p| p.screen).collect();
    let views = points.iter().map(|p| p.view).collect();
    if instance.textures && decoded.texture != 0 && guarded && perimeter >= 6 {
        return Ok(Some((key, projected, views)));
    }
    let on_screen = |[x, y]: [i32; 2]| (x as u32) < 320 && (y as u32) < view_height() as u32;
    if screens.iter().any(|s| on_screen(*s)) {
        return Ok(Some((key, projected, views)));
    }
    // F1715/F1716 outcodes over a, b, c and (for triangles, c again) d.
    let code = |value: i32, limit: i32| match value {
        -32766 => 8,
        v if (0..limit).contains(&v) => 2,
        v if v < 0 => 1,
        _ => 4,
    };
    let rejected = |axis: usize, limit: i32| {
        let combined = screens.iter().fold(0, |acc, s| acc | code(s[axis], limit));
        // DATA_007818 rejects all-left (1), all-right (4) and all-behind (8).
        matches!(combined, 1 | 4 | 8)
    };
    Ok((!rejected(0, 320) && !rejected(1, view_height())).then_some((key, projected, views)))
}

/// The point handler F1698 (and F1769's calls to F1699): the operand event, and with live
/// vertices the projected, depth-keyed queue entry.
#[allow(clippy::too_many_arguments)]
fn emit_point(
    offset: usize,
    material: u16,
    angle: i16,
    flag: bool,
    vertex: u8,
    result: &mut Execution,
    shared: &mut Shared<'_>,
    vertices: Option<&mut Vertices<'_>>,
    working: &mut VertexContext,
    instance: &InstanceContext,
) -> Result<(), String> {
    if angle as u16 & 0xc0 == 0xc0 {
        crate::model::checked_variable(angle as u8, working)?;
    }
    let emission = PointEmission {
        offset,
        material,
        encoded_angle: angle,
        angle: angle_operand(angle, working),
        flag,
        vertex,
        context: working.clone(),
        screen: None,
        view: None,
        scale: instance.operand_scale,
    };
    result.points.push(emission.clone());
    if let (Some(queue), Some(live)) = (shared.order.as_mut(), vertices) {
        let resolved = live.resolve(
            vertex as i8,
            working,
            instance.position,
            instance.generation,
            false,
        )?;
        let key = model_depth::key(resolved.view, instance.key_scale);
        let mut emission = emission;
        emission.screen = crate::model_instance::project(resolved.view);
        emission.view = Some(resolved.view);
        queue.push(key, Primitive::Point(emission))?;
        shared.queue.push(QueueEvent::Point(key));
    }
    Ok(())
}

/// The line handler F1702 (and F1769's calls to F1703).
#[allow(clippy::too_many_arguments)]
fn emit_line(
    offset: usize,
    material: u16,
    ends: [u8; 2],
    result: &mut Execution,
    shared: &mut Shared<'_>,
    vertices: Option<&mut Vertices<'_>>,
    working: &mut VertexContext,
    instance: &InstanceContext,
) -> Result<(), String> {
    let emission = LineEmission {
        offset,
        material,
        vertices: ends,
        context: working.clone(),
        screen: None,
        views: None,
    };
    result.lines.push(emission.clone());
    if let (Some(queue), Some(live)) = (shared.order.as_mut(), vertices) {
        let vertex = live.resolve(
            ends[0] as i8,
            working,
            instance.position,
            instance.generation,
            true,
        )?;
        let key = model_depth::key(vertex.view, instance.key_scale);
        // For drawing only: a failure here leaves the line undrawn
        // rather than failing an execution the original completes.
        let other = live
            .clone()
            .resolve(
                ends[1] as i8,
                working,
                instance.position,
                instance.generation,
                true,
            )
            .ok();
        // F1703 queues through F1704: both ends clipped onto the
        // view (F1705) and stored with x - 1 when positive, or
        // nothing when F1704 rejects the line.
        let mut emission = emission;
        emission.views = other.as_ref().map(|other| [vertex.view, other.view]);
        emission.screen = other
            .and_then(|other| crate::model_thruster::line(&vertex, &other).ok())
            .flatten()
            .map(|points| points.map(|[x, y]| [x as i16, y as i16]));
        queue.push(key, Primitive::Line(emission))?;
        shared.queue.push(QueueEvent::Line(key));
    }
    Ok(())
}

/// F1769, opcode 30 `0x801e`: the object's light points. For each of the `count` stars
/// (global word 4) of type `t`, a stalk line from vertex `2i` to `2i + 1` when global word
/// 1 has bit 5, the star's point at vertex `2i` and, when the depth of vertex slot 0 is at
/// most the type's label limit, the star's label (F1764) naming the sector code with the
/// star's number in bits 26..31.
#[allow(clippy::too_many_arguments)]
fn light_points(
    offset: usize,
    result: &mut Execution,
    shared: &mut Shared<'_>,
    vertices: Option<&mut Vertices<'_>>,
    working: &mut VertexContext,
    instance: &InstanceContext,
) -> Result<(), String> {
    let count = usize::from(working.globals[4]);
    if count == 0 {
        return Ok(());
    }
    let Some(live) = vertices else {
        return Err(format!("Light points at {offset:#x} need live vertices."));
    };
    let Some((tables, field)) = live.stars() else {
        return Err(format!(
            "Light points at {offset:#x} need the original point tables."
        ));
    };
    if field.types.len() < count {
        return Err(format!(
            "Light points at {offset:#x}: {count} stars but {} types.",
            field.types.len()
        ));
    }
    let mut live = Some(live);
    for star in 0..count {
        let kind = StarTables::index(field.types[star]).ok_or_else(|| {
            format!(
                "Star type {} at {offset:#x} is beyond the tables.",
                field.types[star] & 15
            )
        })?;
        let vertex = (2 * star) as u8;
        if working.globals[1] & 0x20 != 0 {
            emit_line(
                offset,
                tables.stalk[kind] as u16,
                [vertex, vertex + 1],
                result,
                shared,
                live.as_deref_mut(),
                working,
                instance,
            )?;
        }
        emit_point(
            offset,
            tables.material[kind] as u16,
            (tables.size[kind] as i32 >> 7) as i16,
            false,
            vertex,
            result,
            shared,
            live.as_deref_mut(),
            working,
            instance,
        )?;
        let Some(vertices) = live.as_deref_mut() else {
            continue;
        };
        // F1769 compares the depth of vertex slot 0, not the star's own.
        let near = vertices.resolve(0, working, instance.position, instance.generation, false)?;
        if i64::from(near.view[2]) <= i64::from(tables.label_depth[kind] as i32) {
            let resolved = vertices.resolve(
                vertex as i8,
                working,
                instance.position,
                instance.generation,
                true,
            )?;
            let [x, y] = resolved.screen.map(i32::from);
            let key = ((2..=317).contains(&x) && (4..=149).contains(&y))
                .then(|| model_depth::key(resolved.view, instance.key_scale));
            let top = ((star as u32 & 0x3f) << 26) | (field.code & 0x03ff_ffff);
            let emission = SystemLabelEmission {
                vertex,
                selector: 0x8400,
                code: top,
                key,
                screen: resolved.screen,
            };
            if let (Some(key), Some(queue)) = (key, shared.order.as_mut()) {
                queue.push(key, Primitive::SystemLabel(emission.clone()))?;
                shared.queue.push(QueueEvent::Label(key));
            }
            result.system_labels.push(emission);
        }
    }
    Ok(())
}

fn resolve_depth_key(
    depth: &DepthCommand,
    vertices: &mut Vertices<'_>,
    context: &VertexContext,
    instance: &InstanceContext,
) -> Result<Option<u32>, String> {
    Ok(match depth {
        DepthCommand::Close => None,
        DepthCommand::First => Some(u32::MAX),
        DepthCommand::Position => Some(model_depth::key(instance.position, instance.key_scale)),
        DepthCommand::Vertex(index) => Some(model_depth::key(
            view_point(vertices, *index, context, instance)?,
            instance.key_scale,
        )),
        DepthCommand::Maximum(indices) | DepthCommand::Minimum(indices) => {
            let mut items = indices.iter();
            let first = *items
                .next()
                .ok_or_else(|| "Empty depth-order vertex list.".to_owned())?;
            let mut key = model_depth::key(
                view_point(vertices, first, context, instance)?,
                instance.key_scale,
            ) as i32;
            for index in items {
                let candidate = model_depth::key(
                    view_point(vertices, *index, context, instance)?,
                    instance.key_scale,
                ) as i32;
                key = if matches!(depth, DepthCommand::Maximum(_)) {
                    key.max(candidate)
                } else {
                    key.min(candidate)
                };
            }
            Some(key as u32)
        }
        DepthCommand::Offset { vertex, amount } => {
            let mut point = match vertex {
                Some(index) => view_point(vertices, *index, context, instance)?,
                None => [0; 3],
            };
            point[2] = point[2]
                .wrapping_add(i32::from(*amount).wrapping_shl(instance.operand_scale as u32));
            Some(model_depth::key(point, instance.key_scale))
        }
    })
}

/// Which instance fields hold defined values. Top-level instances define all of them
/// (the caller supplies them); a child instance (F1750) starts without a basis or its
/// handedness, and without observer/direction when its frame word has 0x4000.
#[derive(Debug, Clone, Copy)]
struct Defined {
    basis: bool,
    handedness: bool,
    base_handedness: bool,
    direction: bool,
}

const ALL_DEFINED: Defined = Defined {
    basis: true,
    handedness: true,
    base_handedness: true,
    direction: true,
};

/// Development bound on child nesting; the original's own bound is the 0x8000-byte
/// F1745 arena, which [`ChildEnvironment::arena`] models exactly.
pub const MAX_CHILD_DEPTH: usize = 32;
const ARENA_SIZE: u32 = 0x8000;

struct Frame<'a, 'v> {
    bytes: &'a [u8],
    vertex_count: u32,
    visibility: &'v VisibilityContext<'v>,
    table: Option<&'a SineTable>,
    instance: InstanceContext,
    defined: Defined,
    /// Instance light vector at +0x134 (copied from parent to child by F1740).
    light: [i32; 3],
    child: bool,
}

struct Shared<'e> {
    order: Option<Order<Primitive>>,
    queue: Vec<QueueEvent>,
    remaining: usize,
    budget: usize,
    children: Option<&'e ChildEnvironment<'e>>,
    arena: u32,
    depth: usize,
    /// Texts queued so far across this execution and its children.
    texts: usize,
}

fn execute_inner(
    input: InnerInput<'_, '_>,
    vertices: Option<&mut Vertices<'_>>,
    context: &mut VertexContext,
    children: Option<&ChildEnvironment<'_>>,
) -> Result<Execution, String> {
    let InnerInput {
        bytes,
        vertex_count,
        visibility,
        table,
        instance,
        budget,
    } = input;
    if !(1..=4096).contains(&budget) {
        return Err("Model execution budget must be 1..=4096 commands.".to_owned());
    }
    let mut working = context.clone();
    let mut working_vertices = match vertices.as_deref() {
        Some(value) => {
            if value.vertex_count() != vertex_count as usize {
                return Err("Live vertex cache does not match the model vertex count.".to_owned());
            }
            Some(value.clone())
        }
        None => None,
    };
    // The command budget bounds execution; the order queue keeps its own 1..=4096 guard.
    let order = if working_vertices.is_some() {
        Some(Order::new(4096)?)
    } else {
        None
    };
    if let Some(environment) = children {
        if environment.arena > ARENA_SIZE {
            return Err("Child arena use exceeds its 0x8000 bytes.".to_owned());
        }
    }
    let mut shared = Shared {
        order,
        queue: Vec::new(),
        remaining: budget,
        budget,
        children,
        arena: children.map_or(0, |environment| environment.arena),
        depth: 0,
        texts: 0,
    };
    let frame = Frame {
        bytes,
        vertex_count,
        visibility,
        table,
        instance,
        defined: ALL_DEFINED,
        light: children.map_or([0; 3], |environment| environment.light),
        child: false,
    };
    let mut result = run(&frame, working_vertices.as_mut(), &mut working, &mut shared)?;
    // Finalize before committing. Like the top-level object-draw callers (F1683/F1684
    // call F1780(0,0) after the model returns), close a group the program left open;
    // child runs (F1774) do not close it, so their groups stay open in the parent.
    let (ordered, ordered_keys) = match shared.order.take() {
        Some(mut order) => {
            order.group(None)?;
            let pairs = order.finish_with_keys()?;
            let mut ordered = Vec::with_capacity(pairs.len());
            let mut ordered_keys = Vec::with_capacity(pairs.len());
            for (key, primitive) in pairs {
                ordered.push(primitive);
                ordered_keys.push(key);
            }
            (ordered, ordered_keys)
        }
        None => (Vec::new(), Vec::new()),
    };
    *context = working;
    if let (Some(source), Some(destination)) = (working_vertices, vertices) {
        *destination = source;
    }
    result.queue = shared.queue;
    result.ordered = ordered;
    result.ordered_keys = ordered_keys;
    Ok(result)
}

/// One F1772 run of a program on `working`/`vertices` (already working copies).
fn run(
    frame: &Frame<'_, '_>,
    mut vertices: Option<&mut Vertices<'_>>,
    working: &mut VertexContext,
    shared: &mut Shared<'_>,
) -> Result<Execution, String> {
    let bytes = frame.bytes;
    let visibility = frame.visibility;
    let table = frame.table;
    let instance = frame.instance;
    let program = decode(bytes, frame.vertex_count)?;
    let mut defined = frame.defined;
    let mut result = Execution {
        commands: Vec::new(),
        emissions: Vec::new(),
        points: Vec::new(),
        lines: Vec::new(),
        chains: Vec::new(),
        texts: Vec::new(),
        submodels: Vec::new(),
        colours: Vec::new(),
        polygons: Vec::new(),
        thrusters: Vec::new(),
        surfaces: Vec::new(),
        splines: Vec::new(),
        labels: Vec::new(),
        system_labels: Vec::new(),
        sounds: Vec::new(),
        blocks: Vec::new(),
        children: Vec::new(),
        normal_zero_colour: None,
        queue: Vec::new(),
        ordered: Vec::new(),
        ordered_keys: Vec::new(),
    };
    let mut offset = 0usize;
    let mut returned = false;
    while shared.remaining > 0 {
        shared.remaining -= 1;
        let index = program
            .commands
            .binary_search_by_key(&offset, |c| c.offset)
            .map_err(|_| format!("Execution left a command boundary at {offset:#x}."))?;
        let command = &program.commands[index];
        result.commands.push(offset);
        let code = command.word;
        let payload = &bytes[offset + 2..offset + command.size];
        let relative = match code & 31 {
            0 => {
                returned = true;
                None
            }
            1 => {
                for point in program.points.iter().filter(|p| p.offset == offset) {
                    emit_point(
                        point.offset,
                        point.material,
                        point.angle,
                        point.flag,
                        point.vertex,
                        &mut result,
                        shared,
                        vertices.as_deref_mut(),
                        working,
                        &instance,
                    )?;
                }
                Some(command.size as i32)
            }
            2 => {
                for line in program.lines.iter().filter(|l| l.offset == offset) {
                    emit_line(
                        line.offset,
                        line.material,
                        line.vertices,
                        &mut result,
                        shared,
                        vertices.as_deref_mut(),
                        working,
                        &instance,
                    )?;
                }
                Some(command.size as i32)
            }
            3 | 4 | 7 | 8 => {
                for face in program.faces.iter().filter(|f| f.offset == offset) {
                    let shade = visibility
                        .normals
                        .get(usize::from(face.normal))
                        .filter(|normal| !normal.culled)
                        .map_or([0; 3], |normal| normal.colour);
                    let mut emission = Emission {
                        face: face.clone(),
                        context: working.clone(),
                        screen: Vec::new(),
                        views: Vec::new(),
                        shade,
                    };
                    result.emissions.push(emission.clone());
                    if let (Some(queue), Some(live)) =
                        (shared.order.as_mut(), vertices.as_deref_mut())
                    {
                        if let Some((key, screen, views)) =
                            face_queue_key(face, live, working, &instance, visibility)?
                        {
                            emission.screen = screen;
                            emission.views = views;
                            queue.push(key, Primitive::Face(emission))?;
                            shared.queue.push(QueueEvent::Face(key));
                        }
                    }
                }
                Some(command.size as i32)
            }
            5 => {
                let Some(live) = vertices.as_deref_mut() else {
                    return Err(format!(
                        "Unsupported executed command {code:#06x} at {offset:#x}."
                    ));
                };
                for polygon in program.polygons.iter().filter(|p| p.offset == offset) {
                    let normal = visibility
                        .normals
                        .get(usize::from(polygon.normal))
                        .ok_or_else(|| {
                            format!("Polygon at {offset:#x} references an out-of-range normal.")
                        })?;
                    let mut emission = PolygonEmission {
                        polygon: polygon.clone(),
                        key: None,
                        context: working.clone(),
                        outline: Outline::default(),
                        colour: [0; 3],
                        shade: [0; 3],
                    };
                    if !normal.culled {
                        let lighting = match (polygon.normal, result.normal_zero_colour) {
                            (0, Some(rgb)) => rgb,
                            _ => normal.colour,
                        };
                        emission.shade = lighting;
                        emission.colour =
                            resolve_colour(polygon.material, working.colour, lighting);
                        // F1721 keys the outline by its first vertex's view (F1470), then
                        // runs the segment handlers, which project through F1471.
                        let vertex = live.resolve(
                            polygon.vertex as i8,
                            working,
                            instance.position,
                            instance.generation,
                            false,
                        )?;
                        emission.key = Some(model_depth::key(vertex.view, instance.key_scale));
                        let inputs = model_polygon::Inputs {
                            matrix: working.matrix,
                            scale: instance.operand_scale,
                            normal_records: live.normal_records(),
                        };
                        emission.outline = model_polygon::run(
                            &polygon.segments,
                            &polygon.segment_bytes,
                            polygon.flag,
                            &inputs,
                            &mut |index| {
                                live.resolve(
                                    index as i8,
                                    working,
                                    instance.position,
                                    instance.generation,
                                    true,
                                )
                            },
                        )
                        .map_err(|error| format!("{error} (polygon at {offset:#x})"))?;
                    }
                    let key = emission.key;
                    result.polygons.push(emission.clone());
                    if let (Some(key), Some(queue)) = (key, shared.order.as_mut()) {
                        queue.push(key, Primitive::Polygon(emission))?;
                        shared.queue.push(QueueEvent::Polygon(key));
                    }
                }
                Some(command.size as i32)
            }
            15 => {
                let distance = instance
                    .position
                    .iter()
                    .fold(0u32, |sum, c| sum.wrapping_add(c.wrapping_abs() as u32))
                    / 0x20000;
                if distance < 63 {
                    result.sounds.push(SoundEmission {
                        offset,
                        sample: (code >> 5 & 15) as u8,
                        volume: (63 - distance) as u8,
                    });
                }
                Some(command.size as i32)
            }
            18 => {
                let Some(live) = vertices.as_deref_mut() else {
                    return Err(format!(
                        "Unsupported executed command {code:#06x} at {offset:#x}."
                    ));
                };
                for label in program.labels.iter().filter(|e| e.offset == offset) {
                    let vertex = live.resolve(
                        label.vertex as i8,
                        working,
                        instance.position,
                        instance.generation,
                        true,
                    )?;
                    let [x, y] = vertex.screen.map(i32::from);
                    let key = ((2..=317).contains(&x) && (4..=149).contains(&y))
                        .then(|| model_depth::key(vertex.view, instance.key_scale));
                    let emission = LabelEmission {
                        label: label.clone(),
                        key,
                        screen: vertex.screen,
                    };
                    if let (Some(key), Some(queue)) = (key, shared.order.as_mut()) {
                        queue.push(key, Primitive::Label(emission.clone()))?;
                        shared.queue.push(QueueEvent::Label(key));
                    }
                    result.labels.push(emission);
                }
                Some(command.size as i32)
            }
            22 => {
                let Some(live) = vertices.as_deref_mut() else {
                    return Err(format!(
                        "Unsupported executed command {code:#06x} at {offset:#x}."
                    ));
                };
                for spline in program.splines.iter().filter(|e| e.offset == offset) {
                    let normal = visibility
                        .normals
                        .get(usize::from(spline.normal))
                        .ok_or_else(|| {
                            format!("Spline at {offset:#x} references an out-of-range normal.")
                        })?;
                    let mut emission = SplineEmission {
                        spline: spline.clone(),
                        key: None,
                        points: None,
                        views: None,
                        colour: [0; 3],
                    };
                    if !normal.culled {
                        let lighting = match (spline.normal, result.normal_zero_colour) {
                            (0, Some(rgb)) => rgb,
                            _ => normal.colour,
                        };
                        emission.colour = resolve_colour(spline.material, working.colour, lighting);
                        // F1471 on +5, +4, +3, +2 (cache-checked), then the near test.
                        let mut resolved = [None; 4];
                        for slot in (0..4).rev() {
                            resolved[slot] = Some(live.resolve(
                                spline.vertices[slot] as i8,
                                working,
                                instance.position,
                                instance.generation,
                                true,
                            )?);
                        }
                        let [b2, b3, b4, b5] = resolved.map(Option::unwrap);
                        if [b2, b3, b4, b5].iter().all(|v| v.view[2] >= 0x40) {
                            let key = model_depth::key(b4.view, instance.key_scale);
                            emission.key = Some(key);
                            emission.points = Some([b4.screen, b2.screen, b5.screen, b3.screen]);
                            emission.views = Some([b4.view, b2.view, b5.view, b3.view]);
                            if let Some(queue) = shared.order.as_mut() {
                                queue.push(key, Primitive::Spline(emission.clone()))?;
                                shared.queue.push(QueueEvent::Spline(key));
                            }
                        }
                    }
                    result.splines.push(emission);
                }
                Some(command.size as i32)
            }
            9 => {
                for thruster in program.thrusters.iter().filter(|t| t.offset == offset) {
                    let mut entries = Vec::new();
                    let mut screens = None;
                    let mut width = None;
                    // F1738 projects A, and B only when A is more than 0x200 deep.
                    if let Some(live) = vertices.as_deref_mut() {
                        let [a, b] = thruster.vertices;
                        let near = live.resolve(
                            a as i8,
                            working,
                            instance.position,
                            instance.generation,
                            true,
                        )?;
                        let far = if near.view[2] > crate::model_thruster::MIN_DEPTH {
                            Some(live.resolve(
                                b as i8,
                                working,
                                instance.position,
                                instance.generation,
                                true,
                            )?)
                        } else {
                            None
                        };
                        screens = far.map(|far| [near.screen, far.screen]);
                        let queued = crate::model_thruster::entries(
                            thruster,
                            &near,
                            far.as_ref(),
                            crate::model_thruster::Instance {
                                scale: instance.operand_scale,
                                key_scale: instance.key_scale,
                                colour: working.colour,
                            },
                        )
                        .map_err(|error| format!("{error} (thruster at {offset:#x})"))?;
                        width = queued.width;
                        entries = queued.entries;
                        if let Some(queue) = shared.order.as_mut() {
                            for entry in &entries {
                                queue.push(entry.key, Primitive::Thruster(entry.clone()))?;
                                shared.queue.push(match entry.shape {
                                    crate::model_thruster::Shape::Line { .. } => {
                                        QueueEvent::ThrusterLine(entry.key)
                                    }
                                    crate::model_thruster::Shape::Glow { .. } => {
                                        QueueEvent::Thruster(entry.key)
                                    }
                                });
                            }
                        }
                    }
                    result.thrusters.push(ThrusterEmission {
                        thruster: thruster.clone(),
                        context: working.clone(),
                        entries,
                        screens,
                        width,
                    });
                }
                Some(command.size as i32)
            }
            21 => {
                let (Some(live), Some(queue)) = (vertices.as_deref_mut(), shared.order.as_mut())
                else {
                    return Err(format!(
                        "Unsupported executed command {code:#06x} at {offset:#x}."
                    ));
                };
                let key = if code & 0x8000 != 0 {
                    None
                } else {
                    let vertex = live.resolve(
                        (code >> 5) as u8 as i8,
                        working,
                        instance.position,
                        instance.generation,
                        false,
                    )?;
                    Some(model_depth::key(vertex.view, instance.key_scale))
                };
                queue.nested(key)?;
                shared.queue.push(QueueEvent::Nested(key));
                result.blocks.push(BlockEmission { offset, key });
                Some(command.size as i32)
            }
            // Beams (17): operand records only; their drawing is not ported and
            // they are absent from `ordered`.
            17 => Some(command.size as i32),
            24 => {
                for chain in program.chains.iter().filter(|c| c.offset == offset) {
                    let mut emission = ChainEmission {
                        chain: chain.clone(),
                        context: working.clone(),
                        size: 0,
                        points: Vec::new(),
                    };
                    if let Some(live) = vertices.as_deref_mut() {
                        // F1474 then the instance-scale adjustment: a negative
                        // (0x8000) value is an absolute 0..0x7fff size; any
                        // other is shifted by +0x148 - 7.
                        let mut size = angle_operand(chain.size as i16, working);
                        if size & 0x8000 != 0 {
                            size &= 0x7fff;
                        } else {
                            let shift = instance.operand_scale.wrapping_sub(7);
                            size = if shift < 0 {
                                ((size as u32) >> (shift.wrapping_neg() as u32 & 31)) as i32
                            } else {
                                size.wrapping_shl(shift as u32 & 31)
                            };
                        }
                        // F1766 skips the whole chain when the instance is at or
                        // behind the eye; otherwise F1666 divides by its depth.
                        if instance.position[2] > 0 {
                            let key = model_depth::key(instance.position, instance.key_scale);
                            let size =
                                crate::model_thruster::special_divide(size, instance.position[2])?;
                            // F1766 runs each pair's high byte and then its low
                            // byte: it stops before a high byte of 0x7f, and after
                            // the high byte when the low byte is 0x7f, so `7f nn`
                            // still draws vertex nn (the hyperspace cloud's one
                            // dot at its centre).
                            'pairs: for pair in &chain.pairs {
                                for index in [pair[1], pair[0]] {
                                    if index == 0x7f {
                                        break 'pairs;
                                    }
                                    let point = live.resolve(
                                        index as i8,
                                        working,
                                        instance.position,
                                        instance.generation,
                                        true,
                                    )?;
                                    if point.view[2] < 0x40 {
                                        continue;
                                    }
                                    let x = i32::from(point.screen[0]) - 1;
                                    let y = i32::from(point.screen[1]) - 1;
                                    if (x as u16) < 0x140 && (y as u16) < 0x9e {
                                        emission.points.push([x as i16, y as i16]);
                                    }
                                }
                            }
                            emission.size = size;
                            if let Some(queue) = shared.order.as_mut() {
                                queue.push(key, Primitive::Chain(emission.clone()))?;
                                shared.queue.push(QueueEvent::Chain(key));
                            }
                        }
                    }
                    result.chains.push(emission);
                }
                Some(command.size as i32)
            }
            25 => {
                if code & 0x8000 != 0 {
                    // F1770 resets the current basis to identity.
                    working.basis = crate::model::IDENTITY;
                    defined.basis = true;
                } else {
                    if !defined.direction {
                        return Err(format!(
                            "Direction alignment at {offset:#x} reads +0x38, undefined in this child instance."
                        ));
                    }
                    let d = working.direction;
                    let (a, b) = match code >> 5 & 3 {
                        0 => (d[1], d[2]),
                        1 => (d[2], d[0]),
                        2 => (d[0], d[1]),
                        _ => {
                            return Err(format!(
                                "Direction alignment at {offset:#x} uses undefined axis 3."
                            ))
                        }
                    };
                    // F1771: F1467's root of the 32-bit `a*a + b*b`; a zero root
                    // leaves the basis. Each component over the root through F1523
                    // (its magnitude, sign restored) is F1744's cosine and sine.
                    let root = crate::model_math::table_root(
                        a.wrapping_mul(a).wrapping_add(b.wrapping_mul(b)) as u32,
                    );
                    if root != 0 {
                        let ratio = |value: i32| {
                            let magnitude =
                                crate::model_math::fraction(value.unsigned_abs(), root) as i32;
                            if value < 0 {
                                magnitude.wrapping_neg()
                            } else {
                                magnitude
                            }
                        };
                        crate::model_math::rotate_by(
                            &mut working.basis,
                            (code >> 5 & 3) as u8,
                            ratio(a),
                            ratio(b),
                        )?;
                    }
                }
                Some(command.size as i32)
            }
            // F1767: bit 15 set -> F1768, which restores the vertex frame (+0x14) from
            // the current basis (+0x9c); `0x801e` instead draws the object's light
            // points (F1769). Otherwise the vertex record and cache bases advance by a
            // variable count (F1473). Cached vertices are not invalidated either way.
            30 => {
                if code == 0x801e {
                    // F1769 loops over the count in global word 4 (+0xa4).
                    light_points(
                        offset,
                        &mut result,
                        shared,
                        vertices.as_deref_mut(),
                        working,
                        &instance,
                    )?;
                } else if code & 0x8000 == 0 {
                    return Err(format!(
                        "Vertex base offset command {code:#06x} at {offset:#x} is not ported."
                    ));
                } else {
                    if !defined.basis {
                        return Err(format!(
                            "Frame restore at {offset:#x} reads the basis, undefined in this child instance."
                        ));
                    }
                    working.matrix = working.basis;
                }
                Some(command.size as i32)
            }
            31 => {
                for surface in program.surfaces.iter().filter(|s| s.offset == offset) {
                    // F1874 resolves the origin through F1470 (view) and each axis
                    // through F1472 (object), then passes `axis - origin` vectors.
                    let mut origin_view = None;
                    let mut axis_vectors = None;
                    if let Some(live) = vertices.as_deref_mut() {
                        let origin = live.resolve(
                            surface.origin as i8,
                            working,
                            instance.position,
                            instance.generation,
                            false,
                        )?;
                        let mut vectors = [[0i32; 3]; 3];
                        for (position, index) in surface.axes.iter().enumerate() {
                            let axis = live.resolve(
                                *index as i8,
                                working,
                                instance.position,
                                instance.generation,
                                false,
                            )?;
                            vectors[position] = std::array::from_fn(|i| {
                                axis.object[i].wrapping_sub(origin.object[i])
                            });
                        }
                        origin_view = Some(origin.view);
                        axis_vectors = Some(vectors);
                    }
                    result.surfaces.push(SurfaceEmission {
                        surface: surface.clone(),
                        context: working.clone(),
                        origin: origin_view,
                        axes: axis_vectors,
                    });
                }
                Some(command.size as i32)
            }
            10 => {
                for text in program.texts.iter().filter(|t| t.offset == offset) {
                    let normal = visibility
                        .normals
                        .get(usize::from(text.normal))
                        .ok_or_else(|| {
                            format!("Text at {offset:#x} references an out-of-range normal.")
                        })?;
                    let culled = normal.culled;
                    let mut queued = None;
                    if let (false, Some(live)) = (culled, vertices.as_deref_mut()) {
                        let anchor = live.resolve(
                            text.vertex as i8,
                            working,
                            instance.position,
                            instance.generation,
                            false,
                        )?;
                        if let Some(queue) = shared.order.as_mut() {
                            let id = shared.texts;
                            shared.texts += 1;
                            queue.push(
                                model_depth::key(anchor.view, instance.key_scale),
                                Primitive::Text(id),
                            )?;
                            queued = Some(id);
                        }
                    }
                    result.texts.push(TextEmission {
                        text: text.clone(),
                        culled,
                        context: working.clone(),
                        shade: normal.colour,
                        queued,
                    });
                }
                Some(command.size as i32)
            }
            14 | 27 => {
                for submodel in program.submodels.iter().filter(|s| s.offset == offset) {
                    let mut main = None;
                    if let Some(live) = vertices.as_deref_mut() {
                        main = Some(live.resolve(
                            submodel.vertex as i8,
                            working,
                            frame.instance.position,
                            frame.instance.generation,
                            true,
                        )?);
                    }
                    result.submodels.push(SubmodelEmission {
                        submodel: submodel.clone(),
                        context: working.clone(),
                    });
                    if let (Some(environment), Some(main), Some(live)) =
                        (shared.children, main, vertices.as_deref_mut())
                    {
                        let call = Call {
                            submodel,
                            main,
                            parent: working,
                            defined: &defined,
                        };
                        let run = match call_child(frame, call, live, environment, shared) {
                            Ok(run) => run,
                            Err(error) => ChildRun {
                                offset: submodel.offset,
                                model: submodel.model,
                                radius: 0,
                                outcome: ChildOutcome::Failed(error),
                                setup: None,
                            },
                        };
                        result.children.push(run);
                    }
                }
                Some(command.size as i32)
            }
            26 => {
                for colour in program.colours.iter().filter(|c| c.offset == offset) {
                    let index = if colour.selector == 0 {
                        0
                    } else {
                        selector(colour.selector, working)?;
                        (variable(colour.selector, working) & 7) as usize
                    };
                    let value = colour
                        .materials
                        .get(index)
                        .copied()
                        .unwrap_or(colour.materials[0]);
                    let lighting = match (colour.normal, result.normal_zero_colour) {
                        (0, Some(rgb)) => rgb,
                        _ => {
                            visibility
                                .normals
                                .get(usize::from(colour.normal))
                                .ok_or_else(|| {
                                    format!(
                                        "Colour at {offset:#x} references an out-of-range normal."
                                    )
                                })?
                                .colour
                        }
                    };
                    let rgb = resolve_colour(value, working.colour, lighting);
                    let target = if colour.to_normal_zero {
                        if material(value).texture != 0 {
                            return Err(format!(
                                "Colour at {offset:#x} needs the runtime texture colour table."
                            ));
                        }
                        result.normal_zero_colour = Some(rgb);
                        ColourTarget::NormalZero
                    } else {
                        working.colour = rgb;
                        ColourTarget::Instance
                    };
                    result.colours.push(ColourEmission {
                        colour: colour.clone(),
                        material: value,
                        target,
                        rgb,
                    });
                }
                Some(command.size as i32)
            }
            6 => {
                let Some(live) = vertices.as_deref_mut() else {
                    return Err(format!(
                        "Unsupported executed command {code:#06x} at {offset:#x}."
                    ));
                };
                let (depth, consumed) = model_depth::decode(&bytes[offset..])?;
                if consumed != command.size {
                    return Err(format!(
                        "Depth command at {offset:#x} consumed {consumed} bytes, expected {}.",
                        command.size
                    ));
                }
                let key = resolve_depth_key(&depth, live, working, &instance)?;
                shared
                    .order
                    .as_mut()
                    .ok_or_else(|| "Depth execution requires an order queue.".to_owned())?
                    .group(key)?;
                shared.queue.push(QueueEvent::Group(key));
                Some(command.size as i32)
            }
            13 | 29 => {
                selector(payload[0], working)?;
                selector(payload[1], working)?;
                let destination = usize::from(code >> 8 & 7);
                if frame.child && destination == 7 {
                    return Err(format!(
                        "Arithmetic at {offset:#x} would overwrite the child's external vertex pointer (+0x70)."
                    ));
                }
                arithmetic_with_table(code, [payload[0], payload[1]], working, table)?;
                working.undefined_locals &= !(1 << destination);
                Some(command.size as i32)
            }
            19 | 20 => {
                selector(payload[0], working)?;
                let branch = bit_branch(code, [payload[0], payload[1]], working)?;
                if branch.is_none() {
                    returned = true;
                }
                branch
            }
            11 | 12 => {
                let branch = visibility_branch(
                    code,
                    i16::from_le_bytes([payload[0], payload[1]]),
                    visibility,
                )?;
                if branch.is_none() {
                    returned = true;
                }
                branch
            }
            // F1741 0xc000: the basis from three vertices' object positions, each
            // through F1472 behind a cache check, in byte order.
            28 if code & 0xc000 == 0xc000 => {
                let Some(live) = vertices.as_deref_mut() else {
                    return Err(format!(
                        "Unsupported executed command {code:#06x} at {offset:#x}."
                    ));
                };
                let [a, b, c] = [payload[0], payload[1], payload[2]]
                    .map(|index| live.object(index as i8, working));
                working.basis = crate::model_math::vertex_basis(a?, b?, c?);
                defined.basis = true;
                Some(command.size as i32)
            }
            28 => {
                let table =
                    table.ok_or("Model rotation execution needs the original sine table.")?;
                if code & 0xc000 == 0x8000 && !defined.basis {
                    return Err(format!(
                        "Rotation at {offset:#x} reads the basis, undefined in this child instance."
                    ));
                }
                if payload[0] & 0xc0 == 0xc0 {
                    selector(payload[0], working)?;
                }
                table
                    .rotate_command(working, code, i16::from_le_bytes([payload[0], payload[1]]))
                    .map_err(|error| format!("{error} ({code:#06x} at {offset:#x})"))?;
                if code & 0xc000 != 0x8000 {
                    // F1739 rewrote the basis and its handedness from the frame.
                    defined.basis = true;
                    defined.handedness = defined.base_handedness;
                }
                Some(command.size as i32)
            }
            _ => {
                return Err(format!(
                    "Unsupported executed command {code:#06x} at {offset:#x}."
                ))
            }
        };
        let Some(relative) = relative else {
            break;
        };
        offset = offset
            .checked_add_signed(relative as isize)
            .ok_or_else(|| format!("Execution branch leaves the program at {offset:#x}."))?;
    }
    if !returned {
        return Err(format!(
            "Model execution exceeded its {}-command budget; context unchanged.",
            shared.budget
        ));
    }
    Ok(result)
}

struct Call<'c> {
    submodel: &'c Submodel,
    main: crate::model_instance::Vertex,
    parent: &'c VertexContext,
    defined: &'c Defined,
}

/// F1687: a child is culled when its sphere (view centre, radius) lies entirely behind
/// z = 0x40 or entirely outside the x or y view cone (signed compares; |x| of
/// `i32::MIN` stays negative, as `neg` leaves it).
pub fn child_culled(view: [i32; 3], radius: i32) -> bool {
    let near = view[2].wrapping_add(radius);
    if near < 0x40 {
        return true;
    }
    near < view[0].wrapping_abs().wrapping_sub(radius)
        || near < view[1].wrapping_abs().wrapping_sub(radius)
}

/// F1748/F1749 -> F1750 -> F1740 -> F1773/F1774 for one child call whose position
/// vertex `call.main` the parent has already projected.
fn call_child(
    frame: &Frame<'_, '_>,
    call: Call<'_>,
    parent_vertices: &mut Vertices<'_>,
    environment: &ChildEnvironment<'_>,
    shared: &mut Shared<'_>,
) -> Result<ChildRun, String> {
    let Call {
        submodel,
        main,
        parent,
        defined,
    } = call;
    let offset = submodel.offset;
    // Opcode 14 passes the instance colour; opcode 27 its material through F1692/F1694:
    // nibble RGB (zero when textured) plus the instance colour for local materials.
    let colour = match submodel.material {
        None => parent.colour,
        Some(value) => {
            let decoded = material(value);
            if decoded.local_colour {
                std::array::from_fn(|i| decoded.rgb[i].wrapping_add(parent.colour[i]))
            } else {
                decoded.rgb
            }
        }
    };
    let argument = match submodel.scale {
        ChildScale::Literal(value) => i32::from(value),
        ChildScale::Local(index) => crate::model::checked_variable(0xc0 | index, parent)?,
    };
    let model = environment.models.model(submodel.model)?;
    // F1750: child scale = argument + descriptor +0x14 + +0x18 - parent +0x130.
    let scale = argument
        .wrapping_add(model.scale)
        .wrapping_add(model.scale_offset)
        .wrapping_sub(frame.instance.key_scale);
    let radius = model.radius.wrapping_shl(scale as u32);
    let mut record = ChildRun {
        offset,
        model: submodel.model,
        radius,
        outcome: ChildOutcome::Culled,
        setup: None,
    };
    if child_culled(main.view, radius) {
        return Ok(record);
    }
    // Extra vertices: +4 always, then +2, +3, +5 up to the first 0x7f, each through
    // F1471 behind the generation check.
    let mut sources = [None; 5];
    sources[0] = Some(ExternalSource {
        view: main.view,
        screen: main.screen,
    });
    if let Some(extra) = submodel.extra {
        for (k, index) in [extra[2], extra[0], extra[1], extra[3]]
            .into_iter()
            .enumerate()
        {
            if k > 0 && index == 0x7f {
                break;
            }
            let vertex = parent_vertices.resolve(
                index as i8,
                parent,
                frame.instance.position,
                frame.instance.generation,
                true,
            )?;
            sources[k + 1] = Some(ExternalSource {
                view: vertex.view,
                screen: vertex.screen,
            });
        }
    }
    // F1740: frame mode 6 copies the parent basis and its handedness; any other mode
    // permutes/negates the parent frame through F1739.
    let word = u32::from(submodel.frame);
    let (matrix, base_handedness, base_handedness_defined) = if word >> 8 & 7 == 6 {
        if !defined.basis {
            return Err(format!(
                "Child call at {offset:#x} copies the parent basis, which is undefined."
            ));
        }
        (parent.basis, parent.handedness, defined.handedness)
    } else {
        let (matrix, handedness) =
            crate::model_math::frame_rows(&parent.matrix, parent.base_handedness, word)
                .map_err(|error| format!("{error} (child call at {offset:#x})"))?;
        (matrix, handedness, defined.base_handedness)
    };
    // F1745 (inside F1774): two saved pointers, a frame word, six external and n own
    // 36-byte cache slots and 8-byte normals; no room means the child is skipped.
    let size = 12 + (6 + model.vertex_count) * 36 + model.normal_count * 8;
    if shared.arena + size > ARENA_SIZE {
        record.outcome = ChildOutcome::NoRoom;
        return Ok(record);
    }
    if shared.depth >= MAX_CHILD_DEPTH {
        return Err(format!(
            "Child call at {offset:#x} exceeds the {MAX_CHILD_DEPTH}-level nesting bound."
        ));
    }
    if !(0..=31).contains(&scale) {
        return Err(format!(
            "Child call at {offset:#x} gives model {} scale {scale}, outside 0..31.",
            submodel.model
        ));
    }
    let position = main.view;
    let shade_bytes: Vec<u8> = environment.shades.iter().flatten().copied().collect();
    let mut locals = [0i32; 64];
    locals[0] = parent.locals[0].wrapping_add(1);
    locals[1] = parent.locals[1];
    locals[2] = parent.locals[2];
    for (k, chunk) in shade_bytes.chunks_exact(4).enumerate() {
        locals[12 + k] = i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    }
    let undefined_locals = (parent.undefined_locals & 0b111) | 0b1111_1111_1000;
    // F1685 runs unless the frame word has 0x4000; F1774 then raises +0x44 to the
    // child scale - 8 if lower, shifting the observer.
    let lit = word & 0x4000 == 0;
    // A lit call inside an unlit (0x4000) child copies that child's +0x134,
    // which F1740 left as stale arena memory. The explosion's fire (model 155 ->
    // 320/322/323 -> 329..331) depends on it; those children's materials are
    // unlit, so only the normals' shade choice reads the vector. The port uses
    // the vector the unlit child inherited, the last light written down the
    // chain, rather than refusing the call.
    let mut observer = None;
    let mut direction = [0; 3];
    if lit {
        let (mut shifted, shift) = crate::model_math::shift_down(position);
        let mut observer_scale = shift as i32;
        let difference = observer_scale + 7 - scale;
        if difference < 0 {
            observer_scale -= difference;
            let by = (difference.wrapping_neg() as u32) & 31;
            shifted = shifted.map(|value| value >> by);
        }
        let mut point = crate::model_math::transform_rows(shifted.map(i32::wrapping_neg), &matrix);
        direction = crate::model_math::transform_rows(frame.light, &matrix);
        let raise = scale - 8 - observer_scale;
        if raise > 0 {
            observer_scale += raise;
            point = point.map(|value| value.wrapping_shl(raise as u32));
        }
        observer = Some((observer_scale as u32, point));
    }
    let normals = match observer {
        Some((observer_scale, point)) => crate::model_normals::evaluate(
            model.normals,
            model.normal_count,
            model.vertices,
            model.vertex_count,
            &crate::model_normals::NormalContext {
                scale: scale as u32,
                observer_scale,
                observer: point,
                light: direction,
                shades: environment.shades,
            },
        )
        .map_err(|error| format!("Child model {} normals: {error}", submodel.model))?,
        // F1480 still writes normals 0 and 1 from the shades; the others would be
        // computed from undefined observer/light fields.
        None => vec![
            crate::model_normals::Normal {
                culled: false,
                colour: environment.shades[3],
            },
            crate::model_normals::Normal {
                culled: false,
                colour: environment.shades[4],
            },
        ],
    };
    let visibility = VisibilityContext {
        depth: position[2],
        scale,
        detail: frame.visibility.detail,
        normals: &normals,
    };
    let mut context = VertexContext {
        matrix,
        scale: scale as u32,
        globals: parent.globals,
        locals,
        colour,
        basis: crate::model::IDENTITY,
        base_handedness,
        handedness: 0,
        direction,
        undefined_locals,
    };
    let instance = InstanceContext {
        position,
        generation: 2,
        key_scale: frame.instance.key_scale,
        operand_scale: scale,
        textures: frame.instance.textures,
    };
    let externals = ExternalVertices {
        position,
        generation: 2,
        sources,
    };
    // F1774 points the child's +0x154 at its own normal records (polygon circles).
    let mut vertices = Vertices::child(model.vertices, model.vertex_count, externals)
        .map_err(|error| format!("Child model {} vertices: {error}", submodel.model))?
        .with_normal_records(model.normals);
    vertices.set_jitter(parent_vertices.jitter());
    let setup = ChildSetup {
        instance,
        context: context.clone(),
        observer,
        externals,
    };
    let child = Frame {
        bytes: model.commands,
        vertex_count: model.vertex_count,
        visibility: &visibility,
        table: frame.table,
        instance,
        defined: Defined {
            basis: false,
            handedness: false,
            base_handedness: base_handedness_defined,
            direction: lit,
        },
        light: frame.light,
        child: true,
    };
    shared.arena += size;
    shared.depth += 1;
    let execution = run(&child, Some(&mut vertices), &mut context, shared);
    shared.depth -= 1;
    shared.arena -= size;
    let execution = execution
        .map_err(|error| format!("Child model {} at {offset:#x}: {error}", submodel.model))?;
    parent_vertices.set_jitter(vertices.jitter());
    record.outcome = ChildOutcome::Executed(Box::new(execution));
    record.setup = Some(setup);
    Ok(record)
}
