//! The modern GPU renderer.
//!
//! This crate knows nothing about the game. It draws a [`Scene`]: a presentation-only description
//! of what the camera sees (view-space triangles and lines, and an image to show behind them),
//! built by the caller from the frame the game has already decided, and returns the picture. The
//! game's rules, state and original renderer never depend on it, so it can be switched on and off
//! at any time.
//!
//! The renderer uses wgpu (Vulkan, Direct3D 12 or Metal) offscreen: 4x multisampling, a reversed-Z
//! 32-bit depth buffer (precise from a docking bay to a planet) and perspective-correct clipping by
//! the GPU, so geometry passing behind the eye is cut cleanly. The image is read back to the caller.
//! See `docs/notes/modern-graphics.md`.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// An RGB image, one `0x00RRGGBB` word a pixel, row after row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u32>,
}

/// How view space maps to the picture: a point `(x, y, z)` (x right, y up, z ahead of the eye)
/// lands at `x * x_scale / z` and `y * y_scale / z` in normalised coordinates (-1..1). `near` is
/// the nearest depth drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    pub x_scale: f32,
    pub y_scale: f32,
    pub near: f32,
}

/// A texture as the original keeps it: a map of colour indices (row after row), coloured per
/// face from a palette, so one map serves every face that uses it in any colours. Each texel is
/// `index | detail << 8`: the palette index, and a brightness detail (128 is 1, 255 nearly 2) an
/// enlarged map can carry from finer art. Every map is resampled to a [`MAP_SIZE`] square layer
/// if it is not one already (coordinates are 0..1 either way).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IndexMap {
    /// Equal non-zero ids promise equal texels, so a map already on the GPU is not uploaded
    /// again; 0 uploads it every time.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// Shared, so a caller's map is handed over without a copy.
    pub texels: std::sync::Arc<[u16]>,
}

/// The side of the GPU's map layers.
pub const MAP_SIZE: u32 = 512;
/// The maps kept on the GPU at once (about 45 MB with their mip levels).
pub const MAP_LAYERS: u32 = 64;
/// The colours of one palette (a face's slots); an index past them reads the last.
pub const PALETTE_SIZE: usize = 64;

/// A triangle in view space, its corners' colours (RGBA, 0..1), and optionally a texture, which
/// the corners' colours then shade (white shows it as it is). Textures are mapped with perspective correction, filtered smoothly in
/// colour (each of the four nearest texels looked up in the palette, then blended) and blended
/// between mip levels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub positions: [[f32; 3]; 3],
    pub colours: [[f32; 4]; 3],
    pub texture: Option<FaceTexture>,
}

/// Which of the scene's [`Scene::maps`] a triangle shows, in which of its
/// [`Scene::palettes`], and the corners' coordinates in the map (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FaceTexture {
    pub map: u32,
    pub palette: u32,
    pub uvs: [[f32; 2]; 3],
}

/// A palette: [`PALETTE_SIZE`] colours (RGBA, 0..1; an alpha below a half is a hole).
pub type Palette = [[f32; 4]; PALETTE_SIZE];

/// A line in view space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub positions: [[f32; 3]; 2],
    pub colour: [f32; 4],
    /// Its width in pixels (anti-aliased; at least one).
    pub width: f32,
}

/// A line as the GPU reads it: both ends, the colour, the width and the place in its batch.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct WideLine {
    a: [f32; 3],
    width: f32,
    b: [f32; 3],
    layer: f32,
    colour: [f32; 4],
}

/// One object's geometry, depth-tested within itself. Batches are drawn in order, each over the
/// last (the caller's painter order between objects, which may use different view-space scales),
/// so only the faces of one batch hide each other by depth. Within a batch a later triangle wins
/// over an earlier one at the same depth (decals lying on a hull).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Batch {
    pub triangles: Vec<Triangle>,
    pub lines: Vec<Line>,
    /// Atmospheres drawn over the batch's faces (a planet's).
    pub atmospheres: Vec<Atmosphere>,
    /// Lights and stars: round glows facing the eye, hidden by the batch's nearer faces.
    pub sprites: Vec<Sprite>,
}

/// A point of light at `position` (view space): a soft round core `size` pixels across in
/// `colour`, and optionally a fainter halo `halo_size` pixels across round it. Its light is added
/// to what lies behind it, and may be brighter than white (1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Sprite {
    pub position: [f32; 3],
    pub size: f32,
    pub colour: [f32; 3],
    pub halo_size: f32,
    pub halo: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct SpriteVertex {
    position: [f32; 3],
    size: f32,
    colour: [f32; 3],
    halo_size: f32,
    halo: [f32; 3],
}

/// A planet's atmosphere: a shell from the surface to `outer` round `centre` (view space),
/// traced per pixel. Its haze is thicker the longer and lower a view ray runs through it, and
/// lit by the star on the side facing it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Atmosphere {
    pub centre: [f32; 3],
    pub radius: f32,
    pub outer: f32,
    /// The haze colour (0..1).
    pub colour: [f32; 3],
    /// The unit direction towards the star, in view space.
    pub sun: [f32; 3],
    /// How dense the haze is (1 the usual).
    pub density: f32,
}

/// [`Atmosphere`] as the GPU reads it: everything in units of the outer radius.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Shell {
    centre: [f32; 3],
    radius: f32,
    colour: [f32; 3],
    density: f32,
    sun: [f32; 3],
    pad: f32,
}

/// What to draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// The picture's size in pixels.
    pub width: u32,
    pub height: u32,
    /// Shown behind everything, stretched over the whole picture (smoothly filtered).
    pub background: Option<Image>,
    pub projection: Projection,
    pub batches: Vec<Batch>,
    /// How strongly bright parts glow (0 none): they are blurred at half size and added back.
    pub bloom: f32,
    /// How many times the picture's size the 3-D view is drawn at before it is filtered down
    /// (1 to 2; on top of the 4x multisampling). Sizes given in pixels are the picture's.
    pub supersample: f32,
    pub maps: Vec<IndexMap>,
    pub palettes: Vec<Palette>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    colour: [f32; 4],
    /// The primitive's place in its batch, which pulls it very slightly nearer.
    layer: f32,
    uv: [f32; 2],
    /// The map's layer, or -1 for the colour.
    texture: f32,
    /// The palette's first colour in the palette buffer.
    palette: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    scale: [f32; 4],
    /// The picture's width and height in pixels.
    viewport: [f32; 4],
}

