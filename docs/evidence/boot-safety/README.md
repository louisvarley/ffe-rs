# New-game pad safety (spawn-safety stream)

The DOS default new game boots on Hope at Old Blackelk in state `0x30`
(F829/F998/F994). F687/F686's start entry spawns F688's traffic, but the port
does not force a pirate group beside the pad (a normal group uses the anchor
orbit, not `PIRATE_DISTANCE`), and a landed view runs F1012/F1048 instead of the
flight step. The source's pirate group for Hope is about 1.44e9 km away.

| File | What it shows |
|---|---|
| `port-hope-boot-before-destroyed.png` | **before the fix**: the pad attacked at 12:00:00, hull 0.0 %, the log's `Osprey X attack fighter 5 destroys your ship. Recovered at the starport.` |
| `port-hope-boot-0000.png` | the port's boot frame: the landed surface view, the start line only (`Landed at Old Blackelk.`), no attacker |
| `port-hope-boot-60s.png` | the same view after 3,000 frames (60 s of 1x game time): still no hit, the same single log entry |

Reproduce:

```powershell
$env:FFE_WORLD_CACHE = (Resolve-Path .\data.bin).Path
$env:FFE_SHOT_DIR = (Resolve-Path .).Path + '\docs\evidence\boot-safety'
cargo test -p ffe-rs --test boot_safety --locked -- --ignored
```

The test asserts that F688's traffic exists at boot, that no pirate is within
F931's range of the pad, `player_state == 0x30`, `nav.landed_view()`, no
`ShipEvent::Hit` for 3,000 frames, and one message-log entry with the
`1-Jan-3250 12:00:00` stamp. The failing pre-fix test recorded the first hit at
frame 64 (`Osprey X attack fighter 5`, 50 damage, every 32 frames).

## Executed-original comparison

`docs/evidence/landed/original-default-start.png` is the original's boot frame.
The reference adapter's `--state-combat` (JJFFE 2.8a7, start type 1) reports the
player's hull 100 and shields `[0, 0]` unchanged from frame 161 through 899:
the original is not attacked on its start pad either. The port's own
`--state-combat` equivalent is the `boot_safety` run above.
