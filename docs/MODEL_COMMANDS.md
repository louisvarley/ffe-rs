# Model command map

The original dispatcher is `F1772` (`jjffe-master/ffe.asm:111186`). It indexes the
handler table at `DATA_007816` (`jjffe-master/ffedat.asm:25528`) by
`word & 31`, then advances by the handler's returned pointer. This file records
that table and the current Rust coverage. It is the map to use before extending
the executor past opcode 6.

"Decode" = `ffe-core::model_commands` accepts the physical record and knows its
size. "Execute" = `ffe-core::model_execute::execute_live` handles it without an
explicit unsupported-command error.

| Opcode | Original handler | Meaning (source comment) | Record size | Decode | Execute | Notes |
|---:|---|---|---:|---|---|---|
| 0 | `F1754` | return / terminator | 2 | yes | yes | only `word == 0` is accepted |
| 1 | `F1698` | point source | 8 | yes | yes | payload verified against F1699: material word, F1474 packed-angle word, `payload[4] & 0x80`, vertex byte. Operand event only; no projection/pixels. |
| 2 | `F1702` | line | 6 | yes | yes | payload verified against F1703: material word, vertices `[byte2, byte3]`. Operand event only; no clipping/pixels. |
| 3 | `F1707` | non-reflected triangle | 8 | yes | yes | material, A/B, normal byte, C; perimeter B/A/C |
| 4 | `F1711` | non-reflected quad | 10 | yes | yes | material, A/B/C/D, normal word; perimeter C/A/D/B |
| 5 | `F1721` | complex polygon | 6 + L | yes | yes | v0.21: word +0 material, byte +2 normal (low 7) / flag (bit 7), byte +3 segment length L, then L segment bytes (`DATA_007817`, 7 handlers). Culled: skipped. Otherwise keyed by F1777 of the first segment vertex's view. Segments not executed. |
| 6 | `F1696` | unified-z block start/end | variable | yes | yes | composed in v0.11; modes 0/4/5/6 close, 1/3 max/min, 2 vertex, 7 position/first/offset |
| 7 | `F1706` | reflected triangle | 8 | yes | yes | emits XOR-1 mirrored face then ordinary face |
| 8 | `F1710` | reflected quad | 10 | yes | yes | same reflection rule |
| 9 | `F1738` | thruster glow | 8 | yes | yes | v0.23: word +0 material, bytes +2/+3 vertices A/B, word +4 signed width. Operand event; A (and B if A is deeper than 0x200) projected; glow/line queueing not ported. |
| 10 | `F1747` | 3D text string | 10 | yes | yes | verified v0.14: +0 material, +2 normal (culled skips), +3 hi nibble glyph set / lo nibble size, +4 vertex, word +4 frame, word +6 string ID. Operand event; glyph programs not executed. |
| 11 | `F1752` | visibility branch | 4 | yes | yes | F1751 predicate |
| 12 | `F1755` | inverse visibility branch | 4 | yes | yes | F1751 inverse |
| 13 | `F1758` | arithmetic | 4 | yes | yes | 16 operations; trig needs the sine table |
| 14 | `F1749` | child-model call (via `F1750`) | 4 or 8 | yes | yes | verified v0.14: model `code >> 5`, +0 vertex, word +0 frame; byte 1 bit 7 adds +2..+5 extra vertices (order +4,+2,+3,+5, `0x7f` ends). Operand event; child not executed. Earlier "text (second form)" label was wrong. |
| 15 | `F1759` | — | — | **no** | no | unsupported record form |
| 16 | `F1815` | — | — | **no** | no | unsupported record form |
| 17 | `F1760` | two-vertex shape (`Beam`) | 14 | yes | yes | v0.28: word +0 material, word +2 vertices (low/high byte), words +4..+10 to F1761. Operand record; drawing not ported. |
| 18 | `F1763` | — | — | **no** | no | unsupported record form |
| 19 | `F1756` | bit branch | 4 | yes | yes | zero-mask branch |
| 20 | `F1757` | inverse bit branch | 4 | yes | yes | inverse |
| 21 | `F1697` | nested depth block start/end | 2 | yes | yes (live) | v0.26: without bit 15, F1781 block keyed by F1777 of vertex `code >> 5`'s view; with bit 15, F1782 end. Both close the group. `Order::nested`. |
| 22 | `F1737` | spline | — | **no** | no | unsupported record form |
| 23 | `F1765` | — | — | **no** | no | unsupported record form |
| 24 | `F1766` | vertex-pair chain (`Chain`) | 6 + 2k | yes | yes | v0.28: material, F1474 size, 2-byte pairs through the first holding 0x7f. Operand record; drawing not ported. |
| 25 | `F1771` | align basis with instance direction | 2 | yes | partial | v0.28: bit 15 resets the basis (F1770); otherwise rotation from `VertexContext::direction` via F1467/F1523/F1744 — zero pairs exact, non-zero needs the sqrt table. |
| 26 | `F1762` | set colour | 6 or 20 | yes | yes | verified v0.15: word +0 material, byte +2 normal (low 7) / normal-0 target (bit 7), byte +3 selector; non-zero selector adds seven alternative words (20 bytes). Updates instance colour or normal 0. |
| 27 | `F1748` | coloured child-model call (via `F1750`) | 8 or 12 | yes | yes | v0.24: model `code >> 5`, byte +0 vertex, word +0 frame, byte +3 scale (signed high nibble, or local `>> 4` with bit 3), word +4 child material; extras at +6 when byte 1 bit 7. Operand event; child not executed. |
| 28 | `F1741` | `0x9c` matrix transformation | 4 (6 for `0xc000`) | partial | partial | `0x8000` local rotation (v0.6); `0x0000`/`0x4000` base-relative F1739 + optional F1743 verified v0.16 (6,048 cases); `0xc000` vertex-defined basis unsupported |
| 29 | `F1758` | arithmetic (second entry) | 4 | yes | yes | same helper as 13 |
| 30 | `F1767` | 2 | — | bit 15 only | no | bit 15: vertex frame := basis (F1768); `0x801e` light points (F1769) and the variable vertex-base offset (F1473) are errors |
| 31 | `F1874` | curved surface (via `F1876`) | 16 + 16n | yes | yes | v0.25: word +0, origin vertex +2 (view), axis vertices +3..+5 (object), words +6/+8/+10, then 16-byte entries to a zero word (F1823). Operand event; F1876 rendering not ported. |