const SAMPLES: u32 = 4;
/// The most primitives of a batch that are told apart by their order.
const LAYERS: usize = 4095;
/// BGRA, so a texel's bytes are the little-endian `0xAARRGGBB` word callers use.
const COLOUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
/// The 3-D view and the bloom are drawn in floating point, so the star and lights can be
/// brighter than white; the last pass brings them back to [`COLOUR_FORMAT`].
const SCENE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The bloom's passes: bright parts at half size, a separable blur, and the sum.
const POST: &str = r#"
struct Post { params: vec4<f32>, texel: vec4<f32> };
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var extra: texture_2d<f32>;
@group(0) @binding(2) var filtered: sampler;
@group(0) @binding(3) var<uniform> post: Post;

struct FullOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> FullOut {
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    var out: FullOut;
    out.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

// The parts brighter than the threshold (params.z), fading in over the last stretch, from four
// samples of the full picture round each half-size texel.
@fragment
fn fs_bright(i: FullOut) -> @location(0) vec4<f32> {
    let t = post.texel.xy * 0.5;
    var sum = vec3<f32>(0.0);
    for (var k = 0; k < 4; k = k + 1) {
        let offset = vec2<f32>(f32(k & 1) - 0.5, f32(k >> 1u) - 0.5) * t * 2.0;
        let c = textureSample(source, filtered, i.uv + offset).rgb;
        let peak = max(c.r, max(c.g, c.b));
        sum = sum + c * smoothstep(post.params.z, 1.0, peak);
    }
    return vec4<f32>(sum * 0.25, 1.0);
}

// Nine taps of a Gaussian along params.xy (in source texels).
@fragment
fn fs_blur(i: FullOut) -> @location(0) vec4<f32> {
    var weights = array<f32, 5>(0.2270270, 0.1945946, 0.1216216, 0.0540540, 0.0162162);
    let step = post.params.xy * post.texel.xy;
    var sum = textureSample(source, filtered, i.uv).rgb * weights[0];
    for (var k = 1; k < 5; k = k + 1) {
        let offset = step * f32(k);
        sum = sum + textureSample(source, filtered, i.uv + offset).rgb * weights[k];
        sum = sum + textureSample(source, filtered, i.uv - offset).rgb * weights[k];
    }
    return vec4<f32>(sum, 1.0);
}

// The picture plus the blurred glow times params.w, brought back to the screen's range: colours
// up to white stay exactly as they are, and a colour brighter than white runs towards white by
// how far over it is (a star's hot core is white whatever its tint).
@fragment
fn fs_combine(i: FullOut) -> @location(0) vec4<f32> {
    // Four points of the output pixel (texel.zw) in the supersampled picture: a box filter.
    let quarter = post.texel.zw * 0.25;
    var base = vec3<f32>(0.0);
    for (var k = 0; k < 4; k = k + 1) {
        let offset = vec2<f32>(f32(k & 1) * 2.0 - 1.0, f32(k >> 1u) * 2.0 - 1.0) * quarter;
        base = base + textureSample(source, filtered, i.uv + offset).rgb * 0.25;
    }
    let glow = textureSample(extra, filtered, i.uv).rgb;
    let c = base + glow * post.params.w;
    let over = max(max(c.r, max(c.g, c.b)) - 1.0, 0.0);
    return vec4<f32>(min(c + vec3<f32>(over * 0.6), vec3<f32>(1.0)), 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct PostUniforms {
    params: [f32; 4],
    texel: [f32; 4],
}

/// The brightness (largest channel) above which a pixel starts to glow.
const BLOOM_THRESHOLD: f32 = 0.8;

const SHADER: &str = r#"
struct Uniforms { scale: vec4<f32>, viewport: vec4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) colour: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) texture: i32,
    @location(3) @interpolate(flat) palette: u32,
};

@group(0) @binding(3) var face_maps: texture_2d_array<f32>;
@group(0) @binding(4) var<storage, read> palettes: array<vec4<f32>>;

// One texel of a map at a mip level, through the face's palette, its brightness detail applied,
// premultiplied by its alpha (so holes blend away cleanly).
fn texel(layer: i32, level: i32, at: vec2<i32>, size: vec2<i32>, palette: u32) -> vec4<f32> {
    let t = textureLoad(face_maps, clamp(at, vec2<i32>(0), size - vec2<i32>(1)), layer, level);
    let index = min(u32(t.r * 255.0 + 0.5), 63u);
    let colour = palettes[palette + index];
    return vec4<f32>(colour.rgb * (t.g * 2.0) * colour.a, colour.a);
}

// The four nearest texels at a level, blended.
fn bilinear(uv: vec2<f32>, layer: i32, level: i32, palette: u32) -> vec4<f32> {
    let size = vec2<i32>(textureDimensions(face_maps, level));
    let p = uv * vec2<f32>(size) - 0.5;
    let base = vec2<i32>(floor(p));
    let f = fract(p);
    let a = texel(layer, level, base, size, palette);
    let b = texel(layer, level, base + vec2<i32>(1, 0), size, palette);
    let c = texel(layer, level, base + vec2<i32>(0, 1), size, palette);
    let d = texel(layer, level, base + vec2<i32>(1, 1), size, palette);
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Reversed-Z: the clip z is the near distance and w the view depth, so the stored depth is
// near / z: 1 at the near plane, falling towards 0 far away.
// A later primitive of a batch is moved nearer by a tiny factor (about 8 depth steps each), so
// a decal drawn after the face it lies on stays visible.
@vertex
fn vs_scene(
    @location(0) p: vec3<f32>,
    @location(1) c: vec4<f32>,
    @location(2) layer: f32,
    @location(3) uv: vec2<f32>,
    @location(4) texture: f32,
    @location(5) palette: f32,
) -> Out {
    var out: Out;
    out.position = vec4<f32>(p.x * u.scale.x, p.y * u.scale.y, u.scale.z * (1.0 + layer * 4.8e-7), p.z);
    out.colour = c;
    out.uv = uv;
    out.texture = i32(texture);
    out.palette = u32(palette);
    return out;
}

@fragment
fn fs_scene(i: Out) -> @location(0) vec4<f32> {
    // The mip level from the texture's footprint (worked out before any branch).
    let footprint = i.uv * vec2<f32>(textureDimensions(face_maps, 0));
    let spread = max(length(dpdx(footprint)), length(dpdy(footprint)));
    if (i.texture < 0) {
        return i.colour;
    }
    let top = f32(textureNumLevels(face_maps) - 1u);
    let lod = clamp(log2(max(spread, 1.0e-6)), 0.0, top);
    let level = i32(floor(lod));
    let near = bilinear(i.uv, i.texture, level, i.palette);
    let far = bilinear(i.uv, i.texture, min(level + 1, i32(top)), i.palette);
    let texel = mix(near, far, fract(lod));
    // A hole in a masked texture shows what lies behind. The corners' colours shade the
    // texture (white leaves it as it is).
    if (texel.a < 0.5) {
        discard;
    }
    return vec4<f32>(min(texel.rgb / texel.a * i.colour.rgb, vec3<f32>(1.0)), 1.0);
}

struct WideOut {
    @builtin(position) position: vec4<f32>,
    @location(0) across: f32,
    @location(1) @interpolate(flat) half: f32,
    @location(2) @interpolate(flat) colour: vec4<f32>,
};

// A line as a quad: both ends projected, then moved apart across the line by half its width
// (plus a pixel for the anti-aliased edge) in pixels. The ends must be in front of the eye.
@vertex
fn vs_wide(
    @builtin(vertex_index) index: u32,
    @location(0) a: vec3<f32>,
    @location(1) width: f32,
    @location(2) b: vec3<f32>,
    @location(3) layer: f32,
    @location(4) colour: vec4<f32>,
) -> WideOut {
    let z = u.scale.z * (1.0 + layer * 4.8e-7);
    let ca = vec4<f32>(a.x * u.scale.x, a.y * u.scale.y, z, a.z);
    let cb = vec4<f32>(b.x * u.scale.x, b.y * u.scale.y, z, b.z);
    let half_view = u.viewport.xy * 0.5;
    let sa = ca.xy / ca.w * half_view;
    let sb = cb.xy / cb.w * half_view;
    var along = sb - sa;
    if (dot(along, along) < 1.0e-6) {
        along = vec2<f32>(1.0, 0.0);
    }
    along = normalize(along);
    let half = max(width, 1.0) * 0.5;
    let reach = half + 1.0;
    let normal = vec2<f32>(-along.y, along.x) * reach;
    var ends = array<u32, 6>(0u, 1u, 1u, 0u, 1u, 0u);
    var sides = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    let side = sides[index];
    var clip = ca;
    var past = -along;
    if (ends[index] == 1u) {
        clip = cb;
        past = along;
    }
    // Half a pixel past each end, so the joins of a curve's steps close.
    let offset = normal * side + past * 0.5;
    clip.x = clip.x + offset.x / half_view.x * clip.w;
    clip.y = clip.y + offset.y / half_view.y * clip.w;
    var out: WideOut;
    out.position = clip;
    out.across = side * reach;
    out.half = half;
    out.colour = colour;
    return out;
}

@fragment
fn fs_wide(i: WideOut) -> @location(0) vec4<f32> {
    let coverage = clamp(i.half + 0.5 - abs(i.across), 0.0, 1.0);
    if (coverage <= 0.0) {
        discard;
    }
    return vec4<f32>(i.colour.rgb, i.colour.a * coverage);
}

struct SpriteOut {
    @builtin(position) position: vec4<f32>,
    @location(0) offset: vec2<f32>,
    @location(1) @interpolate(flat) size: f32,
    @location(2) @interpolate(flat) colour: vec3<f32>,
    @location(3) @interpolate(flat) halo_size: f32,
    @location(4) @interpolate(flat) halo: vec3<f32>,
};

// A quad round the projected point, `offset` in pixels from its centre. Pulled 2% nearer, so
// a light on a hull is not hidden by the face it sits on.
@vertex
fn vs_sprite(
    @builtin(vertex_index) index: u32,
    @location(0) p: vec3<f32>,
    @location(1) size: f32,
    @location(2) colour: vec3<f32>,
    @location(3) halo_size: f32,
    @location(4) halo: vec3<f32>,
) -> SpriteOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[index];
    // Wide enough for the halo's tail to fade out (it is 1/2000 at 1.8 halo radii).
    let extent = max(size * 0.5, halo_size * 0.9) + 1.5;
    var clip = vec4<f32>(p.x * u.scale.x, p.y * u.scale.y, u.scale.z * 1.02, p.z);
    clip.x = clip.x + corner.x * extent * 2.0 / u.viewport.x * clip.w;
    clip.y = clip.y + corner.y * extent * 2.0 / u.viewport.y * clip.w;
    var out: SpriteOut;
    out.position = clip;
    out.offset = corner * extent;
    out.size = size;
    out.colour = colour;
    out.halo_size = halo_size;
    out.halo = halo;
    return out;
}

@fragment
fn fs_sprite(i: SpriteOut) -> @location(0) vec4<f32> {
    let r = length(i.offset);
    let core = 1.0 - smoothstep(i.size * 0.5 - 0.7, i.size * 0.5 + 0.7, r);
    var glow = 0.0;
    if (i.halo_size > 0.0) {
        let q = r / (i.halo_size * 0.5);
        glow = exp(-2.5 * q * q);
    }
    let alpha = max(core, glow * 0.85);
    if (alpha < 0.004) {
        discard;
    }
    let colour = mix(i.halo, i.colour, core / max(alpha, 0.0001));
    return vec4<f32>(colour, alpha);
}

struct ShellOut {
    @builtin(position) position: vec4<f32>,
    @location(0) ndc: vec2<f32>,
    @location(1) @interpolate(flat) centre: vec3<f32>,
    @location(2) @interpolate(flat) radius: f32,
    @location(3) @interpolate(flat) colour: vec3<f32>,
    @location(4) @interpolate(flat) density: f32,
    @location(5) @interpolate(flat) sun: vec3<f32>,
};

@vertex
fn vs_shell(
    @builtin(vertex_index) index: u32,
    @location(0) centre: vec3<f32>,
    @location(1) radius: f32,
    @location(2) colour: vec3<f32>,
    @location(3) density: f32,
    @location(4) sun: vec3<f32>,
) -> ShellOut {
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = 1.0 - f32(index & 2u) * 2.0;
    var out: ShellOut;
    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.ndc = vec2<f32>(x, y);
    out.centre = centre;
    out.radius = radius;
    out.colour = colour;
    out.density = density;
    out.sun = sun;
    return out;
}

// The view ray through the pixel meets the shell (radius 1) and maybe the planet (`radius`).
// The cross product gives the ray's closest approach without the cancellation of the usual
// quadratic, which matters when the planet is far away.
@fragment
fn fs_shell(i: ShellOut) -> @location(0) vec4<f32> {
    let d = normalize(vec3<f32>(i.ndc.x / u.scale.x, i.ndc.y / u.scale.y, 1.0));
    let c = i.centre;
    let along = dot(c, d);
    let side = cross(c, d);
    let low2 = dot(side, side);
    if (low2 >= 1.0) {
        discard;
    }
    let half = sqrt(1.0 - low2);
    let far = along + half;
    if (far <= 0.0) {
        discard;
    }
    let near = max(along - half, 0.0);
    var end = far;
    let r2 = i.radius * i.radius;
    var ground = false;
    if (low2 < r2) {
        let hit = along - sqrt(r2 - low2);
        if (hit > 0.0) {
            end = min(end, hit);
            ground = true;
        }
    }
    // Near the limb a ray that meets the ground blends towards one that misses it, so the
    // terrain's own outline (which is not quite this sphere) shows no seam.
    let edge = select(0.0, smoothstep(i.radius * 0.94, i.radius, sqrt(low2)), ground);
    let path = mix(max(end - near, 0.0), max(far - near, 0.0), edge);
    // The longest path that misses the planet: a ray grazing the surface.
    let longest = 2.0 * sqrt(max(1.0 - r2, 1.0e-6));
    // 0 at the shell's edge, 1 at the surface: the haze is densest low down.
    let depth = clamp((1.0 - sqrt(low2)) / max(1.0 - i.radius, 1.0e-4), 0.0, 1.0);
    var thickness = path / longest * (0.35 + 0.65 * depth);
    // The original already mixes the haze into the ground's colours by distance (F1840), so
    // over the surface the shell only deepens it towards the limb.
    thickness = mix(thickness, thickness * thickness * 1.5, select(0.0, 1.0 - edge, ground));
    let alpha = 1.0 - exp(-4.0 * i.density * thickness);
    // The original's ring is the haze colour all the way round, with the star's light added on
    // the side facing it (F1879); here the light brightens the haze where the ray runs through
    // it, with a little forward glow round the star.
    let middle = normalize(d * (0.5 * (near + end)) - c);
    let lit = clamp(dot(middle, i.sun) * 1.4 + 0.3, 0.0, 1.0);
    let glow = pow(max(dot(d, i.sun), 0.0), 16.0) * 0.5;
    let colour = i.colour * (0.7 + 0.5 * lit) + vec3<f32>(glow);
    return vec4<f32>(min(colour, vec3<f32>(1.0)), alpha);
}

struct BackOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(1) var back_texture: texture_2d<f32>;
@group(0) @binding(2) var back_sampler: sampler;

@vertex
fn vs_back(@builtin(vertex_index) index: u32) -> BackOut {
    // One triangle covering the picture.
    let x = f32((index << 1u) & 2u);
    let y = f32(index & 2u);
    var out: BackOut;
    out.position = vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, y);
    return out;
}

