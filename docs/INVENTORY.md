# Phase 0 inventory — FFED3D / First Encounters

Status: **Phase 0 study complete; port not started.** This document is the inventory required before the Rust port. It describes what is in `legacy/ffed3d/`, what the engine does, what the player-supplied game files contain, and where the numerical and behavioural risks are.

Snapshot studied:

- `legacy/ffed3d-master/` (the reference source; the directory name in this checkout is `ffed3d-master`, the brief calls it `legacy/ffed3d/`).
- `legacy/Elite3/` and `game/`: 71 files each, byte-identical (verified by SHA-256). See `docs/GAME_FILES.md`.
- Legacy executable self-identifies as **First Encounters English Release 1.02 — 21/04/95**, running on the **JJFFE 2.8a6** reverse-engineered executable with the **Anisotropic mod 0.9c** and further FFED3D changes. The splash itself says: “This is a reverse engineered version of a commercial game / It is illegal to own it even if you own the original.”

> The legacy source has **no licence** and descends from Frontier Developments' code. Nothing from `legacy/` or `game/` is committed. See `docs/LEGAL.md`.

---

## 1. Reference source tree

### 1.1 Assembly

| File | Size / lines | Role |
|---|---:|---|
| `ffe.asm` | 2,617,907 B / 136,423 lines | The reconstructed x86 game code: 1,715 functions, module/event dispatch, simulation, UI, sound/music wrappers, text and 2D rasteriser. It `%include`s `ffedat.asm` at line 2115. |
| `ffedat.asm` | 1,096,095 B / 29,701 lines | Reconstructed `DATA` section: 4,486 labels, 1,203,054 bytes of tables, strings, model pointers, bitmap pointers, global state/BSS. Ends with `resb` arrays (BSS). |
| `ffebmp.asm` | 7,615,954 B / 175,471 lines | Reconstructed graphics/model data: 4,323 labels, 1,314,768 bytes. Contains the model command streams (from `DATA_000090` upwards), the 465-entry model table at `DATA_004681`, the `DATA_008240`/`DATA_008241` bitmap index tables, and the indexed bitmaps. |
| `doc/firstenc.rec` | 3,119,134 B | Decompiler output for the original `FIRSTENC.EXE` (entry `0x00410000`). Useful cross-check, not source to copy. |
| `doc/ffed3ddb.c` | 4,209,924 B | Hex-Rays output/symbol database for the original executable. |
| `doc/srctools/*.txt` | ~200 KB | Jongware/community reverse-engineering notes: object structures, physics, AI, missions, sysgen, starports, ship indices. High-value documentation. |

Counts were produced by parsing every function label, `Pos`, call and data label in the three `.asm` files. `ffe.asm` contains **1,715 functions** and **323,169 bytes** of function spans (the `.text` section is 430,080 bytes virtual; the difference is jump tables, inline data and padding). 331 function labels carry a descriptive `_FUNC_000NNN_Name` name; the rest are `FUNC_NNNNNN` with comments.

The original executable is a 32-bit PE:

| Section | VA | Virtual size | Raw offset | Raw size |
|---|---:|---:|---:|---:|
| `CODE` | `0x00010000` | 430,080 | `0x00000E00` | 429,568 |
| `DATA` | `0x00080000` | 2,469,888 | `0x00069C00` | 1,621,504 |
| `.idata` | `0x002E0000` | 4,096 | `0x001F5A00` | 1,536 |
| `.edata` | `0x002F0000` | 4,096 | `0x001F6000` | 512 |
| `.reloc` | `0x00300000` | 40,960 | `0x001F6200` | 39,424 |

Image base `0x00400000`, entry `0x00410000`. The reconstructed assembly uses original-style `Pos` values and absolute data symbols; `ffedat.asm`/`ffebmp.asm` are the original `DATA` section split into two NASM objects. The raw section is 1.62 MB, the rest of the 2.47 MB virtual size is BSS.

### 1.2 C/C++ modules

| File | Lines | Role |
|---|---:|---|
| `win/gfx.cpp` | 6,159 | The FFED3D D3D9 replacement for the model command interpreter and the 2D primitives. Parses the original model command stream (`sendToRender`), builds vertex buffers, implements sprites, text, lines/quads/triangles, texture triangles/quads, galaxy background, planet hooks. |
| `win/planet.cpp` | 5,740 | Planet tessellation/LOD support, icosahedron subdivision helpers, collision/landing checks, station placement (`C_PlaceStation`), GPU planet draw. Mostly a reimplementation of the original procedural planet code. |
| `win/w32video.cpp` | 3,250 | Win32 window, D3D9 device, render loop (`Render`), viewport/aspect handling, render-to-texture loading screens, model/texture/effect loading, panel composition, HUD text/sprite submission, console. |
| `win/w32sound.cpp` | 873 | Sound effects (DirectSound), HMP→MIDI conversion, songs via BASS/MP3 or MCI, volume, streaming. |
| `win/w32input.cpp` | 806 | DirectInput 5 mouse/keyboard/joystick, key bindings from `ffewin.cfg`, mouse sensitivity, joystick calibration/dead zones. |
| `win/w32dir.cpp` | 256 | Paths (`FX`, `MUSIC`, `DATA`, `COMMANDR`), file dialogs, drive/directory enumeration. |
| `win/w32time.cpp` | 72 | `QueryPerformanceCounter` timer, `SystemInit`/`SystemCleanup` ordering. |
| `win/ffemisc.cpp` | 876 | Double-precision vector/matrix helpers, `rotateobject`, system data (`CreateSystemData`, `RefreshSystemData`, `SystemTick`), wingmen, nearby-system selection, `GetInstance`/`GetModel`. |
| `win/sysgen.cpp` | 1,086 | Extended-universe system overrides (`Override_F869/F870/F871`), planet effects, government/economy tables, starport supply. |
| `win/market.cpp` | 801 | Market generation/refresh, stock/price formulae, pirates, cargo, double-price ads, radar cargo display. |
| `win/bbs.cpp` | 442 | Bulletin-board ad generation, military BBS entries. |
| `win/missions.cpp` | 1,409 | Passenger/assassination/military mission generation and completion, military bases. |
| `win/ships.cpp` | 1,368 | Ship creation/spawn, equipment choice, damage, bounty, shipyard data, traders. |
| `win/logic.cpp` | 626 | Lua 5.1 scripting layer (console, pre-launch/auto scripts), FFED3D addition. |
| `win/ffeScrMethods.cpp` | 155 | Lua bindings for spawning objects/enemies and equipment. |
| `win/bbs.cpp`, `win/market.cpp`, `win/missions.cpp`, `win/ships.cpp`, `win/sysgen.cpp` | — | Replace or wrap original assembly routines; see `EXTERN _C_...` in `ffe.asm` and the `extern "C"` declarations in `win/misc.h`. |
| `win/XFile/*` | 1,100 | DirectX `.x`/`.X` model loader and animation hierarchy (FFED3D HD models). |
| `win/Render/*` | 300 | D3D9 vertex declarations/buffers. |
| `hmp.cpp` | 272 | HMP (Human Machine Interfaces music) to Standard MIDI File converter. |
| `ffecfg.c` | 186 | INI parser (`[section]`, `key=value`, case-insensitive). |
| `win/ffetypes.h` | 475 | Core structures (`ModelInstance_t`, `Model_t`, `GlobalVars`, `DrawMdl_t`, equipment/AI flags). |
| `win/misc.h` | 471 | Game externs, `equipdata_t`, `starport_t`, `systemObject_t`, government/world enums, `KM_DIST`. |
| `win/planet.h` | 246 | Planet/vertex/tessellation declarations. |
| `win/ffe3d.h` | 335 | D3D includes, `MODEL`, `CUSTOMVERTEX`, `FFELIGHT`, `xModel`, material constants. |
| `Utils/xmath.h` | 4,882 | Vector/matrix math; contains an **x87 `round`** helper (`fld`/`fistp`) used by rendering. |
| `resources/` | — | FFED3D-added load screens, skyboxes, `texNNN` textures, `.X` models, shaders, Lua scripts, `ffewin.cfg`. **Not original base content; not to be committed.** |

