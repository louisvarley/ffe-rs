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
targeter/combat-computer marks, not the fixed gunsight; the port draws the
combat-target circle (see §8) and keeps its own lock brackets for the navigation
target, which stand in for F1113 (they need the targeter and blink state).

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

The keys `M`/`m` (missile launch) are a separate Gui action: F1056
(`ffe.asm:78017`, `cplayer.txt:566`) finds the first non-zero missile at
`ship+0xd6..+0xdf` and calls `F1096` (`ffe.asm:80303`), which calls `F941`
(`ffe.asm:70399`) to launch a missile object. Missiles are objects with AI,
not hitscan; see §7 and §8. (F1054's `M` handler is at `ffe.asm:78204`; F1056
itself is the joystick/keyboard read.)

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
- `ffe_core::missile` and `ffe_rs::missile`: F941's ammo/type/model/damage
  tables, the launch impulse, F758/F717's tracking, the 60-update lifetime,
  the 131 m F758 fuse, F721 contact and F720's damage word. `Combat` keeps the
  ten `ship+0xd6..+0xdf` words; `Navigator` owns the flying missiles and
  `MissileField` applies hits through the existing F953 path. M launches at the
  `DATA_008875` combat target (Shift+T), F1120's ten pylon lamps show the
  remaining ammo, and F1114's circle marks the combat target. See §8.

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
- **Missiles (2026-10-25):** `cargo test -p ffe-core missile` (7 tests: the
  type/damage tables, the launch separation, tracking, the 131 m fuse, contact,
  the 60-update expiry and a ballistic launch); `cargo test -p ffe-rs --lib
  missile` (the model-6 spec, F953 shields→hull→death, the timeout) and the
  F1120 pylon-lamp test in `console`. `cargo test -p ffe-rs --test missile_view
  -- --ignored` with `FFE_WORLD_CACHE` locks a pirate, launches the two 0x82
  missiles at 120 m, draws `missile-launch.ppm` (the missile object and F1114's
  circle), `missile-hit.ppm` (F1536's model 157 after the shields absorb 200)
  and `missile-kill.ppm` (F942 after the second missile), and checks the frames
  differ.
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
(157) shield flash. A killed ship shows a short death marker (F942's model 0x9b
produces no shapes in the port's executor yet, so model 157 at a larger scale
stands in) and is removed after `DEATH_SECONDS`.

**Not reachable in this build:**

- **Ship targets and the target hull/shield readout.** The generated systems
  contain NPC traffic (`docs/notes/npc.md`), so F953's damage, F942's
  destruction, model 157's shield-hit flash and the `(235,1)` readout are
  reachable for a locked or hit ship. The ships do not fire and have no full
  combat AI; the death effect is a 157 stand-in for F942's 0x9b object.
- **Rear and turret lasers.** The new-game Eagle has one mount; the port has
  no outfitting/save equipment. The firing paths (mount 1/2/3, turret axes and
  gun offsets) are implemented and tested, but no playable loadout reaches
  them.
- **Missiles** are implemented for the player (see §8): `M`/F1096/F941, the
  missile object, tracking, fuse, damage and the F1120 pylon lamps. NPC ships
  still do not launch missiles; the `F754` AI launch path and the missile view
  (F10) are not wired.
- **Hit effects/explosions.** `F939`'s particles, `F942`'s death effect and
  the `F952` explosion are not ported; only the model 156 muzzle cone is drawn.
- **Sound cues** (0x0a/0x0b/0x1c/0x1d/0x14) are not played.
- **The combat targeter** (`DATA_007246/247`, F1113/F1114 boxes and circle,
  blink `DATA_007252`): the port draws F1114's circle for the missile target
  (§8); F1113's box and the F1116/F1120 targeter lamps away from the equipment
  page remain absent, and the port's autopilot brackets remain for the
  navigation target.
- **Upper-palette colour.** F1100 uses colour 95 and the HUD uses the runtime
  upper palette, which the port approximates with the fixed low palette.
- The port's own status block at y119..157 covers the lower centre where the
  muzzle cone and the target `Hull:` line would draw; the dashboard's retire-port-UI
  slice clears this.

**Owner decisions:** NPC traffic is implemented at the traffic level (see
`docs/notes/npc.md`); decide whether to port the full AI and ship fire, police
response/wanted status, F942's model-155 explosion, the combat targeter, and
missiles. The port's lock brackets remain a deviation from F1113/F1114.

## 8. Missiles and the missile lock (2026-10-25)

### 8.1 The original's path

- **Pylons and types.** A ship has ten pylon words `ship+0xd6..+0xdf`
  (`plstate.txt`). A code's low nibble indexes `DATA_007220`
  (`ffedat.asm:18441`, types 0..9 -> models 4..13); bit `0x40` is F941's mine
  flag. F998's new-game Eagle has two `0x82` words: type 2, model 6
  (damage 200, `DATA_006269[6]`, `ffedat.asm:10124`).
- **Lock.** F1010 (`ffe.asm:74816`) calls F1062/F1064 each frame when
  `DATA_007246`/`DATA_007247` is set; F1063/F1064 (`ffe.asm:79014`, `79072`)
  scan the objects for one within 6 pixels of the crosshair (0xa3,0x4b),
  skip the navigation target and the previous combat target, set
  `DATA_008875` and play sample 0x15. `T` toggles `DATA_007246/7248` and
  Shift+T toggles `DATA_007247/7249` (`ffe.asm:437fe`). F1111
  (`ffe.asm:81954`) draws F1113's box for the navigation target and F1114's
  circle at (0x9f,0x4f) for the combat target, both gated by the blink
  `DATA_007252` = `(DATA_008799 / 0xc22e) & 1` (F1023, `ffe.asm:41460`).
- **Launch.** The `M` key (F1054, `ffe.asm:78204-78236`) scans
  `ship+0xd6..+0xdf` for the first non-zero word, stores its 1-based index at
  `player+0xc4` (`DATA_008858`) and calls F1096 (`ffe.asm:80303`). F1096
  requires that index and the combat target `DATA_008875`; without a target it
  shows message 0x98c1, otherwise it calls F941 (`ffe.asm:70399`) and clears
  `DATA_008858`. F941 clears the pylon word, allocates an object with control
  `0xb`, AI state `0x13` (missile), `0x14` (mine) or `0x16` (model-4 mine),
  owner `+0x100`, target `+0xfe`, and sets its initial velocity to
  `-(+0x140 radius) * up` plus (odd pylon) or minus (even)
  `right * 0x2000`, doubled for a launcher radius >= 0x4650; F1514
  (`ffe.asm:90538`) adds that once to the position.
- **Guidance.** F758 (`ffe.asm:50035`, event 6) increments the age at `+0x9e`
  and explodes above `0x3c` (60 updates), gets the target from `+0xfe`, calls
  F596 (`ffe.asm:39869`) for the lead direction, and while the distance
  exponent is above 4 adds 0x1108 to the local z of the aim, sets the thrust
  vector with F599 (`ffe.asm:40373`) and the turn rates with F586
  (`ffe.asm:39246`: yaw `x >> 3`, pitch `-(y >> 3)`). F717
  (`ffe.asm:47399`, event 5) runs F603/F601/F585 to integrate velocity and
  orientation; F585 is `ffe.asm:39023`, F601 `ffe.asm:40563`, F1512
  `ffe.asm:90407`.
- **Fuse and damage.** F758 detonates at a distance exponent <= 4, i.e. under
  2^(13+4) mm (131 m); F718 (`ffe.asm:47435`) runs the F569 contact test and
  F721 (`ffe.asm:47661`) calls F720 (`ffe.asm:47622`) with
  `DATA_006269[model]`, which calls F953. F719 (`ffe.asm:47573`) is the
  explosion/`F925` removal. F718's ECM branch reads `DATA_008835 & 4` and
  destroys models 4, 7, 8, 9, 10, 11, 12 and 13 (not 5 or 6); F1057
  (`ffe.asm:78789`) can retarget a missile whose target is the player.
- **HUD.** F1120 (`ffe.asm:82429-82620`) shows buttons 0x3b..0x44 for pylons
  9..0 with image `word & 0xf`, and hides 0x4f while `DATA_007247` is set.
  F1099/F1100's missile view (5) reads the last missile (`DATA_008877`); the
  missile view itself needs the F10 equipment.

### 8.2 What the port implements

- `ffe_core::missile`: the type/model, damage, AI-state and lifetime tables;
  `Spec` from the model's first two statistics words; the launch impulse;
  F758/F717's tracking, 60-update lifetime, 131 m fuse, F721 contact and
  F720's damage word. It is cache-free and uses mm and mm/s, like the NPC
  ships.
- `ffe_rs::missile::MissileField`: built specs from `ShipStats` and the F1533
  radius, the live missile list, and the F953 hand-off through
  `NpcShip::hit` (so the missile and the laser share shields -> hull -> F942).
- `Combat`: `missiles: [u8; 10]` = F998's `[0x82, 0x82, 0..]`,
  `first_missile`, `take_missile` and `missile_count`; serde default so old
  saves have no missiles. `Flight::validate` rejects a code with no model.
- `Navigator`: `missile_lock` (`DATA_008875`) and `missiles`; the combat lock
  is set by Shift+T on a locked ship (the port's T cycles, so the shift goes
  to the combat target instead of the previous navigation target when a ship
  is locked), and is cleared when the ship dies or the system changes.
- `main`: `M` scans the pylons, requires the combat lock, launches at the
  ship's position and axes, and messages the outcome; `tick_missiles` applies
  F953 after `tick_ships`.
- `Screen`: draws F941's missile objects through the ordinary model renderer,
  draws F1114's circle at (0x9f,0x4f) while the combat lock blinks, and feeds
  the pylon words to `ShipSystems.slots` so F1120's ten lamps show the
  remaining ammo on the F2 page.
- **Timing from the executed original.** `--missile-probe` (see 8.3) shows a
  model-6 missile gaining 4 velocity units a frame, so the AI's event-5 `tics`
  is 0x20 (not the frame's 994); F758's age increments once per 20 frames, so
  guidance runs at 0.4 s and the 60-update lifetime is 24 s. The port carries
  F717's thrust vector every frame and updates F758's aim/fuse every 0.4 s.
- **Deliberate floating-point simplification:** the original's Q15 thrust
  words become mm/s2 with the AI's 0x20 tics (0.59 mm/s2 per word), F585's
  quantised turn words become a continuous maximum rate (turn =
  `(word/8) * TAU * 50 / 65536` rad/s), and F597's shifted lead fudge becomes
  a continuous intercept point. The launch impulse, fuse distance, lifetime,
  damage, model and pylon tables are the original's.

### 8.3 Evidence

- `cargo test -p ffe-core missile`: 7 tests (tables, launch, tracking, fuse,
  contact, expiry, ballistic).
- `cargo test -p ffe-rs --lib missile`: 5 tests (model 6 spec, F953 shield
  branch, F942 kill, timeout, ammo scan).
- `cargo test -p ffe-rs --test missile_view -- --ignored` with
  `FFE_WORLD_CACHE`: the lock -> launch -> shield hit -> second launch -> kill
  sequence at 120 m, with `missile-launch.ppm`, `missile-hit.ppm` and
  `missile-kill.ppm`; the frame differs with the missile removed and with
  F1114's circle off.
- The reference `--state-combat` probe already confirms F998's two `0x82`
  words in the executed new game (section 6). `--missile-probe 400` (new
  adapter mode, `tools/reference/headless.c`) launches F1096 in the executed
  new game: the pylon words go `[130,130,0..]` to `[0,130,0..]` and a model-6
  object appears with control 0xb, AI `0x13`, owner 107 and target 114. Its
  velocity gains 4 units a frame and its `+0x9e` age goes 0 -> 1 at frame 259
  and 4 at frame 399, so F758 runs every 20 frames; the first guidance update
  turns the missile toward the target. That probe is the calibration for the
  port's 0.4 s guidance interval, 24 s lifetime and 0.59 mm/s2 per thrust
  word.

### 8.4 What remains

- **Missile view (5)** and F1099's `DATA_008877` range readout are not wired
  (no F10 equipment).
- **NPC launches.** F754 lets an AI ship launch at `+0xfe`; the port's ships do
  not fire or launch, so their missiles and the F1057 decoy/retarget path are
  absent. The core `Missile` and `MissileField` are owner-agnostic and can take
  that later.
- **ECM.** F718's `DATA_008835 & 4` branch and F1057's retarget are not
  reachable (no equipment/ECM model); the port has no ECM key.
- **F1113's navigation-target box** and the targeter's blink lamps still use
  the port's brackets; F1114's circle is drawn.
- **Sound cues** 0x15 (lock), 0x14 (launch), 0x20 and the F719 explosion are
  not played (the audio layer is synthetic).
- **The original's exact F597 lead and F585 quantisation** are approximated in
  floating point; the tables and gross behaviour match.
- **Player missile damage** has no receiver: the port has no player
  shields/hull damage model, so NPC missiles would need one (owner decision).
- **Owner decisions:** whether to add the missile view, NPC launching and the
  player damage model; whether to port F1113/F1116's targeter display fully.
