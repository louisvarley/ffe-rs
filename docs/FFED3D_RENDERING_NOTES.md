# FFED3D rendering notes (Phase 0 study)

Purpose: understand how the original First Encounters software renderer composed the screen, how the FFED3D D3D9 port reproduces/replaces it, how the earlier HD work was done, and which quirks the Classic renderer must reproduce. This is **knowledge**, not code to copy. The Classic target is the original DOS look, not FFED3D's smoothed D3D look.

Sources: `ffe.asm` (2D/3D software routines and data), `ffedat.asm` (buffers/palette), `ffebmp.asm` (models/bitmaps), `win/gfx.cpp`, `win/planet.cpp`, `win/w32video.cpp`, `win/Render/*`, `win/XFile/*`, `doc/jongware/*`, `doc/srctools/*`.

---

## 1. The original screen

### 1.1 Resolution and layout

- Logical screen: **320 × 200**, 8-bit indexed colour, 256-entry palette.
- The screen is split:
  - **3D view:** 320 × **158** (`DATA_007774`, height `0x9e`). The top 158 rows are the space view.
  - **HUD/console:** the remaining **42 rows** (158 + 42 = 200) hold the cockpit panel, scanner, text and buttons.
- FFED3D's `Render` reproduces the split: `ViewPort(true)` sets the 3D viewport height to `viewport.Height * 0.795` (158/200 ≈ 0.79) and the 2D panel uses the full viewport. The clear rectangle for the HUD starts at `curheight/200*157`.
- FFED3D doubles the *software* back buffer to **640 × 400** (`DATA_009226` is 256,000 bytes = 640×400; `F1601_DrawHLine` uses a 640-byte stride; `F1882_FlipScreenLow` blits 640×400). The logical UI remains 320×200 and is scaled. **DOS used 320×200; this is a FFED3D deviation.**
- The original VGA mode is 320×200 displayed on a 4:3 monitor, i.e. non-square pixels (pixel aspect 1.2). FFED3D's `aspectfix` uses 1.6 (16:10, square pixels). **Classic should use the original 4:3 presentation** (see the open question in `INVENTORY.md`).

### 1.2 Buffers

- `DATA_007773`: full back buffer structure. `DATA_007774`: upper (3D) buffer. `DATA_009226`: pixel storage (256,000 bytes in FFED3D; 64,000 in DOS).
- `DATA_009253`: 200,000-byte workspace (texture/span/planet scratch).
- `DATA_009274`: 108,000-byte planet vertex workspace; `DATA_009277`: 48,000-byte planet array.
- `DATA_009252`: 256 × 3 bytes palette, 6-bit components (0–63).
- FFED3D's `pWinPal32` expands the 6-bit palette to 32-bit with `(r<<18 & 0xfc0000) + (g<<10 & 0xfc00) + (b<<2)`; the maximum component is 252, not 255. `pWinPal15/16` are the 15/16-bit variants. `VideoSetPalValue` applies `RedBase/GreenBase/BlueBase` brightness offsets via `pPalTable`.
- Palette index 0 is not a universal transparent colour: the bitmap blitter treats zero as transparent only in masked blits (`F1625`/`C_BlitClipInternal`), while opaque blits copy index 0. Sprite masks are explicit.

### 1.3 Composition order

The original frame is composed into the back buffer, then flipped:

1. Clear upper buffer (`F1618`) / optional full clear.
2. Draw the galaxy/star background into the 3D area.
3. Draw 3D models/planets into the 3D area.
4. Draw the cockpit/HUD panel and sprites over the lower 42 rows and (for some panels) over the 3D area.
5. Draw text (bitmap font, optional shadow).
6. Draw the mouse pointer.
7. `F1637_FlipScreen` / `F1882_FlipScreenLow` blits the buffer to video.

FFED3D instead builds a vertex list during the same sequence and submits it in `Render` (galaxy/nebula/stars → panel → sprites → 3D models → transparent list → panel sphere → text/sprite HUD → console). Same visual order, different mechanism.

