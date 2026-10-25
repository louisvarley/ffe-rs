# Crosshair, lasers and combat (JJFFE 2.8a7)

Researched from `jjffe-master/ffe.asm` (the gameplay reference), `ffedat.asm`,
`srctools/{plstate,claser,cplayer,cagent,shipidx}.txt` and the recovered
`FIRSTENC.EXE` string table. Implemented in `crates/ffe-core/src/laser.rs`,
`crates/ffe-rs/src/combat.rs`, `screen.rs` and `main.rs`.

## 1. Crosshair and sights

### 1.1 Fixed gunsight (`FUNC_001111`, `ffe.asm:81954`)

`F1099` calls it in the flight HUD (`ffe.asm:80594-80603`) when:

- the view is not 4 (external) **and** not 6 (combat computer); and
- `ship[0xd2 + view]` is non-zero, i.e. the laser fitted to that view's mount.

`F1111` blits **bitmap 0x61 (97)** — secondary table id 1, 61x61, masked zero —
at `(129, 48)` (`FUNC_001623_BlitIndex`). The bitmap's centre is `(159, 78)`,
the 320x158 view centre (160,79) one pixel off. It is the original's art and low
palette, not four port lines. It draws in front, rear and both turret views when
that view has a laser; **never** in external or combat.

`F1111` then draws the locked-target box (`F1113`, when `DATA_007246` and
`DATA_007252` are set) and a circle at `(0x9f, 0x4f)`
(`FUNC_001612_DrawCircle`, when `DATA_007247` and `DATA_007252`). Those are the
targeter/combat-computer marks, not the fixed gunsight; the port's own lock
brackets stand in for them (they need the combat targeter and blink state).

### 1.2 The velocity marker (`FUNC_001112`, `ffe.asm:81996`) — not a target X

The dashboard audit called bitmap 98 the "target X". The code says otherwise:
in the **front view only**, `F1099` converts the ship's velocity vector
(`ship+0x8c`, `ffe.asm:80605-80628`) into the object frame, adds 255 to z and
projects it with `F1479`, then calls `F1112`. `F1112` blits **bitmap 0x62 (98)**
— secondary table id 2, 35x35 — at `(x-19, y-18)` when `x > 14`, `y > 14`,
`x <= 0x131`, `y <= 0x8f`. It is the velocity/prograde marker (`views.md` was
right); there is no target X in this routine.

The port draws it at the projected velocity direction, at the view centre for a
zero velocity, front view only. `bitmap 98` is no longer described as a target
reticle.

## 2. Firing and projectiles

### 2.1 Keys and view-to-mount (`F1040`, `ffe.asm:76834`)

The fire input is the **Space** key held (`DATA_007696`, the scancode-0x39
entry; `ffedat.asm:23894`). `F1040` reads it every frame (not a Gui action):

- view 5 (missile) does nothing;
- views 0..3 fire mounts 0..3 (front, rear, top turret, bottom turret);
- views 4 and 6 (external, combat computer) fire **mount 0** (front).

The keys `M`/`m` (missile launch) are a separate Gui action: `F1056`
(`ffe.asm:78017`, `cplayer.txt:566`) finds the first non-zero missile at
`ship+0xd6..+0xdf` and calls `F1096` (`ffe.asm:80303`), which calls `F941`
(`ffe.asm:70399`) to launch a missile object. Missiles are objects with AI,
not hitscan; see §7.

### 2.2 `F931` (`ffe.asm:69550`)

The fire routine:

1. `al = ship[0xd2 + mount]`; zero returns.
2. `DATA_008836 = laser code`.
3. Return if `ship+0xe8 != 0` (pulse refire), `ship+0xea > 0xf000` (heat),
   or `ship+0x11e == 0` (energy).
