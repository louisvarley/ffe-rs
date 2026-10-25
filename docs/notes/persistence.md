# Saves, commander files and configuration

Status: **native version-2 saves, quick-save/load, versioned config; legacy
commander files are recognised and left untouched.** Implemented in
`crates/ffe-rs/src/saves.rs`, `navigation.rs` (`NavigatorSave`), `flight.rs` and
`settings.rs` on 2026-10-25.

This note maps the original's commander I/O, defines the port's own format, and
records what is deliberately not claimed.

---

## 1. The original's commander I/O (JJFFE 2.8a7)

### 1.1 The save/load path

| Routine | `ffe.asm` | Role |
|---|---|---|
| `F65` | 6898 | writes a commander: version dword, global block, table blobs, object array, object payloads; then `F67(handle, 0)` |
| `F66` | 6994 | the read side of `F65`; `F67(handle, 1)` rebuilds the object links |
| `F67` | 7100 | read/write the per-object payloads for the 150 `(0x96)` present-object records |
| `F69`/`F70` | 7227/7513 | write/read the loose global arrays (400-byte dwords, 20-byte records, 5 x 0x38 maps, mission/station lists) |
| `F170`/`F171` | 15526/15607 | compress zero runs, XOR-scramble, append/check the checksum; the body is the object array `DATA_009133` (0x984a bytes), every other dword from `DATA_008862` to `DATA_008653`, 0x40 bytes at `DATA_008663`, the player block at `DATA_008804` (0x556a) and strings at `DATA_008656` (0x51) |
| `F174`/`F175` | 15776/15825 | public save: a dialog, `DirMakeCmmdrName`, `F69` + `F65`, then `fwrite` the mangled remainder |
| `F176`/`F177` | 15889/15947 | public load: `fopen`, `F70` + `F66`, read the remaining bytes, `F171` before the caller sees them |
| `F178` | 16015 | remembers the last load's `FileInfo` |
| `F184` | 16246 | increments the two-character extension (`DATA_009119/9120`, `000`..`999`) and wraps |
| `F185` | 16264 | load selection: rejects a file outside `0x258..DATA_008670` bytes, calls `F176`, then `F171`, then clamps fields (bay, velocity/cargo words, rank negatives, the F228 "fudge" defaults) |
| `F210` | 17245 | save selection: `F214` zeroes unused object slots, `F170` mangles, `F174` writes, `F184` increments the extension |
| `F192`/`F194`/`F222` | 16526/16589/18280 | load/save dialogs and the load hotkey; `F220`/`F185` load the remembered file |

`F65`'s bytes, in order:

```
u32 version (must be 1)
0x47 dwords (284 bytes) from DATA_008548 = 32-byte block + 252-byte block
0x69 (105) table blobs: F65 writes each descriptor's +0x18 size from +0x14
0x96 (150) records x 0x24 bytes from DATA_008561
F67(0): for each present record, its payload (F69 writes them)
F175: the caller's F170 body (already compressed/scrambled, with a trailing CRC)
```

The file therefore is **not** a memory dump and is not portable to another build
without that build's table sizes and field map. The exact name is
`COMMANDR\<name>.<3 chars>` on the DOS/JJFFE side; `DirMakeCmmdrName` prepends the
directory and appends the extension.

### 1.2 What the source save contains

- `DATA_008548`/`DATA_008549` (284 bytes): the global commander header block,
  followed by the table blobs and the object records. The exact field map of the
  284-byte block is not established from the source alone (it is used through
  many separate labels, e.g. `DATA_008550..8560`); do not label offsets without
  a reference specimen.
- The `F170` body: the 0x984a-byte object array (position, velocity, orientation,
  ship model, equipment, shields/hull, cargo, AI state for every object), the
  player block (`DATA_008804`, 0x556a), the galaxy/system tables, starport and
  mission/contract arrays, the two RNG words `DATA_008818/8820`, the game clock
  and the commander name/settings.
- The two RNG words, the object array and the starport records are why a loaded
  original game resumes mid-flight, including its current traffic.

### 1.3 When a save is allowed

