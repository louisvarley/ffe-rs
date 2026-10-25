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

`F1111` then draws the blinking auto-targetter cursor while `DATA_007252` is
set: `DATA_007246` (navigation, colour 0x11) is a 9x9 box at (160,79) drawn by
`F1113` with size 4; `DATA_007247` (combat, colour 0x10) is a circle of radius
six at (159,79) (`FUNC_001612_DrawCircle`). They are armed targetter modes, not
lock marks; the locked target's display is `F1100`'s targeting tunnels
(`docs/notes/scanner.md`).

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
  path exercised by a synthetic frame and live ship targets), and F1776's
  model 156 at the gun.
- `main`: Space fires the view's mount; the hit tests `nav.bodies` and the live
  NPC ships (`docs/notes/npc.md`) with their interaction radii; a hit message
  names the body or ship and the laser, and a ship hit runs F953/F942, sets the
  F1536 flash and locks the ship. The new-game `Flight` starts with the F998
  loadout.

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
- **NPC traffic frames (2026-10-25):** `cargo test -p ffe-rs --test npc_view -- --ignored`
  draws `npc-system.ppm` (traffic, a ship label and the target readout),
  `npc-hit.ppm` (model 157) and `npc-kill.ppm` (the death marker). The live
  `--capture-traffic <dir>` mode shows the same in the real new game
  (`traffic-00/10/20.ppm`). See `docs/notes/npc.md`.