`win/lua/`, `win/bass.h`, `include/GL/`, `lib/` are third-party headers/libraries (Lua 5.1, BASS, Mesa GLU headers, import libraries). The GLU tessellator is used for polygon tessellation in `gfx.cpp`.

### 1.3 Documentation in the reference

`doc/srctools/` is the most useful part for the port: `gmstruct.txt` (global state layout), `plstate.txt` (object/player state), `cphys.txt` (physics, including exact shift/cap behaviour), `csysgen.txt` (procedural systems), `cplayer.txt`, `cagent.txt`, `cbbs.txt`, `ccoll.txt`, `chyper.txt`, `claser.txt`, `cname.txt`, `cstarport.txt`, `csystem.txt`, `shuttle.txt`, `shipidx.txt`. `doc/jongware/` contains the Frontier Galaxy articles and mesh notes. These are study aids; the assembly is the authority.

---

## 2. Execution model

`_asmmain` (`ffe.asm:70152`) is the whole game:

1. Zero `0x556a` bytes at `DATA_008804` (the main `GameState` block) and `0x51` bytes at `DATA_008656`.
2. `F1527_SysInit` → `SystemInit` (platform: timer, dirs, video, input, sound), `KeybInit`, `DisplayInit`, `SoundInit`, `TimerInit`.
3. Splash screen, then the module/event system.
4. The main loop alternates `TimerAdvance`, module event dispatch, input, rendering, and `FlipScreen`.
5. `F1528_SysCleanup` / `F1529_SysExit` tear down.

### 2.1 Modules and events

The game is a set of **21 modules** (index 0–20), each with an 8-byte event table entry. The module table is `DATA_004892` in `ffedat.asm`; entries include:

| Index | Table | Role |
|---:|---|---|
| 0 | `DATA_007118` | boot/loader |
| 1 | `DATA_007104` | flight/space |
| 2 | `DATA_006798` | system/universe |
| 3 | `DATA_007254` | station/starport |
| 4 | `DATA_005091` | ship/console |
| 5 | `DATA_007239` | combat |
| 6 | `DATA_007105` | navigation |
| 7 | `DATA_006163` | starport |
| 8 | `DATA_007230` | services |
| 9 | `DATA_006273` | market |
| 10 | `DATA_004962` | system menu / file dialogs |
| 11 | `DATA_006254` | comms |
| 12 | `DATA_005285` | info |
| 13 | `DATA_002319` | video/intro |
| 14 | `DATA_005105` | military base |
| 15 | `DATA_005038` | missions |
| 16 | `DATA_007186` | BBS |
| 17 | `DATA_005054` | shipyard |
| 18 | `DATA_005285` | info (alias) |
| 19 | `DATA_002285` | journal |
| 20 | `DATA_001700` | mission scripts |

Events are dispatched by `F144(event, module, param)`; `F148(event, param)` broadcasts events to every active module with bit 2 set in its 8-byte state. Well-known events (from `doc/srctools/plstate.txt` and the assembly):

- 0: init
- 1: switch-to (param = key)
- 2: various (0x15 rollback, 0x10 week, 0xf day)
- 3: inactive per-frame
- 4: active per-frame
- 5: per-frame time update (`{day, tics, PhysObj*}`)
- 6: object update
- 8: switch-from
- 10: seconds
- 11: object event (death/damage)

The main loop calls event 5 for the player state and event 6/11 for objects; AI/traders use event 5/6. This event bus is the natural seam for the Rust `GameState`/`Sim` design.

### 2.2 Global state

There is one large mutable block at `DATA_008804`, zeroed at startup, size **0x556a bytes**. `doc/srctools/gmstruct.txt` gives a field-by-field offset map; key examples:

| Offset | Global | Meaning |
|---:|---|---|
| `0x000` | `DATA_008804` | game tics (32-bit, starts `0x80000000`) |
| `0x004` | `DATA_008807` | game days (starts `0x121cf7`) |
| `0x010` | `DATA_008812` | graphics detail |
| `0x020` | `DATA_008815` | background colour |
| `0x0c8` | `DATA_008861` | player object pointer |
| `0x0cc` | `DATA_008862` | starport object pointer |
| `0x0e8` | `DATA_008870` | player state (flight/docking/etc.) |
| `0x0e9` | `DATA_008871` | player interface flags |
| `0x0fa` | `DATA_008881` | start type |
| `0x114` | `DATA_008888` | player cash |
| `0x118` | `DATA_008889` | cargo space |
| `0x134` | `DATA_008896` | 32-word cargo array |
| `0x19c` | `DATA_008911` | Elite rating points |
| `0x1a0/0x1a4` | `DATA_008912/13` | Federal/Imperial rank points |
| `0x24e` | `DATA_008949` | star-map display flags |
| `0x250` | `DATA_008950` | galgen player system vector |
| `0x3f4c` | `DATA_009014` | detail levels |
| `0x3f5c` | `DATA_009028` | sound flags |
| `0x4ef4` | `DATA_009085` | 312-byte contract array (6 × 52) |
| `0x50f8` | `DATA_009095` | fines |

Other important standalone globals:

- `DATA_ObjectArray` / `DATA_009133` — instance list: 116 object slots (`InstanseList_t`, 116 × `ModelInstance_t` + 116 state bytes). Object 0..114 used; 0x72 = 114 is the last update slot.
- `DATA_StarportArray` / `DATA_008999` — 18 × `starport_t` (market, BBS, shipyard).
- `DATA_ffeModelList` / `DATA_004681` — 465 model slots (ships, planets, effects, HUD parts, etc.).
- `DATA_PlayerObject`, `DATA_PlayerIndex` (player object index, usually 0x72? see `plstate`; the player is at `DATA_008861`).
- RNG: `DATA_008818`/`DATA_008820` (two 32-bit seeds), `DATA_RandomizerFunc` → `F1530` (returns low 16 bits), `DATA_FixedRandomFunc`, `DATA_BoundRandom`, sysgen-local `DATA_008843`/`DATA_008844`.
- `DATA_GameTics`, `DATA_GameDays`, `DATA_FrameTime`.
- `DATA_StockData` (33 goods × 4 dwords), `DATA_EquipmentData` (64 entries), `DATA_PlayerCargo` (32 words), `DATA_CurrentSystem`, `DATA_CurrentAllegiance`, `DATA_SystemTechLevel`, `DATA_PoliceLevel`, `DATA_CurrentDanger`, `DATA_CurrentPirates`, `DATA_CurrentTraders`.
- `DATA_MilitaryMissions[26]`, `DATA_PackageArray[60]`, `DATA_ContractArray[312]`.
- `DATA_009252` — 256-byte palette (6-bit RGB).
- `DATA_009226` — 256,000-byte screen/back buffer region (the 320×200 8-bit framebuffer is `DATA_007773` → `DATA_009226`).
- `DATA_009253` — 200,000-byte workspace; `DATA_009274` — 108,000-byte planet vertex workspace; `DATA_009277` — 48,000-byte planet array.