---

## 2. Original 2D rasteriser

The software primitives are in `ffe.asm` around `0x54xxx–0x58xxx`:

| Routine | Purpose |
|---|---|
| `F1601_DrawHLine` | Horizontal line/span fill. |
| `F1602_DrawPolySegment` | Scanline polygon segment. |
| `F1603_CalcLineGrad` | Edge gradient/height setup. |
| `F1604_DrawTriangle` | Flat-shaded triangle, upper-screen clipping. |
| `F1606_DrawQuadVClip` | Quad, vertical clip only. |
| `F1607_DrawQuad` | Quad, no clipping. |
| `F1608_DrawQuadInternal` | Quad with a line function. |
| `F1609_DrawSprite` | Sprite from one of two sprite sets (`flag`). |
| `F1610/F1611_DrawParticle` | Clipped/unclipped roughly circular particles. |
| `F1612_DrawCircle` | Two pixels per scanline, vertical clip. |
| `F1613_DrawSpans` | Span-list rasteriser (up to 158 scanlines, up to 10 x points per span). |
| `F1616_DrawLine` | Bresenham-style line. |
| `F1617_BlitVidToBuf` | Copy video to buffer (and enable pointer). |
| `F1618/F1619_ClearUpperBuf/ZeroUpperBuf` | Clear upper buffer to colour/zero. |
| `F1620–F1626` | Bitmap index lookup, clipped blits, masked blits. |
| `F1627/F1628_DrawBoxToBuf` | Unclipped box; masked version skips 0x28–0x2b. |
| `F1629_DrawPixelToBuf` | Single pixel (used by the galaxy). |
| `F1630–F1636` | Software mouse pointer. |
| `F1637_FlipScreen` | Update input/pointer, set palette, blit. |
| `F1641_SetWholePalette` | Set all 256 palette entries. |
| `F1642_GenerateTexSteps` | Affine texture-mapping setup (u/v steps per scanline). |
| `F1643_SelectTexFunc` | Select one of four texture functions (transparent/non-transparent, 0x40/0x80). |
| `F1644/F1645/F1646` | Textured triangle, fixed UVs/arbitrary UVs. |
| `F1647/F1648/F1649` | Textured quad, fixed/arbitrary UVs. |

### 2.1 Edge rules and clipping

The triangle/quad routines use fixed-point edge walking (`F1602/F1603`). Heights are integer; gradients are 16.16 (or similar) and accumulated per scanline. Clipping is mostly vertical to the 158-row 3D area; some functions clip to the full 200 rows. The exact edge rule (top-left inclusion, rounding of `dx/dy`, first/last scanline) is visible in the asm and must be ported bit-for-bit for the software reference. Do not substitute a generic rasteriser.

### 2.2 Text

- Text is a **proportional bitmap font**, not a TTF. `F1586_TextWriteDetailed` / `F1587_TextWriteInternal` parse a string with embedded control codes:
  - `0x00 0x??`: draw bitmap index.
  - `0x01 0x??`: set colour.
  - Other codes set position/height/wrap.
- `F1597_CharAdvanceWidth` returns the advance width; `F1598_DrawChar` draws a glyph; `F1588_DrawShadowedChar` draws a one-pixel shadow.
- `F1589_WriteStringShadowed` and `F1399/F1402` add wrapping/shadow variants.
- The font glyphs are embedded indexed bitmaps; the glyph set and spacing are part of the Classic look. FFED3D replaces this with Arial via `D3DXFont` (scaled by `curwidth/640`), which is **not** Classic.
- Text colours are palette indices; the standard colours are defined in `DATA_007776+` (white, red, etc.).

### 2.3 Sprites and bitmaps

