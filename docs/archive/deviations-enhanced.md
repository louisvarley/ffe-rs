# Deviations: Enhanced presentation (archived)

> Archived from `docs/DEVIATIONS.md` section 3 on 2026-11-11. These rows describe
> the Modern GPU engine and Enhanced presentation, removed with the Classic-only
> split on 2026-11-10. Kept as history.

## 3. Presentation — Enhanced (opt-in, off by default)

> **Removed in this Classic-only checkout (2026-11-10).** The Modern engine that
> implemented the rows below, its Classic/Modern switch (ENH-021) and the
> `assets.bin`/FFED3D art paths were deleted. The rows are kept as the original
> project's record; none of them is present or planned here.

All rows are **presentation-only**, off by default, switchable at runtime, and never feed the simulation. They are **not** deviations when off; they are documented here because they intentionally change the look when on. The master button covers all of these except where noted.

| ID | Feature | Status | Dependencies / notes |
|---|---|---|---|
| ENH-021 | Modern graphics engine switch (`modern::Graphics`): Classic / Modern on Port Options and Ctrl+G, persisted as `graphics=` | implemented | Presentation only: the engine reads the finished 320x200 frame and never touches game state; Classic is the default and unchanged. First technique: edge-directed 4x upscaling (Scale2x twice) and a smooth fit. See `docs/notes/modern-graphics.md`. |
| ENH-001 | HD textures (`upscaled.bin`, per-asset fallback) | model and planet textures implemented from the owner's `assets` (packed into `assets.bin` by the build) | Modern with a GPU: an upscaled `assets/images/<category>/<name>.png` is turned back into the bitmap's colour indices (nearest colour of the `indexed/` key among the indices round the same texel) with a brightness detail, and every face colours it from its own slots, as the original does; files are read again when they change. A missing or unchanged file falls back to the original bitmap. Classic ignores it. See `docs/notes/modern-graphics.md`. |
| ENH-002 | Native-resolution rendering | implemented for the flight view's models | Modern graphics with a GPU: the executor's faces and the planet surfaces' triangles (recorded in view space beside F1840's queue, which is unchanged) are drawn by `ffe-render` (wgpu) at the window's view size with the original's projection, over the original's frame, under its HUD. Stars and dots stay the original's pixels for now. In Modern the planets' surfaces are subdivided one level finer than the "Level of Shape Detail" setting (presentation only), and faces are shaded smoothly across shared corners where the original's flat lighting steps (same texture or hue, similar normals). See `docs/notes/modern-graphics.md`. |
| ENH-003 | Smooth texture filtering | implemented for model textures | Perspective-correct, mipmapped; textures enlarged with Scale2x first. |
| ENH-004 | Anisotropic filtering | implemented for model textures (8x) | smooth-filtering |
| ENH-005 | MSAA | implemented (4x) for the GPU 3-D view, with 1.5x supersampling on top | native-res |
| ENH-006 | TAA | proposed | native-res |
| ENH-007 | Sharp scalable HUD/text | sights and main-font text implemented (Modern) | Text in the main font is drawn with a real TrueType/OpenType font at the window's resolution (the owner's `assets/fonts`, else Windows' Bahnschrift or Segoe UI; else the owner's upscaled atlas), each glyph in the original's place: its baseline, capitals 6.5 of the original's 7 rows, condensed evenly to the original's advances, centred in its own; h⁻¹/s⁻¹ as a letter and a raised "-1", the arrows and box the original's. The fixed gunsight (bitmap 97), the velocity marker (98), the targetter's cursor and the missile-lock circle are drawn as anti-aliased lines at the window's resolution, in the bitmaps' own design and colours, where the original's pixels are let through to the 3-D view. Same places, same rules for when they show; Classic unchanged. See `docs/notes/modern-graphics.md`. Must match Classic metrics when off. |
| ENH-008 | HDR + tone mapping | proposed | native-res |
| ENH-009 | HDR display output | proposed | HDR + capable surface/OS |
| ENH-010 | Bloom/lens effects | bloom, star glow, glare and lens flare implemented (Modern with a GPU) | The GPU picture's brightest parts (largest channel over 0.8) are blurred at half size and added back at 0.6; a star in view gets a halo sprite eight times its radius in its colour, drawn before the star. The 3-D view is drawn in floating point (colours up to white are shown exactly; brighter ones run towards white), and the star adds a white-hot core, a wide glare and four lens-flare ghosts, dimmed by how much of it the planets hide; the core lies behind ships and stations (which hide it, the bloom spreading round their edges) and the glare and ghosts dim by how much of the disc they cover. See `docs/notes/modern-graphics.md`. |
| ENH-011 | Shadows (incl. ship self-shadowing) | planet eclipses implemented (Modern with a GPU); self-shadowing proposed | An object (station, ship) with a body between it and the star keeps only the share of the star's disc it still sees (umbra, penumbra, annular), down to 30 % of its light in full shadow. The original shadows nothing. See `docs/notes/modern-graphics.md`. |
| ENH-012 | Ambient occlusion | proposed | native-res |
| ENH-013 | Atmospheric scattering + clouds | atmosphere implemented (no clouds) | Modern graphics with a GPU: each planet with an F1877 haze record gets a shell from the surface to the ring's radius (`radius * (1 + word8 / 32768)`), traced per pixel in the record's haze colour and lit by the star, in place of the ring's dithered quads (which are set aside in Modern only). See `docs/notes/modern-graphics.md`. |
| ENH-014 | Higher-quality starfield/nebulae | generated sky implemented (Modern with a GPU) | In open space the GPU generates the sky for each pixel's direction: a Milky Way band along the original nebula band's plane with nebula clouds and dust lanes, and three layers of stars, over the player's space colour. One fixed seed for the whole galaxy, never the game's random numbers. It replaces the original's star points and nebula model (0x13b) in Modern only; "Background stars" off leaves the stars out, and inside a planet's haze the original's sky stays. See `docs/notes/modern-graphics.md`. |
| ENH-015 | Engine glows/particles | lights, engine glows, particle chains and glowing beams/explosions implemented | The models' dots (F1699 to F1801: lights with a halo and a core) are drawn as soft round GPU sprites at their view positions, hidden by nearer faces of their object; spawn timing and sizes stay the core's. F1805's engine glows (quads over bitmaps 0x123/0x65) are drawn as animated GPU flames from the nozzle along the plume (sized from the original's quad, in the glow's own colours; the thruster entry now also keeps A's and B's view positions); thin plumes (F1803 lines) as GPU lines. F1814's particle chains are GPU glows at their view positions (the chain entry now also keeps them), and beams, explosions and a hit's sparks give off light: their saturated colours run past white (grey debris stays grey) and the bloom spreads them. An explosion's coloured faces, complex polygons and textured fire clouds (bitmaps 117, 119, 126, 219) are animated GPU fireballs, or rounded streaks along long faces, drawn over what lies behind them rather than added to it. Space dust is the GPU's speed dust instead of the original's clouds (model 0x6d): motes fixed all round the ship in layers matched to its speed, drawn as streaks rushing out from the way ahead (presentation only, its own hash; see `docs/notes/dust.md`). Complex polygons (opcode 5) are lifted onto their plane from their resolved vertices and filled on the GPU. |
| ENH-016 | Material maps | proposed | native-res, smooth filtering, HD/map pack |
| ENH-017 | Widescreen/ultrawide **presentation** (letterbox/pillarbox, HUD anchoring, no frustum change) | proposed | Does not reveal more of the scene. |
| ENH-018 | Frame interpolation | proposed | Continuous transforms only; no event interpolation. |
| ENH-019 | Master button (all presentation-only on/off) | proposed | Never touches §4. |
| ENH-020 | Reset to Classic | proposed | Master off + selection cleared. |
| ENH-022 | Animated water and inland drainage | Modern water implemented, inland drainage in preview | The GPU renders waves, surf, tides, and sun and moon glints. Streamed Modern terrain draws cached regional river ribbons. Lake fill is a preview on the Classic-derived terrain path. See the [Modern worlds roadmap] (`MODERN_WORLDS.md`, removed with the Modern engine). |
| ENH-023 | Procedural woodland, shrubs, succulents and rock scatter | Modern presentation implemented | Planet-fixed woodland patches select and place generated props on the rendered terrain. Mesh detail and density fall with distance. These props are visual scenery. |
| ENH-024 | Procedural building lights | Modern presentation implemented | Outdoor starport models get facade windows that fade in around sunset and red warning beacons on tall roofs. Authored emission maps take precedence over procedural windows. |
| ENH-025 | Banded planetary rings | Modern presentation implemented | The original ring selection and plane drive a translucent radial profile with mutual shadows between the rings and the globe. See the [ring implementation notes](../notes/rings.md). |
| ENH-026 | Direct facility rendering | Modern presentation implemented | Live station, lift and starport views use FFED3D parts and procedural fallbacks positioned from original gameplay data. Missing hulls and settlements use approximate silhouettes. See the [facility coverage] (`notes/modern-stations.md`, removed with the Modern engine). |

Every implemented ENH row must have an “off equals Classic” test and an explicit
capability/error path. None may change the simulation. Live switching and
image-only packs are approved; the advanced effects listed here are deferred
research, not a requirement to implement them all in the first slice.

---

