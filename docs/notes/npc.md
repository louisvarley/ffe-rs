# NPC ships in generated systems (F688, F635, F942, F1536)

Implemented 2026-10-25 in `crates/ffe-core/src/npc.rs`, with the playable
wiring in `ffe_rs::navigation` (`Navigator::ships`, `spawn_traffic`,
`tick_ships`, `lock_ship`, `damage_ship`) and `ffe_rs::screen`
(`SystemShip`, `draw_system_ships`). Read from JJFFE 2.8a7 `ffe.asm` and
`ffedat.asm`; the DOS release's `FIRSTENC.EXE` has the same tables.

The ships exist so the ported combat path (`docs/notes/combat.md`) has live
ship targets: F953's shields/hull damage, F942's destruction, F1536's
model-0x9d shield flash and F1099's target readout are now reachable in a
generated system instead of only in synthetic tests.

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
  - `RND(+8) × F689`: traders (`F772(1)` → F635 type 1 + `2*DATA_008996`);
  - `F35(0x16) × F690`: the type-0x11 branch (Thargoids; `F35` is not ported);
  - `DATA_008997` × F691: ships parked in station bays (the cache's ship-data
    table is not loaded, so the port has none);
  - `RND(+8) × F699`: shuttles (`F772(0xf)`);
  - `+6` patrol groups × F698, ×4 in-system: F694/F695/F696 make a lone pirate
    or a group of three around the player;
  - `RND(+6) × F692`: a no-op.
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
- **F674/F680** (`ffe.asm:45338`, `45536`) clear and restore the AI ships when
  leaving/entering a system; **F927** recycles a slot. The port clears on
  hyperspace and recycles dead/far ships in `tick_ships`.
- **AI states** are `DATA_006258` (`ffedat.asm:9862`): four routine slots per
  state, dispatched by F778/F780/F777/F779 on object `+0xff`. F752/F725/F727
  are the common movement/physics steps; F740/F790/F789 are the station,
  depart and attack states; F694/F695 set pirate state 5/0xb. The port uses a
  traffic-level state machine, not these routines (below).