- `F1620_BmpIndexToPtr` maps an index to a bitmap pointer from `DATA_008240`; `DATA_008241` is the alternate sprite set used by `F1609` when `flag` is set.
- Bitmap header: `u8 type=1`, `u8 height`, `u16 width`, 4 bytes zero, then `width*height` palette indices (top-to-bottom).
- `F1623/F1624` clip to screen/custom rectangle; `F1625/F1626` are the internal blit loops. `F1626` takes a flag for masked/transparent copy.
- `CABIN0..5.BMP` are standard 320×158 8-bit BMPs, loaded by `F?` (`cabin%i.bmp`); the cockpit view is blitted as bitmap index 96 with a small expansion/clip adjustment.
- FFED3D's `C_BlitClipInternal` converts each bitmap to a textured quad and scales it to the current window; it also special-cases panel indices and the cabin.

---

## 3. Original 3D pipeline

### 3.1 Model representation

Models are command streams, not meshes (see `INVENTORY.md` §3.5). The interpreter (`sendToRender` in FFED3D; the corresponding asm routines around `0x5xxxx–0x63xxx`) does:

1. Read the model header and select the command stream (`Model_t.Mesh_ptr`).
2. For each command, select vertices/normals, apply local matrix/scale/mirror/rotation, optionally skip based on visibility or flags, set material/texture, and emit polygons.
3. Vertices are `ffeVertex` records: 16-bit screen-space `orto_x/orto_y`, 32-bit normals, 32-bit local/world `x,y,z`, plus a second set for the second transform. `DIVIDER = 10000` converts the integer vertex units to float in FFED3D; the original uses fixed-point shifts.
4. Normals are packed bytes; `getNormal`/`getReNormal` decode and transform them.
5. Textures are indexed bitmaps selected by the material command; texture coordinates are generated per primitive or supplied by the command stream.

### 3.2 Transform and projection

- Object orientation is a 3×3 1.31 matrix (`ModelInstance_t.relMatrix`, `globMatrix`); positions are `Point64` with an exponent scale. `F1677/F1678` compute global/relative positions; `Vec64Truncate` shifts by an exponent (`0xd` for Vec32).
- Camera-relative transforms are built per object and per submodel (`mainRotMatrix`, `currentMatrix`, `posMatrix`). The original uses the object's `dist_cam` exponent to scale.
- Projection: FFED3D uses `D3DXMatrixPerspectiveFovLH(35°, aspect 2.0, near 0.1, far 50000)` for the near pass, and `(35°, 2.0, 1, 100000)` for the far pass. The 2.0 aspect matches the 320×158 3D viewport (320/158 ≈ 2.03). The original likely uses the same 35° FOV and a fixed projection; the software reference must derive the exact coefficients from the asm (`F1479`/`doOrtoXY` and the projection code), not from D3DX.
- `C_FUNC_001479_doOrtoXY` shows the FFED3D replacement: it transforms by `world*view*proj` and maps NDC to `(-160..160, -79..79)`, i.e. 320×158 with the origin at centre. The original does the same with integer/fixed-point math.

### 3.3 Shading and lighting

- The original renderer is **flat/Gouraud-ish per face**, using a small ambient ramp and a directional/point light from the local star. The key data is `Ambient2[7][8][3]` in `gfx.cpp`: 7 material types × 8 intensity levels × RGB, each 0–7. The material type is the low bits of the material command; the intensity is the dot product of the face normal and the light direction, clamped/quantised to 8 levels.
- `Ambient[7][3]` is a secondary 7-colour table. `setMaterial` in the original selects a base colour; the palette index is `base + ramp[intensity]`, or the 4-bit packed colour from the command (`r*12+63`, etc.). Textures modulate the palette index; transparent textures use the `0x40/0x80` functions.
- FFED3D uses D3D9 lighting: `selectMaterial` sets `D3DMATERIAL9` diffuse/ambient/specular/emissive; `setupLight` creates a point light at the player/star position with no attenuation and infinite range; `setupModelLight` adds panel lights. It enables `D3DRS_LIGHTING`, `NORMALIZENORMALS`, `COLORVERTEX`, Gouraud shading, linear texture filtering, and per-vertex normals. This is **smoother and brighter** than the original and is not the Classic target.
- Original face normals are often forced to a single normal per polygon (`endCallback` copies `oldNormal` to all vertices), so flat shading is common. Backface culling uses the sign of the dot product with the old normal; the `currentNormal`/`oldNormal` logic is visible in `gfx.cpp` and the asm.

