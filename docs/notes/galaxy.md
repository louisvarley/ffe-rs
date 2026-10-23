# The galaxy table and system parameters (F868-F871)

`ffe_core::galaxy`, cache v15 (`base:world/system-tables`).

The original keeps a hand-made table of **329 systems** (`DATA_007110`, 54-byte
records, followed by a zero code) and four special system codes (`DATA_007116`).
A system is identified by a 32-bit code: bits 0-12 the x sector, bits 13-25 the
y sector, bits 26-31 a small variant.

- **F868** returns a system's index in the table. A code that is not in it gives
  -1 for one of the four specials (compared after masking the top six bits to
  two), otherwise a class from -2 to -8 by the squared distance from the galactic
  centre `(0x1718, 0x1524)`, where each coordinate offset is increased by
  `(code >> 26) >> 2 & 7`: below 9 is -2, then 9, 25, 49, 100, 289, 19,600.
- **F869** reads the details of a record (bytes `+5..+9`, the word `+10` and the
  dwords `+46`, `+50`), **F870** the market parameters (population byte `+4`,
  danger byte `+12`, allegiance word `+10`) and the stock string (31 bytes from
  `+13` into the 32-byte buffer `DATA_008790`, zero to byte 37, and 0xd in byte 32,
  which is the first byte of the next label), **F871** the control nibbles of the
  byte at `+44` (imperial high, federal low), the allegiance (`+10 >> 6`), the
  greater control and the player's two ranks (`DATA_008912`, `DATA_008913`)
  assigned by which control is greater (imperial wins ties only for allegiance 1).

## Reference differences

JJFFE appends systems to the table (records 329 onwards; the original's
record 329 is the zero terminator). The oracle skips codes that only JJFFE knows.

## Oracle

`--galaxy`: every table code, the specials, and 5,500 random codes (anywhere, near
the centre and very near it); 5,835 lookups compared (341 table systems, all
seven classes), 0 mismatched; every tampered field and an empty trace rejected.
`tools/reference/verify-world.py`.
