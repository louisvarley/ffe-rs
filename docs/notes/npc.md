# NPC ships in generated systems (F688, F635, DATA_006255, F931, F942)

Implemented 2026-10-25 in `crates/ffe-core/src/npc.rs`, with the playable
wiring in `ffe_rs::navigation` (`Navigator::ships`, `spawn_traffic`,
`tick_ships`, `lock_ship`, `player_attack`) and `ffe_rs::screen`
(`SystemShip`, `draw_system_ships`). Read from JJFFE 2.8a7 `ffe.asm` and
`ffedat.asm`; the DOS release's `FIRSTENC.EXE` has the same tables and routines
with one behavioural difference at F724 (below).

The ships exist so the ported combat path (`docs/notes/combat.md`) has live
ship targets: F953's shields/hull damage, F942's destruction, F1536's
model-0x9d shield flash and F1099's target readout are reachable in a generated
system. Since the AI slice (2026-10-25) they also fly the original's
approach/attack/hold states, fire the ported lasers at the player and make the
police respond to a wanted player, so F953's player damage is reachable too.
Since the docking slice they also run F689/F745/F746/F747/F733/F734/F735/
F651/F736/F761/F762's station cycle: traders request a bay, dock, wait out the
original's two-to-three day object time, launch through the station's door and
resume transit, instead of orbiting for ever.

## 1. The original's objects and where ships came from

- **F880** (`ffe.asm:64596`) creates one physics object per generated system
  object (star, planet, moon, station, starport) and nothing else. The
  generated system lists do not contain ship records.
- **F649-F657** (`ffe.asm:44337-44793`) are the *station traffic record*: a
  station's bytes `+0xc9` mode, `+0xca` current ship, `+0xce` highest bay,
  `+0xd0..+0xd7` bay occupancy, `+0xd8` timeout and `+0x9e` door word. They
  grant/refuse clearance and allocate/release a bay; they do **not** spawn
  ships. The port already has the record and every routine in
  `ffe_core::traffic` (`docs/notes/docking.md`), so the new code reuses
  stations as anchors rather than duplicating it.
- **F688** (`ffe.asm:45818`) is the system traffic spawner. It asks **F869**
  for the galaxy record's fields and calls:
  - `RND(+8) Ã— F689`: traders (`F772(1)` â†’ F635 type 1 + `2*DATA_008996`);
  - `F35(0x16)` (global variable 22) -> F690: the type-0x11 branch. `F35`
    (`ffe.asm:3524`) reads the global block `DATA_008548`; the only setter is
    the mission script bytecode's assignment (F37, `ffe.asm:3646`, calling F34
    at `3507`), and the 71-dword block is saved with the commander (F65,
    `ffe.asm:6898`). There is no direct `F34(0x16, ...)` call in `ffe.asm`, so
    no system field, government, allegiance or random draw sets it: F690 runs
    only when a mission script has set that variable. See "Owner decisions".
  - `DATA_008997` Ã— F691: ships parked in station bays (the cache's ship-data
    table is not loaded, so the port has none);
  - `RND(+8) Ã— F699`: shuttles (`F772(0xf)`);
  - `+6` patrol groups Ã— F698, Ã—4 in-system: F694/F695/F696 make a lone pirate
    or a group of three around the player;
  - `RND(+6) Ã— F692`: a no-op.
- **F927/F1533** (`ffe.asm:69170`, `90970`) allocate a physics slot (0x72 down,
  slot 0x14 reserved) and clone the model's statistics.
- **F772/F773** (`ffe.asm:50502`, `50526`) create the ship: the model comes
  from **F635** (`ffe.asm:43576`), which indexes the nineteen
  `(count, model-list)` records of `DATA_006199`/`DATA_006180..6198`. Types 0
  and 1 are shifted by `2 * DATA_008996`, the system's allegiance index
  (`record +10 >> 6`; Ross 154 is 2, so its traders come from the Imperial
  shipyard list, type 5). F773 writes the AI class to `+0x118`, drive `+0xd0`,
  lasers `+0xd2..`, missiles `+0xd6..`, hull `+0xe4 = mass*4` and shield
  maximum `+0xe2 = random*16`; F1533 zeroes both shield words first and F938
  recharges `+0xe0` to `+0xe2` over time.
- **F702/F1507** (`ffe.asm:46868`, `89917`) place a spawned ship at a random
  phase on an orbit: `radius = ffp 0x5000 / exponent 0x33`, direction from
  F1506, with F1508's sub-step interpolation. The port keeps the *rule* but
  uses a radius in its own unit system (below): the original's raw word is a
  system-wide orbit, not a point near the player, so copying it would spawn
  traffic millions of kilometres away and require the full AI transit.
