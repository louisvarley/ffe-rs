# Ship-name/model-table cross-check evidence

Date: 2026-10-25. Branch `shipnames` off `35a45e0`.

Cache: `data.bin`, verified v29 `ffe-floppy-1.02-en`, SHA-256
`18fb6542b664431e88f0a6a245becd9819ee36e8073cabf56e7fe137ebf58203`.
No asset changed in this slice, so no cache rebuild/version bump was needed.

## Commands

```text
FFE_WORLD_CACHE=<repo>/data.bin cargo test -p ffe-rs --test ship_names -- --include-ignored
FFE_WORLD_CACHE=<repo>/data.bin cargo test -p ffe-rs --test views
FFE_WORLD_CACHE=<repo>/data.bin cargo test -p ffe-rs --test thargoid_view -- --ignored
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```

## Results

```text
running 3 tests
test ship_stats_split_the_shield_and_name_code_words ... ok
test every_model_name_expands_from_its_statistics_code ... ok
test the_shipyard_and_ship_list_use_the_same_expanded_names ... ok

test result: ok. 3 passed; 0 failed; 0 ignored
```

The three checks cover:

1. `+0xc`/`+0xe` split: `ShipStats` decodes shield 150 and name code `0x4028`
   from the corresponding words; F1345's `0x4028` lookup is "Spar attack
   fighter", not slot 43's "Turner Class".
2. All 466 model slots: 84 have a statistics block, all 84 resolve a non-empty
   name through `+0xe` (73 are `0x40xx` ship codes; the rest are 0 = "Mine"),
   and `ffe_rs::ships::model_name` matches the `DATA_004682` expansion.
3. All 58 shipyard hulls: `Model::name` matches the expanded code and
   `Model::shield` matches `+0xc`; `ShipList` (Page Up/Down) agrees except where
   the name is "Unknown". The known mismatches now resolve as:

| model | name | shield (+0xc) | old slot-index name |
|---|---|---|---|
| 0x1f | Saker III fighter | 85 | Viper Defence MkII |
| 0x17 | Eagle long range fighter | 52 | Sidewinder |
| 0x1e | Viper Defence Craft | 130 | Merlin attack fighter |
| 0x22 | Viper Defence MkII | 140 | Cobra Mk III |
| 0x26 | Cobra Mk III | 150 | Lanner |
| 0x2b | Spar attack fighter | 170 | Turner Class |
| 0x2e | Turner Class | 310 | Unknown |
| 0x46 | Lynx Bulk Carrier | 800 | Long-range Cruiser |
| 0x47 | Long-range Cruiser | 100 | (outside the old 14..=70 list) |

`thargoid_view` now checks model 0x31's `+0xc` shield word (180, not the name
code 0x402e = 16,430) and the port's 720-shield approximation; its kill takes 33
shots.

Full `cargo test --workspace` is green. `cargo clippy --workspace --all-targets
-- -D warnings` and `cargo fmt --all --check` are clean.
