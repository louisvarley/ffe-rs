# Polygon and texture fills in the game path

Frames from the playable `Screen` (`cargo test -p ffe-rs --test planet_view --
--ignored` / `--test effects_view`, with `FFE_WORLD_CACHE` set to a v27 cache and
`FFE_SHOT_DIR` to the output directory). The source is the F1560/F1559/F1613
span fill (`docs/notes/fills.md`) and the F1644/F1647 texture rasteriser
(`docs/notes/textures.md`); no frame is hand-painted.

| File | Test | What it shows |
|---|---|---|
| `surface-approach-mars.png` | `planet_view::planet_approach_fills_the_disc` | Mars at 2 radii: F1806/F1808 explicit-coordinate textures from bitmap 0x5e with the emitter's land/sea slots. The test counts 27 background pixels of 27,516 inside the disc (surface colour 0 is legitimate). |
| `planet-445-explicit-texture.png` | `planet_view::planet_models_draw_triangles` | Model 445 through the F6 viewer (216 shapes at that camera), textured rather than flat-shaded. `planet_view::earth_shape_kinds` independently renders 445 at 1.2-3.3 radii and checks every textured shape there is `uv: Some` with bitmap 0x5e (318-1,030 entries). |
| `starport-solid.png` | `planet_view::surface_frame_shows_a_starport` | London's starport (model 0x54) at 500 km: 50,191 changed pixels, longest horizontal run 320, i.e. solid F1604/F1607 faces over the planet, not an outline. |
| `explosion-debris.png` | `effects_view::hit_particle_explosion_and_frames` | The F942/F926 kill frame. The visible blue line cloud is model 153's two-point face records (F926 debris pods). The explosion object itself is model 155; `effects_view::explosion_fragment_polygons_fill` renders its child fragment 320 and fills its F1721 polygons with `fill_polygon`. |

Planet 445 and Mars are the two terrain texture forms: descriptor
(F1805/F1807, bitmaps 0x40/0x121) and explicit-coordinate (F1806/F1808, bitmap
0x5e, slot table at the texture dword's high word offset).