@fragment
fn fs_back(i: BackOut) -> @location(0) vec4<f32> {
    return textureSample(back_texture, back_sampler, i.uv);
}
"#;

struct Targets {
    width: u32,
    height: u32,
    /// The size the 3-D view is drawn at (the picture's, supersampled).
    inner: (u32, u32),
    colour: wgpu::Texture,
    resolved: wgpu::Texture,
    /// The bloom's two half-size buffers and the picture with the glow added.
    glow: [wgpu::Texture; 2],
    bloomed: wgpu::Texture,
    depth: wgpu::Texture,
    readback: wgpu::Buffer,
    padded_row: u32,
}

/// The GPU, its pipelines and the picture's buffers (kept between frames).
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    layout: wgpu::BindGroupLayout,
    triangles: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    background: wgpu::RenderPipeline,
    sprites: wgpu::RenderPipeline,
    shells: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,

    post_layout: wgpu::BindGroupLayout,
    bright: wgpu::RenderPipeline,
    blur: wgpu::RenderPipeline,
    combine: wgpu::RenderPipeline,
    /// The GPU array the scenes' maps are kept in between frames.
    face_maps: Option<FaceMaps>,
    targets: Option<Targets>,
    /// The adapter's name, for the options page and the logs.
    pub adapter: String,
}