The Rust port will replace this with one `GameState` passed as `&mut`; the offset map above is the compatibility checklist for save files and trace serialization.

### 2.3 Function map

The 1,715 functions are clustered by original code address. The table below is a navigation aid (counts include local/thunk functions):

| `Pos` range | Count | Area |
|---|---:|---|
| `0x11000–0x15fff` | 57 | file loading, compression/scramble, save/load dialogs, command files |
| `0x16000–0x1afff` | 103 | string expansion, menus, keyboard/mouse input, file dialogs, display init |
| `0x1b000–0x22fff` | 177 | console/UI, ranks, comms, BBS/journal/mission screens, services |
| `0x23000–0x2ffff` | 246 | equipment, market, missions, shipyard, combat, AI, spawning |
| `0x30000–0x3afff` | 108 | AI/object update, system generation, hyperdrive, system objects |
| `0x3b000–0x46fff` | 185 | object creation/damage/cargo, player states, docking, flight, death |
| `0x4e000–0x53fff` | 154 | palette, string formatting, GUI input, hot areas, math primitives |
| `0x54000–0x58fff` | 120 | 2D primitives, text, bitmap blitting, cursor, screen flip |
| `0x59000–0x63fff` | 130 | texture-mapped triangles/quads, galaxy, planet draw, model command interpreter |
| `0x64000–0x70fff` | 120 | 3D math, sin/cos, planet tessellation, sound/music wrappers |
| `0x71000+` | 20 | module event handlers, `asmmain`, timer, splash |

Exact named entry points that matter for the port are listed in the source comments: `F165–F184` (file/commander I/O), `F261–F264` (console), `F304/F305` (ranks), `F574–F612` (physics/collision/turning/thrust), `F688–F699` (spawns), `F772–F775` (AI spawn/source system), `F829–F903` (start position, system data, galaxy, hyperdrive), `F924–F953` (objects/cargo/damage/equipment), `F1332–F1341` (64-bit math), `F1342–F1402` (strings), `F1406–F1446` (input/GUI), `F1446–F1449` (screenshot), `F1465–F1521` (vector/FFP/matrix), `F1532/F1538` (object/model lookup), `F1569–F1660` (2D/3D drawing), `F1882` (frame flip), `F1900–F1910` (sound).

---

## 3. File I/O and formats

### 3.1 Paths

`DirInit` (`win/w32dir.cpp`) uses:

- `COMMANDR\` — commander/save files.
- `FX\` — digital samples (`*.RAW`).
- `MUSIC\` — songs (`*.HMP`, and FFED3D also looks for `.MID`/`.MP3`).
- `DATA\` — AVI intro files (`*.avi`) if present.
- `ffewin.cfg` — FFED3D platform config.
- `jjffe.cfg` — original JJFFE timer config (`[TIMER] maxfps`).
- `cabin%i.bmp`, `mission.dat` — relative to the game directory.
- If `mission.dat` is not in the current directory, FFED3D `_chdir("game")` (the reference build expects the game in a `game/` subfolder).

### 3.2 Commander/save files

`F170_CdrFileMangleData` / `F171_CdrFileUnmangleData` (`ffe.asm:15867`/`15948`) compress and XOR-scramble the persistent state. The comment states it saves `0x984a` bytes at `DATA_009133` (the object array), every other dword from `DATA_008862` to `DATA_008653`, `0x40` bytes at `DATA_008663`, etc., then compresses zero runs (`{0,n}`) and scrambles with an iterative XOR/CRC. `F174`/`F175` write, `F176`/`F177` read; `F178` records the last loaded file. Save names are 20 bytes + a 3-character extension that is incremented/scrambled (`F184`). The file is not a simple memory dump: it is a defined subset with compression and scrambling.

**Port implication:** save compatibility is frozen. We must reproduce the exact subset, byte order, zero-run compression, scramble, and CRC. The first implementation should be a byte-exact round-trip of the legacy save, tested on saves produced by the reference build. Do not invent a new save format for the default path; a separate modern format may be offered only as an explicit opt-in.

### 3.3 `MISSION.DAT` (journal text)

`MISSION.DAT` is **not** the mission definitions; it is the journal/news text archive. `F76`/`F78` load entries using the offset table at `DATA_002292`/`DATA_002293` in `ffedat.asm`:

- `DATA_002292` holds absolute file offsets for each journal entry (first entry at 1).
- `DATA_002293` holds the end offsets.
- The file is read as 4-byte chunks. Each 32-bit word is bit-permuted (the assembly builds a big-endian word and rearranges bits 31/30/29/28 into positions 31/23/15/7, and 7-bit groups into bytes 3/2/1/0), then written back in reverse byte order.
- The decoded bytes are FF-code text (`0xff` commands), with `0x7f` as a special break, `0x0e` → `0x83`, `0x26` → `0x2b`, `0x0a` → `0x0d`, `0xd5` → `0x27`. `F76`/`F78` then expand bracket codes (`[`) and wrap to 300 columns for the journal viewer.

**Port implication:** the converter must implement the bit permutation and FF-code expansion exactly and verify against the legacy journal output. The decoded text can be stored as UTF-8 with a documented escape for FF codes, but the Classic text expander must be able to reproduce the original glyph sequence.

### 3.4 Indexed bitmaps

**JJFFE implementation update:** the counts below are from the historical
FFED3D study, not the current JJFFE 2.8a7 baseline. Direct source/original-file
verification finds **96 primary and 251 secondary** slots in JJFFE: 345 ordinary
images plus two 32768-byte texture blocks (primary 94/95, zero encoded height).
All payload bytes match both supplied floppy/CD executables. Header words at
4/6 are signed drawing offsets, not merely padding; flag bit 0 masks index zero.
The actual partial Rust cache/layout contract is in [FORMATS.md](FORMATS.md).

The bitmap index table is `DATA_008240` (96 entries) followed by `DATA_008241` (261 entries) in `ffebmp.asm`. `F1620_BmpIndexToPtr` returns `DATA_008240[index]`; `DATA_008241` is the second sprite set used by `F1609_DrawSprite` (flag switch).

Bitmap blob layout (from `C_BlitClipInternal`):

```
offset 0:  u8  type/flags (observed 1)
offset 1:  u8  height
offset 2:  u16 width (little-endian)
offset 4:  4 bytes zero/padding
offset 8:  width*height bytes, 8-bit palette indices, row-major top-to-bottom
```

The first blob `DATA_008242` is 8×13; the largest (`DATA_008096`) is 320×158, exactly the upper 3D screen. `CABIN0..5.BMP` are standard 320×158, 8-bit uncompressed BMPs with a 256-entry palette; the floppy data also has `cabin%i.bmp` loading from disk. The embedded table covers HUD panels, buttons, ships' cockpit/cabin views, fonts (glyph bitmaps are separate), scanner sprites, and effects. The 96/261 counts are index-table entries, not unique blobs; some entries alias.

**Classic requirement:** store the original palette indices, not just RGBA. PNG indexed with `PLTE`/`tRNS` plus a sidecar 6-bit palette is the right representation. The palette is 6-bit per channel (`0..63`), not 8-bit.

### 3.5 Models

`Model_t` (`win/ffetypes.h`) is a 0x5c-byte structure:

```
u16 *Mesh_ptr;        // command stream
s8  *Vertices_ptr;    // packed vertices
u32 NumVertices;
s8  *Normals_ptr;
u32 NumNormals;
u32 Scale, Scale2;    // exponential scale values
u32 Radius;
u32 Primitives;       // model/primitive count
u8  DefaultColorR/G/B, padding;
u32 field_28;
s32 interract_radius, collision_radius;
u16 *Collision_ptr;
ShipDef_t *Shipdef_ptr;
u8 DefaultCharacter;
u16 *Character[1];
```

Models are **not** triangle lists. They are a compact command stream interpreted by `sendToRender` (`gfx.cpp`) / the original assembly. The command word has material bits used by `setMaterial`:

- `0x2000` — no lighting (unlit).
- `0x1000` — use the local colour (`localColor[3]`) instead of the packed colour.
- `0x4000` — texture index `400 + (cmd & 0x0fff)`.
- otherwise bits `0x0f00/0x00f0/0x000f` are 4-bit R/G/B, expanded as `component*12 + 63`.
- Other commands select vertices, normals, submodels, jumps, conditional visibility, matrix/mirror/rotation, and math operations (`doMath`). Rotation/mirror semantics: bits `0..2` mirror, `3..5` rotate 90° around an axis, value 6 uses a preset matrix; `0x38` negates rows (`doc/jongware/rotation.txt`).

`ShipDef_t` is the ship stat block (thrust, lasers, fuel scoop, mass, capacity, price, scale, description string index, crew, missiles, drive, integral drive, Elite bonus, six weapon mount offsets). The ship table is `DATA_004681` in `ffebmp.asm` (465 model pointers); `DATA_004682` is the 209-entry ship-string table. `doc/srctools/shipidx.txt` lists 71 named ship types (0x00–0x47), including missiles, Thargoid craft, and the full FFE roster.

**Classic requirement:** preserve the command stream, not only a triangulated mesh, because submodel conditionals, mirror/rotation, material and palette selection affect the look. A triangulated glTF approximation is fine for Enhanced/HD assets, but the Classic renderer must interpret the original stream (or a lossless semantic projection of it).

### 3.6 Tables and text

- Ship stats: `ShipDef_t` arrays inside the model blobs in `ffebmp.asm`.
- Equipment: `DATA_EquipmentData` at `DATA_005289` in `ffedat.asm`, 64 × 24-byte `equipdata_t` (buy, sell, weight, `three`, type, `eighty`, id, tech level, something, zero). The first entries are the missile/laser/utility list.
- Goods: `DATA_StockData` at `DATA_006169`, 33 × 4 dwords (price, price variance, stock, stock variance), with names in the string table.
- Cargo: `DATA_PlayerCargo` (32 words), `DATA_CargoFromShip` (destroyed-ship cargo indices), `DATA_JettisonedCargoTypes[33]`.
- Missions: `DATA_MilitaryMissions[26]` (24-byte entries), `DATA_PackageArray[60]` (20-byte entries), `DATA_ContractArray[312]` (6 × 52-byte contracts), mission string table `DATA_001699` (468 entries), mission event function table `DATA_001700`, journal event table `DATA_002285`.
- Text: string tables throughout `ffedat.asm`, FF-code expansion (`F1342–F1402`), ship strings at `DATA_004682`, SysMenu strings at `DATA_004894`, starport/BBS/mission text. The original UI strings are mostly in `DATA`.
- The 256-colour palette is `DATA_009252` (256 × 3 bytes, 6-bit).
- Star chart/hand-coded systems: `DATA_007106` (system type info), `DATA_007121–DATA_007138` (star mass/type/radius/temperature/model tables), `DATA_007184` (name syllables), `DATA_007190+` (starport/station templates), `DATA_007792+` (hand-coded Sol and other systems).

### 3.7 Sounds and music

`win/w32sound.cpp` declares 40 sample slots:

- 24 distinct files in `FX/*.RAW` (plus `TEST.RAW`): `AIROPEN, BIP, BONG, BOOM, CAMERA, CHIME, CLUNKIT, DAMAGE, ECM, HYPE, LASER1–4, LAUNCH, MISSEXPL, NECM, NOWT (0 bytes), ROAR, SHIPEXP, SIREN, STATION, WIND`.
- Format: **16-bit signed little-endian, mono, 22,050 Hz** (verified by the DirectSound `WAVEFORMATEX` and sample statistics). `NOWT.RAW` is deliberately empty.
- 16 mixing channels; volume/log-volume and pitch; loop counts in the sample table (station 50, ECM 2, siren 3, etc.).
- 17 songs in `MUSIC/*.HMP`: `QQFRONT, QQATMOSP, QQTRAVEL, QQDRAMA, QQSUSPEN, QQROCK, QQESCAPE, QQPARADI, QQFRONT2, HALLKING, INTRO, BABAYAGA, BAREM, JUPALT, VALKRIES, BLUEDAN, GGOFKIEV`.
- `hmp.cpp` converts HMP (`HMIM` header, track count at offset 0x30) to a type-1 Standard MIDI File; the converter is deterministic and self-contained.
- FFED3D adds MP3/BASS paths and a custom music folder; those are not the original behaviour.

---

## 4. Content audit — where each thing lives

| Content | Location | Hardcoded? | Notes |
|---|---|---|---|
| Ship types, stats, 3D models | `FIRSTENC.EXE` `DATA` (reconstructed as `ffebmp.asm`/`ffedat.asm`) | Yes, inside the executable | 71 named ship types, 465 model slots; no external model files in the floppy release. |
| Ship names/descriptions | `DATA` string tables | Yes, inside EXE | Expanded through FF codes. |
| Equipment list/prices | `DATA_EquipmentData` in `DATA` | Yes, inside EXE | 64 entries. |
| Trade goods/prices/stock | `DATA_StockData` in `DATA` | Yes, inside EXE | 33 goods. |
| Systems (names, types, tech, government, population, danger) | Procedural, with hand-coded Sol/other overrides in `DATA` | Procedural + tables in EXE | See §5. |
| Planets/moons/stations/starports | Procedural from system generator; hand-coded Sol objects in `DATA` | Procedural + tables | Up to 60 system objects; max 18 starports/stations. |
| Planet surfaces | Procedural at runtime (icosahedron/LOD/noise) | Yes, algorithm + constants in code | No bitmap planet textures in the floppy EXE; FFED3D adds GPU textures. |
| Star field/nebulae | Procedural/embedded model + palette | Yes | FFED3D adds skybox TGA/PNG. |
| Missions (BBS/passenger/assassination/military) | Mission tables + code in `DATA`/`ffe.asm`, plus C++ constructors | Yes | `DATA_MilitaryMissions[26]`, `DATA_PackageArray[60]`, `DATA_ContractArray[312]`, 468 mission strings. |
| Journal/news text | `MISSION.DAT` + offset table in `DATA` | File + table | Decoded/expanded at runtime; see §3.3. |
| BBS adverts | Generated from templates/code; 18 slots per starport | Templates in EXE | `starport_t.adverts[18]` (older notes say 12; the C++ struct uses 18). |
| UI text/labels | `DATA` string tables | Yes | Mostly FF-code text. |
| Cockpit/cabin art | `CABIN0..5.BMP` + embedded bitmaps | Files + EXE | 320×158 indexed BMPs. |
| HUD/icons/panels | Embedded bitmaps, `DATA_008240/41` | Yes | 96 + 261 index entries. |
| Sound effects | `FX/*.RAW` | Files | 16-bit PCM 22050 mono. |
| Music | `MUSIC/*.HMP` | Files | HMP, converted to MIDI. |
| Save files | `COMMANDR/*` | User data | Compressed/scrambled subset, §3.2. |
| Intro AVI | `DATA/*.avi` (referenced; absent in this floppy copy) | Files (optional) | CD/large install may include. |
| Config | `ffewin.cfg`, `jjffe.cfg`, `hmiset.cfg`, `SETUP.INI`, `PATCHES.INI` | Files | Platform/sound setup, not content. |
| FFED3D additions | `resources/` (models, textures, shaders, Lua, loading screens) and C++ | Yes, but **not original base content** | Excluded from `data.bin`; see rule 4. |

**Rule 4 finding (preliminary):** all *base* content we have identified is present in the player's `FIRSTENC.EXE`/`MISSION.DAT`/`FX`/`MUSIC`/`CABIN*.BMP`, so the converter can obtain it from the game files. FFED3D adds new ships/models/textures/scripts/UI strings; these are not base content and will not be committed. A few C++ tables (AI equipment weights, market/sysgen formulae) are reverse-engineered logic constants; they are logic, not content. Before committing any table/text that cannot be extracted from the player's files, we will stop and ask. See §9.

---

## 5. Procedural universe and system generation

### 5.1 Coordinates and naming

A system ID packs:

```
systemcode = (sectX & 0x1fff) | ((sectY & 0x1fff) << 13) | (starNumber & 0x3f) << 26
```

`FindDestID` (`win/ffemisc.cpp`) generates a random destination within `sectDist` using `FloatRandom()*2π`, then `F853_GetNumstars(x,y,0) >> 10`; if stars exist, picks `rand % numStars` and returns the packed ID. The galaxy is an infinite 2D sector grid; each sector can contain multiple stars. Names are generated from syllable tables (`F791_GenerateName`, `F830`), with hand-coded sectors checked first (`DATA_007799` table) and special cases (Gateway, Thargoid systems). System names have components A/B, planets get Roman/arabic suffixes, and stations/starports get compound names.

The sector coordinate origin is `(0x1718, 0x1524)` for the player's start; `CoreX/CoreY` in `sysgen.cpp` encode the Federal/Imperial/Alliance cores. `DATA_007106` is the base of the system-type table; `F868` looks up a system index, `F869/F870/F871` return its parameters. `F878_GetSysObjectData` generates the object list (stars, planets, moons, stations) from two 32-bit seeds and a star count. The generator uses its own LCG `F894` (`D8843/D8844`) and name RNG, separate from the global gameplay RNG. **Any mod that inserts/overrides a system must not perturb these seeds for other systems**; the design will key generation on `(sector, star)` only.

### 5.2 System objects

`sysGenParams_t` is `{randseed1, randseed2, numstars, systemcode, techLevel, name[20], ...}`. `F878` fills up to 60 `systemObject_t` entries (68 bytes each):

```
0x00 mass (Earth masses)
0x04 randseed
0x08 parent index
0x0a objType (0x23 starport, <0x20 planets/stars)
0x0c model index
0x0e orbital radius
0x12 latitude
0x14 orbital velocity
0x18 temp pointer to PhysObj
0x1c light output
0x20 unknown
0x22 longitude
0x24 temperature (K)
0x26 name[20]
0x3a/0x3c unknown
0x3e rotation speed
0x3f tree level (copied to object 0xcb)
0x40/0x42 unknown/orientation
```

Object types 0x00–0x11 are stars/planets (binary point, asteroid, barren rock, thin atmosphere, volcanic, ammonia/methane/water corrosive, terraformed, indigenous life, thick corrosive, gas giants, brown dwarf); 0x12–0x1f are star types; 0x20–0x23 are orbital trading post/station/city/starport. The star mass/radius/temperature/model tables are `DATA_007121`–`DATA_007138`. `F881` generates a star/planet pair, `F882` computes orbit parameters (`orbvel = sqrt(r³/mass) * 0x5edb >> 15`), `F883`–`F890` are type-specific "manglers" (temperature/atmosphere/life/stations), `F891` generates starports, `F892` stations, `F893` names. `F880` creates the runtime `PhysObj`s and sets masses/radii/orbits.

`F879`/`C_PlaceStation` place a starport on a planet: build the local matrix at `(lat, lon)`, compute the planet radius from `Model_t.interract_radius`, `Scale+Scale2`, iteratively correct the distance (`dist -= dist>>4`, then `>>4`/shift), and store the 64-bit relative position. This is a known source of the "starports below ground" bug that FFED3D fixes (`FIX: Starports below ground`).

### 5.3 Economy

System parameters come from `F869` (override/description/tech/pirates/police/traders/miners/government) and `F870/F871` (stock flags/population/danger, military activity/ranks). The FFED3D C++ overrides (`Override_F869/F870/F871`) are only used when `DATA_ExtendedUniverse` is non-zero (the "Frontier 3500" start). In the normal start, the original assembly path is used.

Market generation (`market.cpp`) uses system population/danger/supply/tech, per-item price/stock tables, distance from the galactic core, government morality, and `DATA_FixedRandomFunc`. It is deterministic given the system and RNG state. The exact integer/float order matters; see §6.

---

## 6. Numeric model

This is the highest-risk area for “tick-for-tick identical” traces.

### 6.1 Integer widths

- 8/16/32/64-bit little-endian; x86 default `int` is 32-bit.
- `Point32` = three `s32`; `Point64`/`_s64` = three `s64`; `_u64` for unsigned distances.
- Object positions use 64-bit values with an attached **exponential scale** byte (`model_scale`, `dist_cam`, `dist_3face`). `Vec64Truncate` is an arithmetic right shift, not a normalise.
- Angles are 16-bit binary angles (`0x10000` = full turn; `0x4000` = 90°). `Sin16`/`Cos16` tables (F1913/F1914) and `F1505` build rotation matrices.
- Matrix elements are 1.31 fixed-point: `0x7fff8000` = 1.0 (`win/ffemisc.cpp`). `MatBuild*` multiplies/adds with `sar 15`/`sar 16`; rounding is truncation.
- 16.16 fixed-point is used for some orbital/velocity values (`F1515` is `(a_lo*b)>>16 + a_hi*b`; `F1516` scales by 2 with sign handling).
- FFP: a 16-bit-mantissa/16-bit-exponent float. `F1489` converts integer to FFP by doubling until the sign bit is set, then `>>17`; exponent starts `0x20` and decrements. `F1496` normalises the mantissa to bit `0x4000` and decrements the exponent. `F1482` multiplies by adding exponents and `imul`/`sar 15`; `F1483` divides by subtracting exponents and shifting. Value = `mantissa / 2^15 * 2^exponent`. Masses, orbital radii and planet data use FFP. **Do not replace FFP with f32/f64 in the simulation.**

### 6.2 Floating point / x87

The original code uses x87 in 170 instructions. Most are in audio/volume, text/perspective interpolation, and one critical vector normaliser:

- `F1519_fp` (`ffe.asm:92939`, marked “TEST: New vector normalisation function for matrix orthogonalisation”) uses `fild`, `fmul`, `fsqrt`, `fdivrp`, `fistp` with two float constants (`0x30000000`, `0x3f800001`). It is called from `F1520` (orthogonalise a 3×3 matrix), which is called from `F611` (orientation update). This is a **behavioural change introduced by FFED3D** and is used in the normal rotation path.
- `rotateobject` (`win/ffemisc.cpp`) uses `double` `sin/cos`, converts from 1.31 to double, builds a matrix, multiplies and converts back. The original likely used x87 `fsin`/`fcos` or a table; FFED3D’s implementation is a rewrite.
- `F1882`/texture code uses `fild`/`fmul`/`fdiv` for perspective/interpolation; these affect rendering, not simulation.
- C++ market/sysgen/bribe functions use `float`/`double` (`CanBribe`, `GetPopulationMult`, `GetFrontierPriceMult`, `ModifyEquipmentPrice`, `AddPlanetEffects`, `GenSystemBasicType`). The C++ compiler’s x87/SSE choice changes rounding.

`float` vs `double` hazards:

- The C++ code is compiled as 32-bit x86. With MSVC’s default (`/arch:IA32`), float expressions may be evaluated in x87 80-bit and stored to 32/64-bit; with SSE, they are 32/64-bit throughout. The original executable uses x87 for the C/C++ parts. To match traces, the Rust port needs either (a) exact integer/table implementations for all simulation-relevant routines, or (b) a software 80-bit (`f80`) emulation for the sensitive expressions, with explicit rounding at the same points.
- `F1519_fp` is a concrete case: a straightforward Rust `f32`/`f64` port will not be bit-identical. We will implement an `F80` type (or a small x87 emulator) for this and any similar routines found by trace diffing.
- `round` in `xmath.h` is **not** C `round`: it uses x87 `fistp` (round-to-nearest-even by default). If any simulation code calls it, preserve the x87 rounding mode.

**Port plan:** implement explicit newtypes `FFP`, `Fix16_16`, `Fix1_31`, `Vec32`, `Vec64`, `Ang16`, and use `wrapping_*`/`saturating_*` exactly. Keep a trace field for RNG state and key numeric values. Phase 1 will record per-tick hashes of selected fields and full dumps on mismatch.

### 6.3 World scale and units

`KM_DIST = 999539.7368756` game units per kilometre (`win/misc.h`). Therefore 1 unit ≈ 1.00046 mm. A 100 m ship is ~100,000 units; a 6,000 km planet is ~6×10⁹ units. Object positions are 64-bit with an exponent; distances are frequently stored as an exponent byte (`dist_cam`, `dist_3face`) rather than a float. The renderer must use camera-relative coordinates and split near/far passes; see `docs/RENDERING_DESIGN.md`.

### 6.4 RNG

- Main RNG `F1530`: `seed1 += rotl(seed2 ^ 0x1234567f, 25)`; `seed2 = rotl(seed1_old, 13)`; returns `seed1 & 0xffff`. `BoundRandom(m) = rand() % m` (with `m==0` returning 0). `FloatRandom() = rand()/65536.0f`.
- Sysgen RNG `F894`: `edx = seed1+seed2; eax = (edx<<3 | edx>>29)+edx; edx = (edx<<5 | edx>>27); seed2=edx; seed1=eax+edx`.
- Name generation has its own mixing (`F791`).
- Some functions seed from object `unique_Id`, system code, time, or `DATA_008820`; all must be traced.

The reference's RNG use is **order-sensitive**; any extra call (e.g., a rendering effect or a mod loader) would desynchronise the galaxy. Enhanced features must never call the gameplay RNG.

---

## 7. Timing and frame loop

`F1418_TimerAdvance` (`ffe.asm`) is the clock:

1. `TimerFrameUpdate` (Windows messages + audio stream).
2. Read `TimerGetTimeStamp` (milliseconds). Sleep until `tmMinFrame` (default `1000/50 = 20 ms`; `[TIMER] maxfps` configurable, minimum 5 ms).
3. Run the 50 Hz input/timer callback for each elapsed 20 ms (`tmNext50Hz += 20`): key-repeat counters, mouse-button accumulators, splash timeout.
4. Accumulate elapsed time into two counters:
   - `DATA_009157 += elapsed_ms * 0x3e2 / 20` (0x3e2 = 994), the system counter.
   - `DATA_009156 += elapsed_ms * DATA_007706 / 20`, the accelerated game counter.
     Products are unsigned 64-bit; each quotient truncates independently to a
     32-bit result. Counter additions wrap; division overflow traps in legacy.
5. The main loop copies `DATA_009156` into `DATA_009155`, subtracts it from the accumulator, adds it to `GameTics` (`[ebx]`), and carries into `GameDays` (`[ebx+4]`) on **32-bit word overflow**, not at 49,710.

**JJFFE runtime verification update:** the previous 49,710-ticks-per-day and
one-real-second-per-game-day interpretation was incorrect. `GameTics` is the
32-bit day fraction, with 2^32 units per day. At a controlled 20 ms step and
acceleration 994 there are 49,700 fraction units per second (approximately a real
day per game day); do not convert this to a frame-count time model. The short
headless trace uses a synthetic per-read clock and 21 ms updates, not faithful
wall-clock pacing. `ffe-core` preserves the counter arithmetic and day carry,
not yet the complete timer/input scheduling or acceleration policy.
The start state is `GameTics = 0x80000000`, `GameDays = 0x121cf7`.
Day rollover fires event 2 (`0xf`), every 7 days event 2 (`0x10`), and a rollback
event `0x15`; those events have not been ported.

The simulation is frame-driven with an integer tics delta, not a fixed 50 Hz simulation tick. For deterministic tracing we will drive fixed 20 ms frames, inject scripted input, and accumulate exactly as above. This is also the natural place for Enhanced frame interpolation: render between the previous and current simulation states without changing the simulation.

---

## 8. Input, audio, platform

### Input

- DirectInput 5 (`DIRECTINPUT_VERSION 0x500`): mouse, keyboard, optional joystick.
- 256-key state array; `InputKeybGetLastKey` returns the last key; `InputKeybReadStates` copies the array. `F1406_KeybGetKeyState` reads it.
- Mouse mickeys are accumulated; the game uses a 320×200 virtual pointer (`DATA_009208/09`) and clips to the client rect. `InputMouseReadPos` maps window coordinates to 320×200 with the aspect correction (FFED3D uses 1.6; see the Classic aspect question in §10).
- Key bindings are in `[BINDS]` in `ffewin.cfg`, with defaults for pitch/roll/yaw, fire, gear, jump, missiles, ECM, camera, panels, time acceleration, etc. `KeyBinds` reads names like `KEY_ESCAPE`, `KEY_ADD`, `KEY_GRAVE`.
- Joystick: axis exponent/dead zone/max zone, range −512..511, calibration; disabled by default.
- The Classic UI uses hot rectangles (`HotArea`), not native widgets. Buttons/console are bitmap sprites in the 320×200 buffer. Input handling may be modernised (remapping/gamepad) as long as default bindings and pointer mapping match.

### Audio

- 40 sample slots, 24 files, 16 channels, 22,050 Hz mono 16-bit. Volume is logarithmic for some calls (`F1907`) and linear for others (`F1908`); pitch is a 16-bit value.
- Music: 17 HMP songs; original uses HMI/AIL (DOS) or MCI/DirectMusic (JJFFE); FFED3D uses BASS/MP3 and `hmp.cpp` for conversion. The default Rust path must preserve the original trigger points and relative levels; the synth choice is an audio-plumbing decision (see open questions).
- `SoundPlaySample` checks the sound flags (`DATA_009028..DATA_009037`) for effects/music/engine/station/wind, so the audio triggers are part of game state, not just presentation.

### Platform

- Win32, 32-bit, D3D9/D3DX9, DirectSound, DirectInput, BASS, Lua 5.1.
- `SystemInit` order is Timer → Dir → Video → Input → Sound; Video must precede sound/input because of the window.
- `ffewin.cfg` controls fullscreen/window size, aspect fix, palette brightness offsets (`RedBase/GreenBase/BlueBase`), planet detail, sound, mouse, keyboard, joystick, key binds.
- `jjffe.cfg` controls `[TIMER] maxfps`.
- `SETUP.INI`/`HMISET.CFG`/`PATCHES.INI` are the DOS sound setup; `32RTM.EXE`/`DPMI32VM.OVL`/`dos4gw.exe` are DOS extenders; `GAMETEK.1`/`IMAGE.DAT` are installer data.

---

## 9. FFED3D changes vs the DOS 1.02 original

The reference is **not** a clean DOS 1.02. It is JJFFE 2.8a6 + Anisotropic mod 0.9c + FFED3D. The source contains **142 `ANISO` markers** and **26 `FIX:` comments**. This matters because the task freezes the simulation against golden traces, and the traces will be produced by this legacy build.

### 9.1 Bug-fix comments in `ffe.asm`

These are explicit deviations from the original that FFED3D applies unconditionally:

- `FIX: Laser invulnerability` (15387, 49257, 82125)
- `FIX: Station instead of player set to 0x4f` (29079)
- `FIX: incorrect tech prices` (29251)
- bugfix flags in mission payments (35597)
- `FIX: Collision radius` (38375)
- `FIX: Missile retardation` (38534)
- `FIX: Linear FOR switch velocity` (41000)
- `FIX: Autopilot acceleration fudge` (41172)
- `FIX: Mispositioned smoke` (48827)
- `FIX: Combat vel fudge issues` (49299, 51998)
- `FIX: Pulse laser non-limitation` (49683)
- `FIX: AI Time travel during HS` (50131)
- `FIX: Orbital drift bug` (62299)
- `FIX: for binary light bug` (65628)
- `FIX: Starports below ground` (66283)
- `FIX: Binary over-adjustment` (66497)
- `FIX: Binary child under-adjustment` (66516)
- `FIX: Asteroids run out` (69361)
- `FIX: Player hyperspace time-travel` (69426)
- `FIX: Groundhog day bug` (70432)
- `ANISO - disable ray-collision for the missile. BUGFIX` (72632)
- `ANISO - bugfix: cargo space not lost` (73056)
- `ANISO - FIX: Cabin Temp now related to gametime` (77067)
- `ANISO - bugfix: make sure turrets can't turn when paused` (78838)

### 9.2 Other behavioural changes

- **Extended universe / “Frontier 3500” start**: start type 6 sets `DATA_ExtendedUniverse = 1`, `GameDays = 0x1381a7`, disables hand-coded missions/journal, uses the C++ system overrides. The default floppy start does not.
- **Missile thrust**: comment at `F601` says “changed so that missiles have max thrust of 120g instead of 60”.
- **Pirate snap**: “pirates don't ‘snap’ to the player instantly”.
- **Turning rates**: `GetTurningRate`/AI turning changes.
- **Cabin temperature**: related to game time, not frame count.
- **UI/console**: added Lua console, loading tips, new menus/screens, D3D HUD/panel.
- **Graphics**: D3D9 renderer, GPU planets, HD models/textures/shaders, skyboxes, star/nebula models, filtering.
- **Audio**: BASS/MP3, custom music.
- **Data**: extra models/skins/textures, `resources/Scripts/*.lua`, `texNNN` assets, loading screens.
- **Math**: `F1519_fp` test normaliser in the orientation path; `rotateobject` double rewrite; `C_PlaceStation` C rewrite (with a “FIX: change from >> 4 to >> 3” comment that is **not applied** in the code: the code uses `>> 4`).

### 9.3 Consequence for the port

The golden traces must be recorded from **one** engine. The practical choice is the built FFED3D/JJFFE reference, because it can be instrumented and run against `legacy/Elite3/`; the DOS EXE cannot easily be traced. If we port FFED3D as-is, the port matches its traces but inherits its deviations from DOS 1.02. If we port DOS 1.02 instead, we need a different oracle and must undo the `FIX:` changes.

**Recommendation:** treat the FFED3D/JJFFE build as the simulation oracle (frozen), document every known deviation from DOS 1.02 here and in `docs/KNOWN_ISSUES.md`, and offer DOS-original behaviour later only as explicit opt-in toggles if requested. This is the only way to satisfy “golden traces define the simulation” with the source we have. **This needs your explicit approval** (see §10).

The *look*, by contrast, should be the original DOS look, not FFED3D’s D3D look. FFED3D’s renderer is a source of knowledge about the original assets/HD work, not the Classic target. This distinction is central to `docs/RENDERING_DESIGN.md`.

---

## 10. Questions that must be answered before Phase 1/2

1. **Simulation oracle.** Approve FFED3D/JJFFE 2.8a6+0.9c as the frozen simulation reference, with its `FIX:`/`ANISO` behaviour, and DOS 1.02 differences logged? Or do you want DOS-original behaviour where they differ (which requires a different tracing strategy)?
2. **Classic visual reference.** Confirm Classic = the original DOS software look (320×200, 8-bit palette, 320×158 3D view, affine texture mapping, per-face shading), not FFED3D’s smoothed D3D look. The software reference renderer will be built from the original assembly semantics.
3. **Aspect ratio.** The DOS image is 320×200 on a 4:3 display (non-square pixels, pixel aspect 1.2). FFED3D’s `aspectfix` instead treats it as 1.6 (16:10, square pixels). Which is the frozen Classic aspect? Recommendation: 4:3 with optional integer scaling, and document FFED3D’s 16:10 as a known deviation.
4. **6-bit palette expansion.** The original palette is 6-bit (0–63). FFED3D maps to 8-bit with `v << 2` (max 252). Should Classic GPU output use `v << 2` (FFED3D-compatible) or `round(v * 255 / 63)` (full-range CRT-like)? The indexed software reference is unaffected; this only affects the GPU comparison tolerance.
5. **Music default.** The original HMP/MIDI sound depends on the player’s MIDI device. Should the default Rust build use the OS MIDI synth, a bundled SoundFont (changes the sound), or an optional “authentic HMI” emulation? This is audio plumbing, not simulation, but it affects “how it sounds by default”.
6. **CD version.** Please provide a CD copy (or its `FIRSTENC.EXE` hash and file list) when available. Until then, only the floppy 1.02 detector will be implemented; unknown versions get a specific error.
7. **Legacy source VCS.** `legacy/ffed3d-master/` has no licence. Recommendation: keep it git-ignored (as required) and, if you want reproducibility, add it as a pinned **git submodule** to the upstream repo rather than vendoring it. The same applies to `legacy/Elite3/`. Do you approve a submodule, or should both stay as local-only ignored inputs?
8. **Rule 4 gate.** No base content has been found that exists *only* in the legacy source. FFED3D-added content will be excluded. If the converter finds a base table/text that cannot be extracted from the player's files, we will stop and ask before committing it. Confirm this gate.
9. **Trace harness changes.** Phase 1 requires minimal changes to the legacy engine for fixed seed/timestep and trace output. May we maintain those as **patch files/scripts** against a local copy (never committing the unlicensed source), or do you want a separate `legacy-harness/` tree that is also ignored?
10. **Reference build.** The original VS2010 + DirectX SDK Feb 2010 toolchain is not installed and is no longer supported. We built a study binary with VS 2026 + NASM 3.02 + the Microsoft D3DX9 NuGet + Wine `d3d9.h` + a DirectInput shim; it runs and renders the splash/intro. Is that acceptable as the Phase 1 oracle, or do you want a pinned VM/toolchain image?

---

## 11. Build report (Phase 0)

### What the project expects

- Visual Studio 2010 (`.vcxproj`, `v100`), Windows SDK, **DirectX SDK February 2010** (`d3dx9`, `d3d9`, `dinput`, `dsound`), NASM, 32-bit.
- `lib/bass.lib`, `lib/Lua.lib`, `resources/Lua.dll`; runtime `BASS.dll`.
- `StructMemberAlignment = 1 Byte` in both Debug and Release; Release still links the **debug** CRT (`/MTd`) and disables optimisation.
- Output `../build/ffed3d.exe`; the executable expects game data in a `game/` subfolder (or cwd), plus `ffewin.cfg` and `resources` copied to the working directory.

### What we actually did

Toolchain: Visual Studio 2026 (MSVC 14.51), NASM 3.02, Microsoft `Microsoft.DXSDK.D3DX` 9.29.952.8 (headers + `d3dx9.lib` + `D3DX9_43.dll`), Wine `d3d9.h`/`d3d9types.h`/`d3d9caps.h`, and a hand-written DirectInput import shim (`dinput.dll` exports `DirectInputCreateA`; the original `dinput.lib` is not in the Windows SDK). Temporary compatibility patches (only in a scratch copy, never in `legacy/`):

- `#pragma pack(push,1)` around FFED3D structs (`ffetypes.h`, then `ffe3d.h`) to match the project's 1-byte alignment.
- Rename the x87 `round` helper to `xround` to avoid the modern CRT's `round`.
- Rename the global `time` variable in `w32video.cpp` to avoid the CRT function.
- Stub `Dxerr.h`.
- Provide `c_dfDIMouse`, `c_dfDIKeyboard`, `c_dfDIJoystick` data formats and a `DirectInputCreateA` wrapper.
- Bypass the splash key wait for the smoke test (temporary branch patch) so the intro can be observed without a working keyboard device.

Result: `ffed3d_test.exe` (2.47 MB) builds and runs. It displays the JJFFE/Aniso splash, then the “A FRONTIER DEVELOPMENTS PRODUCTION / Copyright © David Braben 1995” intro. The window title reports `FFED3D test=0; test2=0.000000; test3=`, i.e. the render loop is live. This is a **study binary**, not a supported build, and it is not distributed.

### What we could not do

- Build with the exact 2010 toolchain (not installed; DirectX SDK not present). The Wine `d3d9.h` and DirectInput shim are compatibility substitutes, so the binary is not bit-identical to a 2010 build.
- Exercise full gameplay, save/load, or audio in an automated way. Input is not yet scriptable; the splash bypass is a temporary patch. Phase 1 will add a proper deterministic harness.
- Verify the CD version (not supplied).
- Confirm the original pre-patch floppy (not supplied); `FIRSTENC.EXE` already identifies as 1.02. `PATCHES.INI` is AIL sound configuration, not the FFE patch. The installer build directory string is `eng-fp` (English, floppy-patched).

### Reference build caveats

- The source itself contains FFED3D-only content (`resources/`), a Lua console, extra models/textures/shaders, and no licence. None of this is in `data.bin`.
- The reference's 32-bit D3D9 path is not the Classic target; it is a guide to the original data formats and earlier HD work.

---

## 12. What is still unknown / to verify in Phase 1

- Exact mapping from original `DATA` symbols to file offsets for the converter. The data section is 1.62 MB raw and the reconstructed labels are 2.5 MB including BSS; the converter needs a generated symbol map (assemble `ffedat.asm`/`ffebmp.asm`, or parse the FFED3D map file) to slice the original PE deterministically.
- Exact journal bit-permutation/round-trip and the `MISSION.DAT` entry count (the offset table parse is ambiguous; verify against a legacy journal view).
- Exact save-file subset/compression/scramble and the save-name extension scheme.
- Exact original x87 rounding/order in `rotateobject`, `F1519_fp`, market/sysgen and any other trace-sensitive expression.
- The original rasteriser edge rules (F1602/F1603/F1604, F1645/F1646) — needed for the software reference. The F1479 integer projection is now ported and verified (see `docs/STATUS.md`, v0.10).
- The original palette-to-RGB mapping and the DOS aspect/pixel mapping.
- Which `FIX:` changes actually affect a given trace (some may be unreachable in normal play). Trace diffing will tell us.
- CD layout/version hashes and whether it externalises `bmp_8` art and AVI data.
- Whether the floppy `FIRSTENC.EXE` contains any data not represented in `ffedat.asm`/`ffebmp.asm` (the reconstruction may omit dead/unused bytes). The converter will compare extracted blobs against the original section hash and report gaps.

---

## 13. Phase 0 conclusion

The source is a complete, working 32-bit Windows program with a reverse-engineered assembly core and a substantial C++/D3D9 layer. The game logic, content tables, models and most art are embedded in `FIRSTENC.EXE`; the floppy also supplies cabin art, samples, music and the journal archive. The simulation is integer/fixed-point and event/frame driven, with a 50 Hz input cadence and a wrapping 32-bit day fraction (see the corrected timing section). The largest risks are (a) FFED3D's behavioural changes relative to DOS 1.02, (b) x87/fixed-point exactness, and (c) reconstructing the original software renderer and palette look.

The recommended next step is Phase 1: add a deterministic trace harness to a local copy of the reference engine, record golden traces against `legacy/Elite3/`, and use those to pin the numeric model before writing `ffe-core`. **Do not start the port until the questions in §10 are answered and the rendering design in `docs/RENDERING_DESIGN.md` is approved.**
