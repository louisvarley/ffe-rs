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
(`docs/notes/scanner.md`). From 2026-10-25 the port arms them like the source:
`T`/`Shift+T` toggle `DATA_007246`/`DATA_007247` **only with the Auto targetter**
(`ship+0xc8` bit 7), F1010's per-frame acquisition picks the object within six
pixels of the view centre, and F1111 draws the box/circle on F1113's blink. The
port's own lock brackets are gone.

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

The external view therefore *does* fire in the original (`F1040` maps 4 and 6
to mount 0 and F931 uses the ship's axes and front gun, not the external
camera). The port keeps that. The gunsight is still not drawn in view 4, so it
is a blind shot from the external camera, as in the DOS game.

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
is 1500/8 units ahead and the gun 1300/8, so the cone is 200/8 = 25 units
**behind the front eye**; `F1776` hands it to `F1680`, whose F1479 projection
has a near clip, so the original draws no front-view muzzle at all. The port
must not clamp it onto the near plane: doing so invented a flash 62 px below
the crosshair (2026-10-25, see §6.1). The cone is in front of the **external**
camera (whose default is 49 m behind the ship) and shows as a pixel there; the
rear and turret duplicates sit at or behind their cameras too. A long beam
line is **not** in the original; do not add one. The hit flash is model `0x9d`
(157), drawn at a ship whose shields absorbed a hit (`ship+0x151 & 1`, `F1536`
at `ffe.asm:91274-91320`). Stations and planets take neither shields nor hull
in `F953`, so they do not get that flash.

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
fuel/cargo or fail the drive, with message 0x9907. The new game has
`0x20000000` (1 t), so firing is effectively energy-unlimited in normal play.
The port now models the empty-pool recovery in `Flight::{burn_fuel,
refuel_energy}`: the Auto Refueller burns one tonne of hydrogen (military fuel
from drive class 10 up) for `0x20000000` energy; without fuel the warning
latches and the thrust limits stay zero. F938's thrust cost
(`mass * (|x|+|y|+|z|) >> 9 * tics`) is charged each flight frame.