4. For the player: illegal-shooting flag `DATA_008860 |= 4` and `F237(ship,3)`.
5. OR `ship+0x151` with 2 (laser fired) and 4/8/0x10 for front/rear/turret.
6. Gun position: `forward * zoff + up * yoff` from the descriptor words
   (`+0x20/+0x22` front, `+0x24/+0x26` rear, `+0x28/+0x2a` top turret,
   `+0x2c/+0x2e` bottom turret), each word in eighths of a position unit
   (`axis * word * 8 >> 12` with the Q31 row's high word).
7. `F802` -> `F817` (`ffe.asm:53248`) scans live object slots `0x72..1`, tests
   each object's interaction box (`+0x138`, page 13-bit), and writes the first
   hit to `DATA_008711/12/13/14` (hit flag, hit position, object pointer).
8. On a hit: `F930(ship, object)` returns the laser's damage; `F953(object,
   damage)` applies it; `F939` spawns the hit effects. No hit: `F930(ship)`.
9. `F1776` (`ffe.asm:111336`) draws the beam/flash on the firing frame.

**Lasers are hitscan.** There is no laser projectile object, speed or lifetime.
The visible object is model `0x9c` (156), set on a duplicate of the player
object and drawn at the gun by `F1776`; it is a small (about 64 mm after its
`scale` 17) muzzle cone, not a line to the target. The cockpit eye of the Eagle
is 1500/8 units ahead and the gun 1300/8, so the cone is normally a pixel or
two at the gun. A long beam line is **not** in the original; do not add one.
The hit flash is model `0x9d` (157), drawn at a ship whose shields absorbed a
hit (`ship+0x151 & 1`, `F1536` at `ffe.asm:91274-91320`). Stations and planets
take neither shields nor hull in `F953`, so they do not get that flash.

### 2.3 Costs, damage and timing (`F930`, `ffe.asm:69454`)

`F930` indexes the tables by `(code & 0x78) >> 3`:

| Index | Codes | Meaning | Rate (`DATA_007209`) | Heat | Energy | Damage | Refire |
|---|---|---|---|---|---|---|---|
| 1 | 0x88 | 1MW pulse | 0x32 (50) | rate*8 = 400 | rate*16 = 800 | 50 | 0x795c |
| 2 | 0x91 | 5MW pulse | 0xfa (250) | 2000 | 4000 | 250 | 0x795c |
| 3 | 0x98 | (unused pulse) | 0x5dc | 12000 | 24000 | 1500 | 0xffff |
| 4 | 0xa0 | 1MW beam | 1 | `(tics>>7)*8 >> 2` | `(tics>>7)*16` | `tics>>7` | 0 |
| 5 | 0xa9 | 4MW beam | 4 | `... >> 3` | `...*4*16` | `4*(tics>>7)` | 0 |
| 6 | 0xb3 | 20MW beam | 0x14 | `... >> 7` | `...*20*16` | `20*(tics>>7)` | 0 |
| 7 | 0xbc | 100MW beam | 0x64 | `... >> 8` | `...*100*16` | `100*(tics>>7)` | 0 |
| 8 | 0xc6 | 30MW mining | 0x12c | reads `DATA_007208[8]`, which is `DATA_007209[0]` = 10 (the original's out-of-table read) | | | |
| 9 | 0xcf | Thargoid | 0x384 | shift 0x32, also past the table | | | |

`DATA_007209` is `ffedat.asm:18375`; `DATA_007208` (beam shifts 2,3,7,8) is
`ffedat.asm:18369`. `tics` is `DATA_009155`, the accelerated frame time (994 at
1x). Pulse shots set `ship+0xe8`; beams do not, so a beam fires every frame
while its heat stays under 0xf000.

`F929` (`ffe.asm:69365`) removes the energy from `+0x11e`; on empty it may use
fuel/cargo or fails the drive, with message 0x9907. The new game has
`0x20000000` (1 t), so firing is effectively energy-unlimited in normal play.

`F730` (`ffe.asm:48263`) decrements `+0xe8` by the low word of `DATA_009155`;
JJFFE changed the DOS guard from `jng` to `jnz` (the "Pulse laser
non-limitation" fix) so sub-1-tic frames still count down. The port follows
JJFFE and records the difference here.

`F938` (`ffe.asm:70122`) cools `+0xea` by `seconds * 0x400`, or `* 0x1000`
with the laser cooling booster (`ship+0xc8 & 1`). `seconds` is whole game
seconds; the port carries the fraction between frames.

### 2.4 Damage (`F953`, `ffe.asm:71190`)

Only objects with control byte `+0x87 == 3` (player) or `0xb` (AI ship) and a
descriptor take damage; anything else just plays the hit sound.

- damage <= shields (`+0xe0`): shields -= damage; sound 0x1c; `+0x151 |= 1`.
- else: damage -= shields; shields = 0.
  - damage >= hull (`+0xe4` = mass*4): `F942` destroys the object (death
    effects, sound, message).
  - else: hull -= damage; smoke timeout `+0x14e = 0xa`; sound 0x1d;
    `F952` may add an explosion/message.

A 1MW pulse (50) takes two hits to kill a 25 t hull (100) with no shields. The
target readout (`F1099`, `ffe.asm:80856-80900`) shows `Hull: d.d%`
(`hull * 250 / mass` per-mille), `Shields: d.d%` (`current*100/max`) and
`Shield Strength d` (descriptor `+0xe`), or `Shields: None` when the target's
`+0xe2` is zero. It is drawn at `(235,1)`, colour 15, and skips targets whose
model is above `0x50` (stations/planets) or lacks a statistics block.

### 2.5 Range (`F1100`, `ffe.asm:81021`)

`F1100` cuts the player-to-target vector to 13 bits (`F590`), takes its length
(`F1466`), converts to ffp (`F1489`), adds the cut shift minus 0xf to the
exponent and normalises (`F1496`). The displayed number is:

- exponent <= 0x1d: `mantissa * 0xd1d0 >> (0x1d - exponent)` km (`0x8604`);
- else: `mantissa * 0xeb54 >> (0x35 - exponent)` AU (`0x8605`).

It is drawn at the target's screen position minus 5; an off-view target moves
to `(50,50)` (the navigation target to `(50,60)`) and uses `0x8606/07`
("Target d km/AU Behind"), `0x8608/09` ("Target Out of Range (d km/AU)") or
the other arrow variants. `F1100`'s combat-target colour is 0x5f (95); the
navigation target's is 0x11 (17).

### 2.6 Gauges (`F270`, `ffe.asm:20554`)

Three 17x15 bars at `(249,161)`, `(242,161)` and `(237,161)`:

- energy: `DATA_005089[ship+0xd0] * (ship+0x11e >> 16) >> 8 & 0xffff`;
- heat: `ship+0xea`;
- third: `DATA_008921` (only ever reset to zero in this build; meaning not
  established).

`h = value * 12 >> 16` (0..11) and the bar is blitted with `ymin = 0xad - h`,
so it grows upward. The bitmap ids are `0x113/0x112/0x111` (275/274/273);
ids >= 96 are **secondary**, so these are secondary 179/178/177.

## 3. Sounds

`F930` plays sample 0x0a (pulse) or 0x0b (beam) at the object's volume;
`F953` plays 0x1c (shields) / 0x1d (hull); `F939`/`F942` add the explosion
samples. The port does not play them yet (the synthetic audio layer is
elsewhere).

## 4. The new-game ship and its laser (F998)

`FUNC_000998` (`ffe.asm:74028`) for start type 1 (Ross 154) sets, at
`ffe.asm:74099`:

- model `0x17` (23) — the **Eagle long range fighter**, not the Sidewinder
  (model 26 / 0x1a; `shipidx.txt`);
- front laser `ship+0xd2 = 0x88` (1MW pulse);
- drive byte `+0xd0 = 2`;
- two missiles `+0xd6/+0xd7 = 0x82`;
- energy `+0x11e = 0x20000000` (1 t);
- cash `0x3e8` (1000 CR).

The cache's statistics block for model 23 confirms one gun mount
(`+0x4 = 1`), front gun `+0x20/+0x22 = 1300/0`, eye `1500/30`, rear gun
`800/0`, missiles 2. The earlier notes that called start type 1 the
"Sidewinder" are wrong; the port's `NEW_GAME_SHIP = 23` is right.

There is no outfitting or save equipment in the port. It therefore keeps the
F998 new-game loadout (front 1MW pulse, class-2 drive, 1 t energy) for the
sandbox too, and keeps the front laser when the development ship list changes
the hull unless the hull has no gun mount. **Consequence:** with the new-game
Eagle (one mount) only the front view has a laser, so only the front view draws
the gunsight and fire works only there. Rear and turret firing are implemented
and unit-tested with a synthetic loadout, but no playable hull carries a rear
or turret laser today. Do not give turrets a laser by invention.

## 5. What the port implements

- `ffe_core::laser`: F930's table and pulse/beam branches, F931's gates, F730's
  refire, F938's cooling, F953's shields/hull damage, F1100's range words and
  km/AU value, F270's bar height/energy scale, and F817/F819's hit test against
  each body's interaction radius (`body.radius`, object `+0x138`).
- `ffe_rs::combat::Combat`: the four mount codes, energy, heat, refire, drive,
  cooling booster and the new-game loadout; serde with `#[serde(default)]` so
  old saves load with no lasers.
