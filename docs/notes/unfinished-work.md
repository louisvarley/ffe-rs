# Unfinished background work (stopped 2026-10-22; superseded 2026-10-23)

Both tasks were restarted and landed as v0.48a/b (text, labels, glyph programs)
and v0.48c (surface resolution), with the details in `docs/notes/text.md` and
`docs/notes/surfaces.md` and the milestone list in `docs/STATUS.md`. This file
is kept only for the history below.

Two background agents started after the "Engine glows, moving parts and the
spinning station" round stopped (most likely at a usage limit) before writing
any code. Both worked from main at b1d19c4. Neither committed anything; their
research was not saved.

## Text and labels

Ported: the F1344 string expansion for the families the models use, cache v14's
per-character glyph programs, F1747's placement, and F1764/F1812 labels with
cache v13's full string table. Still open: control codes below 0x20, `0x3022`'s
wrap, `0x9c00`-`0x9c03`, depth interleaving and glyph lighting.

## Curved surfaces

Ported and verified: F1874's origin/axis resolution (whole-table oracle, 40
surfaces). Still open: F1876's patch rendering, which is the next milestone step
and needs its own probe hooks (see `docs/notes/surfaces.md`).
