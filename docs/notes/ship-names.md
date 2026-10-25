# Ship names and model table slots

Status (2026-10-25): **fixed**. A model's display name comes from its statistics
block's **`+0xe` ship-name ffcode**, expanded through `DATA_004682`; the cache's
`base:text/model-names` table is keyed by that code, not by the model table
slot. `+0xc` is the shield word. `docs/evidence/ship-names/README.md` records the
cross-check run.

## Sources

| source | what it is | port |
|---|---|---|
| `DATA_004681` | the model table, 465 pointers; table slot n is the model index | `base:model/records`, `base:model-vertices/nnn`, ... |
| model descriptor `+0x38` | pointer to the model's 48-byte statistics block | `base:ship/stats` (cache v12) |
| statistics `+0xe` | ship-name ffcode (`0x4000 \| index`) | `ShipStats::name_code`, F437/F439/F282 |
| `DATA_004682` | 219 pointers to NUL-terminated strings; entry n is the string of code `0x4000 \| n` (**not** slot n) | `base:text/model-names` (cache v11/v13) |
| F1345/F1348 | expand a code: `DATA_004682[code & 0x3fff]`, then nested FF codes | `ffe_rs::ships::name_at`, `ModelViewer::model_string` |
| `DATA_005268..5272` | the arrays of pointers to the ship-name lists (`sh1`.., `fsh1`.., `ish1`..) that F382 selects | `station::SHIPYARD`; **not** names or prices |
| statistics `+0xc` | shield-strength word (Cobra Mk III = 150) | `ShipStats::shield_strength`, shipyard View |
| statistics `+0xe` | name code, as above | `ShipStats::name_code` |
| statistics `+0xa` | F437's Cost word (`word * 1000`) | `model.price_mass` / `shipyard_table` |

`DATA_005268..5272` is often mistaken for a price or name table. It is neither:
it holds the eight `sh1`/`fsh1`/`ish1` list pointers F382 uses to choose the
shipyard's type lists. The name always comes from the statistics `+0xe` code.

## Root cause

The cache's table is correct (219 entries, byte-identical to the executable), but
`crates/ffe-rs/src/ships.rs` read it as `model-names[model_slot]`. Ship names are
not aligned with model slots: the statistics `+0xe` word skips some table indices
and repeats others. That shifted the in-flight names by up to three slots:

| model | old slot lookup | original (`+0xe` -> `DATA_004682`) |
|---|---|---|
| 0x1f (Saker) | `model-names[31]` = "Viper Defence MkII" | `0x401c` -> index 28 "Saker III fighter" |
| 0x17 (Eagle) | `model-names[23]` = "Sidewinder" | `0x4014` -> index 20 "Eagle long range fighter" |
| 0x1e (Viper) | `model-names[30]` = "Merlin attack fighter" | `0x401b` -> index 27 "Viper Defence Craft" |
| 0x22 (Viper MkII) | `model-names[34]` = "Cobra Mk III" | `0x401f` -> index 31 "Viper Defence MkII" |
| 0x26 (Cobra Mk III) | `model-names[38]` = "Lanner" | `0x4023` -> index 35 "Cobra Mk III" |
| 0x2b (Spar) | `model-names[43]` = "Turner Class" | `0x4028` -> index 40 "Spar attack fighter" |
| 0x46 (Lynx) | `model-names[70]` = "Long-range Cruiser" | `0x403c` -> index 60 "Lynx Bulk Carrier" |
| 0x47 (Long-range) | outside the old 14..=70 range | `0x4046` -> index 70 "Long-range Cruiser" |

The same misreading made `+0xe` (the name code, e.g. `0x402e` = 16,430) look like
a shield value, and left `+0xc` (the shield word, e.g. 180) unread.

## Mapping details

- The model table has 465 descriptors in 466 slots (`0x000..0x1d1` in the cache).
  84 slots have a statistics block; 73 of those carry a `0x40xx` name code, and
  the rest carry 0 ("Mine"). Several slots share a code: 0x3d and 0x46 are both
  "Lynx Bulk Carrier"; 0x3f has code 0x403e (index 62, "Unknown").
- The flyable range is ship indexes `0x0e..=0x47` (`doc/srctools/shipidx.txt`),
  model slots 14..=71. The port had stopped at 70, which silently dropped the
  Long-range Cruiser (0x47); the list now uses `LAST_SHIP = 0x47`.
- `base:text/model-names` still holds the whole string table: entries 61, 65,
  71..101 are UI/decal strings (`__Max Laden Wt_1000t`, `&&&&`, station
  markings), not model names. The readable-name filter leaves them out of
  Page Up/Down.
- The shipyard table (`shipyard_table.rs`) was already generated from `+0xe`, so
  its names were right; the in-flight paths now match it.
- `ModelViewer`'s `0x3016`/`0x3022` fallback (the model's own name when no
  registration is set) follows the same code, then falls back to the slot string
  only for models without statistics.

## Tests

- `crates/ffe-rs/tests/ship_names.rs` (local cache test, ignored by default)
  enumerates all 466 model slots: for every slot with a statistics block it
  expands `+0xe` and compares `model_name`; it also checks all 58 shipyard
  models' names and `+0xc` shield words, plus Saker 0x1f (85), Eagle 0x17 (52),
  Viper 0x1e (130), Viper MkII 0x22 (140) and Cobra Mk III 0x26 (150).
- `views.rs` pins `+0xc` = 100 and `+0xe` = 0x4010 for the model-20 block and
  documents that model 20 is the Falcon, not the Eagle.
- `thargoid_view.rs` now checks model 0x31's `+0xc` shield word (180) and the
  720-shield port approximation.
- No asset changed, so the cache stays v29; `data-v28`/`data` are not rebuilt.