- `Screen`: the bitmap-97 gunsight, bitmap-98 velocity marker, F270's three
  bars, F1100's on/off-view range text, F1099's target hull/shield text (draw
  path exercised by a synthetic frame), and F1776's model 156 at the gun.
- `main`: Space fires the view's mount; the hit uses `nav.bodies` with their
  interaction radii; a hit message names the body and laser. The new-game
  `Flight` starts with the F998 loadout.

## 6. Evidence

- `cargo test -p ffe-core laser`: 8 synthetic tests (table values, beam scaling,
  gates, cooling, range words, hit box, damage, gauges).
- `cargo test -p ffe-rs --test combat`: loadout, save compatibility, invalid
  codes and the firing gates (no cache).
- `cargo test -p ffe-rs --test combat_view -- --ignored` with
  `FFE_WORLD_CACHE=<data.bin>`: exact bitmap-97 pixels at `(129,48)`, the
  front/rear/external view matrix, bitmap-98 motion, F1100 text at the target
  and at `(50,50)`, the three gauge bars and the target hull/shield text.
  Set `FFE_SHOT_DIR` to keep the frames (`shots/combat-*.ppm` in local runs).
- **Live game frame:** `ffe-rs.exe --root <repo> --skip-intro --capture-planet <dir>`
  flies the new-game Eagle (front view, one `0x88` front laser) to a planet.
  Frame `planet-1400.ppm` (saved locally as `shots/combat-live-crosshair.ppm`)
  matches all 256 opaque pixels of bitmap 97 at (129,48) exactly, so the
  gunsight is the original art at the original position in the real playable,
  not only in a synthetic test.