impl Renderer {
    /// Open the GPU. An error (no adapter, no device) leaves the caller to show its classic
    /// picture instead.
    pub fn new() -> Result<Self, String> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|error| format!("No GPU adapter: {error}"))?;
        let name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ffe-render"),
            ..Default::default()
        }))
        .map_err(|error| format!("No GPU device: {error}"))?;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ffe-render shaders"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ffe-render bindings"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ffe-render layout"),
            bind_group_layouts: &[Some(&layout)],
            ..Default::default()
        });
        let vertex_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3,
                1 => Float32x4,
                2 => Float32,
                3 => Float32x2,
                4 => Float32,
                5 => Float32
            ],
        };
        let depth = |write: bool, compare: wgpu::CompareFunction| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: Default::default(),
            bias: Default::default(),
        };
        let multisample = wgpu::MultisampleState {
            count: SAMPLES,
            ..Default::default()
        };
        let target = [Some(wgpu::ColorTargetState {
            format: SCENE_FORMAT,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let scene_pipeline = |topology: wgpu::PrimitiveTopology, label: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_scene"),
                    compilation_options: Default::default(),
                    buffers: &[Some(vertex_layout.clone())],
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    // The original culls hidden faces by their normals before they reach us.
                    cull_mode: None,
                    ..Default::default()
                },
                // Nearer is larger with reversed-Z.
                depth_stencil: Some(depth(true, wgpu::CompareFunction::GreaterEqual)),
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs_scene"),
                    compilation_options: Default::default(),
                    targets: &target,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let triangles = scene_pipeline(wgpu::PrimitiveTopology::TriangleList, "triangles");
        let wide_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<WideLine>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3,
                1 => Float32,
                2 => Float32x3,
                3 => Float32,
                4 => Float32x4
            ],
        };
        let lines = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lines"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_wide"),
                compilation_options: Default::default(),
                buffers: &[Some(wide_layout)],
            },
            primitive: Default::default(),
            // Tested against the batch's faces; lines do not hide each other.
            depth_stencil: Some(depth(false, wgpu::CompareFunction::GreaterEqual)),
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_wide"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            multiview_mask: None,
            cache: None,
        });
        let background = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("background"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_back"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(depth(false, wgpu::CompareFunction::Always)),
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_back"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            multiview_mask: None,
            cache: None,
        });
        let shell_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Shell>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3,
                1 => Float32,
                2 => Float32x3,
                3 => Float32,
                4 => Float32x3
            ],
        };
        let sprite_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<SpriteVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x3,
                1 => Float32,
                2 => Float32x3,
                3 => Float32,
                4 => Float32x3
            ],
        };
        // Glows add their light to what lies behind them.
        let additive = [Some(wgpu::ColorTargetState {
            format: SCENE_FORMAT,
            blend: Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::One,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::OVER,
            }),
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let sprites = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sprites"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_sprite"),
                compilation_options: Default::default(),
                buffers: &[Some(sprite_layout)],
            },
            primitive: Default::default(),
            // Tested against the batch's faces, without hiding each other.
            depth_stencil: Some(depth(false, wgpu::CompareFunction::GreaterEqual)),
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_sprite"),
                compilation_options: Default::default(),
                targets: &additive,
            }),
            multiview_mask: None,
            cache: None,
        });
        let shells = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("atmospheres"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_shell"),
                compilation_options: Default::default(),
                buffers: &[Some(shell_layout)],
            },
            primitive: Default::default(),
            depth_stencil: Some(depth(false, wgpu::CompareFunction::Always)),
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_shell"),
                compilation_options: Default::default(),
                targets: &target,
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("background sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let post_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("ffe-render bloom"),
            source: wgpu::ShaderSource::Wgsl(POST.into()),
        });
        let picture = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let post_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bloom bindings"),
            entries: &[
                picture(0),
                picture(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let post_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bloom layout"),
            bind_group_layouts: &[Some(&post_layout)],
            ..Default::default()
        });
        let post_pipeline = |entry: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&post_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &post_module,
                    entry_point: Some("vs_full"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &post_module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let (bright, blur, combine) = (
            post_pipeline("fs_bright", SCENE_FORMAT),
            post_pipeline("fs_blur", SCENE_FORMAT),
            post_pipeline("fs_combine", COLOUR_FORMAT),
        );
        Ok(Self {
            post_layout,
            bright,
            blur,
            combine,
            device,
            queue,
            layout,
            triangles,
            lines,
            background,
            sprites,
            shells,
            sampler,
            face_maps: None,
            targets: None,
            adapter: name,
        })
    }

    fn targets(&mut self, width: u32, height: u32, inner: (u32, u32)) -> &Targets {
        if self
            .targets
            .as_ref()
            .is_none_or(|t| t.width != width || t.height != height || t.inner != inner)
        {
            let sized = |size: (u32, u32), samples: u32, format, usage, label| {
                self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size.0,
                        height: size.1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
            };
            let texture =
                |samples: u32, format, usage, label| sized(inner, samples, format, usage, label);
            let padded_row = (width * 4).div_ceil(256) * 256;
            self.targets = Some(Targets {
                width,
                height,
                inner,
                colour: texture(
                    SAMPLES,
                    SCENE_FORMAT,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                    "colour",
                ),
                resolved: texture(
                    1,
                    SCENE_FORMAT,
                    wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    "resolved",
                ),
                glow: ["glow a", "glow b"].map(|label| {
                    self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some(label),
                        size: wgpu::Extent3d {
                            width: inner.0.div_ceil(2),
                            height: inner.1.div_ceil(2),
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: SCENE_FORMAT,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                }),
                bloomed: sized(
                    (width, height),
                    1,
                    COLOUR_FORMAT,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    "bloomed",
                ),
                depth: texture(
                    SAMPLES,
                    DEPTH_FORMAT,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                    "depth",
                ),
                readback: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("readback"),
                    size: u64::from(padded_row) * u64::from(height),
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                padded_row,
            });
        }
        self.targets.as_ref().unwrap()
    }

    /// Draw the scene and return its picture (`0x00RRGGBB` words, `width * height`).
    pub fn render(&mut self, scene: &Scene) -> Result<Vec<u32>, String> {
        let (width, height) = (scene.width.max(1), scene.height.max(1));
        let supersample = if scene.supersample.is_finite() {
            scene.supersample.clamp(1.0, 2.0)
        } else {
            1.0
        };
        let inner = (
            (width as f32 * supersample).ceil() as u32,
            (height as f32 * supersample).ceil() as u32,
        );
        self.targets(width, height, inner);
        let layers = self.upload_maps(&scene.maps);
        let face_view = &self.face_maps.as_ref().unwrap().view;
        let device = &self.device;
        let targets = self.targets.as_ref().unwrap();
        let uniforms = Uniforms {
            scale: [
                scene.projection.x_scale,
                scene.projection.y_scale,
                scene.projection.near,
                0.0,
            ],
            viewport: [inner.0 as f32, inner.1 as f32, 0.0, 0.0],
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("uniforms"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        // The background (a black pixel without one).
        let back = scene.background.clone().unwrap_or(Image {
            width: 1,
            height: 1,
            pixels: vec![0],
        });
        let back_texture = device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some("background"),
                size: wgpu::Extent3d {
                    width: back.width.max(1),
                    height: back.height.max(1),
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOUR_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &rgba(&back.pixels),
        );
        let back_view = back_texture.create_view(&Default::default());
        // The palettes one after another (one transparent palette when there are none).
        let mut colours: Vec<[f32; 4]> = scene.palettes.iter().flatten().copied().collect();
        if colours.is_empty() {
            colours.resize(PALETTE_SIZE, [0.0; 4]);
        }
        let palette_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("palettes"),
            contents: bytemuck::cast_slice(&colours),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bindings"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&back_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(face_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: palette_buffer.as_entire_binding(),
                },
            ],
        });
        // Every batch's triangles, then its lines, in one buffer.
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut ranges = Vec::with_capacity(scene.batches.len());
        let mut wide: Vec<WideLine> = Vec::new();
        let mut shells: Vec<Shell> = Vec::new();
        let mut sprites: Vec<SpriteVertex> = Vec::new();
        let mut sprite_ranges = Vec::with_capacity(scene.batches.len());
        let mut shell_ranges = Vec::with_capacity(scene.batches.len());
        for batch in &scene.batches {
            let first = vertices.len() as u32;
            for (index, t) in batch.triangles.iter().enumerate() {
                let layer = index.min(LAYERS) as f32;
                let (texture, palette, uvs) = match t.texture {
                    Some(face)
                        if (face.map as usize) < layers.len()
                            && (face.palette as usize) < scene.palettes.len() =>
                    {
                        (
                            layers[face.map as usize] as f32,
                            (face.palette as usize * PALETTE_SIZE) as f32,
                            face.uvs,
                        )
                    }
                    _ => (-1.0, 0.0, [[0.0; 2]; 3]),
                };
                vertices.extend((0..3).map(|k| Vertex {
                    position: t.positions[k],
                    colour: t.colours[k],
                    layer,
                    uv: uvs[k],
                    texture,
                    palette,
                }));
            }
            let middle = vertices.len() as u32;
            let layer = batch.triangles.len().min(LAYERS) as f32;
            let first_wide = wide.len() as u32;
            wide.extend(batch.lines.iter().filter_map(|l| {
                let [a, b] = clip_line(l.positions, scene.projection.near)?;
                Some(WideLine {
                    a,
                    width: l.width * supersample,
                    b,
                    layer,
                    colour: l.colour,
                })
            }));
            ranges.push((first..middle, first_wide..wide.len() as u32));
            let first_sprite = sprites.len() as u32;
            sprites.extend(batch.sprites.iter().map(|sprite| SpriteVertex {
                position: sprite.position,
                size: sprite.size * supersample,
                colour: sprite.colour,
                halo_size: sprite.halo_size * supersample,
                halo: sprite.halo,
            }));
            sprite_ranges.push(first_sprite..sprites.len() as u32);
            let first_shell = shells.len() as u32;
            shells.extend(batch.atmospheres.iter().filter_map(shell));
            shell_ranges.push(first_shell..shells.len() as u32);
        }
        let wide_buffer = (!wide.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("lines"),
                contents: bytemuck::cast_slice(&wide),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let sprite_buffer = (!sprites.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("sprites"),
                contents: bytemuck::cast_slice(&sprites),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let shell_buffer = (!shells.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("atmospheres"),
                contents: bytemuck::cast_slice(&shells),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let vertex_buffer = (!vertices.is_empty()).then(|| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let colour_view = targets.colour.create_view(&Default::default());
        let resolved_view = targets.resolved.create_view(&Default::default());
        let depth_view = targets.depth.create_view(&Default::default());
        let mut encoder = device.create_command_encoder(&Default::default());
        // The background, then one pass a batch: the colour carries over and the depth starts
        // again (reversed-Z: 0 is infinitely far). The last pass resolves the samples.
        let passes = ranges.len() + 1;
        for pass_index in 0..passes {
            let last = pass_index + 1 == passes;
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &colour_view,
                    depth_slice: None,
                    resolve_target: last.then_some(&resolved_view),
                    ops: wgpu::Operations {
                        load: if pass_index == 0 {
                            wgpu::LoadOp::Clear(wgpu::Color::BLACK)
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: if last {
                            wgpu::StoreOp::Discard
                        } else {
                            wgpu::StoreOp::Store
                        },
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &bind_group, &[]);
            if pass_index == 0 {
                pass.set_pipeline(&self.background);
                pass.draw(0..3, 0..1);
                continue;
            }
            let (triangles, lines) = &ranges[pass_index - 1];
            if let (Some(buffer), false) = (&vertex_buffer, triangles.is_empty()) {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.set_pipeline(&self.triangles);
                pass.draw(triangles.clone(), 0..1);
            }
            if let (Some(buffer), false) = (&wide_buffer, lines.is_empty()) {
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.set_pipeline(&self.lines);
                pass.draw(0..6, lines.clone());
            }
            let lights = &sprite_ranges[pass_index - 1];
            if let (Some(buffer), false) = (&sprite_buffer, lights.is_empty()) {
                pass.set_pipeline(&self.sprites);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..6, lights.clone());
            }
            let atmospheres = &shell_ranges[pass_index - 1];
            if let (Some(buffer), false) = (&shell_buffer, atmospheres.is_empty()) {
                pass.set_pipeline(&self.shells);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..3, atmospheres.clone());
            }
        }
        self.bloom(&mut encoder, targets, scene.bloom.max(0.0));
        let shown = &targets.bloomed;
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: shown,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &targets.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(targets.padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([encoder.finish()]);
        let slice = targets.readback.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| format!("GPU wait: {error}"))?;
        receiver
            .recv()
            .map_err(|error| format!("GPU readback: {error}"))?
            .map_err(|error| format!("GPU readback: {error}"))?;
        let mut pixels = Vec::with_capacity((width * height) as usize);
        {
            let data = slice
                .get_mapped_range()
                .map_err(|error| format!("GPU readback: {error}"))?;
            for row in 0..height as usize {
                let start = row * targets.padded_row as usize;
                pixels.extend(
                    data[start..start + width as usize * 4]
                        .chunks_exact(4)
                        .map(|p| u32::from_le_bytes([p[0], p[1], p[2], 0])),
                );
            }
        }
        targets.readback.unmap();
        Ok(pixels)
    }
}

impl Renderer {
    /// The bloom and the last pass: the resolved picture's bright parts at half size, blurred
    /// twice (the second time twice as wide), then added to it and brought back to the screen's
    /// range in `bloomed`.
    fn bloom(&self, encoder: &mut wgpu::CommandEncoder, targets: &Targets, strength: f32) {
        let device = &self.device;
        let view = |texture: &wgpu::Texture| texture.create_view(&Default::default());
        let (resolved, bloomed) = (view(&targets.resolved), view(&targets.bloomed));
        let glow = [view(&targets.glow[0]), view(&targets.glow[1])];
        let (inner_width, inner_height) = targets.inner;
        let full = [
            1.0 / inner_width as f32,
            1.0 / inner_height as f32,
            0.0,
            0.0,
        ];
        let half = [
            1.0 / inner_width.div_ceil(2) as f32,
            1.0 / inner_height.div_ceil(2) as f32,
            0.0,
            0.0,
        ];
        // The last pass reads the supersampled picture at four points of each output pixel
        // (all at its centre without supersampling, which leaves the picture exact).
        let spread = if targets.inner == (targets.width, targets.height) {
            0.0
        } else {
            1.0
        };
        let output = [
            full[0],
            full[1],
            spread / targets.width as f32,
            spread / targets.height as f32,
        ];
        let pass = |encoder: &mut wgpu::CommandEncoder,
                    pipeline: &wgpu::RenderPipeline,
                    target: &wgpu::TextureView,
                    source: &wgpu::TextureView,
                    extra: &wgpu::TextureView,
                    uniforms: PostUniforms| {
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("bloom uniforms"),
                contents: bytemuck::bytes_of(&uniforms),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bloom bindings"),
                layout: &self.post_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(extra),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: buffer.as_entire_binding(),
                    },
                ],
            });
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bloom"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.draw(0..3, 0..1);
        };
        let uniforms = |params: [f32; 4], texel: [f32; 4]| PostUniforms { params, texel };
        // Without bloom only the last pass runs (the glow is multiplied by nothing).
        if strength > 0.0 {
            pass(
                encoder,
                &self.bright,
                &glow[0],
                &resolved,
                &resolved,
                uniforms([0.0, 0.0, BLOOM_THRESHOLD, 0.0], full),
            );
            for spread in [1.0, 2.0] {
                pass(
                    encoder,
                    &self.blur,
                    &glow[1],
                    &glow[0],
                    &glow[0],
                    uniforms([spread, 0.0, 0.0, 0.0], half),
                );
                pass(
                    encoder,
                    &self.blur,
                    &glow[0],
                    &glow[1],
                    &glow[1],
                    uniforms([0.0, spread, 0.0, 0.0], half),
                );
            }
        }
        pass(
            encoder,
            &self.combine,
            &bloomed,
            &resolved,
            &glow[0],
            uniforms([0.0, 0.0, 0.0, strength], output),
        );
    }

    /// Put the scene's maps in the GPU array, uploading (with mip levels) only those not
    /// already there; returns each one's layer. Layers are reused least recently used first.
    fn upload_maps(&mut self, maps: &[IndexMap]) -> Vec<u32> {
        let device = &self.device;
        let faces = self.face_maps.get_or_insert_with(|| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("face maps"),
                size: wgpu::Extent3d {
                    width: MAP_SIZE,
                    height: MAP_SIZE,
                    depth_or_array_layers: MAP_LAYERS,
                },
                mip_level_count: MAP_SIZE.ilog2() + 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rg8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
            FaceMaps {
                texture,
                view,
                layers: vec![None; MAP_LAYERS as usize],
                frame: 0,
            }
        });
        faces.frame += 1;
        let frame = faces.frame;
        let mut out = Vec::with_capacity(maps.len());
        for source in maps {
            let known = (source.id != 0)
                .then(|| {
                    faces
                        .layers
                        .iter()
                        .position(|layer| layer.is_some_and(|(id, _)| id == source.id))
                })
                .flatten();
            let layer = match known {
                Some(layer) => layer,
                None => {
                    // An empty layer, else the one used longest ago.
                    let layer = (0..faces.layers.len())
                        .min_by_key(|&k| faces.layers[k].map_or(0, |(_, used)| used))
                        .unwrap_or(0);
                    upload(&self.queue, &faces.texture, layer as u32, source);
                    layer
                }
            };
            faces.layers[layer] = Some((source.id, frame));
            out.push(layer as u32);
        }
        out
    }
}

