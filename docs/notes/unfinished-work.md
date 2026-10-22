# Unfinished background work (stopped 2026-10-22)

Two background agents started after the "Engine glows, moving parts and the
spinning station" round stopped (most likely at a usage limit) before writing
any code. Both worked from main at b1d19c4. Neither committed anything; their
research was not saved. Restart them from current main.

## Text and labels

Goal: port model text (opcode 10, F1747 with the F1344 string expansion and
F1539 glyph models) and labels (opcode 18, F1764/F1812), so ships and stations
show their registration and name decals. Notes were to go in
`docs/notes/text.md`, checked against the probe's existing scoped hooks
(`trace_text_*`, `trace_label_*` in `tools/reference/make-probe.py`).

How far it got: a throwaway census test that listed every decoded text and
label record (`program.texts`, `program.labels` from
`ffe_core::model_commands::decode`) across the cache's models. No port.

Cache: v12 is taken by ship statistics, so a new asset needs v13 (and
future-version tests then use 14).

## Curved surfaces

Goal: port opcode 31 curved surface patches (F1874/F1876) so rounded hulls and
station parts draw, with notes in `docs/notes/surfaces.md` and the spline
probe hooks (`trace_spline_*`) as the oracle.

How far it got: a throwaway example that listed every opcode-31 record
(`program.surfaces`: offset, word, origin, axes, params) from the floppy 1.02
executable, and a rebuild of the Linux reference. No port.
