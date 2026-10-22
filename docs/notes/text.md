# Model text (opcode 10, F1747) and labels (opcode 18, F1763/F1764)

Status (2026-10-23): **labels ported and verified structurally**; the string
table the text commands index is imported in full (cache v13). The 3D glyph
placement F1747 performs is **read but not ported**. This note is the starting
point for that step; `docs/STATUS.md` records it as the open part of milestone
v0.48.

Rust: `ffe_rs::model_viewer::Shape::Label` (F1812: the string at the projected
screen point, colour 15, clipped, unshadowed), `ModelViewer::model_strings`
(cache v11+/v13), `ModelViewer::set_model_strings` for synthetic viewers.
Census: `crates/ffe-rs/tests/text_census.rs` (local-only).

## What the table holds

`ffedat.asm` declares `DATA_004682` as 209 pointers ("Ship string table"), but
the release executable's table continues to **219** entries; the extra ones
include `police` and the First Encounters logo. Because the commands index the
same table through `0x4000 | n`, the importer scans pointers (a pointer into the
data section holding a NUL-terminated printable string) instead of trusting the
declaration. Cache v13 carries all 219 strings; v11/v12 carried 209, which left
every code above `0x40d0` unresolved.

## The census (floppy 1.02, cache v13)

Decoding every cached model (`text_census`):

- **236 opcode 10 text records** in 76 models, with 136 distinct codes:
  - 130 are model strings (`0x40xx`), all resolvable with the 219-entry table.
    They include ship decals (`0x40af`–`0x40cd`: `0J`, `0K`, registrations),
    station markings (`DOCK OTHER SIDE`, `50 METRES`), and the logo art.
  - 6 need a handler: `0x3016` (50 models), `0x3022` (10), and `0x9c00`–`0x9c03`
    (models 172 and 175).
- **Glyph sets** (`byte 3 >> 4`, the F1539 model selector): 0 (76 models),
  1 (8 models), 2 (26 models). Those are model table slots 0, 1 and 2, all
  already imported with every other model.
- **6 opcode 18 labels** (models 66 and 453), all `0x40xx` model strings.

## F1344's dispatch (read from the assembly)

`F1344(dest, code, StrVars *vars)` stores `vars` in `DATA_009149` and calls
`F1343`:

| code | handler | notes |
|---|---|---|
| `>= 0x8000` | `DATA_007649[(code >> 9) & 63]` with `code & 0x1ff` | FF80 family; index 14 is `FUNC_000277`, which indexes the static string table `DATA_005104` (used by `0x9c00`–`0x9c03`, index 0). |
| `0x4000..0x7fff` | `F1345`: `DATA_004682[code & 0x3fff]` | the model string table; then nested FF codes are expanded (F1348). |
| `0x3000..0x3fff` | `DATA_007652+52[(code - 0x3000)]` | FF30 numeric/date/string writers. `0x3016` is `F1364` (write `vars[3]`, i.e. `vars+0xc`), `0x3022` is `F1392` (the same string, wrapped at 35 columns by F1398). |
| `< 0x3000` | copied as text | not used by model text. |

`F1747` fills the `StrVars` it passes with exactly one field: **`vars+0xc =
object + 0x124`**. So `0x3016` and `0x3022` both expand the object's own string
(the ship's registration/name) — no uninitialised field is involved, and no
other game state is read for the codes the models actually use. `0x3022` adds
F1398's word wrap, which needs the 256-byte character-width table
`DATA_007648` (not yet imported).

## F1747's glyph placement (read, not ported)

The handler resolves the position vertex (F1470) and calls F1740 with the
record's frame word, then F1745 (push a nested command/instance frame). It
expands the string with F1344 and walks the **expanded** bytes:

- `byte < 0x20` is a control/selector byte: the handler advances a component
  counter and, when the counter matches a selector byte at `glyph+0x3c`,
  resolves that glyph-model vertex and copies its view position into the nested
  instance.
- `byte >= 0x20` indexes the glyph model's per-character dword table at
  descriptor **+0x40 + 4 * (byte - 0x20)**. When the first word at that pointer
  is non-zero, its bits 6.. identify a glyph-model vertex; the handler resolves
  it (F1470) and copies the view position into the nested instance.

Finally F1746 pops the frame. So the glyph geometry itself is a nested model
instance whose per-character positions the text handler computes; the glyph
programs live outside the font model's 10-byte top-level extent and are not
imported yet. That is the next step: import the per-character program extents
(bounded, from descriptor +0x40) as a new cache asset, run each glyph program
through `execute_scene` at the computed position, and compare against the
original with the existing `trace_text_*` probe hooks (which already record
F1470/F1539/F1740/F1745/F1344/F1746 arguments).

## Labels (ported)

Opcode 18 decodes to a vertex and a string word. `execute_live` projects the
vertex (F1471), queues an F1812 entry only when the point lies in x 2..=317,
y 4..=149, and keys it by its view. The viewer resolves the string as a model
string and draws it with the original bitmap font at the projected point in
colour 15, clipped to the view — F1812 calls `F1344` then
`F1583_TextWriteDefaultColor`. Unsupported codes are counted in
`Frame::skipped` instead of being drawn as something else.

Station 453's five labels and ship 66's one label resolve with cache v13.

## Not done

- The 3D glyph instance above (opcode 10 geometry).
- `0x3016`/`0x3022` expansion: needs `vars+0xc` (the object's string at +0x124)
  and, for `0x3022`, the `DATA_007648` width table for F1398's wrap.
- `0x9c00`–`0x9c03`: `DATA_005104` string table (planet/system names) and its
  FF80 handler.
- F1583's clipping metrics and the pointer pick test in F1764.
- Any pixel comparison of drawn text.