`F185`/`F210` are reachable from the SysMenu in flight and while docked; the
source has no "docked only" gate in the save path. The only hard exclusion is the
death state: `F1104`/`F917` take over the loop, and the tombstone's
`F918(1)` returns to the front end, so no save dialog exists after `DATA_008870 =
0x2c`. The port mirrors that: flight, landed and docked saves are allowed; a save
after `Flight::death` or on the tombstone is refused.

The load dialog (`F192`) is available from the front end (`F220`/`F222`) and
from the SysMenu. `F185` rejects a file under 0x258 bytes or over `DATA_008670`
with "not a commander" and never modifies it.

---

## 2. The port-native format (version 2)

**It is not DOS/JJFFE compatible and is not claimed to be.** A legacy file is
recognised by its size (`>= 0x258`), non-JSON first byte and version dword 1, and
reported with a clear "read-only, no import without a specimen" error. The port
never writes to a legacy path.

Files live in `<root>/saves/`:

| File | Meaning |
|---|---|
| `quick.json` | F5's quick save; atomically replaced |
| `commander-NNNNNNNN.json` | Ctrl+F5's new-only numbered save (8 digits, 00000001..) |
| `config.cfg` | version-2 preferences (`controls.cfg`/`display.cfg` are read once as migration) |

JSON shape (one top-level object, `deny_unknown_fields` at every level):

```json
{
  "format": "ffe-rs-save",
  "version": 2,
  "commander": {
    "name": "Jameson",
    "ranks": [0, 0],
    "missions": []
  },
  "state": { "...": "Flight: mode, position, velocity, yaw/pitch, full axes, "
                    "credits, hold, combat (lasers/missiles/energy/heat/drive), "
                    "shields/hull, loadout, service days, death=null" },
  "navigation": {
    "system_code": 111449879,
    "clock": 102562290000.0,
    "start_clock": 102562286400.0,
    "new_game": {"start_type": 1, "...": "..."},
    "page": 0, "view": "Closed", "selected": 0,
    "target": null, "locked": null, "destination": null,
    "altitude": 2147483647, "docked": 4,
    "legal": {"wanted": false, "alert": false, "illegal_fire": false,
              "offences": 0, "fines": [0, 0, 0, 0, 0, 0]},
    "ship_lock": null, "missile_lock": null, "missile_view": null,
    "sequence": null, "takeoff": null, "landed": null,
    "final_leg": false, "spawned": null
  },
  "original_flight": false,
  "acceleration": 0
}
```

- `navigation` is absent in version-1 snapshots and the prototype loop.
- `state.axes` is the full original-flight orientation; older saves omit it.
- `system_code` identifies the system, so the bodies are regenerated from the
  cache at the saved `clock` (the source does the same from its clock/date).
  Body indices (`docked`, `target`, `locked`, `landed.index`, `sequence.station`)
  are checked against the regenerated list; a bad index is a load error.
- `commander.ranks` are the two words `DATA_008912/008913` the galaxy tables use
  (the port does not change them). `missions` is a reserved list: no mission
  engine exists in the port, so it is empty in play; it round-trips for a future
  subsystem.
- `original_flight` says the key-O model was running; `acceleration` is the
  console step. Both are port-only facts.
- Traffic, missiles and short-lived effects are **not** stored. The port's RNG is
  not one interleaved stream: markets and traffic seed per system/station/day
  (`EXP-006`/`EXP-015`), so there is no `DATA_008818/8820` pair to write. The
  saved clock and system preserve those deterministic seeds; a loaded game
  respawns traffic. This is the main deliberate difference from the source's
  object-array save.

### 2.1 Validation and errors

- File: regular, non-symlinked, `<= 1 MiB` (JSON), else a clear error.
- `format` must be exactly `ffe-rs-save`; `version` must be 2.
- `Flight::validate`: finite/bounded position/velocity, yaw/pitch, hold length,
  credits/stock, ship range, loadout, lasers/missiles, shield/hull order, docked
  consistency, orientation rows near unit length, and `death == None`.
- `NavigatorSave::validate`: finite/bounded clocks, non-negative altitude,
  known docking/launch/takeoff/landing states, index bounds (after rebuild),
  legal/new-game scalar checks.
- A save on the game-over screen or with `Flight::death` is refused before
  serialization: `SAVE REFUSED: the commander is dead.`
- A load leaves the live state unchanged on any error (the caller applies only a
  fully decoded and validated `SaveGame`). There is no fallback to an older file.