- **Hostility** is not a flag on the object: FFE's AI states and the comms
  code decide it (pirates are the 0xa/0xb groups; police are 0xe; F1036's
  asteroid table is unrelated). The port marks pirates and Thargoids hostile
  for the AI; it does not model police response, wanted status or ship fire.

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
- Each ship's `Spec` comes from the cache's model record and statistics:
  F1533's outer radius (`ModelInfo::radii().0`), the mass word and the shield
  word. Shield maximum is `(shield_strength & 0xff) * 4` (the original's
  equipment budget gives `random * 16`; this is the port's simplification),
  hull is `mass * 4`, and current shields start full (the original's F938
  recharge catches up in play).
- Names are the model-name table (cache v11) plus a serial, e.g.
  `Cobra Mk III 1`; the original's F703/F791 registration generator is not
  ported.

### Flight AI (`NpcShip::tick`)

Three states plus `Dead`:

| State | Behaviour | Original analogue |
|---|---|---|
| `Hold` | orbit the anchor at its spawn radius, turning at 1.2 rad/s | F689/F699 station traffic, F740/F790 waiting/depart states |
| `Cruise` | fly to the anchor (used only as a fallback) | F752/F725's common movement |
| `Attack` | turn and close on the player, stopping at 1.2 km | F694/F695 pirate state 0x5/0xb |
| `Dead` | death marker counts down, then the slot is recycled | F942/F927 |

Ships do **not** fire, dock, use missiles, or run the original's per-state
AI routines (F752/F725/F727, F753's arrival test, F1035's wanted-pirate
spawns). The motion is a constant-speed turn toward a target with an upright
basis; it is deterministic for a given clock, system and player position.

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
  FFP 1.0 (`0x4000`) to draw the fragment set.
- The labels toggle (`l`) includes NPC ships at their `+0x124` name and the
  camera position; `F989`'s bounds and `F1108`'s font/colour are unchanged.
- A click on a ship (or a laser hit) locks it (`Navigator::ship_lock`, a stable
  id), clears a body lock, and shows F1100's range plus F1099's
  hull/shields/shield-strength text. `T` cycles ships when a ship is locked,
  bodies otherwise; the autopilot refuses a ship target (it has no standoff
  or docking face).
- The laser path (`main`) includes live ships after the bodies in
  `laser::hit`; a hit calls `Navigator::damage_ship` and therefore F953's
  shields/hull order and F942's destruction. Messages name the ship and the
  outcome; F1536's flash is set on the shield branch.

## 3. Tests and frames

- `cargo test -p ffe-core npc`: six tests for the F635 table/counts, the
  allegiance shift, F922's bounded determinism, F688 counts/cap and placement,
  F953/F942 damage/flash/death, and the hold/attack motion.
- `cargo test -p ffe-rs --test npc_view -- --ignored` with
  `FFE_WORLD_CACHE=<data.bin>`: builds the new game, asserts traffic spawned
  and is capped, draws a frame with a ship label and target readout, draws the
  model-157 shield flash (pixel-different when the flash flag is off), draws
  a kill frame, fires a ray through `laser::hit`, and checks the death timer
  recycles the ship. Set `FFE_SHOT_DIR` to keep:
  `shots/npc-system.ppm`, `shots/npc-hit.ppm`, `shots/npc-kill.ppm`.
- **Live frames:** `ffe-rs.exe --root <repo> --skip-intro --capture-traffic
  <dir>` puts the player beside a spawned pirate, turns labels on, applies a
  50-point shield hit at frame 10 and a 1000-point kill at frame 20, and saves
  `traffic-00.ppm`, `traffic-10.ppm` and `traffic-20.ppm`. Local runs
  (2026-10-25) show the Ross 154 system traffic, the locked `LANNER` label and
  target brackets, the shield-bubble flash and the death frame. The frames
  show the scripted camera 0.2 km from the target.

## 4. What remains

- **Full AI.** F752/F725/F727, the per-state routines, arrival/docking,
  station launch and F1035's wanted-pirate spawns are not ported. Ships do
  not fire, and no damage reaches the player.
- **Police and hostility.** The police role is spawned as traffic but does not
  react to the player's crimes; wanted status, fines and police spawns
  (F1035/F1036) are absent.
- **Station-bay ships (F691).** `DATA_008997`/`DATA_008999` (the cache's
  ship-data table) is not loaded, so no ships appear in station bays and the
  0x11/0x1c states that leave a bay are not modelled.
- **Exact F702 placement.** The raw ffp orbit is not used; see the deviation.
- **F942's effects.** Ported 2026-10-25: the model-0x9b destruction object and
  F926's debris pods are in `ffe_core::effects` and drawn by `Screen`; the
  twelve fragment shapes render as outlines until the F1560/F1613 polygon fill
  is ported. F942's messages, bounty and escape-pod variants are not ported.
- **Sound.** F953/F942's samples are not played (the audio layer is
  synthetic).
- **Persistence.** Ships are regenerated for the current system; they are not
  saved, and a killed ship stays gone until hyperspace.
- **The target readout** is only for locked ships and uses the port's output
  path; the original's combat targeter (`DATA_007246/247`) is still absent.

## 5. Owner decisions

- Keep the port's station/player-based placement (reachable and legible) or
  port F702's exact system-wide orbit and the full AI transit.
- Decide whether NPC traffic should be capped at five; the original can spawn
  many more (F688's counts × F698 groups).
- Decide whether the port should model police response/wanted status and ship
  fire, and whether the player should have shields/hull (the new-game Eagle
  starts with `+0xe2 = 0` and no player damage model).
- Decide whether the explosion should keep the `0x4000` global word that
  selects model 155's twelve fragments or follow F942's literal `0xffffffff`
  (which selects none), and whether the effect lifetimes should follow the
  original's AI timers.
