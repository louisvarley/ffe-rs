# Complex polygons (opcode 5, F1721)

Working notes for the polygon port. Code: `ffe-core::model_polygon` (segments,
handlers, helpers, outline), `model_commands::Polygon` (structured segments),
`model_execute` opcode 5 (`PolygonEmission::{outline, colour}`),
`model_math::table_root` (F1467), `model_instance::Vertices::with_normal_records`.
Not yet merged into `STATUS.md`/`PORT_LOG.md`/`MODEL_COMMANDS.md`.

## Record and segment format

Opcode 5 record: code word, then word +0 material, byte +2 normal (low 7 bits;
bit 7 = abandon flag, stored at instance +0x128), byte +3 segment length L, then L
segment bytes. Culled normal: F1721 returns past the L bytes. Otherwise it resolves
the colour (F1692/F1693/F1691), resets +0x118 (circle normal cache) and +0x128,
keys the polygon by F1777 of the F1470 view of `byte(+4) & 0x7f`, allocates a
1-word entry with F1778(1, key, 0) holding F1794, then runs the segment words and
returns past the terminator. The decoder therefore requires the terminator to end
exactly at L (all 540 floppy 1.02 polygons do), otherwise the record size would
depend on visibility.

Segment word `w`: handler `DATA_007817[(i16)w >> 9]` (index > 6 or a negative
word is an error); the low byte is the first operand, read sign-extended
(`movsx`) as a vertex index. Bytes after the word (`c0`, `c1`, ...):

| Index | Handler | Bytes | Operands (projection order) |
|---:|---|---:|---|
| 0 | F1722 | 2 | terminator |
| 1 | F1734 | 6 | contour start: S = c1, then curve F1735(S, P1 = c0, P2 = c3, end = c2); low byte unused |
| 2 | F1724 | 4 | contour start: S = low, then edge F1725(S, E = c0); c1 unused |
| 3 | F1730 | 2 | edge to low (F1731) |
| 4 | F1733 | 4 | curve from +0x114 via P1 = low, P2 = c1 to end = c0 |
| 5 | F1723 | 2 | close the contour (as at the end of F1721) |
| 6 | F1736 | 4 | circle: centre low, normal c0, radius c1 << (+0x148 & 31) |

The skip table `DATA_007819` (used by F1726) has the same sizes.

Census (floppy 1.02): 540 polygons in 108 models; segment counts by index
1: 380, 2: 156, 3: 1113, 4: 393, 5: 7, 6: 13; first segments 1 (379), 2 (153),
6 (8); 26 polygons have the flag.

## Handler state and entries

Instance fields: +0x104 contour open, +0x108 entry point (screen dword),
+0x10c previous point behind, +0x110 start vertex, +0x114 previous vertex,
+0x118/+0x11c circle normal byte and view axis, +0x128 flag / -1 abandoned,
+0x12c entry pointer. The original never resets +0x104..+0x114 per polygon; the
port starts them unset and makes a read of an unset field an error (no real
polygon does so).

Every vertex goes through F1471 behind the stamp check (`Vertices::resolve(..,
true)`); "behind" means view z < 0x40 (F1736: z <= 0x40).

- F1725 edge A→B: +0x114 = B, open. A behind: flag → abandon; B behind too →
  closed, clipped; else entry = F1564(A, B), F1795 [entry, B]. A front, B behind:
  flag → abandon; F1795 [A, F1564(B, A)], clipped. Both front: F1795 [A, B].
- F1731 line to P: not clipped: P front → F1796 [P]; P behind → flag abandon, else
  F1796 [F1564(P, prev)], clipped. Clipped: P front → F1732, else +0x114 = P.
- F1732 re-enter at P: q = F1564(prev, P); open → F1796 [q], F1796 [P]; else open,
  entry = q, F1795 [q, P].
- F1735 curve from S via P1, P2 to E: any of the four behind → F1725(S, E);
  else F1797 [S, P1, P2, E].
- F1733 when +0x114 is behind: projects only P1; behind → +0x114 = P1, else F1732.
- F1723 / end of F1721: if open, F1731(start); then if clipped, F1796 [entry].
- F1736 circle: axis = transposed vertex frame (+0x14) × normal record
  `(n >> 1) - 1` bytes +2..+4 (x negated for odd n), F1521 products; then F1554.
  Two F1797 entries, contour closed.
- Abandonment (F1726): skip the remaining segments, +0x128 = -1; F1721 then
  rewinds the arena to its entry (F1779) and writes F1813 there. F1736 passes
  the cursor at its own data bytes, so the skip is simulated on the raw bytes and
  must land on the real terminator (error otherwise). An abandonment in F1721's
  final close would skip the following commands: an error (not reachable when the
  contour starts in front).
- Normal end: F1787(2) F1799 [fn, F1326(colour)].

Queue entries (`model_polygon::Entry`): F1795 `Edge([a, b])` (3 words), F1796
`To(p)` (2 words, edge from the previous word, the last point of the previous
entry), F1797 `Curve([p0..p3])` (5 words).

## Helpers

