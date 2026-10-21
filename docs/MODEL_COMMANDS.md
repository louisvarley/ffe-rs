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
| 5 | `F1721` | complex polygon | — | **no** | no | unsupported record form |
| 6 | `F1696` | unified-z block start/end | variable | yes | yes | composed in v0.11; modes 0/4/5/6 close, 1/3 max/min, 2 vertex, 7 position/first/offset |
| 7 | `F1706` | reflected triangle | 8 | yes | yes | emits XOR-1 mirrored face then ordinary face |
| 8 | `F1710` | reflected quad | 10 | yes | yes | same reflection rule |
| 9 | `F1738` | thruster effects | — | **no** | no | unsupported record form |
| 10 | `F1747` | text string | 10 | yes | no | payload includes text/position state; not yet documented/verified |
| 11 | `F1752` | visibility branch | 4 | yes | yes | F1751 predicate |
| 12 | `F1755` | inverse visibility branch | 4 | yes | yes | F1751 inverse |
| 13 | `F1758` | arithmetic | 4 | yes | yes | 16 operations; trig needs the sine table |
| 14 | `F1749` | text (second form) | 4 or 8 | yes | no | size depends on `word(&2) & 0x8000`; semantics not documented |
| 15 | `F1759` | — | — | **no** | no | unsupported record form |
| 16 | `F1815` | — | — | **no** | no | unsupported record form |
| 17 | `F1760` | — | — | **no** | no | unsupported record form |
| 18 | `F1763` | — | — | **no** | no | unsupported record form |
| 19 | `F1756` | bit branch | 4 | yes | yes | zero-mask branch |
| 20 | `F1757` | inverse bit branch | 4 | yes | yes | inverse |
| 21 | `F1697` | nested execution block start/end | — | **no** | no | submodel/nested program form; needs child model data and a bound |
| 22 | `F1737` | spline | — | **no** | no | unsupported record form |
| 23 | `F1765` | — | — | **no** | no | unsupported record form |
| 24 | `F1766` | — | — | **no** | no | unsupported record form |
| 25 | `F1771` | — | — | **no** | no | unsupported record form |
| 26 | `F1762` | — | 6 | yes | no | structurally decoded; purpose not yet documented. Likely a submodel/aux call; do not guess. |
| 27 | `F1748` | — | — | **no** | no | unsupported record form |
| 28 | `F1741` | `0x9c` matrix transformation | 4+ | partial | partial | only the pure `0x8000` local-basis form executes; base/object-relative and vertex-defined forms error |
| 29 | `F1758` | arithmetic (second entry) | 4 | yes | yes | same helper as 13 |
| 30 | `F1767` | — | — | **no** | no | unsupported record form |
| 31 | `F1874` | — | — | **no** | no | unsupported record form |

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

## What must be done before implementing text/submodel

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
