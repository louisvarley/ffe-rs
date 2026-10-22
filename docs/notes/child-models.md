# Child-model execution (opcodes 14/27, F1748..F1750, F1773/F1774)

Status: ported and verified against the executed original (2026-10-22). This note
records how the original sets up a child instance, what `ffe-core` now does, how it
was checked, and what remains. It does not replace `docs/PORT_LOG.md`,
`docs/STATUS.md` or `docs/MODEL_COMMANDS.md`; fold it into them when merging.

## The original call path

`F1749` (opcode 14) and `F1748` (opcode 27) both end in `F1750(parent, cursor,
vertex*, colour, scale, model, frame)`:

1. **Position vertex.** The handler resolves byte +0 through `F1471` (projected,
   behind the generation check). The returned cache entry pointer is the child's
   position source.
2. **Colour.** Opcode 14 passes the parent colour (+0xf4 dword). Opcode 27 runs its
   word +4 through `F1692`/`F1694`: nibble RGB (zero when textured, bits 14-15) plus
   the parent colour when bit 12 (local) is set; no normal colour.
3. **Scale argument.** Opcode 14: 0. Opcode 27: byte 3 signed high nibble, or
   local `byte3 >> 4` when bit 3 is set.
4. **Descriptor.** `F1538(code >> 5)` (no bounds check).
5. **Child scale** `s = argument + desc[+0x14] + desc[+0x18] - parent[+0x130]`.
6. **Cull** `F1687(view, desc[+0x1c] << s)` (x86 shift, count masked to 5 bits):
   culled when `z + r < 0x40`, or `z + r < |x| - r`, or `z + r < |y| - r` (signed;
   `neg` leaves `i32::MIN`). A culled call only advances the cursor (+4 with extras).
7. **Extra vertices** (frame word bit 15): `F1471` on +4 (always, no 0x7f test),
   then +2, +3, +5 up to the first 0x7f.
8. **Instance** (on F1750's stack, 0x160 bytes, otherwise uninitialised):
   `+0x4..0xc` position = parent entry view; `+0x14c` = parent's (globals);
   `+0x130` = parent's (key scale); `+0x148` = `s`; `+0x54` = parent local 0 + 1,
   `+0x58/+0x5c` = parent locals 1/2; `+0x70` = position entry pointer,
   `+0x74..+0x80` = extra entry pointers; `+0xf4..0xf6` = colour.
9. **`F1740(child, parent, frame)`:** mode `frame >> 8 & 7` = 6 copies the parent
   basis (+0x9c) to the child frame (+0x14) and +0xfc to +0xf8; any other mode
   calls `F1739` on the parent *frame* (+0x14, +0xf8): row permutation 0..5 (6 = 0,
   7 writes through uninitialised pointers), bits 0x2000/0x1000/0x800 negate source
   rows 0/1/2; odd permutations and each negation flip the handedness word. Then
   `memcpy(+0x84, DATA_008831, 24)` (shades = child locals 12..17). Unless the
   frame has 0x4000: copy parent +0x134..0x13c (light) and run `F1685`.
10. **`F1685`:** `(t, n) = F1475(position)` (shift right until the OR of magnitudes
    is below 0x4000); `+0x44 = n`; if `n + 7 < s`, raise +0x44 to `s - 7` and shift
    `t` further; `+0x48 = F1670(-t, frame)` (observer), `+0x38 = F1670(light, frame)`
    (`F1670`: each frame row dotted with the vector through `F1521`).
11. **`F1773 -> F1774(child, desc, desc[+4], desc[+8])`:** `F1745` reserves an arena
    frame of `12 + (6 + n) * 36 + normals * 8` bytes in the 0x8000-byte `DATA_009251`
    (saved cache/normal pointers, **six external cache slots**, n own slots, normals,
    frame word); without room the child is silently skipped. Then +0x150/+0/+0x154
    from the descriptor, the `+0x44` raise to `s - 8` (never positive after F1685),
    `F1689` (zero every stamp, external slots included), `F1480` (normals 0/1 from
    shades 3/4, the rest lazy through `F1481` with +0x38 light, +0x44/+0x48
    observer), `+0xf0 = 2`, `F1772`, `F1746`. No `F1780(0,0)` close: a group the
    child leaves open stays open in the parent; child primitives go into the same
    queue at the point the child runs.

**External vertices (F1478).** A negative vertex index `-k` takes the pointer at
instance `+0x6c + 4k` (so -1 = position vertex, -2..-5 = extras in resolution
order), copies its screen (+0), view (+4..+0xc) and +0x10, sets object = view −
instance position, stamps the slot with the instance generation (also via F1472)
and returns it. The slot lives below the instance's cache base (the six reserved
slots), so -6 and below read non-pointer data. Callers sign-extend vertex bytes:
verified for faces (F1708/F1712), lines (F1703), points (F1699), text (F1747
through F1470), child calls (F1749/F1750), and vertex-program dependencies of
vertex opcodes 5..8, 11/12, 15..20 (F1478/F1477 `xor al; movsx`).