/// The face maps' GPU array and which map each layer holds: `(id, frame last used)`.
struct FaceMaps {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    layers: Vec<Option<(u64, u64)>>,
    frame: u64,
}

/// Upload one map into `layer` of the array, with its mip levels.
fn upload(queue: &wgpu::Queue, texture: &wgpu::Texture, layer: u32, source: &IndexMap) {
    let levels = MAP_SIZE.ilog2() + 1;
    let mut level = resample(source, MAP_SIZE);
    for mip in 0..levels {
        let size = MAP_SIZE >> mip;
        let bytes: Vec<u8> = level.iter().flat_map(|t| t.to_le_bytes()).collect();
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: mip,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 2),
                rows_per_image: Some(size),
            },
            wgpu::Extent3d {
                width: size,
                height: size,
                depth_or_array_layers: 1,
            },
        );
        level = halve(&level, size);
    }
}

/// A map at `size` by `size`, each texel the nearest of the source (indices cannot be blended).
fn resample(map: &IndexMap, size: u32) -> Vec<u16> {
    let (width, height) = (map.width.max(1), map.height.max(1));
    (0..size * size)
        .map(|at| {
            let (x, y) = (at % size, at / size);
            let from = (y * height / size) * width + x * width / size;
            map.texels.get(from as usize).copied().unwrap_or(0)
        })
        .collect()
}

