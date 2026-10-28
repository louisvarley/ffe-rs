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

/// A texture, `0xAARRGGBB` words row after row; texels with alpha below a half are holes. Every
/// texture of a scene is resampled to a [`TEXTURE_SIZE`] square layer if it is not one already
/// (coordinates are 0..1 either way).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Texture {
    /// Equal non-zero ids promise equal pixels, so a texture already on the GPU is not
    /// uploaded again; 0 uploads it every time.
    pub id: u64,
    pub width: u32,
    pub height: u32,
    /// Shared, so a caller's cached texture is handed over without a copy.
    pub pixels: std::sync::Arc<[u32]>,
}

/// The side of the GPU's texture layers.
pub const TEXTURE_SIZE: u32 = 256;
/// The textures kept on the GPU at once (about 90 MB with their mip levels).
pub const TEXTURE_LAYERS: u32 = 256;

/// A triangle in view space, its corners' colours (RGBA, 0..1), and optionally a texture of the
/// scene's [`Scene::textures`] with the corners' coordinates in it (0..1), which replaces the
/// colours. Textures are mapped with perspective correction and smooth filtering.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Triangle {
    pub positions: [[f32; 3]; 3],
    pub colours: [[f32; 4]; 3],
    pub texture: Option<(u32, [[f32; 2]; 3])>,
}

/// A line in view space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub positions: [[f32; 3]; 2],
    pub colour: [f32; 4],
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
    pub textures: Vec<Texture>,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    colour: [f32; 4],
    /// The primitive's place in its batch, which pulls it very slightly nearer.
    layer: f32,
    uv: [f32; 2],
    /// The texture layer, or -1 for the colour.
    texture: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    scale: [f32; 4],
}

const SAMPLES: u32 = 4;
/// The most primitives of a batch that are told apart by their order.
const LAYERS: usize = 4095;
/// BGRA, so a texel's bytes are the little-endian `0xAARRGGBB` word callers use.
const COLOUR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const SHADER: &str = r#"
struct Uniforms { scale: vec4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) colour: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) texture: i32,
};

@group(0) @binding(3) var face_textures: texture_2d_array<f32>;
@group(0) @binding(4) var face_sampler: sampler;

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
) -> Out {
    var out: Out;
    out.position = vec4<f32>(p.x * u.scale.x, p.y * u.scale.y, u.scale.z * (1.0 + layer * 4.8e-7), p.z);
    out.colour = c;
    out.uv = uv;
    out.texture = i32(texture);
    return out;
}

