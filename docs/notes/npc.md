# NPC ships in generated systems (F688, F635, DATA_006258, F931, F942)

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
  - `F35(0x16) Ã— F690`: the type-0x11 branch (Thargoids; `F35` is not ported);
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
- **AI states** are `DATA_006258` (`ffedat.asm:9862`): 0x25 states with four
  routine slots each, dispatched by F706/F704/F778/F780 on object `+0xff`.
  F752/F725/F727 are the common movement and physics steps; F585 applies a
  per-frame turn limited by model size (halved from model 0x35, quartered from
  0x3b); F587 computes the rates and F1512 aims with the target's velocity;
  F753's arrival test picks state 3 under 5 units or F749's reposition (4) over
  10; F694/F695 set the pirate states 5/0xb; F737/F787 are the jump-out states.
  F724 (state 1) is the only AI fire call: it moves and calls **F931**.
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
- F688's count fields (`Details::bytes[3]` for traders/shuttles,
  `bytes[1]` for patrol groups, `bytes[2]` for police) with a cap of
  `MAX_SHIPS = 5`; the port keeps at least one trader and one pirate group
  (the original can roll zero) so a fresh system always has something to
  label and shoot.
- **Placement:** traders, shuttles and police use a random phase on an orbit
  around the first station/starport (`Anchor`), radius
  `clamp(1.2 * station radius, 2.5e6, 0.75 * laser range)` original units.
  The pirate group appears `PIRATE_DISTANCE = 3.5e6` ahead of the player (plus
  a side offset), inside F931's `0x7fffff` hit range. This is the deliberate
  deviation from F702 described above; see "Owner decisions".
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
  combat roles get one front mount (pirates 1MW pulse, police/military 5MW
  pulse, Thargoids the Thargoid laser) and `+0x11e = DATA_006266[drive] << 24`
  ([`drive_energy`]). Traders and shuttles carry none. F773/F774's budget
  (equipment `+0x119`, spent on lasers then missiles) is not modelled.
- Names are the model-name table (cache v11) plus a serial, e.g.
  `Cobra Mk III 1`; the original's F703/F791 registration generator is not
  ported.

### The AI state table (`AI_STATES`)

`DATA_006258` is in `ffe_core::npc` as `AI_STATES: [AiState; 0x25]`, one row
per `object+0xff` value with the original's four routine numbers. The rows
name the approach/docking cycle (0x00-0x04), the pirate states (0x05, 0x0a,
0x0b), station holding (0x06/0x09/0x0d/0x0e/0x11), depart/jump
(0x07/0x0f/0x10/0x1f/0x20/0x21), missiles (0x12-0x1b), the explosion/debris
objects (0x1c/0x1d), dock/launch (0x1e/0x22/0x23) and F786's spawn-in
(0x24). `ai_state(code)` looks a row up and a unit test checks the whole table
against the disassembly.

The port's `State` names the subset it runs and lists its original codes in
`State::original`:

| Port state | Behaviour | Original rows |
|---|---|---|
| `Transit` | fly to the anchor, arrive test, then `Hold` | 0x00/0x01/0x07/0x08/0x12 |
| `Hold` | orbit the anchor | 0x06/0x0d/0x0e/0x11 |
| `Escort` | station-keep in the leader's frame; engage the player when near | 0x09/0x0b |
| `Attack` | turn and close on the player, fire, brake at the standoff | 0x05/0x0a |
| `Pass` | F597's "cannot stop in time" run-through | 0x03/0x04 |
| `Flee` | run and jump out | 0x10/0x1f/0x21 |
| `Dead` | F942's death marker counts down | 0x1c/0x1d |
| `Escaped` | the slot is free after the jump | 0x1f |

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

- `Navigator::legal` is `npc::Legal { wanted, offences }`, the port's
  counterpart of `DATA_008646` bit 0 and `DATA_008647`.
- `Navigator::player_attack(id, damage)` is the player's laser path: it runs
  F953/F942, pays F942's pirate bounty, and if the ship is *clean* (trader,
  shuttle, police or military) sets `wanted` and increments `offences`.
  Pirates and Thargoids are fair game.