/// The next mip level: each texel the index most of its four share (the first of them when they
/// all differ) with their mean detail.
fn halve(level: &[u16], size: u32) -> Vec<u16> {
    let half = (size / 2).max(1) as usize;
    let size = size as usize;
    let mut out = Vec::with_capacity(half * half);
    for y in 0..half {
        for x in 0..half {
            let at = |dx: usize, dy: usize| {
                level[(2 * y + dy).min(size - 1) * size + (2 * x + dx).min(size - 1)]
            };
            let four = [at(0, 0), at(1, 0), at(0, 1), at(1, 1)];
            let count = |t: u16| four.iter().filter(|o| *o & 0xff == t & 0xff).count();
            let index = four.iter().copied().max_by_key(|t| count(*t)).unwrap_or(0) & 0xff;
            let detail = four.iter().map(|t| u32::from(t >> 8)).sum::<u32>() / 4;
            out.push(index | (detail as u16) << 8);
        }
    }
    out
}

/// A line cut to the part at least `near` ahead of the eye, or `None` when none of it is.
fn clip_line([a, b]: [[f32; 3]; 2], near: f32) -> Option<[[f32; 3]; 2]> {
    match (a[2] >= near, b[2] >= near) {
        (true, true) => Some([a, b]),
        (false, false) => None,
        (front, _) => {
            let (inside, outside) = if front { (a, b) } else { (b, a) };
            let t = (inside[2] - near) / (inside[2] - outside[2]);
            let cut: [f32; 3] = std::array::from_fn(|k| inside[k] + (outside[k] - inside[k]) * t);
            Some(if front { [a, cut] } else { [cut, b] })
        }
    }
}