## Verified so far

- `F1772` dispatch, arithmetic/bit/visibility branches, pure rotation, and
  triangle/quad operands: `verify-execution.py` (900 contexts,
  `d2a76e9c…`).
- `F1696` depth keys/groups: `verify-order.py` (11,700 contexts,
  `ab5da346…`) and `F1778/F1780/F1788` sorting (128 sequences,
  `4ab5453e…`).
- Composed `F1696` + `F1777` + `F1778/F1780/F1788` with marker callbacks:
  `verify-order.py` (`composed-order`, 1 synthetic program, keys `[9,9,0]`,
  order `[2,3,1]`, `ce0d1b4f…`). Rust matches `Execution.ordered` and
  `Execution.ordered_keys`.
- Real station roots advance past opcode 6 and stop at the next unsupported
  command: model 74 at `0x000a`/`0xf4`, model 75 at `0x1c6e`/`0x1c8`
  (`executed_reference_station_depth_composition_advances`).
- Point (1) and line (2) operands match the original F1699/F1703 handler
  arguments: `verify-point-line.py`, 2 events, trace `afa955be…`, six tamper
  fields plus empty trace rejected. This is operand emission, not geometry.
- Text (10) and child-model call (14) operands match the original F1747 and
  F1749/F1750 with nested calls recorded (`verify-text-submodel.py`, 7 cases,
  trace `5ba06002…`): decoded fields, the text normal skip, submodel extra-vertex
  order, and the handlers' own cursor advance.
