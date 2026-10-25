# Default start: Hope in Gateway

Frames and checks for the DOS default new game (F147(0)/F829 record 0):

* `original-default-start.png` — the executed JJFFE reference at frame 500 with
  the default start arm (the adapter's start key changed from `0x02` to `0x0b`
  in a local build; no reference source is patched in the tree). It shows the
  landed surface view: buildings/towers, horizon, crosshair, `Set:0.0 kmh⁻¹` /
  `Act:0.0 kmh⁻¹` at the left, `Alt:0 m` / `Hope` at the right and the console.
* `port-hope-landed.png` — `planet_view::new_game_hope_landed_surface_view`
  after `Navigator::new_game(&cache, 0)` (the same F1050/F1054 state).
* `port-hope-launch-0000.png` — the playable's first `--capture` frame after
  `L`/F381 starts F1013's climb; the landed HUD and message log are visible.
* `compare.png` — the original and the port side by side.

The port frame's terrain/starport are the port's current approximations (the
terrain/visuals streams own that); the state, HUD columns and strings are
checked in `crates/ffe-rs/tests/planet_view.rs` and `crates/ffe-rs/src/hud.rs`.

Source derivation (executed original):

* F830(`0x1aa51717`) = `Gateway`.
* The type-0 `--state` probe: `state 0x30`, player index 87, cash 10000, ship
  `0x1f`, and F855's generator words `seed 0x8d17f528`, `mass 0x0a21fe80`
  (170,000,000), `stars 0`, `population`/tech 230.
* F878 at those words and the 3250 clock: planet `Hope` (index 6), starport
  `Old Blackelk` (slot `0x6a`), `Old Curie` at `0x6b` and `Dublin Citadel` at
  `0x67`.

The raw captures come from `jjffe-smoke --game-frame 500 0` (default arm) and
the Rust tests with `FFE_WORLD_CACHE`/`FFE_SHOT_DIR`.
