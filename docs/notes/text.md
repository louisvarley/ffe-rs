# Model text (opcode 10, F1747) and labels (opcode 18, F1763/F1764)

Status (2026-10-23): **ported**. Labels (opcode 18) draw in the original font;
3D text (opcode 10) runs each character's glyph program through the executor in
a frame at the text's position, following the original F1747. The string table
is imported in full (cache v13) and the per-character glyph programs in cache
v14. What remains is listed under "Not done" (control codes, `0x3022`'s wrap,
`0x9c0x`, depth interleaving, glyph lighting, and a reference oracle for text
pixels/queue order).

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

Entry n is **not** model table slot n: it is the string for ffcode `0x4000 | n`.
A model's code is the `+0xe` word in its statistics block, so several slots share
a name (0x3d/0x46 are both Lynx Bulk Carrier) and the table also holds UI and
decal strings. `docs/notes/ship-names.md` maps this for ships; the text
commands themselves resolve `0x4000 | n` directly.

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

## F1747's glyph placement (ported)

The handler resolves the position vertex (F1470) and calls F1740 with the
record's frame word, then F1745 (push a nested command/instance frame). It
expands the string with F1344 and walks the **expanded** bytes:

- `byte >= 0x20` loads the pointer at descriptor **+0x40 + 4 * (byte - 0x20)**.
  That pointer is the start of the character's own command program inside the
  font model's command block. F1747 calls **F1772** on it in the nested frame
  (so the glyph's faces/points are queued), then reads the word at the returned
  cursor - 2: that is the program's **return record**, and its bits 6.. name the
  font vertex that becomes the **next pen position** (F1470, copied into the
  nested frame's position).
- If the glyph is culled (F1686 returns non-zero), F1747 does not execute the
  program: it reads the *next* entry (the same table at +0x40 + 4*(byte-0x20),
  which is +0x44 relative to the previous entry) minus 2 — the current program's
  return word — and only advances the pen.
- `byte < 0x20` is a control/selector byte: the handler advances a component
  counter and, when it matches a selector byte at `glyph+0x3c`, resolves that
  glyph-model vertex and moves the pen. These are the font's own layout anchors,
  not a generic newline, and are not imported.

Finally F1746 pops the frame.

**Imported (cache v14).** `glyph_programs` reads the three font models' tables
(slots 0, 1, 2). Font 0 defines all 95 characters (0x20..=0x7e), font 1 the
first 52 and font 2 all 95; several entries **alias** one program (lowercase
shares the uppercase glyph), so the starts are not sorted and the distinct ones
delimit the programs. Every program must decode as a command stream over the
font model's vertices and end in a return word naming an in-range pen vertex; a
set whose layout differs fails the import. The asset is
`base:model-glyphs/NNN` (starts then the block).

**Rust.** `ModelViewer::draw_texts` resolves the text vertex, builds the F1740
frame from the record's word, then for each character runs the glyph program
with `execute_live` (a fresh font vertex cache, the text frame, full detail) and
converts its primitives through the same path as the model's own. The pen moves
to the resolved anchor vertex. `0x3016` and `0x3022` expand the drawn object's
string (object +0x124); the F6 viewer defaults it to the model's own name, and
`set_registration` supplies a real one.

## Corrections (2026-10-24)

- **The glyph scale is the record's size nibble.** F1747 stores `byte 3 & 15` at the nested instance's `+0x148`, the
  operand scale the font vertices are shifted by; the earlier port used the font model's own scale (0, 8 and 0), which
  made every font 0 and font 2 text a few units wide. Sizes run 1..15 (decals 6..8, station signs 10..12). The census:
  every (font, size) pair of the 236 records.
- **Control codes are new lines.** A byte below 0x20 returns the pen to the line's start and moves both to the font's
  vertex `descriptor +0x3c` seen from there (cache v24 `newline_vertex`: 58, 38, 20). The nested instance's generation
  is a counter (2, then +1 at every pen move), so each glyph resolves the font's vertices afresh.
- **Child models' texts are drawn**: the lift's and the bay's posters are child models (`0xeb`) whose texts ran but were
  never drawn (`ModelViewer::draw_child_texts`).

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

- **Control codes below 0x20** (the font's `+0x3c` component anchors): a string
  containing one stops there and the record is counted in `Frame::skipped`.
  This affects the multi-line logo/station strings, not the ship registrations.
- **`0x3022`'s wrap.** It is expanded unwrapped: F1398 needs the 256-byte
  character-width table `DATA_007648`, which is not imported.
- **`0x9c00`–`0x9c03`**: `DATA_005104` string table (planet/system names) and its
  FF80 handler (index 14).
- **Depth interleaving.** The original queues glyph geometry into the same depth
  order as the hull; the viewer draws text after the model's primitives.
- **Glyph lighting.** The glyph faces are drawn with all normals visible and no
  normal colour (material + instance colour only); the font model's own normal
  block is not evaluated in the text frame.
- **The culled-glyph path** (F1686/F1687 screen cull): every glyph is executed.
  The pen anchors make both paths equivalent in position.
- F1583's clipping metrics and the pointer pick test in F1764.
- **No reference oracle for text yet.** The `trace_text_*` hooks record the
  F1747 nested calls, but no probe mode runs a real text record against the
  original and compares queued glyph geometry. That is the next verification
  step; the port is currently checked by synthetic tests and the local census
  (models 15, 30, 66, 453 render text with nothing skipped).