### 2.2 Atomic writes

`save_atomic` writes `.<name>.<pid>.tmp` beside the destination, `sync_all`s it,
then renames it over the destination (replace-existing). `save_numbered` keeps
the old new-only `create_new` guarantee. A failed write removes its temporary
file and leaves the previous save intact. The numbered path is used by Ctrl+F5;
quick save replaces `quick.json` atomically.

### 2.3 In-game keys and the front end

| Key | Action |
|---|---|
| F5 | quick-save `saves/quick.json` |
| Ctrl+F5 | new numbered save `commander-NNNNNNNN.json` |
| Shift+F5 | load `quick.json`, else the newest numbered save |
| `L` on the intro | load `quick.json`, else the newest numbered save (the original's Load Commander choice) |
| `--load <path>` | load a named native save and skip the intro |
| `--capture-save <dir>` | evidence: save, drop the live objects, load, rebuild the navigator, compare, write frames/log |

`--load` also works after the tombstone returns to the front end (the intro's
`L` is handled there too).

---

## 3. Configuration migration

`config.cfg`:

```
version=2
controls=modern|classic
space_colour=0..4
shape_detail=0..3
other_0..9=on|off
map_0..4=on|off
sound_0..9=on|off
scale=1..6
fit=pixel-perfect|fill-window
filter=nearest|linear
aspect=dos-4:3|square
bind.<action>=<key>        # reserved for remapping; written/read but not used
```

- A missing file is the defaults.
- If `config.cfg` is absent, the old `controls.cfg` (`modern`/`classic`, or the
  new `version=2/preset=...` form) and `display.cfg` are read once. The next
  change writes `config.cfg`; the sidecars are not rewritten.
- A malformed or future-version `config.cfg` is ignored as a whole (defaults),
  not silently replaced by stale sidecars.
- Writes are atomic. Unknown keys are ignored for forward compatibility; a known
  key with a bad value rejects the file. Full-screen is session-only and is not
  written.
- Input remapping is still a screen the port does not have. The `bind.*` keys are
  reserved so a future remap table can use the same file version; they do not
  drive `input.rs` yet.

---

## 4. Evidence

- `cargo test -p ffe-rs --lib saves` and `settings`: complete round trips,
  version-1 migration, corruption/unknown/newer version, oversized input,
  legacy recognition leaving the file byte-identical, atomic replace, quick and
  numbered naming, config migration.
- `cargo test -p ffe-rs --test saves -- --ignored` with `FFE_WORLD_CACHE`:
  complete flight commander (combat/legal/rank/mission/axes), docked pad, landed
  body, and bad-index/stage rejection through the real cache.
- `cargo run -p ffe-rs -- --root <root> --capture-save <dir>`:
  `roundtrip.json`, `save-before.ppm`, `save-after.ppm`,
  `save-roundtrip.txt`; it drops the navigator/state, reloads from disk,
  regenerates the system and asserts both halves compare equal.
- `cargo run -p ffe-rs -- --root <root> --load <save> --smoke-frames 2`:
  the real binary starts from a loaded commander.
- Existing v1 snapshot tests in `combat.rs`, `death.rs`, `views.rs` and
  `playable.rs` still pass, and the old `ffe-rs-experimental` documents migrate.

## 5. Remaining owner decisions

1. **Legacy import.** No original commander specimen is supplied. The source's
   284-byte header field map, the fixed table sizes and the per-object layout
   must be established against reference files before an importer can preserve
   values; the port currently recognises and refuses them read-only. A supplied
   `COMMANDR/*` copy plus expected values is the missing input.
2. **Traffic/RNG exactness.** The port has no single source RNG stream, so a
   loaded game respawns traffic instead of restoring the source's object array.
   Matching it needs the source object/RNG fields and a decision on whether
   despawned/killed traffic is restored.
3. **Mission/rank model.** The format reserves `ranks` and `missions`, but the
   port has no mission engine or rank progression; whether to add them (and in
   what source layout) is open.
4. **Input remapping.** `bind.*` is reserved. The actual remap screen and the
   key table it edits remain unported (`docs/CONTROLS.md`).
5. **Named slots/UI.** The source's file dialog, directory navigation and
   extension bump are replaced by quick/numbered files. A full save/load screen
   (and the `F192`/`F194` layout) is still open.