**Undefined state the original leaves in a child:** locals 3..6 (stack data),
7..11 (vertex pointers), the basis +0x9c and its handedness +0xfc (until opcode 28
or 25 writes them), and with frame bit 0x4000 the light, observer and +0x38 (so
normals other than 0/1 are undefined). The top-level F1683 instance likewise only
defines local 0; the port keeps treating top-level inputs as caller-supplied.

## What is ported (`ffe-core`)

- `model_execute::execute_scene(.., &ChildEnvironment)`: `execute_live` plus child
  execution. `ChildEnvironment` supplies the model table (`ChildModels` trait,
  `ChildModel` = commands, vertices, normals, descriptor +0x14/+0x18/+0x1c), the
  `DATA_008831` shades, the top-level light (+0x134) and the arena bytes already in
  use. `execute_live`/`execute` are unchanged: child calls stay operand events.
- `Execution.children: Vec<ChildRun>` (call offset, model, F1687 radius, outcome
  `Culled`/`NoRoom`/`Executed(Box<Execution>)`, `ChildSetup`). Children share the
  root `Order` queue, `Execution.queue` log (child queue events interleaved where
  they run), the command budget (total over the tree) and the F1476 jitter state;
  each has its own vertex cache, external table, normals (`model_normals::evaluate`
  with the F1685 observer/light) and visibility context (depth = child z, scale =
  child scale, parent detail). Fully transactional: any child error discards the
  whole run. Nesting is bounded by the exact arena rule plus a 32-level guard
  (`MAX_CHILD_DEPTH`, explicit error; never reached by real models: max depth 4).
- `model::{ExternalVertices, ExternalSource, checked_variable}`,
  `VertexContext::undefined_locals`; `Vertices::child`; negative indices in
  `Vertices::{resolve, object}` and in vertex-program dependencies. With no parent
  table (top level), an external reference is an explicit error.
- Undefined-state guards (explicit errors, never invented values): reading a child
  local 3..11 before writing it (arithmetic, bit branch, colour selector, point and
  vertex-curve variables, child scale), arithmetic writing local 7 (would overwrite
  the +0x70 pointer), reading an undefined basis (opcode 28 0x8000, opcode 30
  restore, a mode-6 grandchild frame), opcode 25 alignment without +0x38, frame
  permutation 7, child scale outside 0..31, normals ≥ 2 of a 0x4000 child.
- `model_math::{frame_rows (F1739), shift_down (F1475), transform_rows (F1670)}`;
  opcode 28's base forms now use `frame_rows` (same results; base-transform oracle
  unchanged).
- `model_commands::vertex_reference`: the decoder accepts 0x80..0xff for faces,
  lines, points, text and child calls; `model::validate_records` accepts them for
  vertex opcodes 5..8, 11/12, 15..20. Other forms (beams, chains, thrusters,
  surfaces, curves 21..24, labels, polygons, depth commands) stay strict.