### 3.4 Transparency and ordering

- Transparent polygons are collected and drawn after opaque geometry. FFED3D uses `transpBuf` (20,000 entries) for `GL_TRIANGLE_STRIP`/`GL_TRIANGLE_FAN` with `transparent=true` and for `GL_POINTS`; `drawModelPrimitives` is called twice (opaque then transparent).
- The original uses a span/scanline approach and draws polygons in model/command order; there is no general per-pixel depth buffer in the DOS renderer. Overlapping transparent/opaque surfaces depend on draw order, which is part of the look.
- FFED3D uses the D3D z-buffer (`D24S8`) and `ZWRITEENABLE` per primitive; some models set `zclear` to clear depth between passes.

### 3.5 Backface culling and material quirks

- `checkOrientation` compares the triangle normal with `getNormal(currentNormal)`; if any component has the opposite sign, the face is rejected. This is a crude backface test, not a full winding test.
- Mirror/rotation bits can invert winding; the original often relies on `CULL_NONE`/manual orientation.
- `setMaterial` bit `0x2000` disables lighting; `0x1000` uses the local colour; `0x4000` selects texture `400+n`; otherwise the 12-bit packed colour. Local colour is often set by the model command before a submodel.
- Some models use `GL_POINTS` for stars/particles; FFED3D draws two point sprites (textures 602/603) with fixed point sizes and scaling.

---

## 4. Original planets, stars and space

### 4.1 Planets

- The original planet is a **procedurally tessellated sphere** based on an icosahedron (`DATA_007892_Icosahedron`), subdivided recursively. `planet.cpp`/`planet.h` contain the FFED3D reimplementation and the LOD arrays (`planetLod pLods[50]`, `cLoDTri` with 4 children), plus the original helper functions (`F1816_ArrayInit`, `F1820_ArrayCreateNewElement`, `F1821_ArrayRemoveElement`, `F1823`, `F1825`, `F1826`, `F1827`, `F1829`, `F1833`–`F1840`, `F1846`, `F1849`–`F1867`, `F1868`–`F1874`).
- The planet mesh is built from an icosahedron (`DATA_007892`), with vertices stored in `ffeVertex` and triangles in the LOD tree. `F1874_DrawPlanet` walks the command stream for planet submodels; `C_FUNC_001874_DrawPlanet` is the FFED3D GPU path.
- Surface detail is procedural: the original computes colours from the planet's `unique_Id`/`randseed`, temperature, type and noise; there are no surface texture files in the floppy EXE. The FFED3D shader (`planet.fx`) uses a permutation/noise texture (`permTexture2d`, 256×256 A32B32G32R32F), a gradient texture, a height map, and water/wave textures (`tex720`, `tex722`, `tex723`, `tex94`, `tex800`, `tex713`).
- The planet LOD is distance-based (`control_dist`, `max_divide_deep`, `smart_optimizer` in `ffewin.cfg`/`planet.h`). The original uses the same recursive subdivision with a distance threshold; the exact split rule and vertex normal calculation are in `planet.cpp` (`C_FUNC_001833`, `C_FUNC_001840`, `C_FUNC_001846`, etc.) and the asm.
- Atmosphere: a separate shell/quad pass, additive/alpha blended; FFED3D uses `atmoObj`/`atmo.X` and `ATMO/ATMO2/ATMOSTAR` materials. The original draws an atmosphere ring/shell with palette blending.
- Rings: `Rings(modelNum)` special-cases ringed planets; the ring is drawn as a textured/translucent quad/annulus.

### 4.2 Stars, starfield, nebulae