- **F674/F675/F680** (`ffe.asm:45338`, `45374`, `45536`) clear and restore the
  AI ships when leaving/entering a system. F675 keeps a pirate (class 0xfb) only
  when it is within 0x10 of the player and keeps escape pods (model 0x9a); F680
  restores those and then F688 spawns the new system's traffic. **F927**
  (`ffe.asm:69170`) allocates a slot from 0x72 down, never below 0x14, and
  recycles the highest explosion/debris slot when full.
- **AI states** are `DATA_006255` (`ffedat.asm:9854`): 0x25 states with four
  routine slots each (control, movement, death notification, damage response),
  dispatched by F706/F704/F780/F778 on object `+0xff`. F752/F725/F727 are the
  common movement and physics steps; F585 applies a per-frame turn limited by
  model size (halved from model 0x35, quartered from 0x3b); F587 computes the
  rates and F1512 aims with the target's velocity; F753's arrival test picks
  state 3 under 5 units or F749's reposition (5) over 10; F694/F695 set the
  pirate states 5/0xb; F737/F787 are the jump-out states. F724 (state 2's
  movement routine) is the only AI fire call: it moves and calls **F931**.
- **JJFFE difference:** JJFFE replaced that F931 call with
  `or byte [ship+0x151], 2` ("FIX: Laser invulnerability", `ffe.asm:47856`), so
  JJFFE's AI ships only show the fired flag and do no damage. This port follows
  DOS FFE and lets the AI fire.
- **F1035/F1036** (`ffe.asm:76306`, `76393`) are *surface* spawns, not orbital
  traffic: they test `DATA_008867` (planet altitude) and spawn at state 0x19
  near a building model 0x72. The port's system flight has no surface ships yet.
- **Hostility** is not a flag on the object: FFE's AI states and the comms code
  decide it. Pirates are the 0xa/0xb groups and police the 0xe models; F931's
  player shot sets `DATA_008860` bit 2 ("illegal firing"), F941 (the player's
  missile) does the same, and `DATA_008646` bit 0 / `DATA_008647` carry the
  wanted/alert state that F783 clears when the chase ends. The port models
  pirates/Thargoids as hostile from the start and reconstructs the police
  response from those words (below).

## 2. What the port implements

### Spawning (`ffe_core::npc`)

- The full nineteen-record F635 model table (`SHIP_TYPES`) with the original
  counts, and F635's allegiance shift.
- **F922** (`ffe.asm:68974`) as `Random::bounded`, so model and placement
  draws are the original generator's (`a = rotl16(a + b + rotl5(b))`,
  `b = rotl5(b_old)`, result `(a & 0xffff) * limit >> 16`).
- F688's count fields (`Details::bytes[3]` for traders/shuttles, `bytes[1]`
  for patrol groups, `bytes[2]` for police) with a cap of `MAX_SHIPS = 5`. A
  normal system entry asks F688 for its own counts and **can spawn none**: the
  original may roll zero traffic. The port's old "at least one trader and one
  pirate group" guarantee is now `SpawnOptions::CAPTURE`, used only by
  `Navigator::respawn_traffic` (the scripted capture/`npc_view` fixtures). A
  normal `Navigator::spawn_traffic` entry, including the new game's start, uses
  F688's counts as they are.
- **The Thargoid limit** is `System::thargoids`, the caller's `F35(0x16)`.
  `counts` draws `RND(limit)` into `Counts::thargoids` (F688 passes the limit to
  F922, exactly like the trader count); `spawn` runs F690 after the traders in
  F688's order. `System::new(details, 0)` is the normal case, and
  `Navigator::spawn_thargoid_traffic(limit)` is the mission/script hook. There
  is no rate, system test or other random gate: `RND(limit)` type-0x11 ships
  are attempted, then the port's `MAX_SHIPS` cap applies.
- **Placement:** traders, Thargoids, shuttles and police use a random phase on
  an orbit around the first station/starport (`Anchor`), radius
  `clamp(1.2 * station radius, 2.5e6, 0.75 * laser range)` original units.
  The pirate group appears `PIRATE_DISTANCE = 3.5e6` ahead of the player (plus
  a side offset), inside F931's `0x7fffff` hit range, only for the scripted
  capture (`SpawnOptions::near_player`). A normal system entry uses the anchor
  orbit, as the port's scaled stand-in for F702's system-wide random point, so
  the pad and the arrival point are not forced into a pirate encounter.