/// An atmosphere in units of its outer radius, or `None` when it has no thickness.
fn shell(atmosphere: &Atmosphere) -> Option<Shell> {
    let outer = atmosphere.outer;
    (outer > atmosphere.radius && atmosphere.radius > 0.0 && outer.is_finite()).then(|| Shell {
        centre: atmosphere.centre.map(|c| c / outer),
        radius: atmosphere.radius / outer,
        colour: atmosphere.colour,
        density: atmosphere.density,
        sun: atmosphere.sun,
        pad: 0.0,
    })
}

fn rgba(pixels: &[u32]) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|p| (p | 0xff00_0000).to_le_bytes())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(z: f32, colour: [f32; 4]) -> Triangle {
        Triangle {
            positions: [[-z, -z, z], [z, -z, z], [0.0, z, z]],
            colours: [colour; 3],
            texture: None,
        }
    }

    #[test]
    fn nearer_faces_win_and_the_background_shows_around_them() {
        let Ok(mut renderer) = Renderer::new() else {
            eprintln!("no GPU adapter; skipped");
            return;
        };
        let scene = Scene {
            width: 64,
            height: 48,
            background: Some(Image {
                width: 1,
                height: 1,
                pixels: vec![0x203040],
            }),
            projection: Projection {
                x_scale: 1.0,
                y_scale: 1.0,
                near: 1.0,
            },
            // Drawn far after near: the depth test, not the order, decides.
            batches: vec![Batch {
                triangles: vec![
                    triangle(10.0, [0.0, 0.0, 1.0, 1.0]),
                    triangle(5.0, [1.0, 0.0, 0.0, 1.0]),
                    triangle(20.0, [0.0, 1.0, 0.0, 1.0]),
                ],
                lines: vec![],
                atmospheres: vec![],
                sprites: vec![],
            }],
            maps: vec![],
            palettes: vec![],
            bloom: 0.0,
            supersample: 1.0,
        };
        let pixels = renderer.render(&scene).unwrap();
        assert_eq!(pixels.len(), 64 * 48);
        assert_eq!(pixels[24 * 64 + 32], 0xff0000);
        assert_eq!(pixels[0], 0x203040);
        // Behind the eye nothing is drawn.
        let behind = Scene {
            batches: vec![Batch {
                triangles: vec![triangle(-5.0, [1.0, 1.0, 1.0, 1.0])],
                lines: vec![],
                atmospheres: vec![],
                sprites: vec![],
            }],
            ..scene.clone()
        };
        let pixels = renderer.render(&behind).unwrap();
        assert!(pixels.iter().all(|&p| p == 0x203040));
        // A later batch is drawn over an earlier one whatever its depth, and a later face at the
        // same depth wins within a batch.
        let layered = Scene {
            batches: vec![
                Batch {
                    triangles: vec![triangle(5.0, [1.0, 0.0, 0.0, 1.0])],
                    lines: vec![],
                    atmospheres: vec![],
                    sprites: vec![],
                },
                Batch {
                    triangles: vec![
                        triangle(50.0, [0.0, 1.0, 0.0, 1.0]),
                        triangle(50.0, [0.0, 0.0, 1.0, 1.0]),
                    ],
                    lines: vec![],
                    atmospheres: vec![],
                    sprites: vec![],
                },
            ],
            ..scene
        };
        let pixels = renderer.render(&layered.clone()).unwrap();
        assert_eq!(pixels[24 * 64 + 32], 0x0000ff);
        // Drawn twice the size and filtered down: the same picture size and colours.
        let finer = renderer
            .render(&Scene {
                supersample: 2.0,
                ..layered.clone()
            })
            .unwrap();
        assert_eq!(finer.len(), 64 * 48);
        assert_eq!(finer[24 * 64 + 32], 0x0000ff);
        assert_eq!(finer[0], 0x203040);
        // A textured face: the left half of its map is index 1 (red in its palette) and the
        // right index 0 (a hole); the detail doubles the right-hand red column's brightness...
        // which red cannot exceed, so it stays red.
        let mut texels = vec![128u16 << 8; 4 * 4];
        for row in texels.chunks_mut(4) {
            row[0] = 1 | 128 << 8;
            row[1] = 1 | 255 << 8;
        }
        let mut palette: Palette = [[0.0; 4]; PALETTE_SIZE];
        palette[1] = [1.0, 0.0, 0.0, 1.0];
        // White corners show the texture as it is.
        let mut face = triangle(5.0, [1.0, 1.0, 1.0, 1.0]);
        face.texture = Some(FaceTexture {
            map: 0,
            palette: 0,
            uvs: [[0.0, 1.0], [1.0, 1.0], [0.5, 0.0]],
        });
        let textured = Scene {
            batches: vec![Batch {
                triangles: vec![face],
                ..Batch::default()
            }],
            maps: vec![IndexMap {
                id: 0,
                width: 4,
                height: 4,
                texels: texels.into(),
            }],
            palettes: vec![palette],
            ..layered
        };
        let pixels = renderer.render(&textured).unwrap();
        assert_eq!(pixels[30 * 64 + 26], 0xff0000);
        assert_eq!(pixels[30 * 64 + 38], 0x203040);
        // An atmosphere round a planet straight ahead, lit from behind the eye: the haze shows
        // just outside the planet's edge, not far away from it.
        let hazy = Scene {
            batches: vec![Batch {
                atmospheres: vec![Atmosphere {
                    centre: [0.0, 0.0, 100.0],
                    radius: 20.0,
                    outer: 26.0,
                    colour: [0.4, 0.6, 1.0],
                    sun: [0.0, 0.0, -1.0],
                    density: 1.0,
                }],
                ..Batch::default()
            }],
            maps: vec![],
            palettes: vec![],
            bloom: 0.0,
            supersample: 1.0,
            ..textured
        };
        let pixels = renderer.render(&hazy).unwrap();
        let blue = |p: u32| p & 0xff;
        // The planet's edge is at 20/100 of the half-width (6.4 pixels from the centre).
        assert!(blue(pixels[24 * 64 + 32 + 7]) > 0x60, "haze at the limb");
        assert_eq!(
            pixels[2 * 64 + 2],
            0x203040,
            "clear sky far from the planet"
        );
        // Two lights: one in front of a face shows, one behind it is hidden by it.
        let light = |position: [f32; 3]| Sprite {
            position,
            size: 6.0,
            colour: [1.0, 1.0, 0.0],
            halo_size: 0.0,
            halo: [0.0; 3],
        };
        let lit = Scene {
            batches: vec![Batch {
                triangles: vec![triangle(10.0, [0.0, 0.0, 1.0, 1.0])],
                sprites: vec![light([0.0, 0.0, 5.0]), light([5.0, 0.0, 20.0])],
                ..Batch::default()
            }],
            ..hazy
        };
        let pixels = renderer.render(&lit.clone()).unwrap();
        // Yellow light added to the blue face behind it.
        assert_eq!(pixels[24 * 64 + 32], 0xffffff, "the near light");
        // A five-pixel line across the picture, one end behind the eye (cut at the near plane).
        let ruled = Scene {
            batches: vec![Batch {
                lines: vec![Line {
                    positions: [[-10.0, 0.0, 10.0], [30.0, 0.0, -10.0]],
                    colour: [1.0, 0.0, 1.0, 1.0],
                    width: 5.0,
                }],
                ..Batch::default()
            }],
            ..lit.clone()
        };
        let line = renderer.render(&ruled).unwrap();
        assert_eq!(line[24 * 64 + 4], 0xff00ff, "on the line");
        assert_eq!(line[20 * 64 + 4], 0x203040, "four pixels off it");
        // Bloom: a white face glows onto the dark round it only when bloom is on.
        let bright = Scene {
            background: None,
            batches: vec![Batch {
                triangles: vec![Triangle {
                    positions: [
                        [-10.0, -10.0, 100.0],
                        [10.0, -10.0, 100.0],
                        [0.0, 10.0, 100.0],
                    ],
                    colours: [[1.0; 4]; 3],
                    texture: None,
                }],
                ..Batch::default()
            }],
            ..ruled
        };
        let plain = renderer.render(&bright).unwrap();
        let glowing = renderer
            .render(&Scene {
                bloom: 1.0,
                ..bright
            })
            .unwrap();
        // Six pixels above the centre, past the small triangle's apex (2.4 pixels up).
        let near = 18 * 64 + 32;
        assert_eq!(plain[near], 0, "no glow without bloom");
        assert!(glowing[near] & 0xff > 0, "glow round a bright face");
        assert_eq!(
            glowing[24 * 64 + 32],
            0xffffff,
            "the face itself stays white"
        );
        // The far light lands 5/20 of the half-width (8 pixels) right of centre, inside the face.
        assert_eq!(
            pixels[24 * 64 + 40],
            0x0000ff,
            "the far light is behind the face"
        );
    }
}