- **Reference:** `tools/reference/headless.c` now has a `--state-combat <frames>`
  mode that runs the real new game and prints the player object's laser words.
  `jjffe-smoke.exe --state-combat 170` in a disposable directory prints, from
  frame 161:
  `model 23, mounts 1, lasers [136,0,0,0], missiles [130,130,0,0,0,0],
  drive 2, shields [0,0], hull 100, refire 0, heat 0, energy 536870912
  (0x20000000), cash 1000`. This confirms F998's start-type 1 loadout against
  the executed original: one `0x88` front pulse laser, two `0x82` missiles, an
  Eagle (model 23), 1 t energy and no shields. The existing `--state` output is
  unchanged.
- The hit/damage path is synthetic (no NPC ships); see §7.

## 7. Reachability and what remains

**Reachable today:** the crosshair in the front view of the new-game Eagle, the
velocity marker, the energy/heat bars, Space firing the front pulse laser, the
muzzle cone at the gun, the on-screen target range for the locked body, and
hit messages. Hits on stations/planets are detected (their interaction radius)
but do no damage, exactly as F953 skips non-ship objects.

**Not reachable in this build:**

- **Ship targets and the target hull/shield readout.** The generated systems
  contain planets, moons, stations and starports, no NPC ships. F953's damage,
  F942's destruction, model 157's shield-hit flash, and the `(235,1)` readout
  need a ship object with shields/hull. `Screen::target_damage` and the damage
  functions are implemented and tested; no live body sets them.
- **Rear and turret lasers.** The new-game Eagle has one mount; the port has
  no outfitting/save equipment. The firing paths (mount 1/2/3, turret axes and
  gun offsets) are implemented and tested, but no playable loadout reaches
  them.
- **Missiles.** `M`/`F1096`/`F941` and the missile object/AI (models 6..13,
  lock, tracking, damage) are not ported. The Eagle carries two 0x82 missiles.
- **Hit effects/explosions.** `F939`'s particles, `F942`'s death effect and
  the `F952` explosion are not ported; only the model 156 muzzle cone is drawn.
- **Sound cues** (0x0a/0x0b/0x1c/0x1d/0x14) are not played.
- **The combat targeter** (`DATA_007246/247`, F1113/F1114 boxes and circle,
  blink `DATA_007252`) is not ported; the port's autopilot brackets remain.
- **Upper-palette colour.** F1100 uses colour 95 and the HUD uses the runtime
  upper palette, which the port approximates with the fixed low palette.
- The port's own status block at y119..157 covers the lower centre where the
  muzzle cone and the target `Hull:` line would draw; the dashboard's retire-port-UI
  slice clears this.

**Owner decisions:** model the combat targeter and NPC ships (which unlocks
damage, hit flashes and missile lock), port missiles, and/or keep the port's
lock brackets as a deviation. The target damage readout is dead code until
ships exist; it is not shown for stations.
