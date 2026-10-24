# Data formats (`data.bin`, `upscaled.bin`, mods)

Status: **partial base cache implemented; full archive/pack design below remains proposed.**
A separate partial external
export now exists (see [README.md](../README.md)); its manifest is not the
`data.bin` or upscale-pack schema below. It decodes uncompressed 8-bit BMPs,
including the original zero `bfSize` cabins, preserves their 8-bit RGB palettes
and indices in PNG, and wraps signed 16-bit mono 22050 Hz PCM in WAV.
Owner answers in [CLAUDE.MD](../CLAUDE.MD)
take precedence. Use deterministic local `data.bin` plus optional image-only
`upscaled.bin`; both stay untracked. Meshes, material maps and gameplay mods below
are deferred. The export must include explicit per-image UHD guidance and a
contact sheet; correct image sizes are validated rather than silently changed.

---

## Implemented experimental save format (not legacy/native-final saves)

The playable example writes `saves/prototype-00000001.json` and increasing
eight-digit numbers. JSON has `format: "ffe-rs-experimental"`, `version: 1` and
`state`. State records mode, position/velocity vectors, yaw/pitch, integer credits/
cargo/stock/ticks and the docking-request flag. Unknown/duplicate fields, wrong
versions, non-finite/out-of-range values and inconsistent docked state fail.
Reads are limited to 16 KiB. `serde_json` float-roundtrip parsing preserves the
binary floating-point state exactly on save/load.

Each F5 creates a new file exclusively, writes and syncs it; previous snapshots
are never overwritten. A failed/interrupted write can leave a partial newest
snapshot. F9 loads only the highest number and reports invalid data without
changing live state, never silently falling back to an older snapshot. Scenario
reset does not delete snapshots. No legacy commander import/export, migrations
or compatibility promise for this development-only format. No original game
files are used as save destinations. Final crash-atomic saves remain planned.

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

### Implemented precursor: `ffe-content-cache` versions 1 through 8

The runnable `build-data`/`inspect-data` commands use the following **partial**
contract, not the full schema/examples in sections 2.1 onward.

- ZIP, stored/uncompressed entries only, manifest first; remaining entries sorted
  by ASCII path, fixed 1980-01-01 timestamp, no comments. Stored mode avoids an
  unnecessary decompressor for this small initial cache; full-cache compression
  is deferred. Identical inputs/tool versions produce identical bytes.
- Compact JSON plus final newline, with deterministic struct field order (not
  the proposed lexicographically sorted-key writer). Unknown fields/versions
  are rejected. `format = "ffe-content-cache"`, `format_version` in `1..=8`,
  `complete_game_assets = false`, and nonempty `unresolved` are mandatory.
- Manifest includes tool version, source profile, 48 inspected source records,
  and sorted `files` with stable IDs, paths, byte sizes, SHA-256 and typed `asset`
  descriptors. It does not hash itself; all other entries are covered.
- `bitmaps/primary/000.idx` ... `095.idx`, and
  `bitmaps/secondary/000.idx` ... `250.idx`. IDs are
  `base:bitmap/<table>/<three-digit-slot>`; aliases keep distinct slot identities.
  Ordinary payloads are top-down indices without headers, width*height bytes.
  Metadata preserves width, height, signed x/y drawing offsets, mask-zero bit,
  table, index and original executable header offset.
- Primary slots 94/95 have type `texture_block`, width 64 and exactly 32768
  payload bytes each. The zero header height is not converted to an invented
  image height. These blocks are not yet eligible for upscale export.
- `palettes/low.rgb6`, ID `base:palette/low`, contains the fixed low 128 RGB
  colours: 384 six-bit components. The upper 128 colours are runtime state,
  not a second guessed static palette. Preview colour expansion is not baked in.
- `cockpits/cabin0.png` ... `cabin5.png`, IDs `base:cockpit/cabinN`, are indexed
  320x158 PNGs retaining each BMP's separate 8-bit RGB palette. No alpha guessed.
