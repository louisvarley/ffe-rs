# Data formats (`data.bin`, `upscaled.bin`, mods)

Status: **proposal — awaiting approval.** This document justifies the chosen formats and defines the serialization rules needed for deterministic conversion and stable modding.

---

## 1. Choice of human-readable format

The brief allows RON, TOML or JSON for tables and text, with the choice justified.

**Chosen:**

- **RON** for content tables, text, missions, fonts/metrics and model sidecars.
- **TOML** for `mod.toml` (the brief requires it) and the app config.
- **JSON** for ZIP manifests (`manifest.json`), generated JSON Schema, and debug dumps.

**Why RON for content:**

- Comments and trailing commas make hand-editing practical for modders.
- Strong typing for enums/structs/optional fields, matching the Rust content types directly; no stringly-typed tags or duplicate-key ambiguity.
- Serde support; good error spans from `ron` (line/column) for `validate-mod`.
- No implicit type coercion (unlike TOML dates/numbers); arrays preserve order.
- Nested arrays of structs are readable; TOML’s `[[array.of.tables]]` becomes unwieldy for models/text.
- JSON lacks comments and is noisy for long text; it remains the right choice for machine manifests.

**Determinism:** RON has no canonical form, so `ffe-convert` uses a **canonical RON writer**:

- struct fields in declaration order (never a `HashMap`);
- maps as sorted `BTreeMap` keys;
- arrays in source order;
- fixed float formatting (shortest round-trip; no NaN/Inf in data);
- no optional fields when `None`;
- LF line endings, two-space indent, one trailing newline.

The writer is covered by a build-twice byte-equality test. If a future `ron` version changes formatting, the writer (not the crate’s pretty-printer) is the authority.

**Why not TOML for content:** TOML has no null, no comments inside arrays of tables, weak support for nested variants, and dates/times that can be coerced. **Why not JSON for content:** no comments, verbose, easy to break by hand. JSON is still used for manifests and schemas because it is universally toolable.

**JSON Schema:** `schemars` derives a schema from the Rust content types. It is generated into `schema/` and shipped for editors/CI. RON documents are validated against the Rust types directly by `validate-mod`; the schema is a documentation/editor aid, not the primary validator. This satisfies “JSON Schema generated from the Rust types” without pretending RON is JSON.

---

## 2. `data.bin` layout

```
manifest.json
palettes/base.json
fonts/<font_id>.png
fonts/<font_id>.json
textures/<asset>.png
textures/<asset>.json
models/<asset>.glb
models/<asset>.ffe.ron
audio/fx/<asset>.wav
audio/music/<asset>.mid
tables/goods.ron
tables/equipment.ron
tables/ships.ron
tables/governments.ron
tables/star_types.ron
tables/systems.ron
tables/starports.ron
text/ui.ron
text/ships.ron
text/missions.ron
text/bbs.ron
journal/entries.ron
missions/templates.ron
missions/military.ron
README.md
```

Names are ASCII, lowercase, stable; asset IDs may contain `/` and are URL-encoded only if necessary (prefer `base:texture/400` → `textures/base_texture_400.png`).

### 2.1 `manifest.json`

```json
{
  "format_version": 1,
  "game_version": "ffe-floppy-1.02-en",
  "source": {
    "executable": {"path": "FIRSTENC.EXE", "size": 2097152, "sha256": "d919..."},
    "files": [
      {"path": "FIRSTENC.EXE", "size": 2097152, "sha256": "d919..."},
      {"path": "MISSION.DAT", "size": 779951, "sha256": "3cf3..."}
    ]
  },
  "tool": {"name": "ffe-tools", "version": "0.1.0"},
  "palette": "palettes/base.json",
  "asset_ids": ["base:font/0", "base:model/38", "base:texture/400"],
  "files": [
    {"path": "textures/base_texture_400.png", "size": 1234, "sha256": "..."}
  ]
}
```

Rules:

- `format_version` is a major integer; readers reject unknown majors and warn on newer minors.
- `source.files` lists every input file used, with SHA-256.
- `asset_ids` is sorted lexicographically and is the canonical list for `upscaled.bin`.
- `files` is sorted by path and covers every file except `manifest.json`; each file has size + SHA-256.
- The manifest itself is canonical JSON: sorted keys, no whitespace, `\n` at EOF.
- The ZIP is deterministic: sorted entries, fixed timestamp 1980-01-01 00:00:00, fixed deflate level 9, no extra fields, no comments; manifest first.

### 2.2 Palette

`palettes/base.json`:

```json
{
  "id": "base:palette/main",
  "bits": 6,
  "colours": [[0,0,0], [63,63,63], ...]
}
```

