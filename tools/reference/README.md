# Local reference oracles

`tools/reference/` holds the differential tests that compare this port with the
executed JJFFE 2.8a7 reference and with values pinned from the original's source
(`jjffe-master/ffe.asm`, `ffedat.asm`) or from the supplied game files.

Nothing here is public CI. The reference binaries embed the player's game data
and must not be distributed. They are built locally and are ignored by git.

## Build and run

```powershell
cargo build --release --locked -p ffe-tools -p ffe-rs
.\target\release\ffe-tools.exe build-data --game .\game --out .\data.bin
.\tools\reference\build-smoke.ps1 -Nasm .\.cache\tools\nasm-3.01\nasm.exe
python .\tools\reference\verify-all.py
```

`build-smoke.ps1` produces `.cache/jjffe-reference/jjffe-smoke.exe` (the plain
new-game adapter) and `jjffe-commands.exe` (the same game with primitive-emission
hooks). It needs NASM and the MSVC x86 tools. Rebuild both after changing anything
under `tools/reference/`; trace hashes that include original pointers change with
the rebuild (the child-model trace's event 26 is the documented example), so read
those oracles by their counts as well as their hash.

**Build `data.bin` from the same checkout as the code.** The cache format moves
between streams; a cache built by another branch fails the older tests with
`unknown variant` before any comparison runs.

`verify-all.py` checks every input first and then runs each oracle, printing one
line each:

```
PASS smoke (0.3s)
SKIP world-linux (60.1s)
     SKIP sysgen: ...
54 passed, 1 skipped, 0 failed of 55 oracles in 708.1s
```

Exit status is 0 when every oracle passes or is environment-gated, 1 on a real
failure and 2 when an input is missing. `--only NAME` runs a subset, `--list`
prints the names, and `--timeout N` bounds one oracle. Child scripts run with
UTF-8 I/O; paths outside the console codepage no longer break the report.

## What the oracles prove

Each row is one `verify-all.py` entry. A "differential" oracle compares the Rust
port with a trace produced by executing the original; a "pinned" oracle checks
values taken from a cited label in `ffe.asm`/`ffedat.asm` or from the game's
assets, with no reference run. Current result is the 2026-10-25 run in this
checkout.

### Core data, primitives and the model pipeline

| Oracle (script) | What it proves | Result |
|---|---|---|
| `smoke` (`verify-smoke.py`) | the reference adapter starts, presents frames and produces a bounded trace | PASS |
| `core` (`verify-core.py`) | F922's RNG, the timer and the game clock (1,536 + 60 + 238 cases) | PASS |
| `assets` (`verify-assets.py`) | floppy 1.02 / CD 1.01 profile hashes and every imported asset against `game/` | PASS |
| `font` (`verify-font.py`) | F1399 glyphs and the blit helper | PASS |
| `models` (`verify-models.py`) | F1479's vertex interpreter over the model table | PASS |
| `normals` (`verify-normals.py`) | F1480/F1754 normal records | PASS |
| `transforms` (`verify-transforms.py`) | F1520/F1519/F1552 rotations and the transform helpers | PASS |
| `rotation-operands` (`verify-rotation-operands.py`) | the rotation operand decoder | PASS |
| `visibility` (`verify-visibility.py`) | F1687/F1688's visibility tests | PASS |
| `depth` (`verify-depth.py`) | F1777's depth key | PASS |
| `instance` (`verify-instance.py`) | F1479 projection and the live vertex slots | PASS |
| `jitter` (`verify-jitter.py`) | F1476's jitter streams | PASS |
| `order` (`verify-order.py`) | F1696/F1777/F1780/F1788's composed order | PASS |
| `commands` (`verify-commands.py`) | the opcode decoder and the command helpers | PASS |
| `execution` (`verify-execution.py`) | the bounded executor | PASS |
| `point-line` (`verify-point-line.py`) | F1698/F1702 operand events | PASS |
| `text-submodel` (`verify-text-submodel.py`) | F1747 text and F1749/F1750 child-model calls | PASS |
| `colour` (`verify-colour.py`) | F1762's colour records | PASS |
| `base-transform` (`verify-base-transform.py`) | F1741/F1739 base transforms | PASS |
| `polygon` (`verify-polygon.py`) | opcode 5's polygon keys and the raster helpers | PASS |
| `textures` (`verify-textures.py`) | the texture queue/rasterisers | PASS |
| `lines` (`verify-lines.py`) | the line rasteriser | PASS |
| `fills` (`verify-fills.py`) | the fill/span rasterisers | PASS |
| `thrust` (`verify-thrust.py`) | F1121's thrust limits and acceleration | PASS |
| `physics` (`verify-physics.py`) | F1041/F603's gravity and the player step | PASS |
| `station` (`verify-station.py`) | models 74/75 through the whole F1772 executor | PASS |
| `station-queue` (`verify-station-queue.py`) | F1708/F1712/F1709/F1713/F1714's face queue | PASS |
| `station-models` (`verify-station-models.py`) | all 466 table slots the original completes, in command order | PASS |
| `child-models` (`verify-child-models.py`) | every model's child runs (1,330 child runs) | PASS |
| `child-models (depth 0x50000)` | the same with a model deep enough for far thruster vertices | PASS |
| `vertex-programs` (`verify-vertex-programs.py`) | every comparable vertex program, screen and view position | PASS |
| `thrusters` (`verify-thrusters.py`) | F1738/F1704/F1805's thruster glows | PASS |
| `animation` (`verify-animation.py`) | F866/F1675 spin, orbits and F1537/F1680/F1683 set-ups | PASS |

### World, flight and the playable

| Oracle (script) | What it proves | Result |
|---|---|---|
| `world` (`verify-world.py --scenarios worldgen terrain-mid terrain planet names galaxy`) | F880's generated systems and physics objects at synthetic clocks, the nineteen terrain midpoint routines, F1876's distance query, F576's planet contact, F791's names and F868-F871's galaxy tables | PASS (Windows worldgen fix, below) |
| `world-linux` (`verify-world.py --scenarios sysgen surface-draw`) | F875/F878's 4,160 systems and F1876's draw-mode emitter queue | SKIP on Windows: `--sysgen` needs a working SIGALRM watchdog and `--surface-draw` needs POSIX `fork`/`open_memstream`; both run on Linux |
| `traffic` (`verify-traffic.py`) | F650-F655, F657 and F867's station traffic record | PASS |
| `pads` (`verify-pads.py`) | F650's pad offset, including the open-air branch (8,192 cases) | PASS |
| `market` (`verify-market.py`) | F619/F620's price and stock formulas | PASS |
| `lift` (`verify-lift.py`) | the docking/launch lift scenes | PASS |
| `intro` (`verify-intro.py`) | F961/F962, the F829/F998/F994 start and the new game | PASS |
| `intro-palette` (`--intro-live <frames>`) | the real boot/intro loop (F961/F962/F1330/F1331) with no start key: each frame's tics, the upper palette it builds, and the final 320x200 frame + palette (`game-frame.bin`) | local evidence, no verifier |
| `collide` (`verify-collide.py`) | F569/F570/F575/F578's contacts (4,096 cases) | PASS |
| `world-tick` (`verify-world-tick.py`) | a whole F1041 player tick in a seven-body world (4,096 cases, 584 contacts) | PASS |
| `sectors` (`verify-sectors.py`) | F868's sector lookup and grid | PASS |
| `hyperspace` (`verify-hyperspace.py`) | F830/F840's distances, fuel and transfer time | PASS |
| `system-names` (`verify-system-names.py`) | the imported system name strings | PASS |
| `map-stars` (`verify-map-stars.py`) | F1769's star light points | PASS |
| `dots` (`verify-dots.py`) | F1699-F1701/F1800/F1801's map dots | PASS |
| `galaxy-map` (`verify-galaxy-map.py`) | a whole map frame from `--game-frame` against the port's frame | PASS |
| `surface` (`verify-surface.py`) | the landed frame's haze sky and terrain coverage from the executed original (`sky_matches: true`, `header_close` 0.9934) plus `planet_view`'s landing sequence | PASS |

### The 2026-10-25 wave: combat, NPCs, HUD, takeoff and preferences

| Oracle (script) | What it proves | Result |
|---|---|---|
| `combat` (`verify-combat.py`) | F998's start-type 1 loadout from the executed new game (`--state-combat`: model 23, one `0x88` mount, two `0x82` missiles, 1 t energy, 1000 CR); F931's hitscan calls (F802/F930/F953/F939, no F941/F773/F1533); F1111's bitmap 97 at (129,48) in laser views only, F1112's bitmap 98, F270's bars, F1099/F1100's text and model 156's muzzle cone through `combat_view` | PASS |
| `ship-types` (`verify-ship-types.py`) | F635's nineteen `(count, model list)` records parsed from `ffedat.asm` `DATA_006199`/`DATA_006180..6198` against `SHIP_TYPES` (19 records, 237 models) | PASS |
| `npc` (`verify-npc.py`) | F635/F688 counts and roles, F922, F953/F942 damage/death, F1536's model-157 flash and the traffic view (`npc_view`) | PASS |
| `dashboard` (`verify-dashboard.py`) | F271's ten-entry message log in the 195..320 right-hand window, F273's scroll, F1099's HUD positions and F2/record 0x0c's lamps and engines-off page | PASS |
| `takeoff` (`verify-takeoff.py`) | F1013's `0x26` hand-off above 200,000 units (or `DATA_008918 != 0`), F1031's `0x20`/station-bit decisions and the observed pad-to-flight modes `[0x24, 0x26] -> 0x00` | PASS |
| `prefs` (`verify-prefs.py`) | every page title, shape/space/sound/other/map label and music entry against `DATA_004963..67` (61 string checks) plus the six prefs unit tests (including the sound-row toggles and the captured space-colour ramp) | PASS |

## The Windows `--worldgen` fix

`--worldgen` writes each generated object's 68 name bytes. F877/F878 build part
of the name from an uninitialised byte, so the same case can read
`Testname C,D1\xe7` in one run and `...\xf3` in the next under Windows ASLR.
`verify-world.py` now compares the two runs after masking the object-name fields
`0x26..0x3f` (system records) and `0x124..0x138` (physics objects), which are
exactly the ranges `world_reference.rs` treats as loose/uncertain through
`unspecified_name()`. The **raw first run** is still what the Rust oracle
compares, so no compared byte was dropped: the normalisation is only the
repeatability decision. All other scenarios still compare byte for byte.

The split `world`/`world-linux` entries make the platform gates explicit:
`--sysgen` can hang in F878 on Windows because `win-compat.h` stubs the SIGALRM
watchdog (the script times out after 60 s and reports SKIP); `--surface-draw`
cannot run at all because its per-case fault catcher needs `fork` and
`open_memstream`. Both remain required on Linux.

## Hashes

No reference binary was rebuilt for this wave, so the recorded reference hashes
in `docs/STATUS.md` are unchanged. The values checked here:

| Oracle | Trace SHA-256 | Note |
|---|---|---|
| `world-tick` | `4258a0b2756c09c53c102a805a365cce9e6b7c51770894d801875603fa43d6ac` | unchanged; cache v26 built from this checkout |
| `ship-types` | `ae421da6e844b377033ca95d094458ea0894575fc0a0f1ca7acc350e8391d2cd` | new source-pinned trace; the table fix below changes no other hash |
| `worldgen` | varies with ASLR | the raw trace keeps the original's uninitialised name byte; read it by its counts (1,024 cases), not this hash |

`ship-types` found and fixed one transcription error: table slot `0x3` had
`DATA_006181`'s list (`0x1d, 0x1e, ...`) instead of `DATA_006183`'s
(`0x17, 0x19, 0x38, 0x38, 0x3a, 0x3a, 0x41, 0x41`). Slots 2/4/5 had the right
data but wrong Federal/Imperial labels. The playable's Ross 154 trader uses
allegiance 2 (slot 5), so gameplay is unchanged; the oracle now pins all
nineteen records.

## Gaps

- `--sysgen` and `--surface-draw` are not exercised on Windows (see above); the
  Linux build (`build-linux.sh`) still runs them.
- F688's spawner is not executed by a reference probe: the port's placement and
  minimum-one-trader/pirate rules are deliberate deviations, so the oracle
  covers the F635 table and the view/unit expectations rather than F688's exact
  count sequence. F635's population < 4 trader scaling (`DATA_008991`, F635 at
  `ffe.asm:43597`) is also not wired into `model_for`; the table oracle pins the
  data, not that branch.
- HUD, takeoff, NPC and preference expectations are pinned from the asm/cache,
  not from a whole-frame original capture. The whole-frame pixel oracle remains
  future work; `galaxy-map` is the only frame-level comparison today.
- The child-model trace hash moves with every reference rebuild (event 26's
  uninitialised base handedness); read that oracle by its counts (440 compared,
  0 mismatched).