- `audio/fx/<name>.wav`, IDs `base:effect/<name>`, contains all 23 original PCM
  effects with canonical WAV headers, including the empty `nowt` sentinel.
- V2 adds `fonts/main.fnt`, ID `base:font/main`, type `bitmap_font`, descriptor
  `first_code:32`, `glyph_count:116`, `rows:9`, original `source_offset`.
  The 1160-byte payload is 116 ten-byte records: nine row masks (MSB is left),
  then an advance in 1..9. Glyph codes are original bytes 32..147, not Unicode.
  V1 rejects this new asset kind; old tool versions reject v2 entirely.
- V3 adds `models/074.vtx` and `075.vtx`, IDs `base:model-vertices/074` and `/075`,
  type `model_vertices`. Descriptor fields: `index`, `vertex_count`, `scale`,
  `descriptor_offset`, `source_offset`. Count includes mirrored vertices: 54/88,
  hence 27/44 six-byte records (162/264 bytes). Original scales are 13/14.
  Record byte 0 selects the operation; bytes 2..4 contain signed coordinates,
  vertex references or a variable selector depending on that operation.
  Raw records remain byte-exact, not expanded into a guessed static mesh.
  V1/v2 reject this new kind. Counts, scale, references, cycles and unsupported
  operations are checked before exposing a payload.
- V4 adds `models/074.cmd` and `075.cmd`, IDs `base:model-commands/074` and `/075`,
  type `model_commands`, with `index`, `vertex_count`, `descriptor_offset`,
  `source_offset`. Payloads are byte-exact physical programs, 516/758 bytes.
  These extents are verified for both exact-hash releases, not inferred from an
  early conditional return or an adjacent pointer. V1..v3 reject the new kind.
  The shared core decoder walks all physical records (including alternate
  branches), rejects unknown forms/truncation, validates branch targets as
  record boundaries and bounds face vertex references. It does not execute
  visibility, transforms, submodels, render ordering or a full command VM.
- V5 adds `models/074.nrm` and `075.nrm`, IDs `base:model-normals/074` and `/075`,
  type `model_normals`, with `index`, `vertex_count`, `normal_count`,
  `descriptor_offset`, `source_offset`. Payloads contain 48/258 original bytes;
  expanded counts 18/88 include two implicit normals and mirrored pairs.
  V1..v4 reject this kind. Six-byte record shape and anchor bounds are validated;
  importer/lab additionally validate literal vertex anchors and face normal IDs.
- V6 adds `math/sine.i32`, ID `base:math/sine`, type `sine_table` with
  `source_offset`. Exactly 16,384 little-endian signed 32-bit entries (65,536
  bytes), nonnegative/nondecreasing from zero. V1..v5 reject the new kind.
  Full bytes match JJFFE `DATA_007975` in both exact-hash original executables.
  RVA is `0x106390` (floppy) / `0x105334` (CD); do not extrapolate the font delta.
- V8 adds two original tables (floppy 1.02 only; the CD 1.01 cache omits both and
  says so in `unresolved`, because its RVAs could not be checked):
  `textures/descriptors.bin`, ID `base:texture/descriptors`, type
  `texture_descriptors` with `count` (148) and `source_offset`: JJFFE
  `DATA_007811`, 148 records of 30 bytes (dword bitmap id | orientation << 12,
  dword colour count 1..=7, 7 nibble RGB triples, a zero pad byte; record 0 all
  zero), RVA `0x102da8`; and `palettes/fixed-map.bin`, ID `base:palette/fixed-map`,
  type `fixed_colour_map` with `source_offset`: JJFFE `DATA_007770`, 4096 bytes,
  one per 12-bit colour, 0 or a low-palette index + 1, RVA `0x101b10` (directly
  after the low palette). Both match the JJFFE tables byte for byte. V1..v7 reject
  the new kinds. Floppy 1.02 v8 has 1698 assets (v7 1696). Semantics:
  `docs/notes/textures.md`.