- The original starfield is procedural points/sprites in the 3D view, with star colours from `starColors[11][3]` (15-level palette entries). The star's light colour drives the illumination of ships/planets.
- FFED3D uses `stars.X` (a large point/quad model) and `nebula.X` (a skybox-like model) with `skybox0..5.tga`, `stars.tga`. This is a visual replacement, not the original starfield.
- The galaxy map is a 2D projection of the star catalogue; `C_Galaxy`/`F847–F849` draw points/lines into the buffer. The map's coordinate transform and clipping are part of the Classic UI.

### 4.3 Space dust/particles

- Space dust/particles are `GL_POINTS`/sprites with palette colours and point sizes; the original draws small clusters around the camera to convey motion. FFED3D uses point sprites with textures 602/603, fixed sizes 15/5–10, and `D3DRS_POINTSCALE`.
- Engine glows/smoke are sprites/particles attached to objects; `smoke_timeout`, `laser_flags`, `ecm_timeout` drive them. They are presentation, but their spawn timing is driven by simulation state; the render description must carry them without letting the renderer mutate the simulation.

---

## 5. FFED3D D3D9 architecture

### 5.1 Device and buffers

- `RenderSystem::Initialize` creates a D3D9 device at the requested window/fullscreen size, `D3DFMT_A8R8G8B8` back buffer, `D3DFMT_D24S8` depth/stencil, `D3DSWAPEFFECT_COPY`, hardware vertex processing with software/reference fallbacks.
- One large dynamic vertex buffer (`vertexBuffer`, `MAXVERT = 3,000,000`) with `VertexXYZNDT1` (position, normal, diffuse, UV). A second small buffer is used for text sprites. `vertexType[]` describes each primitive run (type, amount, texture, lighting, transparency, radius, tangent).
- `modelList[6000]` holds per-model draw state: world/scale/rotation matrices, light, material, cull, z flags, `dist`, `landingGear`, `skinnum`, missile submodels. `ffText[2000]` and `spriteList[400]` hold HUD text/sprites.
- `Render` batches: galaxy/nebula/stars, panel (render-to-texture 1024×1024), sprites, opaque 3D, transparent 3D, panel sphere, text, HUD sprites, console.

### 5.2 Viewport and aspect

```cpp
if (aspectfix) {
    viewport.Y = (curheight - curwidth/1.6f)/2;
    viewport.Width = curwidth;
    viewport.Height = curwidth/1.6f;
} else { full window }
if (enable) viewport.Height *= 0.795f; // 3D area only
```

