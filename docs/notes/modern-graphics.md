# Modern graphics: the switch, the separation and the first technique

Status (2026-10-27): **first slice implemented.** `crates/ffe-rs/src/modern.rs`, the presenter in
`crates/ffe-rs/src/display.rs`, the Port Options rows in `prefs.rs`, `Ctrl+G` in `main.rs`. Deviation
ENH-021. Design background: `docs/RENDERING_DESIGN.md` (this is its "master button" and a first
presentation-only feature, without the wgpu backend yet).

## How the two engines are kept apart

```
game state ──► Screen (the original's drawing, unchanged) ──► 320x200 logical frame
                                                                   │
                                     ┌─────────────────────────────┴───────────────┐
                                     ▼                                             ▼
                        Classic: display::blit                   Modern: modern::ModernRenderer
                        (nearest/linear scale, 4:3)              (its own techniques, window res)
                                     └──────────────► window ◄─────────────────────┘
```

- The game always draws the original's frame, the same in both modes. Captures, the reference
  comparisons, input hit-testing (`display::logical_point`) and saves all use that frame and are the
  same in both modes.
- The modern engine receives only `&[u32]` (the finished frame) and a destination rectangle. It cannot
  reach the simulation, the RNG, the clock or input; it allocates its own buffers.
- The choice is `DisplaySettings::graphics` (`modern::Graphics::{Classic, Modern}`), checked once per
  presented frame. Flipping it is instant and needs no restart, reload or state change.
- Classic is the default (first run and any config without `graphics=`), and its output is
  byte-for-byte what the presenter produced before this change.

## Switching

- **Escape → Port Options → Graphics: Classic / Modern** (right-hand column; the icons are click
  targets like the original's, and the keyboard rows work too).
- **Ctrl+G** flips it from anywhere; a message names the new mode. (Plain G is still the galaxy map;
  with Ctrl held it is not.)
- Saved in `config.cfg` as `graphics=classic|modern` with the other display choices.

## First technique: edge-directed upscaling

The original's frame is 320x200, and every scaler so far either repeats pixels (blocky staircases on
every polygon edge, line, circle and letter) or blurs them. The modern engine instead enlarges the frame
4x with **Scale2x applied twice** (the AdvMAME rules: a pixel's corner takes a neighbour's colour only
where the two neighbours on that side agree and the other two do not), giving 1280x800. That turns
staircases into clean diagonals and rounds circles and letters, but leaves flat areas and one-pixel
details exactly as they were: no new colours and no blur. The 1280x800 image is then resampled
bilinearly into the window's fit rectangle, which also applies the DOS 4:3 stretch smoothly (Classic
repeats every fifth row).

Why first: it is presentation-only by construction, cheap (a few milliseconds a frame), needs no new
assets or GPU code, improves every screen at once (flight, HUD, menus, maps), and exercises the switch
and the separation that every later technique will plug into.

Limits: it works from the 320x200 pixels, so it cannot add detail the frame does not have. The
crosshair, the station's edges and text improve most; textures and the planet's surface mostly keep
their look. Tests: `modern::tests` (the Scale2x rules) and
`display::tests::the_engines_share_the_frame_and_only_modern_smooths_it` (same frame, frame untouched,
Classic unchanged and blocky, Modern smooth only on edges). The presentation evidence
(`--capture-display DIR`) now writes `display-modern-4x.png` beside the Classic images.

## GPU 3-D view: the models at the window's resolution

Status (2026-10-27): **implemented for the flight view's models and planets** (stations, ships, the lift, the
beam, effects). `crates/ffe-render` (the GPU renderer, no game knowledge), `crates/ffe-rs/src/modern3d.rs`
(the bridge), `Screen::draw_frame` and `model_viewer::Solid`. Deviation ENH-002/003/004/005.

This is FFED3D's idea (hand the original's model programs to the GPU instead of the software
rasteriser) done with the port's own executor and a modern API:

```
executor (unchanged) ──► Frame { shapes, solids }      solids: each face's view-space corners,
                                │                       flat colour and texture (bitmap, slot
                 Classic ◄──────┤                       colours, F1644/F1647 corner texels)
   every shape drawn into       │ Modern + GPU
   the 320x200 frame            ▼
                 Screen::draw_frame: faces set aside as one batch per object; the other shapes
                 (stars, planets, dots) drawn into the frame; shapes an object draws after its
                 faces (lights, labels) drawn after the world phase, over the 3-D view
                                │
                 modern3d::World { batches, textures, snapshot of the frame after the world }
                                ▼
   ffe-render (wgpu: Vulkan / D3D12 / Metal), offscreen at the window's view size:
     background  = the frame behind the models (Scale2x, then smooth filtering)
     each batch  = its faces with 4x MSAA, a reversed-Z 32-bit depth buffer, perspective-correct
                   textures (Scale2x-enlarged to 256, mipmapped, 8x anisotropic, masked texels
                   discarded), the original's lit flat colours
     then        = every pixel the game drew after the world phase (HUD, gunsight, scanner) on top
```

- **Same picture, finer.** The projection is the original's F1479 (`160 + 256x/z`, `79 - 256y/z`,
  nothing nearer than 64), so every face lands where Classic puts it; the GPU clips at the near plane
  itself, so faces passing the eye are cut cleanly instead of being dropped or span-clipped.
- **Order.** Objects are drawn in the original's painter order, each over the last (their view-space
  scales can differ). Within an object the depth buffer decides, and a later face at the same depth
  wins by a tiny order bias, so decals painted on a hull stay visible.
- **Separation.** The faces are copies taken while drawing. Classic never sets them aside, so its
  frame is the original's; Modern without a GPU (no adapter) keeps the faces in the frame and only
  upscales. The game state, the RNG and the clock are not reachable from `ffe-render` or `modern3d`.
  Switching (Ctrl+G, Port Options) still takes effect on the next frame.
- **Cost.** About 5 ms a frame at 1920x1440 on an RTX 3090 (`--capture-display` prints it), most of
  it the synchronous read-back into the window buffer.
- **Evidence.** `--capture-display DIR` writes `display-modern-3d.png` (the free-flight station at
  1920x1440). Tests: `ffe-render`'s depth/clipping/batch-order/texture test, and
  `modern3d::tests` (fans, texture coordinates, and the GPU picture landing where F1479 projects
  with the HUD left on top). GPU tests skip themselves on a machine without an adapter.

### Planets

Status (2026-10-27): **implemented** (planet surfaces from orbit to the ground).