- V9 adds `lighting/default.bin` (`base:lighting/default`, type
  `default_lighting`, 36 bytes: the 3-word default light + the 8 x 3 shade table,
  JJFFE `DATA_007231`, RVA `0x0eec60`) and `lighting/star-shades.bin`
  (`base:lighting/star-shades`, type `star_shades`, `count` 6, six 24-byte
  tables, JJFFE `DATA_007759`, RVA `0x101858`). Floppy 1.02 only.
- V10 adds `models/descriptors.bin` (`base:model/descriptors`, type
  `model_descriptors`, `count` 466, 8 bytes per slot: descriptor +0x18 and +0x1c)
  and `ships/stats.bin` (v12, `base:ship/stats`, type `ship_stats`, `count` 466,
  48 bytes per slot from descriptor +0x38, zero-filled for null pointers).
- V11 adds `text/model-names.bin`, ID `base:text/model-names`, type
  `model_names` with `count`, `source_offset`: the readable model string table
  (`DATA_004682`, RVA `0x0d1444`), NUL-terminated entries in table order, entry n
  naming model slot n. The table is **scanned**, not cut at the 209 entries the
  JJFFE 2.8a7 declaration lists: the release executable continues past them (219
  entries on floppy 1.02, including "police" and the First Encounters logo), and
  the model text commands index the same table as `0x4000 | n`. A scan below 209
  entries is rejected as a misplaced table. Floppy 1.02 only.
- V13 extends that table to the scanned length (219 entries; v11/v12 caches hold
  the first 209, which left six text/label codes above 208 unresolvable). No new
  asset kind or path changed, so only the payload and its `count` differ.
- V14 adds `models/000.glp` ... `002.glp`, IDs `base:model-glyphs/000..002`,
  type `glyph_programs` with `index`, `count`, `first_char` (0x20),
  `vertex_count`, `scale` and `source_offset`: each font model's per-character
  command programs, as `count` little-endian u32 start offsets relative to the
  payload's block, then the block. Characters may alias a program (lowercase to
  uppercase; font 1 defines 52 of the 95 possible entries), so the starts are
  not sorted. Every distinct start must begin a program that decodes over the
  font model's vertices. Semantics: `docs/notes/text.md`.
- V13 has 1778 assets; v14 has 1781; v15 has 1782 (adds `base:world/system-tables`,
  `world/system-tables.bin`, 20,984 raw bytes of the executable's `DATA_007106..7138`:
  the 329-record galaxy table of 54-byte records at offset 432, the four special
  system codes at 18,252 and the system generator's tables from 18,492; see
  `ffe_core::galaxy`. The relocated handler pointers at 19,048 are not used).
- Total: v1 has 377 assets/378 ZIP entries; v2 378/379; v3 380/381; v4 382/383;
  v5 384/385; v6 385/386; v10 1776; v13 1778 (v11 and v12 add one asset each to
  v10). Tools read every version. The playable requires v2 font content;
  F4 requires v3 station vertices and reports an upgrade error for v2. V3 keeps
  points-only inspection; v4 adds declared-face inspection. The lab requires
  matching command/vertex model indices, counts and descriptor offsets.
  V5 adds a fourth, normal-filtered outline view, requiring matching normal/vertex
  model indices, vertex counts and descriptor offsets.
  No submodels, gameplay tables, other font data,
  journal, music, CD media or replacement art. No version can
  label any archive as complete game content.
- Reader limits: 64 MiB archive/aggregate payload, 2048 assets, 4 MiB manifest.
  Unsafe/duplicate/unsorted paths, duplicate IDs, symlinks, extra/missing entries,
  unsupported compression/version, CRC/digest mismatch and invalid basic typed
  shapes fail explicitly. Entries are read in memory, never extracted by path.
  SHA-256 is an integrity check, not an authenticity signature; source freshness
  is not automatically compared to the installation by `inspect-data`.