- F1564 near point: binary search from the behind point toward the front one
  (x/y steps `sar`, z step `shr`) until the z step is 0; x/y << 2 if both fit
  (`>> 29` is 0 or -1); shrink while |x| or |y| >= 0x1f00; screen
  (x + 0xa0, 0x4f - y) in 16 bits.
- F1557 circle point: z < 0x40 → fail; shrink x, y, z while (|x| | |y|) >
  0x800000; z 0 → fail; (v << 8) / z, then halve both while either is outside
  -0x3fff..0x3fff.
- F1554 circle: s = ax² + ay² (32-bit). s < 9 → F1556: u = (r, 0, 0),
  v = (0, r·0xaaaa >> 15, 0). Else len = F1467(s), u = (-((ay << 15)/len·r >> 15),
  (ax << 15)/len·r >> 15, 0), k = (r << 10)/3, v = (ax·az/len·k >> 15,
  ay·az/len·k >> 15, -len·k >> 15). F1555: curves [C+u, C+u+v, C-u+v, C-u] and
  [C+u, C+u-v, C-u-v, C-u]; any F1557 failure → no curves.
- F1467: table root; entry i of `DATA_007977` (indices 0..2047 used) equals
  `isqrt(i << 21)` exactly, so `table_root` computes it.
- F1547/F1549 curve (draw time): n = 2/3/4/5 for Manhattan distance between the
  first point and the four points' truncated mean < 4 / < 30 / < 25000 / else;
  16.16 forward differences with A3 = (d - a + 3b - 3c) << (16 - 3n),
  A1 = 3(b - a) << (16 - n), A2 = 3(c - 2b + a) << (16 - 2n); 2^n steps
  (x += dx; dx += ddx; ddx += dddx) each an F1560 edge, then an edge to d.

`model_polygon::edges` returns the F1560 edge sequence of a queued polygon.
F1560 (y clip at 0x9e, span insertion), F1559 and F1613 (fill) are not ported.
`fill_path` bridges the edge chains into one even-odd path for the F6 viewer
(a presentation helper, not original code).

## Not ported

- F1691's texture colour in the executor: for a textured material F1721 adds
  bytes +8..+10 of `DATA_007811 + 30 * texture` (the descriptor's first slot
  colour). `PolygonEmission.colour` excludes it because the executor has no
  descriptor table, and the oracle does not compare those fills. The viewer
  completes it (2026-10-27): `PolygonEmission.shade` keeps the normal colour
  F1693 adds and `ModelViewer::polygon_colour` runs `flat_colour` on it. This
  replaced a fixed grey stand-in that drew every starport pad 0x555 instead of
  the original's 0x222 (Sirocco Station, Merlin: fixed palette entry 22,
  (10, 10, 8), confirmed in the reference's start frame).
- Rasterisation (F1560/F1559/F1613) and the palette conversion (F1326/F1327).

## Verification (Linux reference build)

- `verify-polygon.py` (new, `--polygon-helpers`): 4,136 F1467, 2,048 each of
  F1564, F1557, F1554 (504 succeed, 169 via F1556) and F1547 (5/9/17/33 edges)
  cases; all match; repeat, a tamper per kind and the empty trace rejected.
  Trace SHA-256 `fab32ea5c531d403c1872d25a6959da65aefea3469b875733436d5cc7b08105f`.
- `verify-station-models.py`: compared 439 models, 18 not executed natively,
  0 mismatched; 452 polygon fills with 1,601 entries (1,057 F1796, 206 F1795,
  338 F1797) and 5 abandonments compared entry for entry; 124 textured fill
  colours not compared. Trace SHA-256
  `a14d86090617a67c2f298b2cdc96b79174ef42de6a7d214d342008fbb72329d9` (was
  `4bcf902e…`; 434 compared). The original now crashes in 8 slots
  (179, 227, 259, 270, 316, 335, 388, 458) instead of 13.
- Unchanged: `verify-station.py` `b9e3c831…`, `verify-station-queue.py`
  `350b6d2c…`, `verify-text-submodel.py` `b095c38c…`,
  `verify-vertex-programs.py` `b3ed1b7d…` (452 programs).
- Census: 439 of 466 execute natively (unchanged).

The station-model setup (identity frame, z 2000) puts few vertices behind the
near plane, so the clipping paths (F1564 inside F1725/F1731/F1732) are checked
by the helper oracle and synthetic tests (`crates/ffe-core/tests/model_polygon.rs`)
rather than by real-model traces.

## Probe and adapter changes (justified trace change)

- `make-probe.py`: in F1721, F1326 → `trace_polygon_colour` (walks the arena
  from the polygon's F1794 entry to the F1799 fill: events 19/20/21) and F1779 →
  `trace_polygon_abandon` (event 22, then the original rewind).
- `headless.c`: station-model sets instance +0x154 to the descriptor's normal
  records (+0xc), as F1774 does for every model (read by F1736 and F1761);
  nested-event capacity 256 → 4096; new `--polygon-helpers` mode.
- `exports.inc`: aliases for F1794-F1797, F1799, F1467, F1564, F1557, F1554,
  F1547.