- 256 entries, each `[r,g,b]` in 0..63.
- The display expansion rule is not baked into the palette (Classic stores indices).
- Additional palettes (e.g. UI/damage flash) are separate entries with IDs.

### 2.3 Textures

`textures/base_texture_400.png` is an **indexed PNG**:

- bit depth 8, colour type 3 (indexed), `PLTE` with the palette expanded to 8-bit for preview, optional `tRNS`.
- Pixel values are the original indices; the engine uses the sidecar palette, not `PLTE`.
- No `tIME`, no text chunks, no gamma/ICC (Classic must not colour-manage).
- Deterministic encoder: one filter strategy (e.g. adaptive with a fixed tie-break), fixed zlib level.

`textures/base_texture_400.json`:

```json
{
  "id": "base:texture/400",
  "kind": "texture",
  "width": 64,
  "height": 64,
  "palette": "base:palette/main",
  "transparent_index": null,
  "source_index": 400,
  "notes": "model texture 400"
}
```

- `kind`: `texture`, `sprite`, `hud`, `font`, `background`, `cockpit`, `mask`, `data`.
- `transparent_index` is the index treated as transparent in masked blits, or null.
- `source_index` is the original bitmap/texture index for traceability.
- Assets used only as data (LUT/height/noise) are still stored if the engine needs them, but marked `data` and excluded from upscaling.

### 2.4 Fonts

`fonts/base_font.png` is an indexed atlas; `fonts/base_font.json`:

```json
{
  "id": "base:font/main",
  "atlas": "fonts/base_font.png",
  "glyphs": [
    {"code": 32, "x": 0, "y": 0, "w": 3, "h": 8, "advance": 4, "shadow": true}
  ],
  "line_height": 10,
  "space_width": 4
}
```

- Glyph codes cover the original character set and control-code expansions.
- The Classic renderer uses this atlas and metrics; Enhanced “sharp HUD” may rasterise a vector font but must match the Classic metrics/layout when off.
- The original font bitmaps are extracted from the EXE; no TTF is used for Classic.

### 2.5 Models

Two representations per model:

1. `models/<asset>.glb` — glTF 2.0 binary for tools, HD editing and optional Enhanced rendering. `extras.ffe` preserves:
   - original model index, scale/radius, shipdef pointer/id;
   - submodel tree and command-program ID;
   - material command bits, texture IDs, palette IDs;
   - collision/character pointers as IDs.
2. `models/<asset>.ffe.ron` — the exact semantic command stream:

```ron
(
  id: "base:model/38",
  source_index: 38,
  scale: 12,
  scale2: 3,
  radius: 12345,
  shipdef: Some("base:ship/cobra_mk3"),
  commands: [
    SetVertex(0), SetVertex(1), SetVertex(2),
    Triangle,
    Material(bits: 0x4123),
    Texture(400),
    Submodel(index: 1, transform: ...),
    SkipIfNotVisible(...),
    ...
  ],
  vertices: [...],
  normals: [...],
)
```

- The command stream is lossless; the GLB is a convenience/HD representation. Classic consumes `ffe.ron` (or a binary cache built from it) and reproduces the original interpreter.
- `extras.ffe` is documented in `docs/MODDING.md`; mods may supply either, but a mod model without a semantic stream can only be used by Enhanced, not Classic (validation error if it claims Classic support).

**Why glTF 2.0:** open, widely supported, binary GLB, stores meshes/materials/textures/animations; `extras` is an official extension point. It is not sufficient alone for Classic, hence the sidecar. A fully custom format would be less tool-friendly; a pure glTF would lose command semantics. This combination is the best of both.

### 2.6 Audio

- Effects: `audio/fx/<id>.wav`, PCM 16-bit signed little-endian, mono, 22,050 Hz, canonical 44-byte RIFF header, no extra chunks. This is a lossless representation of `FX/*.RAW` (which is headerless PCM in the same format).
- Music: `audio/music/<id>.mid`, Standard MIDI File type 1, 480 PPQN (or the HMP’s native division if it is musically meaningful; document the choice), track names preserved. Conversion from HMP via `hmp.cpp` semantics is deterministic; the converter is tested by re-parsing the SMF and comparing events to the HMP track data.
- Optional Enhanced audio (e.g. OGG) would be a separate pack/feature; Classic uses WAV/MIDI.
- No game audio is committed; `data.bin` is generated from the player's files.

### 2.7 Text and FF codes

Original text is a byte stream with FF codes (`0xff` commands) and bracket expansions. To keep it human-readable and lossless, store UTF-8 with a documented escape:

```
"Welcome to [Pn's World.\nPress \u{FF00}..."   // example; final syntax below
```