`F730` (`ffe.asm:48263`) decrements `+0xe8` by the low word of `DATA_009155`;
JJFFE changed the DOS guard from `jng` to `jnz` (the "Pulse laser
non-limitation" fix) so sub-1-tic frames still count down. The port follows
JJFFE and records the difference here.

`F938` (`ffe.asm:70122`) cools `+0xea` by `seconds * 0x400`, or `* 0x1000`
with the laser cooling booster (`ship+0xc8 & 1`). `seconds` is whole game
seconds; the port carries the fraction between frames.

### 2.3.1 The unit of `DATA_009155` (2026-10-25)

`DATA_009155` is elapsed time in **1/49710 s tics**, not a 50 Hz frame count
and not a frame counter. At 1x a 50 Hz frame is 994 and a 60 Hz frame 828; the
time-acceleration factor multiplies it (10x is 9942 at 50 Hz). `main` already
passed the right value (`elapsed * 50 * TIME_ACCELERATION[acceleration]`: the
table holds tics per 50 Hz frame, 994 at 1x, so that is 49700 tics a second);
the 2026-10-25 audit extracted it as `combat::frame_tics` and added the test
below so the unit cannot be mistaken for a 50 Hz tick or a frame count. At
60 Hz a 1MW pulse fires at frames 0 and 38 (0.63 s), at 50 Hz 0 and 32.

The executed original pins it (`jjffe-smoke --laser-probe 240`, a new adapter
mode that calls the real F931 on the player object): `DATA_009155` is 1043 in
the adapter's synthetic clock, F730 subtracts 1043 every frame, and shots land
at frames 165, 195 and 225 — 30 frames = ceil(0x795c/1043). Each shot adds 400
heat, removes 800 energy and sets `+0x151 = 6` (fired + front mount).
`tools/reference/verify-combat.py` asserts the cadence, heat and energy from
that probe.

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

The km value is in **hundredths** and the AU value in **thousandths**:
`0x8604/05` expand through FF80 index 3 (`F1115`) to `DATA_007282/283`,
whose FF30 writers are `FUNC_001381_FF30Write2dpFracV1` for the km string and
`FUNC_001377_StringConvertFraction` with three places for AU. The suffix is
concatenated with no space, and trailing zeros stay: `5400` AU is `5.400AU`,
`5` AU is `0.005AU`, `100` km is `1.00km`
(`ffedat.asm:18889-18915`). The on-view string starts with an ignored 0x81
byte and a newline (`0x0d`), so the text sits 10 rows below F1100's
`target - 5` origin: `(x-5, y+5)`. The off-view strings are
`Target <n>km/AU Behind` (`0x8606/07`) and
`Target Out of Range (<n>km/AU)` (`0x8608/09`). With the target in front but
outside the view, `0x860a..0x8611` pick a left/right arrow, or the
below-target 0x7f/0x80 variants when the target's `+0x9a` is greater than 79:

| code | string |
|---|---|
| `0x860a/b` | `}` (0x7d, left arrow) ` Target <n>km/AU` |
| `0x860c/d` | `Target <n>km/AU ~` (0x7e, right arrow) |
| `0x860e/f` | 0x7f, ` Target <n>km/AU` (below, left) |
| `0x8610/11` | `Target <n>km/AU ` 0x80 (below, right) |

The colour is the caller's COLPAL, not a 12-bit palette word. `F1099` calls
`F1100` for the combat target (`player+0xed`) with **0x10** (0x5f = 95 in the
missile view, `ship+0xea == 5`) and for the navigation target (`player+0xec`)
with **0x11** (`ffe.asm:80555-80595`). `F1587` uses that byte directly as a
video index; the fixed low palette (`F1641`'s `DATA_007769`) makes 0x11 green
`(0,60,0)` = `#00f000` and 0x10 red `(60,0,0)` = `#f00000`. The fixed map
reaches the same entries through `F1327` (`DATA_007770[0x0f0] = 17`,
`[0xf00] = 16`); the dynamic upper half is not involved. Entry 95 is amber
`#f0a810`, which is why the old hard-coded 95 was wrong.

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

`FUNC_000998` (`ffe.asm:74028`) dispatches on `DATA_008881`: type 1 is the
Ross 154 arm, and **type 0 is the DOS default** (`F147(0)`). The default arm at
`ffe.asm:74159` sets:

- model `0x1f` (31) - the **Saker Mk III fighter** (`shipidx.txt`), with
  F1533's `DATA_002655` statistics: 2 gun mounts, 28 t mass, 23 t cargo,
  shield word 16412, eye `1800/400`, front gun `2200/0`, rear `1900/-200`;
- front laser `ship+0xd2 = 0x88` (1MW pulse); the other mounts stay zero;
- drive byte `+0xd0 = 2`;
- two missiles `+0xd6/+0xd7 = 0x82` (`KL760 Homing Missile`);
- energy `+0x11e = 0x20000000` (1 t);
- general equipment `ship+0xc8 = 0x40240020`: Scanner (18), Automatic Pilot
  (21), Atmospheric Shielding (30), Navigation computer (5); no Combat
  Computer, radar mapper, shield generator or cabins;
- cash `0x2710` (10,000 credit units);
- `sub [DATA_008889],0x10` for the drive/equipment allowance, then the F2 page
  subtracts the 6 t of fitted items and shows **1 t free hold**.

The executed default arm confirms model 31, mounts 2, lasers `[136,0,0,0]`,
missiles `[130,130,0,0,0,0]`, drive 2, shields `[0,0]`, hull 112 and cash
10000 (`docs/evidence/saker/`). The type-1 Ross 154 arm keeps model `0x17`
(23) with one mount and the same four combat words; the port supports it with
`--start 1`.

There is no outfitting or save equipment in the port. It therefore keeps the
F998 new-game loadout (front 1MW pulse, class-2 drive, 1 t energy) for the
sandbox too, and keeps the front laser when the development ship list changes
the hull unless the hull has no gun mount. **Consequence:** the Saker has two
hull mounts, so F1 offers front/rear/external, but only the front mount has a
laser: only the front view draws the gunsight and fire works only there. Rear
and turret firing are implemented and unit-tested with a synthetic loadout, but
no playable hull carries a rear or turret laser today. Do not give turrets a
laser by invention.

## 5. What the port implements

- `ffe_core::laser`: F930's table and pulse/beam branches, F931's gates, F730's
  refire, F938's cooling, F953's shields/hull damage, F1100's range words and
  km/AU value, F270's bar height/energy scale, and F817/F819's hit test against
  each body's interaction radius (`body.radius`, object `+0x138`). F819 only
  accepts a **non-negative** distance along the ray (it skips a negative one),
  so a box that contains the gun — a planet's atmosphere radius around a nearby
  ship, or the starport the player is beside — is not a hit; the port now
  requires `near >= 0` (2026-10-25). The box is tested axis-aligned in the system
  frame here; F819 aligns it with the target's own axes, which only changes
  grazing shots (see §7).
- `ffe_rs::combat::frame_tics`: `DATA_009155` from the frame's seconds and the
  time-acceleration factor (§2.3.1), used by both the refire countdown and
  F930's beam costs.
- `ffe_rs::combat::Combat`: the four mount codes, energy, heat, refire, drive,
  cooling booster and the new-game loadout; serde with `#[serde(default)]` so
  old saves load with no lasers.
- `Screen`: the bitmap-97 gunsight, bitmap-98 velocity marker, F270's three
  bars, F1100's on/off-view range text, F1099's target hull/shield text (draw
  path exercised by a synthetic frame and live ship targets), and F1776's
  model 156 at the gun with F1680's near clip (no clamped front-view flash).
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
  For the type-1 arm, `--state-combat 170` prints from frame 161:
  `model 23, mounts 1, lasers [136,0,0,0], missiles [130,130,0,0,0,0],
  drive 2, shields [0,0], hull 100, refire 0, heat 0, energy 536870912
  (0x20000000), cash 1000`. For the DOS default arm (a local probe build using
  the default start key `0x0b`) it prints from frame 161:
  `player 87, model 31, mounts 2, lasers [136,0,0,0],
  missiles [130,130,0,0,0,0], drive 2, shields [0,0], hull 112, refire 0,
  heat 0, energy 536870912 (0x20000000), cash 10000`, and the F2 page shows the
  Saker, its four general items, the 1MW pulse front mount, Cargo Space 1 t
  and no shield generators. This confirms F998's type-0 loadout against the
  executed original; `docs/evidence/saker/` holds the page. The existing
  `--state` output is unchanged.
- **NPC AI frames (2026-10-25):** `cargo test -p ffe-rs --test npc_ai -- --ignored`
  with `FFE_WORLD_CACHE` puts the player in front of a pirate and ticks the real
  AI until F931/F930/F953 damage arrives, kills a clean ship for the police
  response and carries a pirate through a jump. With `FFE_SHOT_DIR` it keeps
  `npc-ai-before.ppm`, `npc-ai-hit.ppm` (the labelled pirate and `Hull: 50.0%`),
  `npc-ai-police.ppm` and `npc-ai-restored.ppm`. The player's own damage words
  have a save-compatibility test in `tests/combat.rs`. See `docs/notes/npc.md`.
- The hit/damage path has live ship targets now (the traffic above); the
  target-readout draw path also has a synthetic frame test. See §7.

### 6.1 The 2026-10-25 laser audit

Owner report: "lasers do not seem to fire quite right." Reproduced and fixed:

- **Refire/beam tics checked (not a bug).** The port's old
  `elapsed * 50 * TIME_ACCELERATION[acceleration]` was already 994 tics at
  50 Hz and 828 at 60 Hz, and F730/F930 therefore used the right word. The
  executed probe still pins it (shots at frames 165/195/225, 30 frames apart at
  `tics` 1043) and the port fires at 0/38/76 in the new `laser_view` test. The
  conversion is now `combat::frame_tics` with a unit test, so a future change
  to a 50 Hz frame count is caught (§2.3.1).
- **Invented muzzle.** The port clamped the model-156 origin's z to 1000, which
  drew the cone at (160,140), 62 px below the gunsight, in the front view. F1776
  draws it at the gun (1600 units behind the front eye) and F1680's near clip
  drops it. `combat-beam-front.ppm` is now pixel-identical with and without the
  beam; `combat-beam-external.ppm` shows the original external-view pixel.
- **Gun-containing boxes.** `laser::hit` clamped the entry distance to 0, so a
  planet/station box around the gun was hit at distance 0 and swallowed the
  shot (reproduced by the new `laser_view` setup 200 m from a pirate in a
  system where the starport/planet box surrounds the gun). F819 skips a negative
  distance; the core test
  `a_box_that_contains_the_gun_is_not_hit` covers it.
- **Sound bank.** F930 selects `laser1`/`laser2` from the code's
  `(code & 0x78) >> 3` index, not the mount; `main` used the mount. A beam in
  the front mount now plays `laser2`.
- **Checked and kept.** The shot is the mount's nose-parallel ray from the gun,
  not an auto-aim at the crosshair; the 1 px art offset of bitmap 97
  (`(159,78)` vs F1479's `(160,79)`) is in the original and does not miss a
  ship-sized box at 200 m. External view fires mount 0 (F1040). The front/rear/
  turret mount mapping and `ship[0xd2+view]` are unchanged and tested.
- **Tests/frames.** `cargo test -p ffe-rs --test combat` (run geometry, mount
  axes, 60 Hz refire); `cargo test -p ffe-core --lib laser` (inside-box hit);
  `cargo test -p ffe-rs --test combat_view -- --ignored` (front cull, external
  muzzle); new `cargo test -p ffe-rs --test laser_view -- --ignored` fires at
  60 Hz at a pirate and keeps `laser-aim-before.ppm`, `laser-shot-1..3.ppm`,
  `laser-hit-1..3.ppm` (shields 100 -> 50 -> 0, hull 100 -> 50, model-157 flash
  and F1099's readout). `verify-combat.py` now runs `--laser-probe` and the new
  view test.

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
  a particle; the port runs it in `Flight::take_damage_with_systems` and
  `ffe-rs::damage` (equipment bits, drive and thruster words) with the
  original's messages.
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
  `SystemEffect.mass`, so F942's fragment set draws. The F1560/F1559/F1613 span
  fill is ported and wired (`docs/notes/fills.md`), so F1721 polygon entries
  fill in the playable, and `effects_view::explosion_fragment_polygons_fill`
  checks a model-155 fragment's polygons cover pixels. The line cloud still
  visible in `effects-kill.ppm` is model 153's two-point face records (F926's
  debris pods); those are edges in the original too, not unfilled polygons.

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
  AI (see above); docking and NPC missile launches remain absent. Thargoid
  traffic exists through F690/`Navigator::spawn_thargoid_traffic` but has no
  system trigger (its `F35(0x16)` count is mission-script data); see
  `docs/notes/npc.md`.
  F942's model 0x9b explosion, F939's hit particle and F926's debris are ported
  and drawn (see §7).
- **Rear and turret lasers.** The new-game Eagle has one mount; the port has
  no outfitting/save equipment. The firing paths (mount 1/2/3, turret axes and
  gun offsets) are implemented and tested, but no playable loadout reaches
  them.
- **Missiles** are implemented for the player (see §8): `M`/F1096/F941, the
  missile object, guidance, fuse, damage and the F1120 pylon lamps. The combat
  target `player+0xed` (`DATA_008875`) is set by clicking a ship, by the
  equipment-gated auto-targetter, or by the port's older `Shift+T` fallback.
  NPC ships now launch through F773/F754 and evade through F757/F935; the
  player's F1054 chaff uses the Chaff cargo word and F1057's retarget. The
  missile view (F10) and ECM are wired (see §8.2).
- **Hit effects/explosions.** `F939`'s particle, `F942`'s explosion and the
  `F926` debris are ported in `ffe_core::effects` and drawn by `screen`; see §7.
  `F952` is subsystem damage, not a visual effect.
- **Laser and hit cues** (0x0a/0x0b/0x1c/0x1d/0x14) are not played; the ECM
  (0x19/0x1a) and lock (0x15) cues are (see §8.3).
- **The combat targeter** is ported (`docs/notes/scanner.md`): F1100/F1113's
  targeting tunnels, F1114's boxes, F1111's blinking F1612 circle/box cursor
  and the `DATA_007252` game-second blink. The Auto targetter (general bit 7)
  is now fitted through the starport Upgrades list; `T`/`Shift+T` arm the
  navigation/combat cursor and F1010/F1062/F1064 acquire the object within six
  pixels of the view centre (see §8.2). The port still lets `T` cycle locks
  when the item is **not** fitted (reported deviation) so the sandbox autopilot
  remains usable without a starport visit.
- **Target colour.** F1100's colour is a COLPAL low index (0x11 navigation /
  0x10 combat, 0x5f in the missile view), not the runtime upper palette; the
  port resolves it through the imported low palette (`F1641`/`base:palette/low`).
  Entry 95 is only the missile-view colour, not the normal target colour.
- The port's own status block at y119..157 covers the lower centre where the
  muzzle cone and the target `Hull:` line would draw; the dashboard's retire-port-UI
  slice clears this.

**Remaining laser differences (2026-10-25):**

- The hit test uses an axis-aligned box in the system frame; F819 aligns the
  box with the target's axes. A centred shot is the same, so this only changes
  grazing shots and would need each target's orientation in `laser::Target`.
- The original has no visible front-view muzzle (model 0x9c is behind the eye)
  and the sound is the firing cue; keep it that way unless the owner wants a
  port-only HUD flash. Model 156 is one pixel in the external view.
- F929's empty-energy recovery from fuel/cargo (`0x9907`) is ported (the Auto
  Refueller burns the hold's hydrogen/military fuel); the new game's 1 t pool
  is effectively unlimited while fuel remains.
- F952's subsystem damage is ported for equipment, the drive and the six
  thrusters; F952's outer `F922(cargo space) < DATA_008889` cargo split still
  has no score word, and the target readout's exact colour/arrow forms remain
  as before.

**Owner decisions:** NPC traffic and its flight/combat AI are implemented at the
state-rules level (see `docs/notes/npc.md`); decide whether to port the
target-computer equipment and the auto-targeter arming that switches
F1113/F1114's tunnels on (the port draws them only when the targeter flags are
set), missiles, and a shield generator for the player (the AI's fire is lethal in
two hits without one). F1100's per-target text colour, two-decimal km /
three-decimal AU format and off-view arrow forms are ported; the port's own
lock brackets are gone. For
the effects, decide whether their lifetimes should follow the
original's AI timers (F764/F769/F927) rather than the port's per-effect seconds,
and whether the model-155 explosion keeps the `EXPLOSION_MASS` word `0x4000`
that selects its twelve fragments or follows F942's literal `0xffffffff` (which
selects none). The fills now fill the fragments' polygons; the fragment anchors
and the debris line records are separate questions (F942's distance word `+0x7e`
is still not modelled).

