# Galaxy map console-page evidence

The F834/F840 console page and its map-local F5-F12 keys are covered by:

- `cargo test -p ffe-rs --lib console::tests` and `... map_keys::tests`:
  the row's visibility, exact bitmap ids and radio-group order, the function
  key table, the hot-area table and key ownership.
- Local frame tests (ignored without the content cache):
  `cargo test -p ffe-rs --test galaxy_map_view -- --ignored`.
  They need a v29 `data.bin` in `FFE_WORLD_CACHE`; frames go to
  `FFE_SHOT_DIR`.

```powershell
$env:FFE_WORLD_CACHE = (Resolve-Path .\data.bin).Path
$env:FFE_SHOT_DIR = Join-Path $env:TEMP 'opencode\galaxy-frames'
cargo test -p ffe-rs --locked --test galaxy_map_view -- --ignored
```

The 2026-10-25 run produced `galaxy-map.ppm` (F834 entry page),
`map-page-controls/select/details/picture/next/previous.ppm`,
`map-page-f5-controls.ppm`, `galaxy-map-destination.ppm`,
`hyperspace-arrival.ppm`, and the script's `map-script-f5/f6/f7/f9.ppm`.
Frames stay in the temporary directory; they are not committed.

## Comparing the console row with the original

The original frame is reproduced from the executed game, not from the port:

```powershell
& .\.cache\jjffe-reference\jjffe-smoke.exe --game-frame 400 0x3d
# F3 then F6/F7/F8/F9/F10/F5, one key per 40 frames:
& .\.cache\jjffe-reference\jjffe-smoke.exe --game-frame 440 0x3d 0x40
& .\.cache\jjffe-reference\jjffe-smoke.exe --game-frame 440 0x3d 0x41
& .\.cache\jjffe-reference\jjffe-smoke.exe --game-frame 440 0x3d 0x42
& .\.cache\jjffe-reference\jjffe-smoke.exe --game-frame 440 0x3d 0x43
& .\.cache\jjffe-reference\jjffe-smoke.exe --game-frame 440 0x3d 0x44
& .\.cache\jjffe-reference\jjffe-smoke.exe --game-frame 440 0x3d 0x3f
```

`game-frame.bin` holds the 320x200 indexed frame and its 256-entry palette.
The reference states confirmed the table in `docs/notes/galaxy-map.md`:

| State | Records that change in the console row |
|---|---|
| F834 entry | only 0x24 is lit (image 1) |
| F6 | 0x23 image 1; 0x24 returns to image 0 |
| F7 | 0x22 image 1; 0x24 image 0 |
| F8 | 0x21 image 1; 0x24 image 0 (picture screen otherwise not ported) |
| F9 | 0x22 image 1 and 0x20 image 1 (radio groups 1 and 10) |
| F10 | 0x22 image 1 and 0x1f image 1 |
| F5 | no visible change after entry: F834 re-shows the page, it does not jump |
| F11/F12 | the map scene only; on the map page they are the original zoom keys |

The reference also shows the screen buttons, the time-acceleration meter, the
scanner globe and the three ship lamps still visible under the map row. The
port's `Screen::draw` therefore overlays `Console::apply_map` on the flight
page instead of clearing it.

The original console-row icon positions are the same `DATA_005100` records the
port draws (0x1c..0x24, 25x25, table positions 261..125 at y 177..163), so the
row follows the same table data. Colours differ where the original's runtime
palette approximates the 12-bit values, as for the rest of the map (EXP-008).

## Key ownership

`crates/ffe-rs/src/map_keys.rs` is the single table. `owns_keys` is true only
for `NavView::Galaxy` with the 3D page available; the port's Tab system map,
`n` places list and every closed/other view keep the global keys. The main loop
gates F5 save/load, F6-F10 views, F11 screenshot and F12/Shift+F12
viewer/lab on that predicate, so those keys are unchanged outside the map. The
unit test `only_the_galaxy_page_owns_the_keys` pins that boundary.