- `ffe-convert::embedded::decode_model_descriptor` (+0x14/+0x18/+0x1c);
  `decode_model_vertices` accepts a program whose only evaluation failure is an
  external reference. Consequence: `build-data` would now import a few more models
  into a newly built cache (format unchanged; the existing data.bin is untouched).

## Verification

New adapter mode `jjffe-commands --child-model N` (tools/reference/headless.c) and
`tools/reference/verify-child-models.py` (Rust side:
`crates/ffe-rs/tests/child_models_reference.rs`). Setup as `--station-model`
(identity frame/basis, z 2000, descriptor scale, all top-level normals visible) plus:
shades `DATA_008831[i] = i*29+3` (also at the top-level +0x84 / Rust locals 12..17),
top-level light (0x3000, -0x5000, 0x2000), arena 0. Child calls run the real F1538,
F1687, F1740 and F1773 (text glyphs remain skipped); faces go through the real
F1708/F1712 with every queue operation logged. Recorded: each face's operands with
its nesting depth, model/cull outcome and radius, frame word, the built child
instance (depth, scale, colour, position, FNV-1a of frame + base handedness +
locals 0..2, key scale, and when defined +0x44 and the observer/light hash),
whether F1774 ran it, all nested events, points/lines, the whole queue log, final
root state.

Result (floppy 1.02, Linux reference):

- **417 models compared, 0 mismatched**; 161 make child calls. **1,239 child runs
  executed** (8,866 child faces, nesting depth up to 4), 696 culled, 0 without
  arena room. Stations 74/75 now run with their children.
- 28 not executed natively, all explicit: models 0-2 (opcodes 6/10/2 forms), 89
  (release data differs), 94 (a 0x4000 child's text tests normal ≥ 2, undefined in
  the original), 148 (child scale 32), 229/319 (top-level external references),
  315/444 and 18 parents (vertex opcode 0xd). Model 361 is excluded: its vertex data
  differs between the JJFFE declarations (in the reference binary) and floppy 1.02.
- Repeat runs identical; tampering a child face depth, a child set-up hash, a cull
  outcome, an observer hash or a queue key, and an empty trace, are all rejected.
- Trace SHA-256 (Linux LF): `3f30ca979e8024aa2d9db47edc97cbd71765128fe17eb968923c4dbcc3be7886`.
- Original crashes (19 slots) are the 13 `--station-model` crashes plus 200, 215,
  225, 321, 324, 460: top-level models with external references that the real face
  handlers now resolve through null pointers (Rust: explicit error).

Existing oracles re-run with the modified adapter, all unchanged:
`verify-station-models.py` 438 compared (was 434: models with external references
in branches not taken now decode), 0 mismatched, trace `4bcf902e…` (unchanged);
`verify-station.py` `b9e3c831…`; `verify-station-queue.py` `350b6d2c…`;
`verify-text-submodel.py` `b095c38c…`; `verify-vertex-programs.py` 452 compared,
`b3ed1b7d…`. Census (`model-census`, top level only): **443 of 466** (was 439);
11 of the remaining failures are explicit top-level external-reference errors.

Synthetic tests: `crates/ffe-core/tests/model_children.rs` (5 tests: child run with
external vertex and shared queue, culled/no-room, top-level external error and
transactional failure, undefined-local/basis guards, F1739/F1475/F1670 helpers).
Two existing tests changed intentionally: a face byte 255 and a negation of vertex
255 are no longer invalid (they are external references); both now use positive
out-of-range bytes.

## Not done

- Text glyph programs (F1747's nested F1773 run with F1539 fonts and F1344
  strings) are still not executed; the adapter keeps skipping them.
- The child's lazily evaluated normals are computed eagerly with
  `model_normals::evaluate`, which still requires literal anchor records; no
  compared child hit that restriction.
- External references in beams, chains, thrusters, surfaces, labels, polygons,
  depth commands and curve vertices 21..24 remain decoder errors (not verified).
- Point/line queueing, thruster glow, polygon segments, pixels: unchanged.
- The F6 viewer and the cache format do not use `execute_scene` yet (the cache
  lacks descriptor +0x18/+0x1c); wiring it in needs those fields.
- CD 1.01 not checked.
