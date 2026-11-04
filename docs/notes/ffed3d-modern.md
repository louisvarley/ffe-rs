# Modern graphics with FFED3D's assets

Status (2026-11-04): **decided, being built.** The launcher requires a full
FFED3D install for Modern and passes it to the game (`--ffed3d DIR`); the
game does not draw with it yet.

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

## Plan

Each stage keeps the original model as the fallback and never touches game
state; Classic stays as it is.

1. **Asset folder and textures**: read `--ffed3d` into the Modern engine;
   `texNNN` on the original models' textured faces (FFED3D's numbering, full
   colour, lit); the six `CABIN*.png` backdrops.
2. **`.X` reader** (`ffe-render` or a new crate): the text and binary token
   formats and MSZIP (`bzip0032`); frames, frame matrices, meshes, normals,
   texture coordinates, materials and animation sets; tests on synthetic
   files (no FFED3D files in the repository).
3. **Replacement models**: per object, the `.X` mesh at FFED3D's placement
   with `tris.ini`, the skin by FFED3D's/AJ's rule, sub-models as FFED3D
   draws them, the landing-gear animation; checked against the original
   model's extent.
4. **Shaders**: the AJ group shaders (`ships.fx`, `models.fx`, `asteroids.fx`)
   ported to WGSL with the same inputs; per-model `effect.fx` later.
5. **Sky**: skybox, `stars.x`, `nebula.x`.
6. **Planets and atmospheres**: `DrawGeosphere`/`planet.fx`/`atmosphere.fx`
   ported, with the port's own depth precision (`docs/RENDERING_DESIGN.md`).
