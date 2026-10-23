# Name generation (F791-F800, F921)

`crates/ffe-core/src/names.rs`, from the cache's `base:text/names` (v18: the range
tables `DATA_006274..6279` and the 510 strings of `DATA_006272`).

- **F921(range)**: `a += b; b = rol(b, 5); a += b; a = rol(a, 16)`, result
  `(a & 0xffff) * range >> 16`.
- **F791(type, seed)** first mixes the seed: types below 6 `b = rol16(a) + a`,
  `a = rol(a, 7)`; types 6 to 0x15 `a += t` three times with `t = rol16(a)`, then
  rotated left 7 and 4, `b = t`. Types: 1 male and 2 female full names, 3 a
  first name or full name (female when F921(0xff) has bit 0x40), 4 a surname, 5 a
  first name, 6 to 0xe the various ship-owner and organisation names, 0x10 planets,
  0x11 starports and stations (a surname part from F793/F794 inserted by the string
  code `FF 30 16`), 0x12 to 0x14 ship identifiers (`AB-123`, ` +123A`, `AB*  1`).
  Types 0x16 to 0x19 (commander, ship type, crew) need game state and are not ported.
- **F793**: a plain surname (string 0x59 + F921(0x8a)) when the seed's low byte is at
  least 0x19, otherwise a first name (0x4f + F921(10)) + "s" + "on" unless bit 3 of
  the seed's second byte is set.
- The inserted surname part runs through the bracket expander F77: in the one name
  string with a bracket code ("M[nich") the bracket stays and the two letters go.

`--names` matches 8,130 names; 62 are skipped because JJFFE spells some accented names
(codes 0x82, 0x83, 0x8a in the game's font) with plain letters.