- Builds never overwrite an existing output or write into the supplied game
  directory. Interrupted writes are reported; retry with a new destination.
  Cache files stay local and ignored; no source/assets are bundled with tools.

Verified PE DATA layouts, selected only after exact executable hash/size:

| Profile | Primary RVA | Secondary RVA | Low palette RVA | Main font RVA |
|---|---|---|---|---|
| English floppy 1.02 | `0x1b069c` | `0x1b081c` | `0x101990` | `0x1013b0` |
| English CD 1.01 | `0x1af640` | `0x1af7c0` | `0x1009b0` | `0x1003d0` |

The model pointer table is at RVA `0x0d0cfc` (floppy) / `0x0d0cf8` (CD),
not a guessed offset from the bitmap/font layout difference. Slots 74/75 map
to JJFFE `DATA_002907`/`DATA_002912`. Descriptor +4 points to the six-byte
vertex records, +8 is the expanded vertex count, +20 is the scale. The importer
follows both pointers only through file-backed DATA and preserves both raw offsets.
The payloads match `DATA_002905`/`DATA_002910` exactly in both releases.
Descriptor +0 points to commands `DATA_002903`/`DATA_002908`, now imported in v4.
Descriptor +12 points to normals `DATA_002906`/`DATA_002911`, imported in v5;
+16 gives the expanded normal count. Child models are not yet imported.

Normal evaluation follows F1480/F1481: entries 0/1 use shade slots 3/4 and remain
visible. Other entries use signed normal components shifted eight bits, mirrored
X components/anchors, scale-difference shifts masked to five bits, and wrapping
32-bit products/sums. A nonnegative observer dot culls; a nonnegative light dot
returns black, otherwise `(dot >> 27) & 7` selects the supplied shade triplet.
This is not a wide-dot or floating-point approximation. The lab uses its own
observer/light/shade inputs, not original gameplay instance state.

The command word's low five bits dispatch the operation. This bounded decoder
supports the record forms encountered in these two root programs: return,
point/line, triangle/quad (ordinary/reflected), unified-z blocks/lists, text,
conditional visibility/bit branches, arithmetic, submodel call, colour and basic
matrix transform. Recognition is structural, not a promise those operations
execute. Triangle payloads are material word, vertex bytes A/B, **normal byte**,
vertex C; perimeter is B/A/C. Quad payloads are material, A/B/C/D, normal word;
perimeter is C/A/D/B. Reflected commands emit XOR-1 vertex/normal IDs first, then
the ordinary face. Material descriptors retain raw nibble RGB, texture index,
unlit and local-colour flags; no guessed DOS shading/palette is baked in.

Separate native F1758 helpers implement 14 arithmetic operations and F1753/1756/
1757 bit-branch outcomes. Signed/wrapping/unsigned arithmetic, x86 shift masking,
16-bit destination writes and signed branch offsets are explicit. Legacy signed
division by zero preserves its numerator; signed overflow and unsigned divide
by zero report errors without committing locals. Global lookup is bounded to
the context's 64 slots. V0.6 adds sine/cosine using an explicitly supplied
`SineTable`; the earlier no-table arithmetic entry point still errors for them.
Lookup masks angles to 16 bits and reflects index `0x3fff - index` in odd
quadrants, not a guessed extra endpoint. F1744 rotation uses per-term Q31
truncation/wrapped addition and negates the source before multiplication.
Axis 3 reports an error; original undefined pointer use is not reproduced.
The packed signed angle operand (F1474) preserves its split/reordered literal
bits; selectors `0x80..bf` read unsigned global words and `0xc0..ff` signed local
values, with higher encoded bits ignored for selectors. The pure `0x8000`
transform form resolves this operand and updates the local basis via F1742/F1743
semantics. Other transform forms explicitly error in this helper; structural
decoding is not a full transform dispatcher. These helpers are verified separately,
not used to approximate full-program execution in the diagnostic catalogue.