- **F694/F695 pirate groups.** The leader is F694's type-0xa ship in state 5.
  The escort uses F695's grouped type-0xb list, spawns at its leader's
  formation offset and keeps station through `+0x102..+0x106` (the port scales
  F695's 500-unit words to 0.4e6/0.6e6 in its frame). `+0xfe` is the leader and
  `+0x100` the target, as in F695.
- **F697's bounty** (`+0x11a`) is `DATA_006268[(random & 0xf) + offset]`:
  offset 4 for a lone pirate, 2 for a group member; the table is
  `PIRATE_BOUNTIES` (0 for most draws, then 50..200 CR). F942 pays it when the
  player destroys the pirate.
- Each ship's `Spec` comes from the cache's model record and statistics:
  F1533's outer radius (`ModelInfo::radii().0`), the mass, shield, forward
  thrust and drive words and the gun-mount byte. Shield maximum is
  `(shield_strength & 0xff) * 4` (the original's equipment budget gives
  `random * 16`; this is the port's simplification), hull is `mass * 4`, and
  current shields start full (the original's F938 recharge catches up in play).
- The front laser (`+0xd2`) and its energy come from the equipment rules:
  pirates get one 1MW pulse and police/military one 5MW pulse mount; Thargoids
  use F774's own selection (`LASER_PROPORTION`/`LASER_TONNAGE`): class 0x11's
  0xb2 proportion and the model's cargo pick the 20MW beam for the 0x31-0x33/
  0x3f warships (203/149 t cargo) and 0xcf for the 0x45 transporter (13,000 t,
  but it has no gun mount). `+0x11e = DATA_006266[drive] << 24`
  ([`drive_energy`]). Traders and shuttles carry none. F773/F774's budget
  (equipment `+0x119`, spent on lasers then missiles) is not modelled.
  The equipment shop's separate **Thargoid Laser is 0xd5**
  (`equipment::THARGOID_LASER`, 50 t, tech 255); it is not one of F774's
  `DATA_006265` entries, whose last code 0xcf the equipment table calls the
  Large Plasma Accelerator.
- Names are the model-name table (cache v11) plus a serial, e.g.
  `Cobra Mk III 1`; the original's F703/F791 registration generator is not
  ported.

### Spawning only in flight (spawn-safety stream)

F687/F686's start entry does call F688, so the new game has the record's traffic
(the executed original's `--missile-probe` object list at frame 199 shows a
docked trader and two orbiting traders at Ross 154; its `--state-combat` keeps
hull 100 and shields `[0,0]` from frame 161 to 899). What it does **not** have is
a pirate group beside the pad: F702 places a spawned ship at a system-wide point,
so a normal entry uses the anchor orbit for the pirate group, not
`PIRATE_DISTANCE` ahead of the player. Only the scripted capture
(`SpawnOptions::CAPTURE`, used by `Navigator::respawn_traffic`) puts a pirate in
front of the camera.

`Navigator::on_pad()` names F994's `0x30` state and `Navigator::landed_view()`
also covers a surface `0x32`/`0x34`; `ffe-rs`'s main loop skips the NPC
`tick_ships`/missile step while it is true, following F1012/F1048's landed
handler (no F1037/F1041/F601/F603/F1512 flight step). The stationary ship is
then outside F817's box and cannot take F953 damage, even if the system's source
traffic includes a pirate group.

### The AI state table (`AI_STATES`)

`DATA_006255` (`ffedat.asm:9854-10006`) is in `ffe_core::npc` as
`AI_STATES: [AiState; 0x25]`. The table is four routine slots per state:
`(control, movement, notify, damage)`. F706 (event 6, the per-frame controller)
indexes `DATA_006255`, F704 (event 5, the physics step) `DATA_006256`, F780 (a
target's death) `DATA_006257` and F778 (this ship taking a hit) `DATA_006258`.
The labels between 6255 and 6258 are the first row's remaining slots, not four
separate arrays: state 0's row is `F742, F727, 0, F789`. The earlier port table
started at `DATA_006258` and read the slots in the wrong order, so every row was
shifted and F724 looked like a "combat" slot; this slice corrects it and a unit
test checks the table against the disassembly (`the_ai_table_is_data_006255`).

The rows name the flight cycle (0x00 F742 navigation, 0x01/0x02 F752/F753 attack
and F724 firing, 0x03 F755/F756 evasion, 0x04 F750 interception, 0x05 F749
reposition, 0x06 F759 static, 0x07 F736 departure, 0x08 F741 new target, 0x09
F770 trading approach, 0x0a F743 patrol station-keeping, 0x0b F751 formation),
the jump timers (0x10 F760, 0x11 F737, 0x12 F740), the hyperspace clouds
(0x1d/0x1e), the docked cycle (0x1f F734 request launch, 0x20 F747 docked
record, 0x21 F735 launch, 0x23 F746 docking, 0x24 F733 docked countdown) and
the missiles (0x13-0x1b). 0x22 F748 is the player autopilot's launch check, not
an NPC state.

The port's `State` names the subset it runs and lists its original codes in
`State::original`:

| Port state | Behaviour | Original rows |
|---|---|---|
| `Transit` | fly to the station/waypoint, then hold | 0x00/0x08/0x09 |
| `Approach` | final leg to F650's bay, F745/F746 | 0x23 |
| `Docked` | in the bay, F747's 2-3 day countdown (F733) | 0x20/0x24 |
| `Launch` | F734's F654 request, F735 waiting for the door | 0x1f/0x21 |
| `Depart` | F736 flies out, F651 releases, F761/F762 choose | 0x07 |
| `Hold` | static/parked | 0x06/0x0d/0x0e/0x0f |
| `Escort` | station-keep in the leader's frame (F743/F751) | 0x0a/0x0b |
| `Attack` | close/fire/intercept | 0x01/0x02/0x04/0x05 |
| `Pass` | F755/F756's evasion/overshoot | 0x03 |
| `Flee` | F761/F738's jump-out cloud | 0x1d |
| `Dead` | F942's explosion object | 0x1c |
| `Escaped` | the cloud is gone and F927 frees the slot | — |

### Flight rules (`NpcShip::tick`)

- **Turning (F585).** The ship turns toward its target at
  `TURN_RATE = 1.2 rad/s`, halved for models 0x35..0x3a and quartered from
  0x3b, exactly F585's model-size limits. The port interpolates the forward
  axis and keeps the up axis upright; F587's roll term is not reproduced.
- **Speed and braking (F601/F597).** The top speed is proportional to the
  statistics' thrust/mass ratio (`71 * thrust / mass`, clamped), so a light
  fighter outruns a freighter and accelerates harder. The acceleration reaches
  that speed in `ACCEL_SECONDS = 3`. The old constant-speed placeholder is
  gone: an approach brakes to `0.7 * sqrt(2 * accel * room)` so it can stop at
  the standoff, and if the ship is too fast to stop (`v^2 > 2 a s`, F597's
  branch) it commits to a `Pass` for 1.4 s instead.
- **Arrival (F753).** `Transit` switches to `Hold` within `ARRIVE = 0.35e6`
  and zeroes its speed; the hold orbit advances by `speed * 0.35 / radius`.
- **Escorts (F695/F743).** An escort's target is its leader's position plus
  `right*+0x102 + up*+0x104 + forward*+0x106`; once the player is within four
  standoffs it engages the player.
- **Fleeing.** A hostile ship whose hull drops to 25 % runs for
  `FLEE_SECONDS = 3` and then `State::Escaped` (F787/F738's hidden jump-out).
  The low-hull trigger is the port's: the original has the jump-out states but
  no hull test that reaches them.
- Positions and the player target are in the system frame (millimetres);
  `Navigator::tick_ships` passes the frame's tics for F730/F938.

### The station traffic cycle (F689, F745/F746, F733/F747, F734/F735, F651/F736, F761/F762)

Traders and shuttles no longer orbit for ever. F689 gives a trader `+0x100` (its
starport destination) and state 0x11; the source then runs the cycle below, and
the port runs the same states over its `npc::Station` snapshot and the station's
`traffic::Starport` record:

| Step | Original | Port |
|---|---|---|
| destination | F689/F700 sets `+0x100`; F741/F742 fly to `+0xfe` | `NpcShip::station` (a body index); `Transit` targets `Station::bay_point` |
| request | F736/F742 at distance 6 call F745; F655 = F654 + F650 | `DockRequest::Request`; `tick_ships` calls `Starport::request_docking(traffic_id, false, 0)` |
| bay | F650 scans `+0xce` down, returns the ship's own bay or the first free one; the 0x98b8 reply resets the record | `Reply::Granted(Bay)` → `begin_approach(bay, pad)`; `Busy`/`Full` retry every second |
| final approach | state 0x23, F746: F596/F589/F599 to the pad, F744's F652 on contact | `Approach` targets F650's pad (open-air) or the slot; `DockRequest::Touch` on arrival |
| docked record | F744 sets state 0x20; F747 waits for mode 3 to end, F653 clears the record, marks the object docked, state 0x24 | `Touch` → `Starport::touch` then `reset` (F653); the bay stays occupied |
| countdown | F733 compares the object time; F747 sets `[+0xac] += 2` days and `[+0xa8] += [D8818]` | `DockRequest` waits `DOCKED_DAYS` (2) plus a random day fraction in `dock_timer` |
| launch | F733 → F734's F654; F735 waits for the station mode 0xfe/0 | `DockRequest::Launch` → `Starport::request`; `Launch` waits for `door == 0` |
| departure | F651 frees the bay; F736 flies out; F761/F762 pick a station, or F738/F925 remove the ship | `DockRequest::Release` → `Starport::release`; `Depart` for `DEPART_SECONDS`, then `DockRequest::ChooseStation` |
| next station | F762's F700 draws a random starport; none → F925 | `Navigator::next_place` walks the system's places (the same station in a one-station system), or `escape()` when there is none |

The bay is held from F650's grant until F651 at departure, so a full station
replies `Busy`/`Full` exactly as `Starport::request_docking` already did. If the
clearance times out (F867, 0x7080 game seconds), `tick_traffic` calls
`touch_failed` on the NPC that held it and the ship asks again. A destroyed
ship's bay is released too (F788). Docked ships are kept in the traffic list but
are not drawn, labelled, locked or hit (`NpcShip::visible`).

The port's deviations: the source retries a full station for ever, the port
gives up after `DOCK_WAIT_SECONDS` and asks F762 for another station; the source
picks a random starport, the port walks them in order; F747's two-to-three days
are counted in game seconds, so the console's time acceleration shortens them;
and FFE's thrust units do not scale to the port's millimetre frame, so traders
and shuttles get `STATION_CRUISE_SPEED`/`STATION_ACCEL` for the approach.

### Firing at the player (F931/F930/F953)

- `NpcShip::fire(tics, player, player_radius)` is F931 for an AI ship: it
  returns unless the ship is hostile, has a front laser, `+0xe8` (refire) is
  clear, `+0xea` (heat) is at most `0xf000` and `+0x11e` (energy) is non-zero.
  The shot then spends F930's heat/energy/refire exactly as the player's does,
  and the ray is tested against the player's interaction box (`F817/F819`,
  `laser::hit`) along the ship's forward axis from its centre.
- The damage is F930's (`50` for a 1MW pulse, `250` for 5MW, ...) and the
  caller runs **F953** on the player through `Flight::take_damage`: shields
  first (`+0xe0` against `+0xe2`), then hull (`+0xe4`, `mass * 4`). The first
  hit records the hull's full value, so a save from before the damage model
  still shows F1099's line. A 1MW pulse takes two hits to destroy a 25 t hull
  with no shields, exactly the F953/F942 arithmetic.
- On destruction the port recovers the player at the nearest starport or
  station with a fresh hull (`playership_destroyed`) and says so. The
  original runs its game-over/escape-pod sequence (F942 removes the player
  object and F924 notifies the flight state); that screen is not ported.
- `Navigator::tick_ships` returns `ShipEvent`s: `Hit` (name, code, damage,
  beam), `PoliceHostile` and `Escaped`. `main` turns a hit into the damage,
  the message and the recovery.
- **JJFFE difference:** JJFFE's F724 only sets `+0x151` bit 1 and does not
  call F931, so JJFFE's AI ships do not damage the player (its "Laser
  invulnerability" fix). The port follows DOS FFE.

### Police and wanted (the reconstruction)

- `Navigator::legal` is `npc::Legal`, the port's counterpart of `DATA_008646`
  bit 0 (`wanted`), `DATA_008647` (`alert`), `DATA_008860` bit 2
  (`illegal_fire`), the offence count and F236's six-starport fine ledger
  (`DATA_009095`). `docs/notes/legal.md` has the full source map.
- `Navigator::player_attack(id, damage)` is the player's laser path: it runs
  F953/F942, pays F942's pirate bounty, and if the ship is *clean* (trader,
  shuttle, police or military) sets `wanted` and increments `offences`.
  Pirates and Thargoids are fair game.
- `Navigator::player_fired(at)` is F931/F941's player branch: the first shot in
  a system (F931's `DATA_008860` bit 2) reports the unlawful-discharge offence
  through F237(ship, 3). F236 records the fine and wanted bit only with a
  nearby starport (F234/F235; `starport_in_range` uses F1680's exponential
  distance byte and the source's `0x13` threshold). The message is F236's exact
  `DATA_005040` text and the fine is a per-starport debt (ten times the printed
  amount), not an immediate cash deduction. A recorded offence provokes every
  police and military ship (`F238`'s response) and sets `alert`.
- `F783` clears `wanted`/`alert` when a police object (`+0x118 == 0xe`) is
  removed; `tick_ships` emits `ShipEvent::AlertCleared`. It does not clear the
  illegal-fire flag.
- The port does **not** spawn extra police; F688's police count is the only
  source. Fines are wired into F655's docking refusal (`>= 0x2710`), but the
  fines office (`F366`) and the F2 legal-status screen are not ported. There is
  no flight HUD badge in the source.

### Death, recycle and persistence (F674/F680/F927)

- F942's destruction (`NpcShip::hit` -> `Damage::Destroyed`) sets `Dead` and a
  0.6 s marker; F927's recycle removes it when the marker or the
  `DESPAWN_RANGE` (30e6) test says so. `Escaped` ships are removed at once.
- **F675** keeps the hostile ships within `PERSIST_RANGE = 2.0e6` of the
  player (the original's class-0xfb pirate and escape-pod rule; the port has
  no pods) as `npc::Persisted` records with their offset from the player.
  `Navigator::jump` fills that list, and `spawn_traffic` restores it (F680)
  before F688 spawns the new system's traffic. The restored ship keeps its
  model, damage, guns and bounty.
- The original's slot allocator is 0x72 down with 0x14 reserved; the port's
  `MAX_SHIPS = 5` cap is a legibility choice, not F927's limit.

### Drawing, labels and targeting

- `Screen::ships` (`SystemShip`) draws each live ship with the verified model
  executor at its position and axes, farthest first.
- A shield hit sets `NpcShip::flash` and draws F1536's model **0x9d (157)** at
  the hit ship with the **ship's** render scale (F1536 copies the ship's
  `+0x7e`). The model is a shield bubble; the original replaces the ship with
  it for the frame, the port draws it over the ship for 0.25 s so it is
  visible.
- A kill draws F942's model **0x9b (155)** at the ship with its random
  orientation and F926's model-0x99 debris pods (ported in
  `ffe_core::effects`, `docs/notes/combat.md` §7.1); the ship itself draws no
  model and its slot is recycled after `DEATH_SECONDS` (0.6 s), like F927.
  Model 155's twelve fragment calls are gated on object global word 4
  (`+0xa4`): F942 writes `0xffffffff`, which selects none, so the port passes
  FFP 1.0 (`0x4000`) to draw the fragment set. F1560/F1613's span fill is
  ported and fills their F1721 polygons (`docs/notes/fills.md`); the remaining
  line cloud is model 153's two-point face records.
- The labels toggle (`l`) includes NPC ships at their `+0x124` name and the
  camera position; `F989`'s bounds and `F1108`'s font/colour are unchanged.
- A click on a ship (or a laser hit) locks it (`Navigator::ship_lock`, a stable
  id), clears a body lock, and shows F1100's range plus F1099's
  hull/shields/shield-strength text. `T` cycles ships when a ship is locked,
  bodies otherwise; the autopilot refuses a ship target (it has no standoff
  or docking face).
- The laser path (`main`) includes live ships after the bodies in
  `laser::hit`; a hit calls `Navigator::damage_ship_at` and therefore F939's
  particle, F953, F942, F926's debris, the crime check and the bounty.

## 3. Tests and frames

- `cargo test -p ffe-core npc`: twenty-four tests. The original sixteen cover the
  F635 table/counts, the allegiance shift, F922's determinism, F688 counts/cap,
  placement, the `DATA_006255` rows, F585's turn divisor, F601's thrust/mass
  speeds, the F697 bounty table, F931's gates and F930's damage, the braking
  standoff, the transit arrival, F694/F695 escorts, F675/F680 persistence,
  F953/F942 damage and the hold/attack motion. This slice adds the traffic
  cycle: the bay request/approach/dock with the bay occupied, F747's 2-3 day
  interval, the door-open launch and F651 release with the next station, the
  full-station give-up (F655's Full and F762), and the F867 clearance timeout
  recycling the ship. The Thargoid tests check F690's `RND(F35(0x16))` limit and
  `DATA_006197` models, `Role::Thargoid`'s type/class/hostility, F774's
  class/cargo laser choice (203 t -> 0x20MW beam, 13,000 t -> 0xcf), and the
  model shield/hull/drive words. `cargo test -p ffe-core equipment` checks the
  Thargoid Laser's equipment record (0xd5, 50 t, tech 255).
- `cargo test -p ffe-rs combat`: adds the player's F953 words
  (`Flight::take_damage`) and their save defaults.
- `cargo test -p ffe-rs --test npc_ai -- --ignored` with
  `FFE_WORLD_CACHE=<data.bin>`: the scripted sequence. It puts the player in
  front of a pirate, ticks until a real `ShipEvent::Hit` arrives, applies it to
  the player's hull, asserts the 1MW/50-damage arithmetic and the second hit's
  destruction; kills a clean trader and asserts `legal.wanted`, the police
  `provoke` and their `Attack` state; and carries a pirate through a
  `Navigator::jump`/`spawn_traffic` round trip. Set `FFE_SHOT_DIR` to keep
  `npc-ai-before.ppm`, `npc-ai-hit.ppm` (the pirate labelled, `Hull: 50.0%`),
  `npc-ai-police.ppm` and `npc-ai-restored.ppm`.
- `cargo test -p ffe-rs --test npc_docking -- --ignored` with the same cache:
  the station cycle in the real system (Ross 154). It waits for the spawned
  trader's final approach, asserts F655 granted a bay, the pad contact leaves
  the bay occupied and starts a 2.40-day interval, fast-forwards the interval,
  and runs the launch through F654, F865 and F651 until the ship is back in
  transit to the next station. Set `FFE_SHOT_DIR` to keep
  `npc-dock-approach.ppm`, `npc-dock-docked.ppm` and `npc-dock-launch.ppm`; the
  ignored run prints the source steps
  (`F745 ... F655 granted`, `F652 touch ... F747 waits 57.6 h`, `F734/F654`,
  `F735/F865 ... F651 frees bay 0`, `F761/F762`). The logged approach is
  128.7 s at `STATION_CRUISE_SPEED` (40,000 mm/s). The PNGs and the full log are
  in `docs/evidence/npc-docking/` (README.md).
- `cargo test -p ffe-rs --test thargoid_view -- --ignored` with the same cache:
  `Navigator::spawn_thargoid_traffic(..., 12)` runs F690 with `F35(0x16) = 12`
  (F688 draws `RND(12)`; this checkpoint's seed produces one);
  the test then asserts model 0x31's original statistics (232 t hull, 16,430
  shield word, 14,715 thrust, drive 14, one mount, 203 t cargo, 90,000 mm
  radius, 928 hull, 184 shields) and F774's 20MW-beam pick, pins the model for
  a stable frame, and fires the new-game 1MW pulse at 60 Hz. Four shield hits
  (184 -> 0) and nineteen hull hits (928 -> 0) kill it in 23 shots. Set
  `FFE_SHOT_DIR` to keep `thargoid-before.ppm`,
  `thargoid-hit-shields.ppm` (shield bubble, 72.8 %), `thargoid-hit-hull.ppm`
  (98.2 % hull), `thargoid-kill.ppm` (0.0 % hull/shields) and
  `thargoid-death.ppm` (F942's model-0x9b explosion); the PNGs and README are
  in `docs/evidence/thargoids/`.
- **Live frames:** `ffe-rs.exe --root <repo> --skip-intro --capture-traffic
  <dir>` shows the traffic, the locked `Moray Starboat 3` label and readout, the
  shield-bubble flash and the death frame from the earlier slice. Since the AI
  slice the same mode also shows a real AI hit: frame 10 carries the message
  `Moray Starboat 3 hits with its pulse laser. Hull 50.0%.` and the player's
  hull line at 50 % (`docs/evidence/npc-ai/live-traffic-10.png`).
- The hit/damage path has live ship targets and a real AI shot; the target
  readout draw path has its synthetic frame test.

## 4. What remains

- **The full per-state AI.** The port runs the flight cycle, the pirate states,
  the station cycle above and the jump states; the missile control rows
  (0x13-0x1b), F770's trading approach's own motion and the per-state exact
  F597/F1512 frame matching are not transcribed. F745/F746's orientation
  matching (F589) and F671's settle onto the pad are approximated by the port's
  approach; the station traffic uses the pad point, not the original's
  orientation lock.
- **Missiles.** F941/F1096 and states 0x12-0x1b are not ported; the Eagle's
  two 0x82 missiles are inert.
- **Police response/spawns.** The wanted state turns existing police hostile;
  it does not spawn patrols, apply fines or show the wanted status outside the
  message line. F1035/F1036 are surface spawns and are absent with the rest of
  the surface traffic.
- **Station-bay ships (F691).** The cache's ship-data table is not loaded, so
  the initial docked traders are not spawned; the traffic cycle starts from
  F689's arriving traders instead.
- **Exact F702 placement.** The raw ffp orbit is not used; see the deviation.
- **F942's effects.** Ported 2026-10-25: the model-0x9b destruction object and
  F926's debris pods are in `ffe_core::effects` and drawn by `Screen`; F939's
  hit particle is drawn at the hit point. The twelve fragment shapes fill with
  the ported F1560/F1613 span rasteriser. F942's messages, bounty and
  escape-pod variants are not ported.
- **Sound.** F953/F942's samples are not played (the audio layer is synthetic),
  though the sound cues from these paths are wired.
- **Thargoids.** F690's rule, the type-0x11 model list and the role are
  implemented, but the count is `RND(F35(0x16))`, whose limit only a mission
  script writes; no ported system sets it, so ordinary traffic has none. The
  source has no military/witchspace/random gate for F690 beyond that draw. The model statistics'
  0x800e drive word maps to drive class 14 and `DATA_006266[14] = 0`, so the
  port's Thargoids (like the source's) have no laser energy until a loadout
  gives them some; they still chase and can be hit/killed. Their model names
  are the original's (`Unknown`, `Transporter`, `Lion Transport`, `Tiercel
  Freighter`, `Thargoid Transporter`), so the label is not literally
  "Thargoid".
- **Persistence of the player's own damage** is in `Flight`'s serde words; the
  NPC traffic itself is not saved (it is regenerated on entry, as F680 does).
- **The original's deaths.** The player's destruction is a starport recovery,
  not F924's game-over/pod sequence.
- **The target readout** is only for locked ships and uses the port's output
  path. The combat targeter is drawn (F1113/F1114) and its arming is now wired
  to the fitted Auto targetter (`docs/notes/combat.md` §8.2); with the item
  fitted, `Shift+T` arms the crosshair acquisition that sets the missile lock.
  The new-game Eagle still has no Auto targetter, so `Shift+T` keeps the port's
  older lock toggle until one is bought.

## 5. Owner decisions

- **Thargoid trigger.** F690's count is `RND(F35(0x16))`, a mission-script
  global limit; the port has no mission scripts, so ordinary traffic is zero and
  the `Navigator::spawn_thargoid_traffic` hook is the only caller that can set
  it. Decide whether to port the mission bytecode/scripts that write 0x16,
  expose a developer/system override, or leave Thargoids to scripted/evidence
  runs. Do not add a government/tech/allegiance rule: none exists in F690.
- **Thargoid armament and energy.** The port follows F774's class/cargo
  selection, so the warships carry the 20MW beam and the transporter's 0xcf is
  mountless. The model data's 0x800e drive word gives drive class 14 and
  `DATA_006266[14] = 0`; the port therefore starts them with no laser energy.
  Decide whether to give mission Thargoids the source's `+0x119` fuel tokens,
  a shield/energy loadout, or leave them chase-only. The player-fitted
  **Thargoid Laser 0xd5** is an equipment record, not an F774 entry; decide
  whether the shop item should map to an AI laser code.
- **Thargoid labels.** The original model-name table names 0x31 "Unknown" and
  0x32-0x33/0x3f "Transporter"/"Lion Transport"/"Tiercel Freighter"; the port
  keeps those names. Decide whether the readout should say "Thargoid" for
  class 0x11 instead of inventing a name change in the cache.
- **Player damage model.** The port runs F953 against the player's `Flight`
  shields/hull words; the new-game Eagle has no shield generator
  (`+0xe2 = 0`), so a pirate's 1MW pulse kills it in two hits. Decide whether
  the new game should carry a shield generator (equipment) or whether AI
  damage should be scaled to keep the sandbox playable.
- **Police response.** The reconstruction makes a wanted player hostile to
  existing police but does not spawn more. Decide whether to port F1035/F1036's
  surface spawns and the station/police alert path, or keep the message-only
  rule.
- **Fleeing trigger.** The 25 % hull jump-out is the port's; the original has
  the states but no trigger. Keep it, remove it, or replace it with the
  original's position-based F737 condition.
- **Traffic cap.** Keep `MAX_SHIPS = 5`, or let F688's counts and F927's slot
  rules produce the original's many more ships.
- **Full docking AI.** Now ported at the state level (F745/F746, F747/F733,
  F734/F735, F651/F736, F761/F762). The open decisions are the port's
  trade-offs: a 2-3 day docked interval counted in game seconds (so time
  acceleration is needed to see a launch), a 30 s give-up at a full station,
  stations chosen in order rather than at random, and the
  `STATION_CRUISE_SPEED` floor. Keep these, or use the original's for-ever
  retry/random choice and accept that a heavy freighter's approach takes tens of
  minutes at 1x.
- **Effect lifetimes.** The port gives each effect its own seconds; decide
  whether to follow the original's AI timers (F764/F769/F927).
- **Explosion fragments.** Model 155 is gated on the object's global word 4
  (`+0xa4`); F942 writes `0xffffffff`, which selects no fragments, so the port
  deliberately passes `EXPLOSION_MASS` 0x4000 to draw the twelve. Keep that
  deviation or follow F942 literally (no fragments); the shapes also need
  F1560/F1613's fill before they are solid.