The player's F953 words have a save-compatible test
(`crates/ffe-rs/tests/combat.rs`) and the AI sequence has its own cached test
(`crates/ffe-rs/tests/npc_ai.rs`, `npc-ai-*.ppm`). The scanner contacts and the
targeter's four-box tunnels have their own cached test (`scanner_view`), and the
effects are framed by `effects_view`.

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
  explosion/`F925` removal. F718's ECM branch reads `DATA_008835` bits 2/3 and
  `DATA_000029`'s per-model dispatch (`ffe.asm:47495-47566`): the standard
  system destroys models 5 and 6, the Naval system also destroys 7, 9 and 10,
  and 4, 8, 11, 12 and 13 are immune. (An earlier version of this note had the
  table inverted.) There is no range or owner test; the burst is global for the
  frame. F1057 (`ffe.asm:78789`) can retarget a model 6..11 missile whose target
  is the player to the chaff object `DATA_007253`, gated per missile on
  `F1530() & 0x1000`.
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
- `Navigator`: `missile_lock` (`DATA_008875`), `missiles` and `missile_view`
  (`DATA_008877`, the last missile the player launched); the combat target is
  set by clicking a ship, by the auto-targetter (below) or by the port's older
  `Shift+T` fallback, and is cleared when the ship dies or the system changes.
  F941's launch stores the new missile's id in `missile_view` for the player's
  owner; F1102 clears it when that missile dies.