- A wanted player makes every police and military ship `provoke()`: they set
  `hostile = true` and switch to `Attack` (`F783` clears the original's alert
  when the chase ends; the port's wanted state stays until a new game).
- The port does **not** spawn extra police; F688's police count is the only
  source. `DATA_008647`'s spawn/response path and the fines/wanted display are
  not ported.

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
  the hit ship with the **ship's** render scale. A kill draws F942's model
  **0x9b (155)** at the ship with the scale decremented; the port's executor
  queues no shapes for 0x9b, so the screen falls back to model 157 at ship
  scale + 2. `DEATH_SECONDS` then removes the slot.
- The labels toggle (`l`) includes NPC ships at their `+0x124` name; a click or
  a laser hit locks a ship (`ship_lock`) and shows F1100's range and F1099's
  hull/shields/shield-strength text. `T` cycles ships when one is locked; the
  autopilot refuses a ship target.
- The laser path (`main`) includes live ships after the bodies in
  `laser::hit`; a hit calls `Navigator::player_attack` and therefore F953,
  F942, the crime check and the bounty.

## 3. Tests and frames

- `cargo test -p ffe-core npc`: sixteen tests for the F635 table/counts, the
  allegiance shift, F922's determinism, F688 counts/cap, placement, the
  `DATA_006258` rows, F585's turn divisor, F601's thrust/mass speeds, the
  F697 bounty table, F931's gates and F930's damage, the braking standoff, the
  transit arrival, F694/F695 escorts, F675/F680 persistence, F953/F942 damage
  and the hold/attack motion.
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
- **Live frames:** `ffe-rs.exe --root <repo> --skip-intro --capture-traffic
  <dir>` shows the traffic, the locked `Moray Starboat 3` label and readout, the
  shield-bubble flash and the death frame from the earlier slice. Since the AI
  slice the same mode also shows a real AI hit: frame 10 carries the message
  `Moray Starboat 3 hits with its pulse laser. Hull 50.0%.` and the player's
  hull line at 50 % (`docs/evidence/npc-ai/live-traffic-10.png`).
- The hit/damage path has live ship targets and a real AI shot; the target
  readout draw path has its synthetic frame test.

## 4. What remains

- **The full per-state AI.** The port names all 0x25 rows but runs a subset.
  F752/F753's complete approach/docking cycle (including F750/F756's final
  approach and F597's frame matching), station launch/departure (0x1e-0x23),
  F740's docked countdown and F762's new-station choice are not ported, so
  traders hold an orbit rather than docking.
- **Missiles.** F941/F1096 and states 0x12-0x1b are not ported; the Eagle's
  two 0x82 missiles are inert.
- **Police response/spawns.** The wanted state turns existing police hostile;
  it does not spawn patrols, apply fines or show the wanted status outside the
  message line. F1035/F1036 are surface spawns and are absent with the rest of
  the surface traffic.
- **Station-bay ships (F691).** The cache's ship-data table is not loaded.
- **Exact F702 placement.** The raw ffp orbit is not used; see the deviation.
- **F942's effects and sound.** Model 0x9b's shapes are not drawn (157
  fallback), the particle curves and samples are absent.
- **Thargoids.** The F690 branch needs F35's random table; no Thargoid traffic
  is spawned, so the port's Thargoid laser/role is only reachable in tests.
- **Persistence of the player's own damage** is in `Flight`'s serde words; the
  NPC traffic itself is not saved (it is regenerated on entry, as F680 does).
- **The original's deaths.** The player's destruction is a starport recovery,
  not F924's game-over/pod sequence.

## 5. Owner decisions

- **Player damage model.** The port now runs F953 against the player's
  `Flight` shields/hull words; the new-game Eagle has no shield generator
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
- **Full docking AI.** Whether to port F752/F753/F750/F756/F740/F762 (so
  traffic really arrives and departs) or keep the orbit stand-in.
- **F942's model 0x9b** and the AI sound cues remain unported; decide whether
  the visibility state or a 0x9b-specific executor is worth it.