F1751 normal/LOD visibility is also implemented as a bounded helper. Negative
operands select the evaluated normal's cull flag by the low 15 bits. Nonnegative
operands compare a signed depth against a shifted threshold, preserving the
legacy shift round-trip overflow check and x86 count masking. Primary detail
bit 0 selects the secondary detail value; nonpositive combined scale leaves
the threshold unshifted. F1752/F1755 use the condition/inverse with the same
relative offsets/conditional return as bit branches. Out-of-range normal IDs
fail explicitly. These helpers do not execute a complete station program or
manage the original lazy-normal buffer.

V0.7's bounded executor consumes the validated physical stream and follows actual
control flow for return, arithmetic, bit/visibility branches, pure local-basis
rotation and triangle/quad operand emissions. It records repeated instruction
visits and emission-time context snapshots. Budget is 1..4096 executed commands,
including the terminating command. Outputs and context changes are committed
only after an unconditional or conditional return; failures expose no partial
emissions and leave the caller's context unchanged.
Local selectors 18..63 are rejected when reached: those legacy local slots alias
matrix/other instance fields, which are not interchangeable with independent
native variables. At that milestone, point/line/text, depth-order blocks,
submodels, colour state and other transform forms were explicit execution errors.
Structurally known but unreached operations may be skipped by a branch. Normal
predicates consume supplied evaluated flags, not a managed original lazy cache;
face events preserve operand order without evaluating vertices, culling, shading
or rasterizing. Both actual station roots stopped at opcode 6 (offset 0xc/0x8);
v0.11 later composes that stateful operation. Skipping it would not have
constituted original station execution. No cache schema or payload changes are
needed for this milestone.

V0.8 adds the standalone F1777 render-order key helper. A view-space signed
`[x, y, z]` point and instance scale at `+0x130` produce a `u32` key. Nonpositive
z returns 1. Otherwise wrapping absolute x/y magnitudes are added, arithmetically
shifted by three and added to z. The scale-adjusted shift preserves x86 count
masking. If a left shift would reach the original unsigned overflow threshold,
the key packs the shift, high-bit flag and arithmetic `z >> 7` instead of
saturating a distance. Do not use a generic distance replacement here.
The 17,303-case comparison
validates only this key helper, not opcode 6, sorting, group allocation or pixels.

V0.9 adds F1696 depth-command decoding/resolution, shared by the physical record
decoder. Modes 0/4/5/6 close the active group; mode 2 uses one signed-byte vertex
index; modes 1/3 select signed maximum/minimum keys from the header vertex and
at least one operand pair. A pair's low byte is another signed vertex index;
high byte 255 continues, any other high byte terminates the list. Mode 7 uses:

- Header `word >> 5 == 0x7ff`: instance position at +4.
- `0x7fe`: key `0xffffffff`.
- `0x7fd`: point `[0, 0, signed_operand << operand_scale]`, consuming one word.
- Other forms: selected vertex plus the same wrapping shifted z offset,
  consuming one word.

Key scale is instance +0x130; operand scale is +0x148. Do not conflate them.
Likewise the instance frame at +0x14 (`VertexContext::matrix`, used by vertex
evaluation) is distinct from the current basis at +0x9c
(`VertexContext::basis`, changed by opcode 28 and read by child/text frames).
Resolution accepts explicit already evaluated view-space points. Missing or
negative/external indices fail; this is not the original lazy vertex resolver.
The original handler comparison primes its vertex cache with synthetic points
and verifies 11,700 mode/scale/shift/point/initial-group combinations, including
operand consumption, resulting active flag, key and original allocation size.

