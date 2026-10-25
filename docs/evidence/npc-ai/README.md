# NPC AI evidence frames

From the cached sequence in `crates/ffe-rs/tests/npc_ai.rs` (2026-10-25). Rebuild
with:

```
FFE_WORLD_CACHE=<repo>/data.bin FFE_SHOT_DIR=<dir> \
  cargo test -p ffe-rs --test npc_ai -- --ignored
```

(`.ppm` frames are produced into `FFE_SHOT_DIR`; these PNGs are the converted
copies.)

- `before.png` — the player 80 km from F694's pirate, which is labelled
  (`Moray Starboat 3`) and already aimed at the player.
- `hit.png` — after one real `ShipEvent::Hit` (F931/F930, 50 damage): the HUD
  reads `Hull: 50.0%`, proving F953 reached the player's hull words.
- `police.png` — after the player destroyed a clean trader: `legal.wanted` is
  set and the police ship (labelled `Naja attack fighter 2`) has been provoked
  into `Attack`.
- `restored.png` — a pirate within F675's 2,000 km carried through a
  `Navigator::jump`/`spawn_traffic` round trip (F680), still hostile.
- `live-traffic-10.png` — the real game (`ffe-rs.exe --root <repo>
  --skip-intro --capture-traffic <dir>`, frame 10, 640×480 presented): the
  locked `Moray Starboat 3`, its F1099 readout, and the message log's
  `Moray Starboat 3 hits with its pulse laser. Hull 50.0%.` — an AI F931 shot
  through `main`'s F953 path, not the cached test's synthetic driver.

The frames are the port's logical output, not original capture frames.