The surface is the original's own: F1876 subdivides the planet for the view and F1840 queues each
triangle with screen points only, but the subdivision's nodes carry their view-space positions
(`terrain::Node` `+4..+0xc`). `terrain_emit::Queue::views`, a recording the port adds and the
original does not have, keeps each queued entry's corners in view space as the emitter writes it,
including the sea-level crossings F1837 makes (`crossing_point`, the point F1479 then projects).
Recording never changes the queue (`surface_draw_reference` checks the words and that each corner
projects to the entry's own screen point, against the executed original). `model_viewer::surface_shapes`
pairs each entry with its corners as a `Solid`: the textured entries (F1805/F1807 with the corner
table, F1806/F1808 with their own coordinates, F1840's height-band textures) carry their bitmap and
slot colours, the flat ones their colour. The planet is then one batch: its hills hide each other by
depth on the GPU, at the window's resolution, with the same textures as the original.

- The atmosphere ring (F1877/F1879) and the planet seen as a disc from far away stay the original's
  pixels, behind the GPU surface.
- Textures are remembered between frames (the screen keeps the last 256 images, the renderer a
  256-layer GPU array, uploading only new ones), so a planet's hundred or so cost about 0.4 ms each
  only when they first appear. A steady frame of a planet costs about 2.5 ms at 1280x800.
- Evidence: `tests/modern_planet.rs` (local cache and GPU) frames Earth at 3, 1.5, 1.2, 1.05 and 1
  radii, checks the GPU covers every pixel where the original drew the planet (over 99.9 %), and with
  `FFE_SHOT_DIR` writes `classic-k*.png`/`modern-k*.png`. `FFE_CAPTURE_MODERN=1` with
  `--capture-planet` also saves `planet-XXXX-modern.png` beside each shot.

### Atmosphere

Status (2026-10-28): **implemented** (ENH-013, without clouds).

F1877 draws a planet's air as a ring of 32 dithered quads from 7/8 of the radius out to
`radius * (1 + word8 / 32768)` (the record's word `+8`), coloured from the record's material to the
sky colour and lit by the star (F1879). In Modern graphics that ring is set aside (its shapes carry
an empty `Solid`) and the GPU draws the same air as a shell (`model_viewer::Shell`: centre, surface
radius, the ring's radius, the material through the palette and the light vector), traced per pixel
after the planet's batch (`ffe-render`'s `fs_shell`):

- each pixel's view ray is met with the shell and the planet's sphere (the closest approach comes
  from a cross product, not the quadratic, so a planet far away keeps its precision);
- the haze is thicker the longer and lower the ray runs through the shell, so it is strongest at the
  limb and thin over the ground, where the original's emitter already mixes the haze into the terrain's
  colours by distance (F1840); near the limb a ground ray blends into a grazing one, so the terrain's
  own outline shows no seam;
- like the original's ring, the haze is its colour all the way round and brighter on the side facing
  the star, with a little forward glow round the star; from inside the shell the sky takes the haze
  colour towards the horizon, as the original colours the sky in daylight.

`tests/modern_planet.rs` now also checks the atmosphere reaches the GPU at every altitude and that
the GPU picture covers every pixel of the original's planet and ring.

### Lights, stars, lines and curves

Status (2026-10-28): **implemented**.

- **Lights.** A model's dot (F1699 to F1801: one particle, or a halo and a core) becomes one GPU
  sprite at its view position (`model_viewer::Sprite`, the core's and halo's F1611 sizes and colours):
  a soft round core with a fainter halo, the original's sizes in the window's pixels, pulled 2 %
  nearer so a light on a hull shows, and hidden by the nearer faces of its own object. A lit point
  (drawn as one pixel by the original) is a small sprite too.
- **Stars and the nebula.** The background stars go to the GPU as points at their directions (a
  batch of their own, before everything), so they stay points at any resolution instead of enlarged
  pixels; the nebula model (0x13b) is drawn as GPU geometry like any other model.
- **Lines and curves.** The executor now also keeps a line's two ends and a curve's four points in
  view space beside the screen points it queues (`LineEmission::views`, `SplineEmission::views`;
  the original's results are unchanged). Lines are drawn as anti-aliased quads about one original
  pixel wide (`ffe-render`'s wide lines, cut at the near plane), and a curve is F1546's cubic
  through its four points, stepped in view space (33 points) instead of on the screen.

### Bloom and the star's glow

Status (2026-10-28): **implemented** (ENH-010, modest; enhanced-2).

- After the scene resolves, `ffe-render` takes the parts whose largest channel is over 0.8 (fading
  in over the last stretch) at half size, blurs them with a nine-tap Gaussian twice (the second
  pass twice as wide) and adds them back at 0.6 (`Scene::bloom`). Ordinary hulls and terrain are
  left alone; the star, lights and sunlit white glow softly.
- A star in view gets a halo sprite eight times its radius across (at least 16 original pixels) in
  its colour, drawn before the star, so the star's disc or model lies over the halo's centre
  (`tests/modern_planet.rs`, `a_star_glows_on_the_gpu`).

### The hyperspace tunnel and engine glows

Status (2026-10-28): **implemented**.

- The hyperspace tunnel (model 0xa5) goes through the same path as the flight view's models, and its
  frame is the world phase of its own (`tests/modern_planet.rs`,
  `the_hyperspace_tunnel_is_drawn_on_the_gpu`: the tunnel fills as much of the view on the GPU as in
  the original, 15,692 against 15,715 pixels in the test frame).
- An engine glow (F1805's textured quad over bitmap 0x123, the core, or 0x65, the shaft) is queued
  with screen corners only; the thruster's queue entry now also keeps A's and B's view positions
  (`model_thruster::Entry::views`, not the original's), so each corner is lifted back into view
  space at the plume's depth at that point (`model_viewer::lift`) and drawn as a textured GPU face,
  hidden by the hull where it is nearer. A thin plume (an F1803 line) is a GPU line from A to B
  (`engine_glows_are_drawn_on_the_gpu`).

### Textures as the original colours them, and the owner's upscaled art

Status (2026-10-28): **implemented** (ENH-001 for model and planet textures).

The original colours a texture per face: its bitmap holds colour indices and each face looks them
up in its own slot colours (lighting, haze, material). The GPU now does the same: each bitmap is one
index map on the GPU (`ffe_render::IndexMap`, a 512-texel layer of an `Rg8` array: the index and a
brightness detail) and each textured face carries a palette (its slots; slot 0 a hole for a masked
bitmap, indices past the slots black, as the original's lookup gives them). The fragment shader looks
up the four nearest texels in the palette and blends them, and blends two mip levels (the mips keep
the index most of each four share). A planet needs three maps on the GPU instead of about a hundred
coloured textures, and new colours cost nothing to upload.

The maps come from the owner's image export when it has a bigger picture than the original
(`modern_art`): `img-export/images/<category>/primary-NNN.png` or `secondary-NNN.png` (texture,
planet_texture, sprite or texture_block) with its `indexed/` twin. The upscaled picture has only one
texture record's colours, so each of its pixels takes the index of the `indexed/` key whose palette
colour is nearest, among the indices the original has round the same texel (two slots that share a
colour in that record stay apart, and the upscaler cannot bring in an index the bitmap never had
there; a tie keeps the texel's own index), and keeps how much brighter or darker than that colour it
was (0.5 to 2) as the detail. Every face then colours the enlarged map with its own slots. The files'
times are checked once a second, so an edited or newly upscaled picture shows within a second, in
the running game. A picture still at the export's size, or a missing file, leaves the original bitmap
(enlarged with Scale2x). Problems reading a file go to the log once per change.

### Finer planets and smooth shading

Status (2026-10-28): **implemented**.

- **Finer planets.** With the GPU view on, the flight view asks F1876 for planet surfaces one
  level finer than the "Level of Shape Detail" setting (`modern3d::TERRAIN_DETAIL`): about twice the
  triangles, finer coastlines and outlines, about 1.5 ms more a frame for a whole planet. Landing and
  collision use the separate distance query and do not change. Two levels are finer still but cost
  about 10 ms a frame; three overflow the original's node arena on a whole planet and leave holes.
- **Smooth shading.** The original lights each face flat, in coarse steps. For each face the GPU
  view now gives each corner a brightness factor that brings it to the mean of the faces meeting
  there that are the same surface lit differently: normals within 50 degrees, the same texture bitmap
  (or both untextured), the same hue, and a brightness step no bigger than lighting makes (within about
  30 %). The face keeps its own colour (or texture) and only its brightness varies across it, so
  big facets of sea or hull shade into each other while sea against land, or a stripe on a hull, keep
  their edges. Each viewer frame also now carries the light it was drawn in (`Frame::light`), passed
  to the modern engine per object (`World::lights`) for the lighting effects to come.

### Eclipses

Status (2026-10-28): **implemented** (ENH-011 for planets' shadows; the original shadows nothing).

Each viewer frame now records where the model's origin was drawn (`Frame::origin`), so the screen
knows each object's place in camera space (the origin over the objects' common scale,
`model_scale`) beside the bodies' centres and radii (`Screen::casters`, from the discs). For each
object (not the bodies themselves), `modern3d::sunlight` works out how much of the star's disc it
still sees past the bodies: none in the umbra, a straight-line share across the penumbra, and at most
the body's share of the star's area when the body looks smaller than the star (an annular eclipse).
The object's colours then keep `0.3 + 0.7 * seen` of their light. A station on a planet's night side
goes dark as it turns into the shadow (`tests/modern_planet.rs`,
`a_station_in_a_planets_shadow_is_eclipsed`).

### Brighter than white: HDR, glare and lens flare

Status (2026-10-28): **implemented**.

- The 3-D view and the bloom's buffers are now 16-bit floating point, so a light can be brighter than
  white. The last pass adds the bloom and brings the picture back to the screen's range: a colour up
  to white is shown exactly as it is (the original's colours do not change), and a colour brighter
  than white runs towards white by how far over it is, so a star's core is white whatever its tint.
- Glows (lights, stars, the star's halo) now add their light to what lies behind them.
- The star in view (`modern3d::Star`: its direction, apparent radius and colour, and how much of it
  the eye sees past the bodies, `sunlight` from the eye) gets, over everything, a core four times
  brighter than white (the bloom spreads it), a wide faint glare and four lens-flare ghosts on the
  line from the star through the middle of the view, all dimmed as a planet covers it and gone when
  it is hidden.

Not yet on the GPU (still the original's pixels behind the models): particle chains (F1814: debris
and exhaust specks), and the HUD and its gunsight.

## What comes next

1. Clouds and shadows on the planet (the rest of enhanced-3).
2. **Per-pixel lighting** from the faces' normals and the star direction instead of the original's
   flat shade, and glows/stars as sprites (ENH-014/015), then modest bloom.
3. **Presentation through a wgpu surface** instead of reading the picture back into the minifb buffer
   (removes most of the frame cost and allows the window's full resolution for the HUD too).
4. **HD images** (ENH-001) from an owner-upscaled pack, resolved by stable asset ID.

Each one is another technique inside `modern`, selected with the same switch, and Classic stays
exactly as it is.