`model_order::Order` models flat F1778/F1780 groups and F1788 ordering as owned
primitive tokens, not executable function pointers or serialized cache bytes.
An active group absorbs member tokens without using their individual keys.
Opening another group closes the preceding one; empty groups are valid.
Finalization requires explicit closure. Sort groups/standalone tokens by
descending **unsigned** key; ties prefer the earlier allocation. Member order
is unchanged. This differs from the **signed** extrema comparisons above.
128 synthetic sequences match the original allocator/sorter/group dispatcher.
The native 1..4096-operation budget counts group changes and pushes, not
the original 50,000-word arena. Budget errors leave queue state unchanged.
Nested child-model lists, forced-header records, real primitive storage,
lazy instance state and rasterization remain outside this helper. Consequently
the composed model executor still rejects reached opcode 6; no static-point
fallback is substituted for animation/transform-aware vertex resolution.

V0.10 adds F1479 integer projection and bounded live view-space vertex/instance
slots (`ffe-core::model_instance`). `project([x,y,z])` rejects `z < 64`, divides
by `z >> 8` when `z > 65536` and `value << 8 / z` otherwise, adds the 160/79 view
centre, then arithmetically shrinks x/y while their wrapped magnitude exceeds
`0x1f00`, returning the 320x158 logical viewport pair. Rejected points leave the
caller's screen words untouched; callers that require a value use `NEAR_SENTINEL`
`[-32766, -32766]`. `Vertices` holds bounded object/view/screen/stamp slots; a
zero stamp re-evaluates the object program (F1478), translation (F1470) computes
view but does not advance the stamp, and projection (F1471) stamps the slot with
the caller's generation. A matching stamp short-circuits evaluation exactly as
the original command callers do. Generation 0 and scale > 31 are explicit errors;
no external, screen-derived or random vertex forms are evaluated. This is not
submodel, clipping or rasterization.

V0.11 composes these helpers in `model_execute::execute_live`: depth commands
resolve vertices through the live view-space slots, compute F1777 keys, and drive
F1780/F1788 groups; face emissions stay command-ordered in `Execution::emissions`
and are also returned render-ordered in `Execution::ordered`. Standalone faces key
from their first emitted view vertex (F1470 view only; projection is not needed
for ordering). `execute_live` derives the vertex count from
the live cache, resolves depth vertices with F1470 (view only), and commits
context/slots/queue only after a real return and a closed queue. It still does not
evaluate geometry, normals, clipping or pixels; point/text (1/2/10/14), child
models (26), other transform forms and colour state remain explicit unsupported
commands, and the abstract `Order` token queue is not the original 50,000-word
arena.

V0.12 adds `Execution.ordered_keys` and `model_order::Order::finish_with_keys`
so effective group/standalone keys are inspectable. The local `--composed-order`
mode drives original F1696/F1777/F1778/F1780/F1788 with marker callbacks; Rust
matches order `[2,3,1]`, keys `[9,9,0]`. This is a helper composition, not the
original dispatcher or face handlers. See `docs/MODEL_COMMANDS.md` for the full
F1772 opcode map and the probe-first plan for point/line/text/submodel.

V0.13 decodes and emits opcode 1 (point source) and opcode 2 (line). Point
records carry material, a raw F1474 packed angle, `payload[4] & 0x80` and the
vertex byte; line records carry material and two vertex bytes. Both validate
vertex references. `execute_live` puts them into the same `Order` stream as
faces; `Execution.ordered` is now `Vec<Primitive>` (`Face`/`Point`/`Line`) with
aligned `ordered_keys`, while `emissions` stays face-only command order.
Standalone points/lines key from their first view vertex. This is operand
emission, not geometry, clipping or rasterization. Text (10/14), submodels
(21/26), other transforms (28) and colour remain unsupported; see
`docs/MODEL_COMMANDS.md`.