- **Effect tests (2026-10-25):** `cargo test -p ffe-core --lib effects` (three
  unit tests: one model-158 particle per hit at the hit point with the source's
  velocity and the `0x9c40` cutoff, F942's one model-0x9b object plus F922/F926
  debris counts and velocities, and the effects' travel and lifetimes).
  `cargo test -p ffe-rs --test effects_view -- --ignored` with
  `FFE_WORLD_CACHE`: `effects-hit.ppm` (F939 at the hit point), `effects-kill.ppm`
  and `effects-kill-after.ppm` (the model-0x9b explosion plus debris and its
  expiry), and a check that model 155 draws 0 shapes for global 4 = `0xffff`
  but at least 12 for `0x4000`, while model 158 draws six.
- **Live effect frames:** `ffe-rs.exe --root <repo> --skip-intro
  --capture-traffic <dir>` now holds the pirate still and places the camera
  2 km behind it; `traffic-00`, `-10`, `-11`, `-20`, `-21`, `-40` and `-79`
  show the shield flash, the green model-155/debris explosion and the clean
  view after its lifetime.
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
- **NPC AI frames (2026-10-25):** `cargo test -p ffe-rs --test npc_ai -- --ignored`
  with `FFE_WORLD_CACHE` puts the player in front of a pirate and ticks the real
  AI until F931/F930/F953 damage arrives, kills a clean ship for the police
  response and carries a pirate through a jump. With `FFE_SHOT_DIR` it keeps
  `npc-ai-before.ppm`, `npc-ai-hit.ppm` (the labelled pirate and `Hull: 50.0%`),
  `npc-ai-police.ppm` and `npc-ai-restored.ppm`. The player's own damage words
  have a save-compatibility test in `tests/combat.rs`. See `docs/notes/npc.md`.
- The hit/damage path has live ship targets now (the traffic above); the
  target-readout draw path also has a synthetic frame test. See §7.

## 7. Reachability and what remains

**Reachable today:** the crosshair in the front view of the new-game Eagle, the
velocity marker, the energy/heat bars, Space firing the front pulse laser, the
muzzle cone at the gun, the on-screen target range for the locked body, and
hit messages. Hits on stations/planets are detected (their interaction radius)
but do no damage, exactly as F953 skips non-ship objects.

**Ship targets (2026-10-25).** Generated systems now carry NPC traffic
(`docs/notes/npc.md`): traders, shuttles, police and a pirate group spawn with
the original's F635 model tables and F688 counts. A ship is labelled by the `l`
toggle, can be clicked or hit to lock, and then shows F1099's hull/shields/shield
strength readout; the laser path includes live ships after the bodies, calls
F953's shields/hull damage and F942's destruction, and draws F1536's model 0x9d
(157) shield flash. A hit also spawns F939's model-0x9e particle at F817's hit
point; a kill spawns F942's model-0x9b explosion and F926's model-0x99 debris
(ported 2026-10-25 in `ffe_core::effects`; see 7.1).

### 7.1 Hit particles, explosions and debris (`ffe_core::effects`)

- **F939** (`ffe.asm:70291`) is called once per laser hit by F931
  (`ffe.asm:69804-69808`) as `F939(player, hit_position, 1)`. It clones the
  ship and sets **model 0x9e (158)**, control byte `0x29` (control 9 with
  F928's "counted explosion" bit 0x20), AI state `+0xff = 0x1a` and AI class
  `+0x118 = 0xf8`, with a random YZT orientation (F1675). It keeps the cloned
  velocity only when `DATA_008867 >= 0x9c40` or the source's `+0x14c` has bit
  0x10, and zeroes `+0x8c/+0x90/+0x94` otherwise. The third argument shifts
  the object's distance word `+0x7e` (bit 1: +2, bit 2: −1); the laser path
  passes 1.
- **F942** (`ffe.asm:70650`) is called by F953 when the hull is used up
  (`ffe.asm:71281-71287`). It creates **model 0x9b (155)**, control 0xb, AI
  state `+0xff = 0x1c`, `+0xa4 = 0xffffffff`, `+0x9e = 0xffff`, a random YZT
  matrix, and the ship's distance word `+0x7e` decremented by one (restored
  when the statistics' mass word `+0x6` is at least 10; at 0x190 or more it
  uses the two radii, which the port does not model). For ship models
  0x0f..0x3f it also spawns debris with **F926** (`ffe.asm:69099`):
  `F922(6) - 2` pods plus `min(4, cargo_space >> 1)` cargo pods, each **model
  0x99 (153)** with a random 0..1023 added to every velocity word. F952
  (`ffe.asm:71154`) is the hull-hit subsystem roll (F945/F947/F948/F949), not
  a particle; the port leaves it out with the equipment damage model.
- **The port:** `effects::hit` spawns one model-0x9e effect at the hit point
  with the ship's velocity (above `PARTICLE_SPEED`) and a random orientation;
  `effects::explosion` spawns the model-0x9b object plus F942's debris count
  from `Spec.cargo` (`+0x116`). Each carries a port lifetime (`HIT_SECONDS`
  0.4 s, `EXPLOSION_SECONDS` 1.0 s, `DEBRIS_SECONDS` 1.2 s) because the
  original's AI timers (F764/F769/F927) are not ported.
  `Navigator::effects` holds them, `tick_ships` ages and expires them, and
  `Screen::SystemEffect` renders them with `render_scene_with`.
- **The model-155 gate.** Model 155's command stream is a chain of arithmetic
  (opcodes 13/29) and bit-branch (19/20) blocks around twelve child fragment
  calls (models 320..325). The branches read object **global word 4
  (`+0xa4`)**: they select no fragment for F942's `0xffffffff`, but all twelve
  for FFP 1.0 (`0x4000`). The port passes `EXPLOSION_MASS = 0x4000` in
  `SystemEffect.mass`, so F942's fragment set draws. The twelve shapes are
  F1721 complex polygons, which the port's renderer draws as outlines because
  the F1560/F1559/F1613 span fill is still unported (`docs/notes/fills.md`);
  the explosion is a wireframe cloud until that stream lands.

**The AI shoots back (2026-10-25).** Hostile ships run the flight/combat states
and call F931 with their front laser (`NpcShip::fire`): heat, energy and refire
are F930's and the damage is applied to the player's `Flight` shields/hull with
F953 (`Flight::take_damage`). A 1MW pulse does 50, so a 25 t hull with no
shields is two hits; the player is recovered at the nearest starport when the
hull is used up. The original's F724 is the only AI fire call, and JJFFE
replaced it with just the fired flag ("Laser invulnerability"); the port follows
DOS FFE. A clean ship hit makes the player wanted and turns the police
hostile.

The player-damage entry points were reconciled with the equipment slice at
integration: `Flight::take_damage(damage, mass)` is the F953 path the AI calls
and fills the hull/shield words from the statistics on first use;
`Flight::ensure_damage(hull_full)` still refreshes them from the ship/Loadout
each frame, and `Flight::hit(damage)` (the equipment slice's name) is kept for
its callers and tests and shares the same internal apply. `Flight::destroyed`
is the single F942 test.

**Not reachable in this build:**

- **Ship targets and the target hull/shield readout.** The generated systems
  contain NPC traffic (`docs/notes/npc.md`), so F953's damage, F942's
  destruction, model 157's shield-hit flash and the `(235,1)` readout are
  reachable for a locked or hit ship. The ships fire and have the flight/combat
  AI (see above); docking, missiles and Thargoids remain absent. F942's model
  0x9b explosion, F939's hit particle and F926's debris are ported and drawn
  (see §7).
- **Rear and turret lasers.** The new-game Eagle has one mount; the port has
  no outfitting/save equipment. The firing paths (mount 1/2/3, turret axes and
  gun offsets) are implemented and tested, but no playable loadout reaches
  them.
- **Missiles.** `M`/`F1096`/`F941` and the missile object/AI (models 6..13,
  lock, tracking, damage) are not ported. The Eagle carries two 0x82 missiles.
- **Hit effects/explosions.** `F939`'s particle, `F942`'s explosion and the
  `F926` debris are ported in `ffe_core::effects` and drawn by `screen`; see §7.
  `F952` is subsystem damage, not a visual effect.
- **Sound cues** (0x0a/0x0b/0x1c/0x1d/0x14) are not played.
- **The combat targeter** is ported (`docs/notes/scanner.md`): F1100/F1113's
  targeting tunnels, F1114's boxes, F1111's blinking F1612 circle/box cursor
  and the `DATA_007252` game-second blink. The *arming* rules (`t` with the
  target computer, F1010/F1060/F1062) are not wired because the port has no
  target-computer equipment; `T` still cycles locks (an existing deviation).
- **Upper-palette colour.** F1100 uses colour 95 and the HUD uses the runtime
  upper palette, which the port approximates with the fixed low palette.
- The port's own status block at y119..157 covers the lower centre where the
  muzzle cone and the target `Hull:` line would draw; the dashboard's retire-port-UI
  slice clears this.

**Owner decisions:** NPC traffic and its flight/combat AI are implemented at the
state-rules level (see `docs/notes/npc.md`); decide whether to port the
target-computer equipment and the auto-targeter arming that switches
F1113/F1114's tunnels on (the port draws them only when the targeter flags are
set), missiles, and a shield generator for the player (the AI's fire is lethal in
two hits without one). F1100's per-target text colour and arrow forms remain a
small deviation; the port's own lock brackets stand in when the targeter is not
armed. For the effects, decide whether their lifetimes should follow the
original's AI timers (F764/F769/F927) rather than the port's per-effect seconds,
and whether the model-155 explosion keeps the `EXPLOSION_MASS` word `0x4000`
that selects its twelve fragments or follows F942's literal `0xffffffff` (which
selects none). Until F1560/F1613 fill the fragments, the explosion draws its
wireframe.

The player's F953 words have a save-compatible test
(`crates/ffe-rs/tests/combat.rs`) and the AI sequence has its own cached test
(`crates/ffe-rs/tests/npc_ai.rs`, `npc-ai-*.ppm`). The scanner contacts and the
targeter's four-box tunnels have their own cached test (`scanner_view`), and the
effects are framed by `effects_view`.
