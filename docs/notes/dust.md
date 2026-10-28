# Space dust: F1035 spawn, F769 lifetime, F715 movement

Implemented 2026-10-26 in `crates/ffe-core/src/dust.rs` (`dust::Field`), wired
into the playable in `ffe_rs::navigation` (`Navigator::dust`, `tick_dust`,
`dust_seed`) and `ffe_rs::main`'s flight frame. **Rendering is not part of this
slice**: `screen.rs` is untouched and no dust pixel is drawn yet. Read from
JJFFE 2.8a7 `ffe.asm`/`ffedat.asm`/`ffedat.lst`; FFED3D's copies of F715/F769/
F1035 are identical.

## 1. The AI table row

`DATA_006258` (`ffedat.asm:9862`; the port's `npc::AI_STATES`) row `0x1b` is
`space dust`, four routine slots:

| slot | routine | source | what it does |
|---|---|---|---|
| control (event 6) | F769 | `ffe.asm:50379` | ages the object and removes it |
| movement (event 5) | F715 | `ffe.asm:47368` | **empty** (`push ebp; mov ebp,esp; pop ebp; ret`) |
| notify (another death) | — | `db 0` | none |
| damage | — | `db 0` | none |

The empty movement slot is the source's own: dust never integrates `+0x8c`.
FFED3D's `ffe.asm:48739` is the same empty routine, so this is not a
decompilation loss. The visible streaming comes from the player's frame moving
past a static object (F1678), not from a dust velocity. The inherited velocity
is used once, at spawn, to place the particle ahead (F1517).

## 2. Spawn: F1035 (`ffe.asm:76306`)

F1035 is called once per flight frame from the flight module's frame handler
(F1019, state `0x2`, `ffe.asm:75178`). Its order:

1. `cmp dword [DATA_008867], 0x1312d00 / jnl` — at **20,000,000 mm (20 km) or
   above** it takes the space branch (F1036 plus dust); below it calls F1015 →
   F1016 → F1017, which are empty in both JJFFE and FFED3D. The port refuses
   below 20 km with `Refusal::BelowAltitude`.
2. `cmp byte [DATA_009018], 0 / jz return` — **the detail preference**.
   `DATA_009018` is the player block's `+0x3f51` byte (`ffedat.lst`:
   `DATA_008804 = 0x38a48`, `DATA_009018 = 0x3c999`, difference `0x3f51`), the
   first "Other Options" row, `DATA_004966`'s `Space dust, clouds etc`
   (`docs/notes/prefs.md`). `FUNC_000219_SysMenuInit` copies the defaults
   `DATA_004895` into `+0x3f4c..+0x3f7d`; the dust byte defaults to `0xff`
   (on). The toggle is `FUNC_000216`'s `JUMP_001362`, `xor byte
   [DATA_009018],0xff`. The port maps it to `prefs.others[0]` and
   `prefs::other_applied(0)` now returns true.
3. `cmp byte [DATA_008630], 0 / jnl return` — the flight module's active byte
   (`ffedat.lst:27749`: "0xff ship module active, 0x7f inactive"; F1003 sets
   `0xff`, F1006 `0x7f`). The port only ticks dust inside the flight branch,
   so the `Spawn::flight_active` flag is true there.
   The "Level of Shape Detail" radio (`+0x3f4c..+0x3f4e`, F228) is **not** a
   spawn gate: F228 only changes the model-drawing constants at
   `player+0x10..0x18`, so it belongs to the follow-up draw (the model's LOD),
   not to F1035's lifecycle.
4. `F1530() & 0xffff < 0x3332` — **one attempt in five**
   (13106 / 65536 = 0.1999) for a single particle. This is a per-frame rule,
   not a count: the source's timer caps the loop at `tmMinFrame` = 20 ms
   (`F1418_TimerAdvance`, 50 fps), so the port accumulates the presented frame
   time and makes one attempt per elapsed 20 ms (`FRAME_SECONDS`). A 60 Hz or
   144 Hz presentation cannot multiply the attempts.
5. `cmp dword [DATA_007706], 0 / jz return` — time is running. F217 sets the
   1x play value `0x3e2`; F229 (SysMenu), F120 (video) and F207 set 0. The
   draw at step 4 is consumed **before** this test, so a paused frame still
   advances the dust stream; the port keeps that order.
6. **F928** (`ffe.asm:69259`) allocates a slot (flags `0x2a`, model `0x6d` =
   109) and **F1533** (`ffe.asm:90970`) clones the player's whole object into
   it. The spawn then writes `+0xb0 = 0` (age), `+0x57 = 0` (not a rotating
   frame), `+0x87 = 0xb` (AI craft), `+0xff = 0x1b` (this row).
7. **F1675** (`ffe.asm:101537`, `MatBuildYZT`) builds the random YZT
   orientation from `tilt = F1530()` and `spin = DATA_008820 & 0xffff`. The
   port stores the two angles and builds the same matrix with
   `model_animation::build_yzt` (the existing verified F1675) when a sine
   table is available.
8. **F1517** (`ffe.asm:90632`) scales the inherited `+0x8c` velocity by
   `DATA_009123 + 7` into a 64-bit vector and **Vec64Add** (`ffe.asm:100373`)
   adds it to `+0x3e`: the particle appears ahead of the player along its own
   velocity. `DATA_009123` is the time acceleration's shift: F208 sets
   `DATA_009013` to 0, 3, 7, 10 or 12 for 1x..10000x (`ffe.asm:17137-17184`)
   and F207 copies it across with `DATA_007706`, so the dust is led the same
   real time ahead (about 1.3 s) at any acceleration
   (`dust::ACCELERATION_SHIFT`, passed by the flight frame as `Spawn::speed`).
   F208's hyperspace branch sets `0x12` with `DATA_009012 = 0xf880000`; the
   port's instant jump does not run it. The first slice read `DATA_009123` as
   always 0 in flight, so under acceleration the dust was led only a 1x
   distance and kept 1x-sized clouds, and almost every cloud was culled as too
   far away: none showed at 1000x.
9. The distance word `+0x7e` is
   `(F1530() & 3) + DATA_008865 + DATA_009123 - 7`: under acceleration each
   cloud is `2^shift` times larger (at 1000x, 0.5..4 km becomes 0.5..4,000 km).
   `DATA_008865` has **no
   writer** in JJFFE 2.8a7 (it follows the mouse accumulators
   `DATA_008863/864`), so it is 0; the port keeps that as `DISTANCE_BIAS`.

The port's field caps at `MAX_PARTICLES = 0x5f` (95): F928 never allocates
below slot `0x14`, so the list cannot hold more dust objects. At 50 fps the
expected live count is `0.2 * 50 * 4 s = 40`, well inside the cap; a full field
drops the oldest, matching F928's recycle of a flagged `0x20` object.

The `main` wiring passes `nav.altitude`, the same F1033 value F908 uses; the
flight step sets it to `i32::MAX` when it reports no contact, so the gate reads
deep space whenever no body is being tracked. The altitude source itself is not
changed by this slice.

## 3. Lifetime: F769 (`ffe.asm:50379`)

```
cmp byte [eax+0x8b],0 / jnz destroy      ; +0x8b hidden
cmp byte [eax+0x88],0x14 / jg destroy    ; F1680 distance class
mov edx,[DATA_009045] / shl edx,8
add [eax+0xb0],edx                       ; age += days << 8
mov edx,[eax+0xb0]
cmp edx,[DATA_007706] / jc keep
destroy: F940
```

`DATA_009045` is the event-6 time F151 receives from F152: the number of whole
game days since the last day marker (`DATA_008804 - DATA_008808) / 0xc22e`,
`ffe.asm:15202`). At 1x `DATA_008804` grows by `0x3e2` (994) per 20 ms frame
(`F1418`: `elapsed_ms * 0x3e2 / 20`), so one day passes per second and F769
adds `0x100` each second. The limit `DATA_007706` is the same acceleration
value (`0x3e2` at 1x), so at 10x both the tick rate and the limit are ten times
larger and the lifetime in **real** time is unchanged:

- 1x: age `0x100, 0x200, 0x300` on the first three seconds; the fourth reaches
  `0x400 >= 0x3e2` and the particle is removed. **Four seconds.**
- 10x: the limit is `0x26d6` (9942), day ticks are 0.1 s apart, and the 39th
  tick removes it (3.9 s). The quantisation is the source's.

The port therefore ticks this counter in real seconds (`Field::tick`), with the
day marker in integer microseconds so a 20 ms frame is exact. `F769`'s second
condition is F1680's distance class (`ffe.asm:101877`):
`max(0, msb(largest |relative coordinate|) - 0xd) - object[+0x7e]`, culled above
`0x14`. The MSB is F1657 (`ffe.asm:100097`). The source computes it from the
camera-frame relative position; the port uses the system-frame delta (an
orthogonal transform can change the largest-component MSB by at most one
class), because the renderer's camera is the follow-up. The `+0x8b` hidden flag
and `+0x88` cull are both checked every tick; the source checks them on its
day tick.

## 4. Movement

There is no per-frame dust movement. F715 is empty. The two motions that exist:

1. **Spawn placement** (F1517): `position += velocity << 7`, the player's
   velocity scaled by 128, so the particle starts ahead of the ship.
2. **The player's own motion**: the object is static in the system frame; as
   the camera moves, the dust passes it. This is what the future draw must
   show; `Particle::position` is absolute millimetres.

Do not "fix" F715 by integrating the velocity: that would make dust travel with
the player and remove the streaming entirely.

## 5. The port's field and determinism

`dust::Field` owns a `npc::Random` stream seeded from the system code
(`navigation::dust_seed`), separate from the NPC traffic's F922/F1530 stream
(EXP-020). The same system therefore always produces the same dust regardless
of what traffic or combat draws happened first; loading a save or replaying a
capture is deterministic. The field is reset on a jump (F674/F909 clear the
system's objects) and is not saved.

`dust::Field::advance(elapsed, spawn)` is the per-frame entry point:
one `attempt` per 20 ms of F1035, then `tick` (F769). `attempt` returns a
`Refusal` naming the gate, which is what the synthetic tests assert.

## 6. What remains

- **Rendering** (the follow-up): iterate `Navigator::dust.particles()`, build
  each orientation with `Particle::orientation(cache sine table)`, project the
  absolute position through the camera and draw model `0x6d`'s dot/point. No
  `screen.rs` change is in this slice, so the playable's picture is unchanged.
- F1036's separate asteroid/debris spawn (10% of frames, parent `0x72`, the
  `DATA_007268` model table) is a different object and was not implemented.
- `DATA_009123`'s `0x12` hyperspace-exit value: the port's jump is instant
  (EXP-007), so there is no exit effect to feed it.
- The inherited parent byte `+0x56` is not stored: all port positions are
  absolute, which is equivalent for a draw that subtracts the camera.
- The 20 ms attempt cadence is the source's frame cap, not a new rule; see
  EXP-020.

## 7. Synthetic tests (cache-free)

`cargo test -p ffe-core --lib dust` runs eleven unit tests in `dust.rs`:

- `the_ai_table_row_is_the_dust_row` — `0x1b` is control F769 / movement F715.
- `the_detail_and_module_gates_come_before_the_draw` — altitude, preference and
  flight-module gates consume no draw; the stopped-time gate is after the draw.
- `the_spawn_is_one_draw_in_five_at_the_original_frame_cadence` — 10,000
  attempts with a fixed seed give 1,950-2,050 particles (0.1999).
- `one_frame_attempts_at_most_once_per_twenty_milliseconds` — 20 ms = one,
  100 ms = five, 40 ms = two.
- `the_lifetime_is_four_whole_day_ticks` — age `0x100` per second, removal on
  the fourth; half-second frames do not age it.
- `dust_holds_its_position_because_f715_is_empty` — the `velocity << 7` spawn
  offset and no drift afterwards.
- `the_random_orientation_is_f1675` — the stored angles rebuild a unit-row
  YZT matrix with `build_yzt`.
- `the_distance_class_is_f1680_and_culls_at_0x14` — F1680/F1657 values, the
  cull, and the `+0x8b` removal.
- `two_fields_with_one_seed_spawn_the_same_dust` — determinism.
- `a_full_field_recycles_its_oldest_particle` — the 95-slot cap.
- `a_reset_clears_the_system_and_reseeds` — jump/entry reset.

`crates/ffe-rs/tests/prefs.rs`'s `others_page_applies_stars_and_reverse_and_keeps_the_rest_inert`
now asserts that row 0 toggles (it was inert before this slice).

## Rendering scale (2026-10-27)

The playable draws each particle as model `0x6d` through the effect path with
`+0x7e` applied as a shift on the model's own scale (as for F939's sparks):
`scale = model.scale + distance_bias`, so -7..-4. Model 0x6d's radius is 65 km
at its own scale (opcode-24 F1766 chains behind three opcode-12 branches), so
a cloud is 0.5..4 km across. The first wiring passed no scale, which drew
every cloud 16..128 times too large: the camera sat inside it and almost no
dot reached the screen. `effects_view::dust_is_drawn_ahead_at_speed` checks
the dots are drawn at 5 km/s, and `dust_keeps_up_under_time_acceleration` that
at 1000x they are drawn and spread across the view (before the shift was
passed, none were). At 1x and high speed the clouds gather round the middle of
the view: each starts on the line of flight about 1.3 s ahead, as in the source.

## The modern engine's speed dust (2026-10-28)

With the modern engine (a GPU) the original's clouds, which all start on the
line of flight and spread out from one point, are replaced by speed dust drawn
by the GPU (`Screen::speed_dust`, `modern3d::dust`). Presentation only: its own
SplitMix64 hash of the place, never the game's random numbers, and nothing
about the dust reaches the simulation. Classic keeps the original's clouds.

- Motes are fixed in the ship's own frame all round it (the travel is the
  ship's velocity times real time times the acceleration; paused, nothing
  moves), at most one in each place of a grid, 80% of places filled, up to
  five spacings each way.
- Six layers, each eight times as far apart as the last (97 m to about
  3,200 km). A layer shows at the speeds where it streams past well (fading in
  from half a spacing a second, full from 4 to 32, gone by 256), so the layers
  hand over smoothly from a few hundred m/s to 10000x time, and motes fade out
  towards the edge of their layer.
- Each mote is a streak from where it is to where it was four frames' travel
  ago (at most three spacings): at speed the streaks rush out from the way the
  ship is going, like a starship's warp. Blue-white, brightest at the mote and
  fading along the trail, wider the nearer it is; past the top of a layer's
  band the head runs over white and blooms.
- The "Space dust, clouds etc" option turns it off; there is none inside a
  planet's haze. `tests/modern_planet.rs`,
  `speed_dust_streams_out_from_the_way_ahead`, checks the streaks point back to
  the way ahead at 300 m/s, 5 km/s and 5 km/s at 1000x, and that paused motes
  are points.