Pointers are mapped through the original PE32 image base and file-backed DATA
section, never through offsets from the newly linked JJFFE binary. Both layouts
match all 347 full table records and the low palette in the JJFFE declarations.
JJFFE counts are 96/251; historical FFED3D counts elsewhere must not be reused.
The main font bytes are identical in both releases and match concatenated
JJFFE `DATA_007750`/`DATA_007751`. Rust glyph pixels/advances match executed
F1598/F1597 in 696 cases; masked/opaque clipping matches F1623 in 28 cases.
The low palette expands by left shift 2 (max 252), not by normalizing to 255.
Cabin palettes are separate 8-bit colours. No guessed runtime upper palette is
used. RGB composition is still a development presentation boundary, not a
complete original dynamic-palette implementation.

The bounded vertex interpreter lives in `ffe-core::model`; no I/O or floats.
Supported operations include literal/reflected pairs, negation, midpoint,
vector add/subtract and variable interpolation. Unsupported screen-space,
random or curve forms and external references fail explicitly rather than
returning zero. Signed Q31 multiplication truncates per term exactly as F1521,
then adds with 32-bit wrapping. A nominal identity uses `0x7fffffff`, not a
mathematically exact +1; preserve its low-bit asymmetry. Midpoints wrap the
sum before truncating towards zero. Variable selectors choose literals, scaled
literals, global u16 or local i32 values. Interpolation uses the original
shift/Q31 sequence, not floating lerp or division by 65535.
Local differential validation covers both station programs, 192 contexts and
13,632 evaluated vertices. It is not a claim of original drawing/projection.

### Proposed full-content layout (not implemented)

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
- Initial Windows playback may synthesize the MIDI locally. If pre-rendered
  playback is simpler, render through a local synth/SoundFont to PCM, then
  optionally FLAC/Ogg/MP3. MIDI-to-MP3 is synthesis, not direct event conversion.
  Preserve loop/trigger timing, account for encoder delay, and include synth,
  SoundFont hash and rendering settings in cache provenance. No new audio is
  bundled without rights review; retain lossless effects and original event data.

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

Owner workflow: write eligible RGBA images under `images/`, plus this manifest,
`sizes.csv`, `excluded.json`, and a local offline `index.html` contact sheet.
The owner returns images under `upscaled/images/` with the same names. Inventory
embedded and external images; do not label a cabin-only export complete.

The manifest/CSV must also carry source hash, category, logical rectangle/atlas
region, intended display coverage, recommendation basis, warnings/caps and
explicit recommended width/height. The initial profile is 3840 x 2160. Use
`ceil(max(display_width/source_width, display_height/source_height))` as an
aspect-preserving scale estimate for a known view, then optionally round up to an
upscaler-friendly factor. Mark scene-dependent estimates and retain overrides.
The Classic 4:3 image occupies 2880 x 2160 on a UHD display; full-canvas art may
need 11x or a convenient 12x, while a 64 x 64 image used at 256 x 256 needs 4x.
There is no universal "4K texture size." See [CLAUDE.MD](../CLAUDE.MD).

The example below is illustrative; extend the implemented schema with those
fields before the exporter is considered complete.

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
2. Read the returned manifest or original export manifest and match stable IDs to
   files under `upscaled/images/`.
3. Allow partial packs, reporting omitted IDs; unknown IDs and unexpected extra
   images are errors. A partial pack is not a claim of complete coverage.
4. Require the declared target dimensions to match decoded PNG dimensions and
   the source aspect/layout contract. Never silently stretch image or atlas data.
5. Reject wrong-sized images with expected/actual dimensions. Provide a separate
   explicitly requested resize operation with alpha preservation and a report.
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
- `pack_dimensions`: wrong dimensions are rejected; explicit resize is separate,
  preserves alpha and reports every change.
- `export_uhd_guidance`: known screen-coverage cases produce explicit aspect-
  preserving sizes; atlas/alpha metadata, caps, exclusions and partial coverage
  are represented accurately in manifest/CSV/contact sheet.
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

## 7. Detailed design checklist

These are design topics to validate during implementation, not a repeat request
for owner approval. Follow [CLAUDE.MD](../CLAUDE.MD); GLB/material/gameplay-mod
parts are future scope, not requirements for the first image exporter.

