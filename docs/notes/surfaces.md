# Curved surfaces (opcode 31, F1874 / F1876)

Status (2026-10-23): **operands and F1874's resolution, and F1876's subdivision (the
triangles it hands the emitter), are ported and verified; the emitter F1840, its set-up F1841
and F1877, and the queue handlers are not.** The executor resolves the surface's
origin and axis vectors exactly as F1874 does — the whole-table oracle compares
all 40 surfaces in 39 models against the original, 0 mismatched. Nothing curved
is drawn yet.

Rust: `ffe_core::model_commands::Surface` (decoded, cache independent),
`model_execute::SurfaceEmission::{origin, axes}` (F1874). The viewer does not
consume surfaces.

## Scope (floppy 1.02, cache v14)

The local census (`crates/ffe-rs/tests/text_census.rs`, `FFE_TEXT_CACHE`) decodes
every cached model: **40 opcode 31 records in 39 models, with 224 16-byte
entries in total**. They are the rounded hulls and station parts.

## The record (ported, v0.25)

A surface command is `2 + 12 + 16 * n + 2` bytes:

| Offset | Field |
|---|---|
| word +0 | passed to F1876 as its `word0` argument (seeds its state) |
| byte +2 | **origin** vertex: resolved through **F1470** (view space) |
| byte +3, +4, +5 | the three **axis** vertices: each resolved through **F1472** (object space) |
| word +6 | passed to F1876 (first parameter) |
| word +8 | passed to F1876 shifted left 16 |
| word +10 | passed to F1876 (third parameter) |
| +12 .. | 16-byte entries, up to a zero terminator word |

`SurfaceEmission::origin` is the origin's view position; `axes[k]` is
`axis[k].object - origin.object` in record units, the same three vectors F1874
builds at `[ebp-0x2c..-0x14]`. Both are `None` for `execute` (no live vertices).

Synthetic test: `surfaces_span_their_entry_list_and_terminator` in
`crates/ffe-core/tests/model_execute.rs` checks the resolved origin and the three
vectors (the Q31 identity truncates each coordinate by one unit, so a record
coordinate of 10 resolves to 9; the differences are exact).

**Oracle.** `make-probe.py` now redirects F1874's `call FUNC_001876` to
`trace_surface`, which records five nested events and then calls the original:

| Event | Fields |
|---|---|
| 50 | `word0`, `word6`, `word10` |
| 51 | the origin's view position (`x`, `y`, `z`) |
| 52 / 53 / 54 | each axis vector (`x`, `y`, `z`) |

`verify-station-models.py` runs every slot through the probe and the Rust
executor compares these events with the rest of the nested stream: **448 models,
0 mismatched, 40 surface resolutions** (trace
`2036461aa89920b50b20586b28eb3c07dac73b0ad356f5bc9f2852386dac2266`). Tampering
model 118's nested events (which carry surfaces) is rejected.

## F1874's call into F1876

F1874 passes twelve arguments (cdecl order):

| Arg | Value |
|---|---|
| 1 | output pointer for the minimum distance (F1874 passes null) |
| 2 | comparison value (0) |
| 3 | the drawn object (`instance +0x14c`) |
| 4 | the instance |
| 5 | pointer to the record's entry list (`record + 0xc`) |
| 6 | pointer to the origin's view coordinates (`slot + 4`) |
| 7 | the nine-dword vector array (three axis vectors) |
| 8 | record word +6 |
| 9 | record word +10 |
| 10 | record word +8 `<< 16` |
| 11 | record word +0 |
| 12 | flag (0 from F1874) |

Only with argument 12 non-zero does F1876 write the minimum distance through
argument 1 and compare it with argument 2 (the tail of F1876), so a model-driven
surface never writes through the null pointer. F1741-style callers (not model
commands) can.

## Helpers ported so far

Three of F1876's pure helpers are in `ffe_core::model_surface`, each with the
probe's pinned values as unit tests:

| Helper | Rust | Probe | What it does |
|---|---|---|---|
| F1875 | `cross_sign` | `--surface-sign` (12 pairs) | sign of `a.x*b.y - a.y*b.x` in exact 64-bit, returned as 0 or -1 |
| F1824 | `mix32` | `--surface-helpers` (8 words) | the 32-bit shift/or/add mixer |
| F1825 | `max_abs_msb` | `--surface-helpers` (9 points) | F1656's MSB of the largest absolute coordinate (INT_MIN -> 31) |

