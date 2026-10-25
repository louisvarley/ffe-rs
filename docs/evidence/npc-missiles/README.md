# NPC missiles and countermeasures

Source: JJFFE 2.8a7 `ffe.asm` F773 (`50526`), F754 (`49779`), F757
(`49988`), F935 (`69987`), F941's evasion branch (`70399`), F1054's chaff
handler (`78204`, the `c` jump at `78290`), F1057 (`78789`), F718
(`47435`), F720/F721 and F953.

Reproduce with:

```powershell
$env:FFE_WORLD_CACHE = (Resolve-Path data.bin)
$env:FFE_SHOT_DIR = (Resolve-Path docs/evidence/npc-missiles)
cargo test -p ffe-rs --test npc_missiles -- --ignored --nocapture
```

The scripted encounter uses the new game's Ross 154 traffic capture:

1. A pirate is placed in F754's band (1,000 km) and given KL760 (`0x82`)
   pylons. F754's roll launches one at `player+0xfe`; F941 clears the
   pylon and the port adds a model-6 object to `MissileField`.
   `npc-missile-launch.png` is the camera 40 m behind that missile.
2. The player fits the Chaff Dispenser (general bit 11) and carries one tonne
   of the Chaff good (`player+0x174` = hold[32]). `deploy_chaff` spawns the
   model-0xa4 object and F1057 retargets the missile to it. The missile's
   `+0xfe` is the decoy, and F718/F721 detonates it there without touching the
   player. `npc-missile-chaff.png` looks at the decoy from 100 m.
3. A second NPC missile hits the player: F720/F721/F953's 200 damage is
   absorbed by a 400-point shield, leaving 200. `npc-missile-hit.png` is the
   frame after the hit.
4. The player launches a missile at the pirate; F941 puts the pirate into
   state 0x0c. The pirate has F935's standard E.C.M. bit, F757 rolls
   `F1530 > 0xa000`, and `Navigator::ecm(false)` destroys the model-6 missile
   with F719's explosion and clears the evasion through F780/F781.

The launcher/pylon counts are source-derived (F773's `DATA_006260` and the
model's `+0x12` mount word); no ammo count is invented. F1054's chaff word is
the cargo array's Chaff commodity, so a market stocks it; the source never
decrements it, and the port follows that (see `docs/notes/combat.md` section
8.4). The executed original's `--missile-probe` calibrates the player's F941
launch and F758 guidance; it does not drive an NPC F754 launch, which is the
one timing/geometry comparison still open.