1. RON for content, TOML for mod/config, JSON for manifests/schema.
2. Indexed PNG + palette sidecar for Classic; RGBA export for upscaling.
3. glTF 2.0 GLB + `extras.ffe` plus a lossless `ffe.ron` command stream.
4. WAV 16-bit/22050 mono + SMF type 1 for audio.
5. The `data.bin`/`upscaled.bin` manifest fields and asset-ID scheme.
6. The exclusion rule and resize policy.
7. The canonical/deterministic serialization rules.
- V16 has 1784 assets (adds `base:model/records`, `models/records.bin`, 20 bytes per model
  table slot (20 bytes, not 16): the descriptor dwords `+0x14` and `+0x18`, the byte `+0x24`, the
  words `+0x2c` and `+0x30`, a flag that the collision pointer `+0x34` is non-null with a
  non-zero first byte, a flag that the statistics pointer `+0x38` is non-null, and the word
  `+0x2e` at 16; and
  `base:gameplay/tables`, `gameplay/tables.bin`, 647 raw bytes of `DATA_007207..7225`).
- V17 has 1787 assets (adds the planet engine's data):
  `base:terrain/tables`, `terrain/tables.bin`, 7,539 bytes of `DATA_007836..7899`
  (RVA `0x104165`) with the code pointers replaced by ranks (type table: first
  column 1 to 4, second column 1 to 3; midpoint table 1 to 19, one per routine
  F1849 to F1867; the height-map pointers 1 and 2), checked against the known
  pattern on import;
  `base:terrain/maps`, `terrain/maps.bin`, the two 128 by 128 signed height maps
  `DATA_007637` and `DATA_007638` (RVA `0xf7d6c`, 32,768 bytes); and
  `base:model/collision`, `models/collision.bin`, one triple of little-endian u32
  (offset, stream length, bytes of backward region words before the stream; length 0
  for none) per model table slot followed by the data: for each model the backward
  words (F576 reads them in pairs backwards from the stream's start until a zero
  distance word; 51 models have a chain) and the collision stream (descriptor
  `+0x34`) cut by walking the stream grammar of F582 (465 of 466 slots have one).
- V18 has 1788 assets (adds `base:text/names`, `text/names.bin`: the 120 bytes of the
  name range tables `DATA_006274..6279` (RVA `0xdf96c` + 24) and then the 510
  NUL-terminated strings of the pointer table `DATA_006272` (RVA `0xdf174`) in table
  order).
- V19 (still 1788 assets) appends to `base:text/names` the 7,706 bytes of the hand-coded
  systems' text from `DATA_007139` (RVA `0xec3d0`) to the end of `La Soeur du Dan Ham`:
  the names F873/F874 copy by offset (`Sol` is at `0x18bd`).
- V20 has 1789 assets: adds `base:market/tables`, `market/tables.bin`, 704 bytes of
  `DATA_006164..6169` (RVA `0xdde08`; the supply, population and danger multipliers and the 33
  commodities' four dwords) followed by the 38 commodity names `DATA_006216..6253` (RVA `0xde970`,
  contiguous), each with its zero; floppy 1.02 only. See `docs/notes/market.md`.
- V21 has 1790 assets: adds `base:world/galaxy-tables`, `world/galaxy-tables.bin`, 18,916 bytes: the hand-made star
  vectors (117 of 6 bytes) and type words (`DATA_006793..6795`, RVA `0xe094c`, 936 bytes), `DATA_006799..6811` (RVA
  `0xe1154`, 17,740 bytes: the 49 special sector keys, their 52-dword index, `DATA_006802`, `DATA_006803/6804`, the
  576-byte hash table, the 128 by 129 byte density grid and the two 32-byte star type tables) and the 15 hyperdrive
  records of 16 bytes (`DATA_007187..`, RVA `0xee224`, 240 bytes); floppy 1.02 only. See `docs/notes/hyperspace.md`.