Proposed escape syntax (exact bytes are authoritative):

- Printable ASCII stays as-is.
- `\n`, `\r`, `\t` for controls where they are literal.
- `\xNN` for a raw byte that has no printable representation.
- `{ff:NN}` / `{ff:NN MM}` for an FF code (the byte after `0xff` and optional argument), e.g. `{ff:00}`, `{ff:80 01}`.
- `{var:name}` for a documented variable expansion where the original uses it; the core expands it deterministically.
- `[[...]]` not used.

The converter must round-trip: decode bytes → escape → parse → bytes == original. The core’s `TextRun` expansion produces glyph runs; renderers never parse escapes.

### 2.8 Tables

RON structs with stable field names, e.g.:

```ron
(
  goods: [
    (id: "base:good/water", name: "Water", price: 30, price_var: 90, stock: 8000, stock_var: 9000),
    ...
  ],
)
```

- `id` is stable and namespaced; `source_index` records the original index.
- Numeric fields use explicit types (`u16`, `i32`, `Ffp` as a struct with `mantissa`/`exponent` or a decimal string? **Decision:** serialize FFP as `{ m: i16, e: i16 }` to avoid float ambiguity; document in the schema).
- Tables are sorted by `id` in canonical output, not by source order, unless order is semantically meaningful (e.g. equipment list order). If order matters, use an explicit `order` field and keep source order.
- Text fields use the escape syntax above.

### 2.9 Journal

`journal/entries.ron`:

```ron
(
  entries: [
    (id: "base:journal/0", source_offset: 1, text: "..."),
    ...
  ],
)
```

- `source_offset` is the original `MISSION.DAT` offset for traceability.
- Text is decoded/expanded to UTF-8 with the escape syntax if it contains FF codes.
- The original encrypted bytes are not stored; the converter must be deterministic and round-trip against the legacy loader.

---

## 3. `upscaled.bin` layout

```
manifest.json
assets/<asset>.png
assets/<asset>.normal.png
assets/<asset>.roughness.png
assets/<asset>.emissive.png
```

### 3.1 Manifest

```json
{
  "format_version": 1,
  "source_game_version": "ffe-floppy-1.02-en",
  "asset_id_list_hash": "sha256:...",
  "assets": [
    {
      "id": "base:texture/400",
      "source_size": [64, 64],
      "target_size": [256, 256],
      "scale": 4.0,
      "file": "assets/base_texture_400.png",
      "provenance": "derived-from-original",
      "maps": {
        "normal": "assets/base_texture_400.normal.png",
        "roughness": "assets/base_texture_400.roughness.png",
        "emissive": "assets/base_texture_400.emissive.png"
      }
    }
  ],
  "tool": {"name": "ffe-tools", "version": "0.1.0"}
}
```

- `asset_id_list_hash` = SHA-256 of the canonical JSON array `[{"id","width","height","kind"}]` for all upscalable assets, sorted by ID. It does not depend on `data.bin` ZIP bytes.
- `source_game_version` must match the loaded `data.bin`.
- `provenance`: `derived-from-original` or `original-work`; required.
- `maps` is optional per map; missing maps fall back to generated/default values or no map.
- `target_size` must equal the PNG dimensions; the runtime rejects mismatches.

### 3.2 Export manifest (`to-be-upscaled/manifest.json`)

```json
{
  "format_version": 1,
  "source_game_version": "ffe-floppy-1.02-en",
  "asset_id_list_hash": "sha256:...",
  "default_scale": 4.0,
  "assets": [
    {
      "id": "base:texture/400",
      "file": "base_texture_400.png",
      "source_size": [64, 64],
      "target_size": [256, 256],
      "scale": 4.0,
      "transparency": "index",   // none | index | alpha
      "transparent_index": null,
      "kind": "texture"
    }
  ],
  "excluded": [
    {"id": "base:palette/main", "reason": "palette/lookup table"},
    {"id": "base:texture/999", "reason": "data texture, not displayed"}
  ]
}
```

- Export images are RGBA, palette applied, transparency handled:
  - `none`: all pixels opaque.
  - `index`: pixels equal to `transparent_index` become alpha 0; others alpha 255 (RGB from palette).
  - `alpha`: source already has alpha (rare; e.g. generated masks).
- For fully transparent pixels, RGB is set to 0 to avoid halo artifacts.
- `target_size` = `round(source * scale)`; per-asset `scale`/`target_size` overrides are allowed.
- Excluded assets are listed with a reason; the exclusion rule is in §3.4.

### 3.3 Validation and resizing

`pack-upscaled`:

1. Load `data.bin` asset list (upscalable assets and their original sizes).
2. Read `upscaled/manifest.json` if present; otherwise match filenames to the export manifest.
3. Check missing/extra files; unknown IDs are errors; extra files are errors unless `--allow-extra` (still not packed).
4. Check aspect ratio: `out_w/out_h` vs `target_w/target_h` within `max(1 px, 0.5%)`; otherwise error.
5. Resize to exact target with Lanczos3 if dimensions differ; preserve alpha; log `(id, from, to, method)`.
6. Verify every output has the expected dimensions and alpha mode; write canonical PNG.
7. Write deterministic ZIP + canonical `manifest.json`; compute `asset_id_list_hash` from `data.bin`.
8. Refuse to pack if `provenance` missing or invalid.

### 3.4 Exclusion rule (explained)

Upscaling changes appearance only when the asset is sampled as colour. Exclude:

- **Palettes/LUTs**: their values are colours/indices, not an image to be filtered; upscaling would create invalid entries.
- **Data textures** (height/noise/roughness inputs, masks, collision/occupancy maps): interpolation changes simulation/rendering inputs, and they are not displayed directly.
- **Font metrics/kerning tables**: not images.
- **Zero/one-pixel sentinels**: no visual benefit; resizing would break lookup semantics.
- **Assets that are never sampled by the renderer** (dead/unused): no visual benefit.

Include textures, sprites, HUD, fonts, backgrounds, cockpit art and other displayed colour assets. The export manifest records every excluded ID and reason; the rule is enforced by a test with a synthetic asset list.

---

## 4. Mod formats

`mod.toml` (required fields):

```toml
id = "mymod"
name = "My Mod"
version = "0.1.0"
authors = ["Name <email>"]
description = "…"
game_version = "ffe-floppy-1.02-en"
load_order = 100
dependencies = [{ id = "othermod", version = ">=0.1,<1" }]
```

Data files use the same RON structs as `data.bin`, with IDs under `mymod:`. A mod can:

- **add**: a new ID not present in base/earlier layers;
- **override**: replace an existing entry (requires `override = true` in the file or `[[override]]` in `mod.toml`);
- **patch**: change listed fields of an existing entry (`patch = ["mass", "price"]`), with type-checked values.

Arrays are replaced by default; `append`/`prepend` are explicit operations in the patch syntax. Conflicts (two mods override the same ID) are errors unless `load_order` makes one later and `allow_override = true` is set; the report shows both files.

Assets:

```
assets/textures/*.png
assets/models/*.glb
assets/models/*.ffe.ron
assets/audio/*.wav
assets/audio/*.mid
```

Mod asset IDs are `mymod:texture/...`, never filenames. `validate-mod` rejects a file whose SHA-256 matches a base asset; it also rejects missing `provenance` for art that is not original work.

---

## 5. Canonical JSON/RON examples and tests

Determinism tests:

- `build_data_is_deterministic`: run `build-data` twice, compare bytes and per-file hashes.
- `manifest_canonical`: parse/re-serialize, compare bytes.
- `text_escape_roundtrip`: every string in `text/` and `journal/` round-trips to the original bytes.
- `ffp_roundtrip`: FFP `{m,e}` values convert to/from the legacy 32-bit pair exactly.
- `upscaled_manifest_hash`: changing `data.bin` ZIP layout but not asset IDs/sizes leaves `asset_id_list_hash` unchanged; changing an asset size changes it.
- `pack_resizes`: synthetic images with mismatched output sizes are resized to exact targets; alpha preserved; logs recorded.
- `mod_validation`: precise file/line/field errors; base-asset hash collision rejected.
- `galaxy_isolation`: adding one system/planet does not change any other system (see `ARCHITECTURE.md` §3.4).

---

## 6. What is deliberately not chosen

- **No proprietary binary formats** for tables/text; all content is inspectable.
- **No scripting language as a data format**; v1 is declarative.
- **No float for FFP/fixed-point** in serialized simulation data; use explicit integer pairs/fields.
- **No 8-bit palette baking** in Classic assets; keep indices + 6-bit palette.
- **No dependency on `data.bin` byte layout** for `upscaled.bin`; only source version + asset ID list.
- **No per-frame screenshots in git**; only hashes/numeric dumps.

---

## 7. Approval

Please approve:

1. RON for content, TOML for mod/config, JSON for manifests/schema.
2. Indexed PNG + palette sidecar for Classic; RGBA export for upscaling.
3. glTF 2.0 GLB + `extras.ffe` plus a lossless `ffe.ron` command stream.
4. WAV 16-bit/22050 mono + SMF type 1 for audio.
5. The `data.bin`/`upscaled.bin` manifest fields and asset-ID scheme.
6. The exclusion rule and resize policy.
7. The canonical/deterministic serialization rules.
