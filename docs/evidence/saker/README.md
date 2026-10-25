# The default start: F998's type-0 Saker Mk III

Frames and values for `F147(0)` / `F998`'s default arm. The DOS boot's "any
other key" leaves the intro with this start; `--start 1` keeps the verified Ross
154 arm.

## Executed original

`original-default-f2.png` is the executed JJFFE reference's F2 ship-status page
after the default start (`--game-frame 400 0x3c`). The local probe build used
the DOS default key `0x0b` instead of the adapter's `0x02`; no reference source
is patched in the tree. It reads:

* **Ship Equipment Status - Saker III fighter** (model `0x1f`).
* **Drive System: Class 1 Hyperdrive** (`ship+0xd0 = 2`).
* **Hull Condition: 100.0%** (`ship+0xe4 = 112`, F1533's 28 t mass × 4).
* **Shield Generators: 0 unit(s)** (`ship+0xe2 = 0`).
* Scanner, Automatic Pilot, Atmospheric Shielding and Navigation computer
  (`ship+0xc8 = 0x40240020`).
* **Front Gun Mounting: 1MW Pulse Laser**, Rear Gun Mounting Empty
  (`ship+0xd2 = 0x88`, the other three mounts zero).
* **Cargo Space: 1 t**, Cargo: 0 t, Cabins: 0, Cash: 1000.0 (10,000 credit
  units).

`--state-combat 200` from the same default-arm probe prints, from frame 161:

```json
{"player":87,"model":31,"mounts":2,"lasers":[136,0,0,0],
 "missiles":[130,130,0,0,0,0],"drive":2,"shields":[0,0],
 "hull":112,"refire":0,"heat":0,"energy":536870912,"cash":10000}
```

So the type-0 record fits `0x88` in front and two `0x82` KL760 missiles, with no
shield generator or cabins, and `0x20000000` (1 t) of laser energy. The Saker's
statistics block is F1533's `DATA_002655`: 2 gun mounts, 28 t hull, 23 t cargo,
eye `1800/400`, front gun `2200/0`, rear `1900/-200`, missiles 2. The rear mount
is empty even though the hull has two mounts.

The free hold reconciles with F998's `0x10` subtraction and the fitted mass:
23 t - 16 t (`sub [DATA_008889],0x10`) - 6 t of fitted items (1MW laser 1,
two KL760 2, Scanner 1, Automatic Pilot 1, Atmospheric Shielding 1; the
Navigation computer is 0 t) = 1 t. The F2 page shows that free hold and the
port uses it as `navigation::DEFAULT_CARGO`.

## Port frames

`port-default-boot.png` is the first frame of the playable after the default
start (`ffe-rs --root . --skip-intro --capture <dir>`, frame `shot-0000.ppm`
scaled 2x by the window). It shows `Hull: 100.0%` (112/112), no shield line
(both words are zero), `Act:0.0 km/h`, `Alt:0 m`, the parent name `Hope`,
`Landed at Old Blackelk.` and the equipment lamps/missile icons. This is the
actual `main` boot path, not only a test fixture.

`port-saker-default.png` is written by
`cargo test -p ffe-rs --test dashboard_view -- --ignored
the_default_start_draws_the_sakers_hull_and_equipment_page` with
`FFE_WORLD_CACHE` and `FFE_SHOT_DIR` at the native 320x200. It is the new-game
`Flight` with `DEFAULT_SHIP = 0x1f`, `Combat::new_game()`,
`Loadout::new_game()` and `ensure_damage(112)`: the hull line, no shield line,
the two missile icons on F1120's pylons and no other missile pylon. The terrain
and starport are the port's current approximations (other streams own them);
the HUD and equipment policy is checked in `tests/dashboard_view.rs` and
`tests/combat_view.rs`.
