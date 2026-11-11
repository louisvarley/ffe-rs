# CLAUDE.MD sections 4-5: renderer and graphics packs (archived, out of scope)

> Archived from `CLAUDE.MD` on 2026-11-11. This checkout is Classic only; the
> modern renderer, graphics packs, upscaling and Enhanced options below are the
> original project's plan and are out of scope here. The audio decision that
> followed them stays in `CLAUDE.MD`.

## 4. Modern renderer: difficulty and recommended approach

Use a new Rust renderer, provisionally **wgpu + winit**, rather than porting D3D9
state calls one-for-one. Verify the chosen versions/backends against target
hardware before committing. This is a recommendation, not an installed stack.

| Work | Relative difficulty | Why |
|---|---|---|
| Window, GPU setup, static textured-mesh demo | Low to medium | Established libraries; little game compatibility work. |
| Original model interpreter and faithful HUD/UI | High | Models are command streams with dynamic state, not just mesh files. |
| Native-resolution original geometry | Medium after the interpreter works | Projection, clipping, depth, sprites, text, and large scales need coherent handling. |
| HD image replacements and filtering | Low to medium after the asset registry | Identity, alpha, UV/layout and memory use matter more than image loading. |
| MSAA and modest bloom | Medium after the GPU pass structure | Capability checks, render-target formats, resolve/composition, and settings. |
| Planet surfaces, atmosphere, robust shadows | High | Procedural detail, horizon/LOD transitions, precision and extreme scene scale. |
| TAA, advanced lighting, HDR display output | High; defer | Motion/history, disocclusion, backend/OS support and new material requirements. |
| Exact original rasterization | High | GPU edge rules, affine mapping, palettes and original draw order differ. |

Keep simulation state in its original compatible numeric representations. Convert
only presentation snapshots into camera-relative GPU coordinates. Use local frames
and evaluate reversed-Z/depth partitioning with ship-to-planet precision tests;
do not convert the whole universe to global `f32` coordinates.

Start with one faithful visual path and one small enhanced vertical slice:

- For exact classic pixels, an original-style CPU indexed framebuffer presented
  through wgpu is a simpler first reference than demanding GPU pixel equivalence.
- For Enhanced, emit backend-neutral geometry/material/sprite/UI data and render
  at native resolution. Share semantic interpretation, not necessarily rasterizers.
- A low-resolution Classic output can remain available; exact DOS visual fidelity
  is required, with original 4:3 display correction and original palette/font/UI.
- Rendering must never mutate simulation, consume its RNG, or determine gameplay
  collisions, docking, targeting, or ship dimensions.

The challenging part is faithful behavior and geometry extraction, not obtaining
anti-aliasing from a modern API. This is a substantial multi-stage game-engine
project, not a short renderer swap. Estimate delivery dates only after the first
reference/importer/flight slice establishes throughput.

## 5. Graphics packs and options

### Content layering

Selected design (not implemented yet):

1. The Windows executable resolves `game/` beside itself (or an explicit override),
   not relative to an unpredictable working directory. On first use, generate
   `data.bin` locally through the same conversion library as the tools.
   Record input hashes, importer/profile version, source asset IDs and checksums.
   Refuse unsupported source layouts; never modify the player's installation.
2. Reuse a validated current cache. Rebuild stale caches to a temporary file and
   atomically replace only after validation. Retain `game/` as the normal source;
   cache-only use is allowed after conversion. If an existing `game/` changes,
   detect staleness before loading incompatible content. Explain cache-only mode.
3. Export eligible displayed images plus size guidance for the owner's local
   upscaler; never require the owner to write an export script or infer all sizes.
4. Validate returned images and build a separate `upscaled.bin`, never edit
   `data.bin` to install HD art. Use deterministic ZIP containers with JSON
   manifests initially, reusing the existing design rather than inventing a format.
5. Resolve stable asset IDs through `base -> selected graphics pack`.
   Missing overrides deliberately use the original asset; invalid overrides
   produce explicit diagnostics. Additional multi-pack ordering can wait.

The pack manifest should include schema/pack version, compatible game/base asset
fingerprints, stable source asset IDs, paths, source/output dimensions, asset kind,
color space, alpha mode, optional material maps, author/license and provenance.
Resolve IDs through the importer registry, never arbitrary executable addresses.
Keep pack paths portable and validate traversal, size/resource limits, duplicate
IDs, checksums, decoded dimensions, and unsupported formats.

Graphics packs may override appearance, not gameplay tables, physics, collision,
missions, saves, or RNG. Future gameplay mods must be a separate, clearly labeled
system with their own compatibility rules. Do not allow executable scripts or
arbitrary shader code in the first graphics-pack format.

### Owner-operated upscale export and 4K sizing

