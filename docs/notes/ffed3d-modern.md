# Modern graphics with FFED3D's assets

Status (2026-11-04): **stage 1 done.** The launcher requires a full FFED3D
install for Modern and passes it to the game (`--ffed3d DIR`); the GPU view
shows FFED3D's `texNNN` textures on the original models' textured faces, and
the cabins are the owner's (`assets/Textures/CABIN<n>.png`) over FFED3D's.

## The owner's decision (2026-11-04)

- Modern graphics use FFED3D's models, textures and shaders: as much of
  FFED3D's rendering as possible, drawn by the port's own engine (wgpu), not
  FFED3D's Direct3D 9 code. The owner will cherry-pick other FFED3D changes
  separately.
- Modern needs a full FFED3D install. The launcher says so on its Engine row
  and refuses to start the game in Modern until its FFED3D check is complete;
  Classic never uses FFED3D.
- FFED3D's assets stay in the player's own folder (here `ffed3d/`, ignored by
  git): nothing from it is committed, converted into the base cache or
  redistributed. Only art comes from FFED3D, never its gameplay (the
  Anisotropic and HellMod changes, `shipdata_*.txt`, its Lua scripts).
- This replaces, for Modern, `CLAUDE.MD`'s "images first, meshes later" (Q11)
  and its description of FFED3D as a rendering research reference only.
- Later the same day: Modern should look very like FFED3D (models, panel, HUD,
  planets, buildings, space dust) on DirectX 12, keeping the port's stars; no
  `.fx` shader port; the owner's `assets/` overrides FFED3D file for file, and
  the build packs all of it into `assets.bin` (see "Plan").

## The sources

- **The install**: FFED3D AJ (`FFED3DAJ.exe`) in `ffed3d/`: `MODELS/` (168
  DirectX `.X` files: 155 MSZIP-compressed binary `bzip0032`, 12 text, 1
  binary; 146 numbered replacement models `MODELS/<n>/model.x` with PNG skins,
  `skins.ini`, `tris.ini` and some `effect.fx`), `Textures/` (571 `texNNN`
  PNGs, the six skybox faces, `stars.tga`, `CABIN0..5.png`, 30 atmosphere
  images), `Shaders/` (`ships.fx`, `models.fx`, `asteroids.fx`, `planet.fx`,
  `atmosphere.fx`), `textures*.cfg` (planet and asteroid skins). `MODS/` holds
  JSGME packs; the installed folders already include the enabled ones, so
  the port reads only `MODELS/` and `Textures/`. The AJ readme (UTF-16,
  `FFED3DAJ_readme.txt`) documents `skins.ini`, `tris.ini`, the shader
  variables and the object types.
- **The source**: `ffed3d-master/`, cloned from
  `github.com/corpsofengineers/ffed3d` (the Google Code export, about r65;
  local and ignored like the other legacy trees). AJ is a later fork: its
  `skins.ini`, `textures.cfg` and per-group shaders are documented only in
  its readme.

## What FFED3D does (from `ffed3d-master/win`)

- **Replacement models** (`w32video.cpp` `loadModels`, `ffe3d.h` `xModel`):
  `models/<n>/model.x` for model slots 0..499, loaded on first use and
  dropped after 500 frames unused. `tris.ini` `[MODEL]`: `skip`,
  `notdrawtext`, `notdrawsubmodels`, `scale`, `scale_x/y/z` (default 1),
  `offset_x/y/z`, `cullmode`; AJ adds `UseExtraAnimations`.
- **Placement** (`gfx.cpp` `C_Break`, `w32video.cpp` render loop): the
  object's world matrix is its rotation (the Q31 rows divided by
  0x80009fff) then its camera-relative position divided by `DIVIDER`
  (10000), in the same units FFED3D uses for the original's vertices
  (`vertex << actualScale / DIVIDER`). The `.X` mesh gets no scale of its own:
  `world = scale_xyz * scale * translate(offset) * object`. One `.X` unit is
  therefore 10000 of the original's (scaled) model units. Direct3D's
  left-handed, row-vector convention matches the original's axes (x right,
  y up, z forward).
- **What still draws**: with a replacement, the model's own faces are
  skipped (`skipCurrentModel`); its sub-models (thrusters, lights, gear,
  turrets) still draw unless `notdrawsubmodels`, and text sub-models
  (models 0..2) on ships 14..70 are skipped. Models 186/187 are skipped
  under a replaced parent.
- **Animation**: ships 15..69 and models 227/234/236 play animation set 0 at
  the landing-gear word (`landingState / 65536` of its length, `/65535` for
  227); with no gear down and more than one set, set 1 loops on time. Other
  models loop set 0.