- Colour (26) results match the original F1762 through F1772
  (`verify-colour.py`, 32 cases, trace `7d232265…`): material selection, local
  and lighting additions, unlit/textured rules, normal-0 writes, byte wrap and
  the 6/20-byte record sizes.
- Opcode 28 base-relative forms match the original F1741 directly
  (`verify-base-transform.py`, 6,048 cases, trace `c9a87a6d…`): row
  permutation, negation, handedness and the conditional rotation.
- With the top-level F1780(0,0) close after return (F1683/F1684), both real
  station programs run to completion in `execute_live`.
- Whole real stations match the original F1772 in command order
  (`verify-station.py`, models 74/75, trace `0ea8f4a7…`): every face with its
  locals/basis, points, text/child calls, and final locals/basis/colour/
  handedness.
- Face draw queues match the original F1708/F1712 queueing for both stations
  in six views each (`verify-station-queue.py`, 12 cases, trace `35e55a26…`):
  normal cull, textured guard/perimeter path, flat outcode rejection and the
  vertex-a key, interleaved with depth groups. Points/lines queueing, glyph/child
  execution and pixels are not covered.

## Coverage across all models (census)

`ffe-tools model-census --game <dir>` runs every model table slot through the
native importer, decoder and executor with synthetic inputs. Floppy 1.02 after
v0.30: **413 of 466** execute, and all that the original completes match
(`verify-station-models.py`). Remaining blockers:

| Blocker (first failing stage) | Models |
|---|---:|
| decode: opcode 22 (spline) | 10 |
| vertices: vertex opcode 0x17 (23) | 7 |
| decode: opcode 28 (unsupported rotation form) | 5 |
| decode: opcode 18 | 3 |
| vertices: vertex opcode 0xd (13) | 2 |
| execute: opcode 30 light points (`0x801e`) | 1 |
| other vertex/normal/reference validation errors | ~24 |

Some declaration blocks hold data after the final terminator; the decoder takes
the longest self-contained prefix (`Program::trailing`, v0.27).

Vertex opcodes supported: 0-2, 5-12, 15-22 (7/8 since v0.22, 21/22 since
v0.29). Not yet: 3/4, 13/14 (screen-space midpoints) and 23/24 (F1477 with an
F1474 angle). All supported vertex programs match the original
(`verify-vertex-programs.py`).

Use it to choose the next opcode; re-run it after each one. Every model that
executes natively is compared with the original F1772 by
`verify-station-models.py` (v0.20: all 180 match in command order).

## Next: executing glyphs and children

Text and child-model calls are operand events (v0.14). Running them needs, in
order: the F1538/F1539 model lookups and real child descriptors (cache v7),
F1740 child frames, F1687 child culling with the descriptor radius, nested
F1772 execution with its own vertex/normal frame, and F1344 string expansion
(which reads game state). Probe each step as below before porting it.

## Probe discipline (applied to text/submodel in v0.14)

Text and submodel handling need the same discipline used for triangles/quads and
point/line: extend the local instrumented probe to capture the **original handler
arguments** (or ordered primitive stream), then port and compare. Do not infer
payload fields from the record size alone.

1. Add `_trace_text` and a submodel-call probe at the relevant call sites in a
   generated copy (see `docs/REFERENCE_HARNESS.md`), capturing every argument.
   Point/line probes already exist (`--point-line`).
2. Document each payload's exact byte layout here (which word is material, which
   bytes are string/position/child-model selectors, and how the cursor advances).
3. Add synthetic reference programs and Rust tests for the operands, then
   implement the corresponding `Primitive` events.
4. For submodels, first decide synthetic-child-only vs. extending the cache to
   v7 with real child model descriptors/commands/vertices/normals.
5. For text, decide whether to emit a resolved string run or an operand event;
   do not dereference arbitrary command-stream pointers.

The composed-order oracle, point/line operand probe and the existing component
oracles do not validate text/submodel semantics, geometry, clipping, shading or
pixels.
