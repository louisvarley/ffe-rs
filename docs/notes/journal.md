# The ship library's journals (MISSION.DAT)

Status (2026-11-03): **archive imported (cache v31); viewer and macros not
ported.** The port's `Screen::Journal` is still the contract log
(`docs/notes/missions.md`).

## Archive

- `MISSION.DAT` (779,951 bytes on floppy 1.02) holds 671 articles. The
  executable's `DATA_002292` (RVA 0x9c0c4, 672 dwords) gives each one's file
  offset, ending at the file's length. Article 0 starts at offset 1; some
  entries are one-byte empty slots (article 28 is one).
- F78 (`ffe.asm:9991`) opens `mission.dat` (`DATA_002308`, mode `rb`), seeks
  to `DATA_002292[n]`, reads up to `DATA_002293[n]` and unscrambles each
  big-endian 32-bit group: bits 31/30/29/28 go to 31/23/15/7 and the 7-bit
  fields 21..27, 14..20, 7..13, 0..6 to bytes 3..0's low bits. The text ends
  at the first NUL. `ffe_core::journal` holds the decoder; the cache asset is
  `base:text/journal`.
- An article is the publication line (`S.L.A.M`, `M.C.S`, `J.F.`, ...), the
  headline, then the body, each ended by `\n`.

## F78's copy pass

After decoding, F78 copies the text into `DATA_002294`'s buffer:

- `0x0e` becomes `0x83`, `&` becomes `+`, `\n` becomes `\r`, `0xd5` becomes
  `'`, and `0x7f` is skipped.
- `[` runs F76, the macro expander (below).
- At the first `\r` (the publication line) it upper-cases the line, centres
  it (`(320 - width) / 4` into the buffer's third byte), inserts `0x1e` and a
  second centring byte, appends message `0xa602` ("By:" style byline through
  F84) and a space, copies the headline, and indents the next line with five
  spaces.
- The whole text is wrapped at 0x12c (300) pixels with F1398.

## F76's macros (`DATA_102307`)

Each macro is `[` plus a two-letter code (or a word) and arguments in
`(...)`. The codes, in table order, and what F76 calls for them:

| code | uses |
|---|---|
| `Gr` | digits after it: emits `0x02 n` (a colour change) |
| `Mi` | F75 argument list, F64 |
| `Ca` | player cash `DATA_008888` via F1377 |
| `Fm`, `Ff`, `Co` | F75 arguments, F791 names (male, female, company) |
| `Cr` | (no calls) |
| `Dr` | F75/F74, F921 random, `DATA_007216`, the date `DATA_008807` via F1358 |
| `Nu` | F75/F74, F921 random |
| `%i` | integer argument |
| `Ri`, `Rf`, `Re` | F305/F304 ranks, `DATA_005110`/`DATA_008911`, F1344 ff-codes |
| `Sh` | the player's ship (`DATA_008861`, F1538) name |
| `Si` | the player's ship object |
| `Sl` | F921 random choice between quoted alternatives (`[SlL1("his","her")`) |
| `Oo` | random choice, recursing into F76 |
| `Pn` | the commander name `DATA_008919` |
| `Zz` | F74, `DATA_008567` |
| `Sy` | F860/F461/F462/F463 system picks, F921, the current system `DATA_008885` |
| `Ci`, `A`, `F`, `C`, `I`, `Impmine`, `impmine` | (no calls: fixed words or flags) |
| `Pl` | `DATA_002295`, F74/F75, F858 planet names |
| `Imp riot`, `F edge`, `Imp edge` | F921 picks from `DATA_002289`/`DATA_002291`/`DATA_002290` |
| `disput` | `DATA_007107..7109` (disputed systems), F921 |
| `Sc`, `Cs` | `DATA_002296`/`DATA_002297`, F858/F860, F1377 |
| `Sr` | F921, F1538/F1344 ship names, recursing into F76 |

`L1`, `L2`, `R(n)` and similar suffixes are F75's argument forms (slots
the article fills once and reuses, e.g. `[SyL1(I)` is "system 1, chosen
Imperial").

## Viewer (F79-F84)

- F81 lists the journals (`DATA_002305`, "Please select one of the
  following journals:") under `DATA_002304`'s "SHIP LIBRARY ARCHIVES".
- The journal records are `DATA_008570`, 56 bytes each (`+0` masthead
  bitmap, `+4` read flag ...). F80 blits the masthead centred, the 0x2f
  arrow while `DATA_008580 < 0x28`, and F79 writes the article text at
  (10, 15) with F1585, clipped to 0x28..0x8c.
- `DATA_002301` ("The new issue of ... is now available. Get it from your
  local newsagent."), `DATA_002306` ("... issues remaining") and
  `DATA_002307` ("No subscription") are the subscription messages.

To port: F81's journal list and subscription state, the issue schedule
(which article each issue shows), F76's macros against the port's names,
systems and ranks, and F79/F80's drawing.