- **Skins** (`xModel::loadSkins`; AJ readme): `skin.png`, `skin1..9.png`;
  AJ's `skins.ini` `[SKINS]` with `useINI=1` names them (sub-folders allowed)
  and `skinPolice`. The skin number is 0..9 from the object's id (99 for
  police); a missing skin falls back to skin 0. Shaders also see `color`
  (the id's last byte) and `type` (the object type: 0xff the player, 0xfb a
  pirate, 0x0e police, ...).
- **Textures** (`w32video.cpp` loader, `gfx.cpp` `setMaterial`):
  `textures/texN.png` (then `.dds`, `.tga`) for N 0..999. A textured face
  (material bit 0x4000) uses `tex(400 + (material & 0xfff))`, in full colour
  modulated by the lighting (white when the face colour is black), not the
  original's per-face palette slots. Lower numbers are interface bitmaps.
- **Shaders**: `effect.fx` in a model's folder, else the group shader (AJ:
  ships, asteroids, other models, planet, atmosphere), with `light`,
  `lightcol`, `ambient` (32/255), `time`, `skinnum`, the matrices and the
  permutation textures.
- **Sky**: `stars.x`, `nebula.x` with `skybox0..5`, `stars.tga`;
  `sphere.x`/`atmo.x` for planets' atmospheres; planets drawn by
  `DrawGeosphere` on six subdivided cube faces with `planet.fx`.

## The owner's overrides

The owner's files win over FFED3D's: `assets/` mirrors an FFED3D install, and a
file at the same path as FFED3D's replaces it (`assets/Textures/CABIN0.png`
replaces `ffed3d/Textures/CABIN0.png`). The build packs every file of `assets/`
into `assets.bin`, which the game reads (`docs/notes/modern-graphics.md`, "The
owner's images"). For each picture: the owner's file, then FFED3D's, then the
original. The owner keeps only the six cabins there for now.

## Plan (owner, 2026-11-04)

Modern graphics should end up looking very like FFED3D: FFED3D's models,
textures, panel, HUD, planets and buildings, drawn by the port's own engine on
DirectX 12, with FFED3D's assets (and the owner's overrides). The port's own
stars and sky stay. FFED3D's `.fx` shaders are not ported: the port lights and
shades with its own shaders. Classic is unchanged throughout.

1. **Textures** (done 2026-11-04): `tex(400 + record)` on the models' textured
   faces, in full colour under the face's lighting (`ffe_rs::ffed3d::Textures`,
   `SolidTexture::record`, `ffe-render`'s `ColourMap`/`FaceTexture::colour`),
   stretched corner to corner over the original bitmap; the cabins
   (`Textures/CABIN<n>.png`). Checked by `tests/modern_screens.rs
   intro_frames_with_ffed3d_textures` (`FFED3D_DIR`).
2. **DirectX 12 window**: wgpu on DirectX 12 on Windows (Vulkan only where DX12
   is missing), presenting straight to a swapchain at the window's resolution
   instead of reading the GPU picture back into the software window; Classic
   shown through the same window; live switching kept.
3. **Space dust**: the original's particles (F1035, unchanged) drawn as FFED3D
   AJ draws them: grey `tex704` billboards, at least `spaceDustSize` (4 of
   1..10) and scaled by `spaceDustScale` (100%), not inside a planet's
   atmosphere; AJ's code is not in the source, so this follows its readme and
   settings. Replaces Modern's streaks.
4. **Models**: complete the `.X` reader (`ffe-convert/src/xfile.rs`: text,
   binary and MSZIP files, frames, animation sets) and draw each replacement
   model at FFED3D's placement with `tris.ini`, AJ's skin choice
   (`skins.ini`, skin number from the object's id, police 99), its sub-models
   and the landing-gear animation: ships, stations, buildings.
5. **Panel and HUD**: FFED3D's 3D console (`MODELS/panel.X` with
   `Textures/panelicons/`, the cockpit mod in the owner's install) and its HUD
   text in FFED3D's font (`FRONR___.TTF`).
6. **Planets and atmospheres**: FFED3D's `DrawGeosphere` (cube-face patches
   with height maps), `textures.cfg`'s skins, colouring, fog and glow, and the
   atmosphere shells, in the port's own shaders.
7. **Effects and lighting**: the star, lasers, explosions and engine glow as
   FFED3D draws them (`tex701`..`tex706` and the like).
8. **Side by side**: the same scenes captured in FFED3D and in Modern, tuned
   until they look alike.