- `main`: `M` scans the pylons, requires the combat target, launches at the
  ship's position and axes, and messages the outcome; `tick_missiles` applies
  F953 after `tick_ships`. `E`/`;`/Insert fire F935's ECM when the equipment is
  fitted (no energy or ammo cost); F10 enters the missile view when the Missile
  Viewer is fitted and `DATA_008877` is live. `T`/`Shift+T` with the Auto
  targetter arm F1111's cursor and the per-frame acquisition sets the target.
- `Screen`: draws F941's missile objects through the ordinary model renderer,
  draws F1114's circle at (0x9f,0x4f) while the combat targetter is *armed*,
  F1113's navigation box while the navigation targetter is armed, and feeds
  the pylon words to `ShipSystems.slots` so F1120's ten lamps show the
  remaining ammo on the F2 page.
- `cargo test -p ffe-rs --test npc_missiles -- --ignored` with
  `FFE_WORLD_CACHE`: the scripted NPC encounter. A pirate launches F754's
  KL760 at the player (owner/target/pylon checked), the player's chaff
  (`player+0x174` = hold[32], F1120's lamp) pulls it to the model-0xa4 decoy,
  a second NPC missile hits the player through F953 (200 shields), and the
  pirate's F757/F935 E.C.M. destroys the player's missile and returns the ship
  to Attack. `docs/evidence/npc-missiles/` holds the launch, chaff and damage
  frames plus a README.
- `cargo test -p ffe-core npc`: adds F754's band/pylon/owner/target test, the
  outside-band and empty-pylon negative cases, F773's pylon/E.C.M. fit and
  F757's burst/notification.
- `cargo test -p ffe-rs --lib missile`: adds the player's own-missile owner
  exclusion, the NPC missile's `HitPlayer`, and the chaff `HitDecoy` events.
- `cargo test -p ffe-core --test market chaff`: the Chaff good is the last
  market commodity (index 32), the word `player+0x174` points at in the hold.
- `cargo test -p ffe-rs --test npc_ai -- --ignored` still passes: the new
  launch path does not disturb F931's laser sequence.
- **Timing from the executed original.** `--missile-probe` (see 8.3) shows a
  model-6 missile gaining 4 velocity units a frame, so the AI's event-5 `tics`
  is 0x20 (not the frame's 994); F758's age increments once per 20 frames, so
  guidance runs at 0.4 s and the 60-update lifetime is 24 s. The port carries
  F717's thrust vector every frame and updates F758's aim/fuse every 0.4 s.
  F754/F757's rolls are taken once per 20 ms of game time, matching the
  controller's event-6 period; the executed original was not probed for an NPC
  launch (the existing probe only drives F1096's player launch).
- **ECM.** `Loadout::ecm()` reads the Naval E.C.M. bit (23) first and the
  standard bit (19); `MissileField::ecm(naval)` applies F718's
  `ffe_core::missile::ecm_destroys` table and returns the losses;
  `Navigator::ecm` spawns F719's model-0x9b explosion (no debris) and clears
  `DATA_008877` if the viewed missile was one of them. The key is F1054's `e`
  (0x65) plus the source's `;` (0x3b) and Insert (0x2); the sound cues are
  0x19/0x1a. There is **no** energy, ammo or range cost in F935/F718; the only
  gate is the fitted bit. `E` is also the port's Modern-controls roll key, so
  holding it both rolls and fires one burst on the press edge.
- **Auto targetter.** `ffe_rs::targeter::Arming` holds `DATA_007246`/`7247` and
  the skip bytes `DATA_007248`/`7249`; `main` gates `T`/`Shift+T` on
  `Loadout::auto_targetter()`. Every frame while armed, it projects the bodies
  and ships with `Screen::body_projection` and calls
  `targeter::acquire`, which is F1061/F1063's F989 window and six-pixel
  Manhattan rule. The navigation acquisition sets `nav.locked`, the combat one
  `nav.ship_lock`/`nav.missile_lock`; the cursor disarms and sample 0x15
  (`Cue::UiBeep2`) plays. The recovered listing has no call to F1010, so this
  per-frame acquisition is the port's (reported).
- **Missile view.** `ViewMode::Missile` and `ViewState::missile` carry
  `player+0xef`'s missile object; `Camera::new` uses the missile's position and
  axes plus its model's eye words (F1054's `JUMP_004340`). `main` fills it from
  `Navigator::viewed_missile`, forces the front view when F1102 clears the id,
  and F1099's view-5 branch skips the nav target, gunsight, velocity marker and
  player HUD while the combat range uses colour 0x5f and measures from the
  missile.
- **Chaff/decoy.** `MissileField::retarget` implements F1057's model 6..11
  range and per-missile `F1530() & 0x1000` gate. `Navigator::deploy_chaff`
  spawns F1054's model-0xa4 object (`DATA_007253`) and `main`'s `c` key applies
  the dispenser bit (11) and the ammo gate. The ammo is not a separate counter:
  `player+0x174` is the cargo array's word for commodity 32, the source's
  `DATA_006248` "Chaff" good (`+0x134 + 2*32`; F555/F556's hold). A market
  therefore stocks it and the player buys tonnes at a station. F1054 only
  *tests* the word and never decrements it, so the port also treats any held
  tonne as a free launch until it is sold; the owner may prefer to spend one
  tonne per launch (see 8.4). The decoy list is capped at 16 and has no source
  lifetime: F927's object-slot recycle is not modelled.
- **NPC launches.** `NpcShip` carries F773's ten pylon words
  (`ship+0xd6..+0xdf`) and general-equipment dword. `fit_equipment` follows
  F773's missile branch: a cargo-tonnage budget (less the F774 laser mass),
  `RND(0..=statistics word +0x12)` missiles, the code from `DATA_006260`
  (MISSILE_CODES: `0xc0/0xc1`, `0x82`, `0x83`, `0x84`) and F773's E.C.M. /
  Naval E.C.M. / energy-bomb draws. `NpcShip::tick` rolls F754 once per 20 ms
  of event-6 time while the ship is in Attack (262 m..4.2 km) or Pass
  (0..8.4 km): `F1530 < 0x2000`, pylon `(F1530 & 7) + 1`, target
  `player+0xfe`. `Navigator::launch_npc_missile` puts the object in the
  same `MissileField` as the player's, and F941's evasion branch is applied on
  the player's launches. F941's failed-pylon branch calls F937; the energy-bomb
  blast itself is not modelled (reported).
- **NPC E.C.M. and evasion.** F941 puts a missile's live AI target into state
  0x0c. The port's `State::Evade` is F757: the ship turns across the incoming
  line (F588's random roll is approximated by a right-hand offset) and rolls
  `F1530 > 0xa000` for F935. F935 checks the naval bit first and the standard
  bit second; `Navigator::ecm` destroys the `DATA_000029` models system-wide,
  spawns F719's explosions and ends the evading ships' state through F780/F781.
  A ship with neither bit never bursts.
- **Deliberate floating-point simplification:** the original's Q15 thrust
  words become mm/s2 with the AI's 0x20 tics (0.59 mm/s2 per word), F585's
  quantised turn words become a continuous maximum rate (turn =
  `(word/8) * TAU * 50 / 65536` rad/s), and F597's shifted lead fudge becomes
  a continuous intercept point. The launch impulse, fuse distance, lifetime,
  damage, model and pylon tables are the original's.

### 8.3 Evidence

- `cargo test -p ffe-core missile`: 9 tests (tables, launch, tracking, fuse,
  contact, expiry, ballistic, ECM's model table and F1057's range).
- `cargo test -p ffe-rs --lib missile`: 8 tests (model 6 spec, F953 shield
  branch, F942 kill, timeout, ammo scan, F941 ids, ECM losses and retarget).
- `cargo test -p ffe-rs --test missile_view -- --ignored` with
  `FFE_WORLD_CACHE`: the lock -> launch -> shield hit -> second launch -> kill
  sequence at 120 m, with `missile-launch.ppm`, `missile-hit.ppm` and
  `missile-kill.ppm`; the frame differs with the missile removed and with
  F1114's circle off.
- `cargo test -p ffe-rs --test ecm_view -- --ignored` with `FFE_WORLD_CACHE`:
  the equipment gate, F718's model-6 loss, F719's effect object and
  `DATA_008877`'s clear, plus the armed navigation cursor and the missile view
  camera/readout. `docs/evidence/ecm/` holds `ecm-before/after.png`, the
  `targeter-armed.png` box and `missile-view.png` (front view for comparison),
  with a README on the source and the model-155 limitation.
- `cargo test -p ffe-core missile` and `cargo test -p ffe-rs --lib missile`
  now also cover `DATA_000029` (models 5/6, the naval 7/9/10 and the immune
  4/8/11/12/13), F1057's model range/gate, the F941 ids and F719's explosion.
- `cargo test -p ffe-rs --test views` covers the missile camera's position,
  axes and cockpit fallback; `targeter`'s unit tests cover the F989 window,
  the six-pixel rule and the arming skip bytes.
- The reference `--state-combat` probe already confirms F998's two `0x82`
  words in the executed new game (section 6). `--missile-probe 400` (new
  adapter mode, `tools/reference/headless.c`) launches F1096 in the executed
  new game: the pylon words go `[130,130,0..]` to `[0,130,0..]` and a model-6
  object appears with control 0xb, AI `0x13`, owner 107 and target 114. Its
  velocity gains 4 units a frame and its `+0x9e` age goes 0 -> 1 at frame 259
  and 4 at frame 399, so F758 runs every 20 frames; the first guidance update
  turns the missile toward the target. That probe is the calibration for the
  port's 0.4 s guidance interval, 24 s lifetime and 0.59 mm/s2 per thrust
  word. It also confirms that F1096 reads the combat target from the same
  `player+0xed` word the port calls `missile_lock`: `headless.c` writes
  `ref_state[0xed]` and the executed launch stores it at the missile's `+0xfe`
  (target 114).

### 8.4 What remains

- **NPC launches** are implemented for F753's exponent 5..8 path and F756's
  state-3 path. F754's state-1/2 alignment update (whether the nose is within
  the source's `0x20000000` dot) is not modelled: the port's Attack covers both
  source rows. F937's energy-bomb blast (the empty-pylon branch) is not
  implemented; the fitted bit is still derived and kept.
- **Chaff.** F1057's retarget, F1054's key/equipment gate and the
  `player+0x174` ammo source are implemented. The source word is the cargo
  array's Chaff word, so it is stocked by buying the good at a market; F1054
  never decrements it. The port follows that (any held tonne is a free launch)
  rather than inventing a consumption rule. **Owner decision:** spend one tonne
  per chaff launch? The source's missing consumer looks like an omission.
- **ECM visuals.** F935's `ship+0x14f = 2` / `ship+0x151 |= 0x20` draws model
  0xa1 at the ship for the activation frame (F1536). The port plays the sample
  and destroys the missiles but does not draw that one-frame ring. F719's
  model-0x9b object is spawned; its command stream collapses to single-pixel
  flats in the port's model renderer, as F942's does (the visible F942 burst is
  F926 debris, which F719 does not spawn).
- **Targeter acquisition order.** F989 walks the object slots from 0x72 down;
  the port projects `nav.bodies` and `nav.ships` in their own order, because it
  does not model object slots. Bodies are the only navigation-target
  candidates; the original can store a ship in `player+0xec`, which the port's
  `locked` index cannot.
- **T without the Auto targetter.** The port keeps `T` cycling locks (and
  `Shift+T` toggling the missile target) when the item is not fitted, so the
  sandbox autopilot/missiles still work before a starport visit. The original
  returns without doing anything.
- **Sound cues.** The lock (0x15), ECM (0x19/0x1a), chaff (0x14) and launches
  (0x20) are posted; F719's 0x06 is not.
- **The original's exact F597 lead and F585 quantisation** are approximated in
  floating point; the tables and gross behaviour match.
- **NPC launch cadence** is source-derived (one F754 roll per 20 ms) but was
  not measured in the executed original. The F757 evasion turn is the port's
  right-hand jink, not F588's exact random roll/pitch words.
- **Owner decisions:** whether to spend one Chaff tonne per launch; whether
  `T` should do nothing without the Auto targetter; whether `E` should stay
  shared with Modern roll; and whether the approximated F597 lead/F585 turn
  quantisation is close enough. The `--missile-probe` reference run calibrates
  the 0.4 s guidance interval, the 24 s (60-update) lifetime and the per-thrust
  acceleration.