@fragment
fn fs_scene(i: Out) -> @location(0) vec4<f32> {
    if (i.texture < 0) {
        return i.colour;
    }
    let texel = textureSample(face_textures, face_sampler, i.uv, i.texture);
    // A hole in a masked texture shows what lies behind.
    if (texel.a < 0.5) {
        discard;
    }
    return vec4<f32>(texel.rgb, 1.0);
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
    colour: wgpu::Texture,
    resolved: wgpu::Texture,
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
    shells: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    face_sampler: wgpu::Sampler,
    /// The GPU array the scenes' textures are kept in between frames.
    face_textures: Option<FaceTextures>,
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
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
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
                4 => Float32
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
            format: COLOUR_FORMAT,
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
        let lines = scene_pipeline(wgpu::PrimitiveTopology::LineList, "lines");
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
        // Textures repeat inside a face's own layer only and are smoothed between mip levels;
        // anisotropic filtering keeps faces seen edge-on sharp.
        let face_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("face sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });
        Ok(Self {
            device,
            queue,
            layout,
            triangles,
            lines,
            background,
            shells,
            sampler,
            face_sampler,
            face_textures: None,
            targets: None,
            adapter: name,
        })
    }

    fn targets(&mut self, width: u32, height: u32) -> &Targets {
        if self
            .targets
            .as_ref()
            .is_none_or(|t| t.width != width || t.height != height)
        {
            let texture = |samples: u32, format, usage, label| {
                self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width,
                        height,
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
            let padded_row = (width * 4).div_ceil(256) * 256;
            self.targets = Some(Targets {
                width,
                height,
                colour: texture(
                    SAMPLES,
                    COLOUR_FORMAT,
                    wgpu::TextureUsages::RENDER_ATTACHMENT,
                    "colour",
                ),
                resolved: texture(
                    1,
                    COLOUR_FORMAT,
                    wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    "resolved",
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
        self.targets(width, height);
        let layers = self.upload_textures(&scene.textures);
        let face_view = &self.face_textures.as_ref().unwrap().view;
        let device = &self.device;
        let targets = self.targets.as_ref().unwrap();
        let uniforms = Uniforms {
            scale: [
                scene.projection.x_scale,
                scene.projection.y_scale,
                scene.projection.near,
                0.0,
            ],
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
                    resource: wgpu::BindingResource::Sampler(&self.face_sampler),
                },
            ],
        });
        // Every batch's triangles, then its lines, in one buffer.
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut ranges = Vec::with_capacity(scene.batches.len());
        let mut shells: Vec<Shell> = Vec::new();
        let mut shell_ranges = Vec::with_capacity(scene.batches.len());
        for batch in &scene.batches {
            let first = vertices.len() as u32;
            for (index, t) in batch.triangles.iter().enumerate() {
                let layer = index.min(LAYERS) as f32;
                let (texture, uvs) = match t.texture {
                    Some((texture, uvs)) if (texture as usize) < layers.len() => {
                        (layers[texture as usize] as f32, uvs)
                    }
                    _ => (-1.0, [[0.0; 2]; 3]),
                };
                vertices.extend((0..3).map(|k| Vertex {
                    position: t.positions[k],
                    colour: t.colours[k],
                    layer,
                    uv: uvs[k],
                    texture,
                }));
            }
            let middle = vertices.len() as u32;
            let layer = batch.triangles.len().min(LAYERS) as f32;
            for l in &batch.lines {
                vertices.extend((0..2).map(|k| Vertex {
                    position: l.positions[k],
                    colour: l.colour,
                    layer,
                    uv: [0.0; 2],
                    texture: -1.0,
                }));
            }
            ranges.push((first..middle, middle..vertices.len() as u32));
            let first_shell = shells.len() as u32;
            shells.extend(batch.atmospheres.iter().filter_map(shell));
            shell_ranges.push(first_shell..shells.len() as u32);
        }
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
            if let Some(buffer) = &vertex_buffer {
                pass.set_vertex_buffer(0, buffer.slice(..));
                if !triangles.is_empty() {
                    pass.set_pipeline(&self.triangles);
                    pass.draw(triangles.clone(), 0..1);
                }
                if !lines.is_empty() {
                    pass.set_pipeline(&self.lines);
                    pass.draw(lines.clone(), 0..1);
                }
            }
            let atmospheres = &shell_ranges[pass_index - 1];
            if let (Some(buffer), false) = (&shell_buffer, atmospheres.is_empty()) {
                pass.set_pipeline(&self.shells);
                pass.set_vertex_buffer(0, buffer.slice(..));
                pass.draw(0..3, atmospheres.clone());
            }
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &targets.resolved,
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
    /// Put the scene's textures in a GPU array with their mip levels, unless they are the
    /// previous scene's.
    /// Put the scene's textures in the GPU array, uploading (with mip levels) only those not
    /// already there; returns each one's layer. Layers are reused least recently used first.
    fn upload_textures(&mut self, textures: &[Texture]) -> Vec<u32> {
        let device = &self.device;
        let faces = self.face_textures.get_or_insert_with(|| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("face textures"),
                size: wgpu::Extent3d {
                    width: TEXTURE_SIZE,
                    height: TEXTURE_SIZE,
                    depth_or_array_layers: TEXTURE_LAYERS,
                },
                mip_level_count: TEXTURE_SIZE.ilog2() + 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: COLOUR_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
            FaceTextures {
                texture,
                view,
                layers: vec![None; TEXTURE_LAYERS as usize],
                frame: 0,
            }
        });
        faces.frame += 1;
        let frame = faces.frame;
        let mut out = Vec::with_capacity(textures.len());
        for source in textures {
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

/// The face textures' GPU array and which texture each layer holds: `(id, frame last used)`.
struct FaceTextures {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    layers: Vec<Option<(u64, u64)>>,
    frame: u64,
}

/// Upload one texture into `layer` of the array, with its mip levels.
fn upload(queue: &wgpu::Queue, texture: &wgpu::Texture, layer: u32, source: &Texture) {
    let levels = TEXTURE_SIZE.ilog2() + 1;
    let mut level = resample(source, TEXTURE_SIZE);
    for mip in 0..levels {
        let size = TEXTURE_SIZE >> mip;
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
            &level,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size * 4),
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

/// A texture as BGRA bytes at `size` by `size` (nearest texel when it is another size).
fn resample(texture: &Texture, size: u32) -> Vec<u8> {
    let (width, height) = (texture.width.max(1), texture.height.max(1));
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let at = (y * height / size) * width + x * width / size;
            let p = texture.pixels.get(at as usize).copied().unwrap_or(0);
            out.extend_from_slice(&p.to_le_bytes());
        }
    }
    out
}

/// The next mip level: each texel the mean of four. Colour is averaged over the opaque texels
/// only, so a masked texture's holes do not darken its edges.
fn halve(level: &[u8], size: u32) -> Vec<u8> {
    let half = (size / 2).max(1);
    let size = size as usize;
    let mut out = Vec::with_capacity((half * half * 4) as usize);
    for y in 0..half as usize {
        for x in 0..half as usize {
            let mut sum = [0u32; 4];
            let mut opaque = 0;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let at = (((2 * y + dy).min(size - 1)) * size + (2 * x + dx).min(size - 1)) * 4;
                let texel = &level[at..at + 4];
                sum[3] += u32::from(texel[3]);
                if texel[3] >= 128 {
                    opaque += 1;
                    for c in 0..3 {
                        sum[c] += u32::from(texel[c]);
                    }
                }
            }
            for total in &sum[..3] {
                out.push(total.checked_div(opaque).unwrap_or(0) as u8);
            }
            out.push((sum[3] / 4) as u8);
        }
    }
    out
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
            }],
            textures: vec![],
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
                },
                Batch {
                    triangles: vec![
                        triangle(50.0, [0.0, 1.0, 0.0, 1.0]),
                        triangle(50.0, [0.0, 0.0, 1.0, 1.0]),
                    ],
                    lines: vec![],
                    atmospheres: vec![],
                },
            ],
            ..scene
        };
        let pixels = renderer.render(&layered.clone()).unwrap();
        assert_eq!(pixels[24 * 64 + 32], 0x0000ff);
        // A textured face: the left half of its texture is red and the right a hole.
        let mut texture = vec![0x00000000u32; 4 * 4];
        for row in texture.chunks_mut(4) {
            row[..2].fill(0xffff0000);
        }
        let mut face = triangle(5.0, [0.0, 1.0, 0.0, 1.0]);
        face.texture = Some((0, [[0.0, 1.0], [1.0, 1.0], [0.5, 0.0]]));
        let textured = Scene {
            batches: vec![Batch {
                triangles: vec![face],
                lines: vec![],
                atmospheres: vec![],
            }],
            textures: vec![Texture {
                id: 0,
                width: 4,
                height: 4,
                pixels: texture.into(),
            }],
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
            textures: vec![],
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
    }
}