- `draw2D`/`ViewPort(false)` uses the full viewport for the HUD; `ViewPort(true)` uses the top 79.5% for 3D.
- The 3D projection aspect is fixed at 2.0 (not the viewport's actual aspect), matching 320×158. On a 16:9 window this means the 3D view is slightly stretched relative to the HUD; `aspectfix` keeps the 320×200 logical area at 1.6.
- `doOrto` uses the full `curwidth × curheight` for HUD; text positions are scaled by `curwidth/640` and `(curheight-aspectfactor)/400` when `aspectfix` is on. This is a FFED3D-specific mapping; Classic must use 320×200 coordinates and a 4:3 presentation.

### 5.3 Model rendering

- `sendToRender` parses the original command stream; `getVertex` transforms the original integer vertices by `1/DIVIDER` and the current matrices. `endCallback` assigns a single normal to all vertices of a tessellated polygon, then `drawModelPrimitives` submits D3D primitives.
- `setMaterial` maps the original material command to `currentColor`, `currentTex`, `doLight`; `selectMaterial` maps the FFED3D material type to D3D `D3DMATERIAL9`. `setupLight`/`setupModelLight` set point lights.
- `drawModelPrimitives` selects the texture (`textures[textNum]`, or `aviTex` for `-100`), lighting, and effect pass. It special-cases planet textures (720 water, 699/700 atmosphere, 94 noise land) and point sprites.
- HD model support: `loadModels` loads `models/<id>/model.x` (DirectX `.X`), `models/<id>/skin.png`/`skinN.png`, and `models/<id>/tris.ini`/`effect.fx`; a background thread loads/drops models with a counter. When present, the `.X` model replaces the command-stream model; otherwise the original stream is drawn. This is the precedent for per-asset HD replacement with fallback.
- `xModel`/`CXFileEntity` is a standard D3DX `.X` loader with animation sets (landing gear etc.). It is not part of the original data.

### 5.4 HD textures

`loadTextures` scans `textures/tex%i.png`, then `.dds`, then `.tga` for `i = 0..999`; `textures[textNum]` is used by the command stream. The original model command selects texture indices `400+n`; FFED3D's `texNNN` naming corresponds to the original bitmap/texture indices. This is exactly the model for `upscaled.bin`: an asset ID (texture index), an HD replacement, and a fallback to `data.bin`.

Loading screens (`textures/loading/Loadscreen.png`, `loading_models/textures/effects.png`) and console textures (`con_logo.tga`, `con_field_*.tga`) are FFED3D additions. The console is a Lua-driven overlay, not original UI.

### 5.5 Planets on the GPU

`DrawGeosphere` (w32video.cpp) renders six cube faces, each as a grid of `chunk_size = 32` patches. `DrawChunk` recursively subdivides a patch until `div >= 16384` or `dist >= mindist`; it renders a height map to an R32F target, then draws the patch with `planet.fx` pass 4/5. It uses the object's `unique_Id` as a seed, `modelList[m].dist`, scale/rotation/world matrices, and textures `tex713/720/722/723/800/94`. This is a GPU replacement for the original recursive tessellator; it can produce cracks between chunks and is sensitive to depth precision.

### 5.6 Panel and HUD

- `PreparePanel` renders the panel icons/buttons into a 1024×1024 texture with an orthographic projection, then draws it as `panel.X` in the 3D view at a fixed transform. The panel lights are point lights at hard-coded positions (`panellight[1..15]`).
- The HUD text uses `D3DXFont` Arial, scaled by `curwidth/640`, with `DrawText`/`DrawConsoleText`. Sprites use `spriteList` and `textSprite`, scaled by 2.5× (FFED3D) and positioned from the 320×200 coordinates.
- The original HUD is a bitmap panel (embedded) with bitmap font text; FFED3D's panel is a 3D model + HD texture, and the text is TTF. **Classic must use the original panel bitmaps and bitmap font.**

### 5.7 Audio/visual timing

- `Render` calls `UpdateDeltaTime` (timeGetTime) for animations; the simulation time is separate (`DATA_008804`/`GameTics`). Model animations (`landingGear`) use `landingGear/65536 * animationLength`; this is presentation only.
- `w32video.cpp` updates the window title with `test=...` debug values; not part of the game.

---

## 6. Units and scales for the renderer

- `KM_DIST = 999539.7368756` units per km ⇒ 1 unit ≈ 1.00046 mm. A Cobra (~65 m) is ~65,000 units; a planet (6,371 km) is ~6.37×10⁹ units; a star is much larger. `Point64` positions and exponent bytes carry this range.
- Model vertices are integer units divided by `DIVIDER = 10000` in FFED3D; the original fixed-point scale uses `Model_t.Scale`/`Scale2`. A model's `Radius`/`interract_radius`/`collision_radius` are in the same units.
- Projection near/far in FFED3D are 0.1/50,000 (near) and 1/100,000 (far) in *model-space after scaling*, not world units; the renderer converts to camera-relative floats. For planet-scale rendering, a single f32 depth range is insufficient. The design will use camera-relative f64 transforms, reversed-Z, and near/far splits; see `docs/RENDERING_DESIGN.md`.
- Original object `dist_cam`/`dist_3face` are exponent bytes, not distances; the renderer uses them to choose LOD/detail and culling, not as depth values.

---

## 7. Faithfulness checklist: original quirks to reproduce in Classic

| Quirk | Original behaviour | Where |
|---|---|---|
| 320×200, 8-bit indexed | 320×158 3D + 42-row HUD | `DATA_007773/74`, `F1618`, `F1637` |
| 6-bit palette | 256 × 3 × 6-bit; `<<2` expansion in FFED3D | `DATA_009252`, `VideoSetPalValue` |
| Non-square pixels / 4:3 | DOS 320×200 on 4:3 (pixel aspect 1.2) | `INVENTORY.md` §10 question |
| Bitmap font | Proportional glyphs, shadow, palette colours | `F1586–F1598` |
| Affine texture mapping | `F1642–F1649`, no perspective correction | `ffe.asm` |
| Flat/quantised shading | `Ambient2[7][8][3]`, 8 intensity levels | `gfx.cpp` |
| Face-normal backface test | `checkOrientation` sign test | `gfx.cpp`, asm |
| Draw-order transparency | Transparent list after opaque; no per-pixel z in DOS | `drawModelPrimitives` |
| Command-stream models | Submodels, mirror/rotate bits, conditional skips | `INVENTORY.md` §3.5 |
| Procedural planets | Icosahedron LOD, noise colours, no texture files | `planet.cpp`, asm |
| Procedural starfield | Palette points/sprites, 11 star colours | `starColors` |
| HUD panel bitmaps | Embedded panel/icons, not TTF/3D panel | `ffebmp.asm` |
| Cockpit bitmaps | `CABIN0..5.BMP` 320×158 | game files |
| Mouse pointer | Software pointer, 320×200 coords | `F1630–F1636` |
| 50 Hz input cadence | `TimerAdvance`, 20 ms nominal | `INVENTORY.md` §7 |
| Frame pacing | `maxfps` cap, variable render vs fixed sim | `F1416/F1418` |

## 8. FFED3D quirks that must **not** leak into Classic

- 640×400 software back buffer / 2× internal resolution.
- 16:10 `aspectfix` (1.6) instead of 4:3.
- TTF (Arial) text instead of the bitmap font.
- Linear/anisotropic texture filtering, D3D lighting, Gouraud normals, specular highlights.
- 3D panel model + 1024×1024 render target instead of the bitmap panel.
- `.X` models/skins, `texNNN` HD textures, skybox TGA, shader planets.
- BASS/MP3 music, Lua console, loading screens/tips.
- 32-bit colour back buffer, `pWinPal32` expansion, `<<2` palette.
- Any FFED3D-added UI/menu text or gameplay fixes (see `INVENTORY.md` §9).

## 9. Implications for the Rust renderer

1. **Classic is a distinct, first-class renderer**, not a feature flag on the Enhanced path. It needs the original indexed framebuffer semantics, palette, font, affine textures, and software-rasteriser edge rules.
2. **The software reference renderer is the oracle**; the GPU Classic path must match it within documented tolerances. Do not derive Classic from FFED3D's D3D output.
3. **The render description must carry original material/texture IDs and command-stream model references**, not just triangles, so Classic can reproduce submodels/conditionals and HD can replace assets per ID.
4. **Large scenes need camera-relative transforms** and a depth strategy independent of the original's exponent tricks. The original's `dist_cam`/`dist_3face` are LOD hints, not a depth buffer.
5. **The HD path should follow FFED3D's per-asset replacement model** (`texNNN`/model skins) but with stable asset IDs and the `upscaled.bin` manifest, not filename scanning.
6. **Planet rendering is the hardest fidelity case**: the original is procedural and LOD-driven; a GPU replacement must either reproduce the CPU tessellation/colours exactly (Classic) or be an opt-in Enhanced effect (atmosphere/clouds/HD). Do not silently replace the Classic planet.
7. **Audio/visual timing is separable**: the simulation runs at 50 Hz with integer tics; frame interpolation/animations are presentation and must not feed back.

See `docs/RENDERING_DESIGN.md` for the proposed feature order, budgets, dependencies and Classic comparison plan.