The exporter will produce `to-be-upscaled/images/`, `manifest.json`,
`sizes.csv`, an offline `index.html` contact sheet, and `excluded.json`.
The owner returns images under `upscaled/images/` using the same filenames.
These files are local outputs, not repository assets. Proposed tool commands
will be documented as runnable only once implemented.

Each CSV/manifest row must include asset ID, source filename/hash, category,
original pixel dimensions, logical rectangle/atlas region, alpha/mask mode,
recommended output width/height, scale, intended display size, sizing basis,
and any cap or manual-review warning. Export every eligible image exactly once,
record aliases, and list exclusions/unresolved assets rather than claiming
"all assets" after exporting only external BMPs.

**4K is a display target, not a requirement to make every image 4096 x 4096.**
Use 3840 x 2160 as the first export profile, with the Classic 4:3 canvas displayed
at 2880 x 2160. For an image of `w x h` intended to cover `W x H` screen pixels,
choose an aspect-preserving integer scale
`ceil(max(W / w, H / h))`, then optionally round up to a supported upscale factor.
Store the actual dimensions, not just a profile name.

- A full 320 x 200 canvas has a limiting scale of 10.8 at that display size:
  at least 11x for both axes, or a convenient 12x workflow (4x then 3x).
  A 320 x 158 cockpit image would be 3840 x 1896 at 12x, then composed using
  its original logical rectangle and display-aspect correction, not stretched
  to fill the entire UHD screen.
- A 64 x 64 texture occupying 256 x 256 pixels in a chosen reference view needs
  only 4x. Different views/distances can require more; no single upscale factor
  guarantees every close-up.
- Use measured screen coverage once representative scenes exist. Until then mark
  repeating/projected textures as estimates, allow per-asset overrides, and
  explicitly report adapter/VRAM caps. Full-frame art must not dictate the scale
  of all small textures.
- Preserve alpha, original anchors, UI metrics, frame alignment and atlas regions.
  Strict pack validation rejects incorrect sizes by default. Offer an explicit
  resize step with a report; do not silently resample finished owner artwork.
- Generate mips during pack/runtime processing where applicable. Record estimated
  decoded VRAM including mips; compressed PNG size does not measure GPU memory.

### What AI upscaling can and cannot do

- Upscale displayed color textures, cockpit/background art and suitable sprites.
  Preserve logical dimensions, UVs, pivots, layout, masks and intended transparency.
- Handle atlas regions and mip borders deliberately; validate halos, seams and
  palette-index transparency. Specify sRGB color vs linear data textures.
- Font glyphs and HUD text need separate review; more pixels do not guarantee
  legibility, faithful glyphs, or correct metrics.
- Do not blindly upscale palettes, lookup tables, collision data, procedural
  seeds, model commands, or simulation inputs.
- Many planets/surfaces and model details are procedural or geometric. Image
  upscaling cannot create improved silhouettes, detailed geometry, reliable UVs,
  or physically meaningful roughness/normal maps automatically.
- Plan optional glTF/GLB mesh replacements later, with explicit scale, origin,
  axes, animations/attachments and stable model IDs. Preserve original gameplay
  collision and attachment semantics. Importing existing `.X` packs is a separate
  compatibility task, not something wgpu provides.
- Higher-resolution images still look low-resolution if rendered only into a
  320 x 200 framebuffer. Expose texture quality and native resolution separately.

Keep conversion/export/upscaling local. Ownership of the game, AI transformation,
Git LFS, or requiring original files does not by itself authorize redistributing
derived art. Existing "rights-holder tolerance" language is not permission.
Default to private local packs until distribution rights are resolved.

### Proposed options UX

| Option | Initial design |
|---|---|
| Graphics mode | Classic / Enhanced; Classic first-run default. |
| Graphics pack | None / selected compatible pack; show version, coverage, validation errors and missing assets. |
| Hi-res textures | Off / On; capability depends on a loaded compatible pack. |
| Render resolution | Classic internal size / native / render scale. |
| Texture filtering | Nearest / linear / anisotropic levels supported by the adapter. |
| Anti-aliasing | Off / supported MSAA levels initially; optional later post-process/TAA modes. |
| Effects | Native resolution + HD images + filtering + MSAA first, then modest bloom; shadows/atmosphere later. TAA/HDR output/material maps deferred. |
| UI and accessibility | Separate HUD/text scaling and readability controls; do not silently change layout. |
| Enhanced master switch | Restore saved presentation settings or disable enhancements; never alter gameplay options. |

Use an AA mode rather than assuming every AA technique can be combined. Query
actual format/sample-count support, and explain unavailable choices. HDR scene
lighting/tone mapping and HDR monitor output are different features.

Live switching is required for Classic/Enhanced and the v1 image options; no
restart is required. Prepare resources before an atomic mode swap while simulation
continues, report loading, and keep the previous valid mode on failure.
Clear temporal history when relevant. Effects may not stall the simulation or
silently change the saved enhanced selection. "Live" does not promise zero
loading time; measure switching latency before setting a numerical target.