F1816/F1817/F1820 are **arena allocators** (a 1499-block 0x48 arena and a
2999-block 0x10 arena, with a longjmp after 0xbb8 allocations), not geometry.
They should be replaced by Rust collections in the port rather than reproduced;
the legacy allocation failure path is not something the renderer needs.

## Entries and mode dispatch (read, not ported)

- **F1823** files the 16-byte entries into a small hash map: each entry's word +0
  is a key, FindMSB normalises it, and its successive `>> 11` / `<< 2 & 0x1fff`
  slices select a bucket. A bucket holds up to two 16-byte records (the entry's
  word +2 and its six payload words). It is lookup plumbing, not geometry; the
  port should build an equivalent map from `Surface::entries` rather than the
  legacy arena.
- **F1833** is a mode dispatch: argument 3 selects either the flat emitters
  (F1715/F1716, plus F1847) or the textured ones (F1831/F1832, plus F1848),
  stores them in `DATA_009280..9282`, scales a constant `0x1dcd6500` by the
  instance's `+0x130`, and calls the table entry `DATA_007843 + 48*mode`.
  F1874 always passes mode 0.
- **F1876's queued words.** The probe now records, as events 60 (start, count)
  and 61 (three words per event), the order-queue words F1876 appends.
  `--station-model N Z` sets the instance depth. At the default z 2000 every
  surface queues **nothing** (their axis vectors are ~1e8 record units, so the
  patch is clipped); these are planet-scale surfaces (models 113-148,
  445/447/449). At z `0x20000000` model 118 queues **1,236 words** (40 event-61
  records) — the captured target for the F1876 port. Events 60/61 are
  documentation only (they contain build addresses) and are excluded from the
  numeric oracle.

## F1876 (not ported)

F1876 is about **1,580 lines** of assembly with its own helper set. From the
top: it copies two constant tables into the frame — `DATA_007893` (0x78 dwords,
480 bytes) and `DATA_007894` (0x3c dwords, 240 bytes); both are small index
patterns (0,2,5,0,1,2,3,5,3,4,1,... and 0,2,2,5,5,0,2,3,3,5,5,9,...), i.e. the
patch's corner/edge/neighbour topology, not trigonometry — then reads the
object's `+0xa0` and evaluates the record's parameter words into the patch grid
before queueing the quads.

Helpers it calls, by count in the routine:

| Helper | Calls | Role |
|---|---|---|
| `F1521` | 10 | the verified Q31 multiply (already ported as `model::multiply`) |
| `DATA_007723` | 7 | indirect call, target not yet identified |
| `F1820` | 7 | not yet read |
| `F1875` | 6 | 64-bit `a.x*b.y - a.y*b.x` sign (2D cross product) |
| `F1825` | 4 | not yet read |
| `F1656` | 4 | not yet read |
| `F1817` | 2 | patch finish/flush (called after the entry loop) |
| `F1692`/`F1691` | 1 each | material colour (ported) |
| `F1877`, `F1479`, `F1816`, `F1823`, `F1824`, `F1833`, `F1834`, `F1835` | 1 each | projection, entry decode, and per-quad emitters |

The final entry loop walks 0x14 (20) 24-byte records in a local table, calling
`F1835` with three 9-dword point arrays and three indices, then `F1817`.

## Plan for the port

1. **Helper set bottom-up.** Done: F1875, F1824, F1825. Next: `F1823` (entry
   decode, 181 lines, calls F1656), then `F1833` (45) and `F1834`/`F1835` (quad
   emission, which call F1868-F1873). F1816/F1817/F1820 are replaced by Rust
   collections (above). Import the two constant tables as cache assets when
   their use is understood (they are small and fixed).
2. **Port F1876's control flow** against those helpers, keeping 64-bit
   intermediates exactly (`Int64MulCreate`/`Int64Sub64` in F1875).
3. **Probe first.** Done for F1874 (events 50-54, above). Still needed for F1876:
   hook the entry decode, each emitted quad's points and key, and the tail's
   distance write, then extend the whole-table oracle to compare them before
   drawing anything.
4. **Draw** the quads in the viewer through the existing polygon/textured paths,
   then include surfaces in the census's "renders" check.

Do not approximate the patch with a generic Bezier/subdivision scheme: the
grid, the entry list and the parameter words are the original's, and the
oracle above is what can show they match.
