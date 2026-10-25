# Thargoid traffic and a kill (F690/F35, F774, F943/F953/F942)

Local-only evidence for the Thargoid slice. The frames are the port's own
renderer from the real `ffe-floppy-1.02-en` v28 cache, produced by:

```text
FFE_WORLD_CACHE=<repo>/data.bin FFE_SHOT_DIR=<dir> \
  cargo test -p ffe-rs --test thargoid_view -- --ignored --nocapture
```

The PPMs are converted to PNG here; the test keeps writing PPMs.

## Source map

| Item | Source | Values |
|---|---|---|
| Thargoid count | `F35(0x16)` (`ffe.asm:45858`; getter `3524`, setter `3507`) | global variable 22, saved with the commander (`F65`, `6898`). No direct `F34(0x16, …)` call exists, so only mission bytecode (F37, `3646`) writes it. |
| Spawn | `F690` (`ffe.asm:46032`), called by `F688` (`45818`) after the traders | one ship per count, F689's placement/setup with type 0x11 |
| Model list | `DATA_006197` (`ffedat.asm:9616`), type 0x11 in `DATA_006199` | `0x45, 0x31,0x31, 0x32,0x32, 0x33,0x33, 0x3f,0x3f,0x3f,0x3f,0x3f` |
| AI class / role byte | F773 writes `+0x118` (`ffe.asm:50550`) | 0x11 |
| AI state | F690 writes `+0xff` (`46050`) | 0x11 = F737 jump countdown (`ffedat.asm:9988` row); the port starts the hostile role in `Attack` |
| Equipment | F773/F774 (`50526`, `50765`) | `DATA_006264[0x11] = 0xb2`; budget × proportion picks `DATA_006265` (75 t -> 0x20MW beam 0xb3, 900 t -> 0xcf) |
| Thargoid Laser (shop) | `DATA_005289` record 65, `+0x12 = 0xd5` | `ffe-core::equipment::THARGOID_LASER`, 50 t, tech 255 |
| Damage/kill | F953/F942 (`71190`, `70381`) | shields then hull; model 0x9b explosion and model 0x99 debris |

There is no F690 branch for government, tech level, military systems,
witchspace or a random draw; the only gate is the script global. The source has
no surrender or ECM/missile special case for class 0x11: F690 is F689's code
with a different type byte and model list, and F773 fits ECM/missiles from the
same budget for every class (the port has no NPC missile/ECM launch for any
role).

## Model statistics (cache v28, `base:ship/stats`)

The test pins model 0x31 because it is `DATA_006197`'s first warship. Values are
the original 48-byte statistics record; the port's `Spec` mapping is in
`crates/ffe-rs/src/ships.rs`.

| model | name | mass | cargo | shield word | thrust | drive | mounts | radius | port hull | port shields |
|---|---|---|---|---|---|---|---|---|---|---|
| 0x31 | Unknown | 232 | 203 | 16,430 | 14,715 | 14 (0x800e) | 1 | 90,000 | 928 | 184 |
| 0x32 | Transporter | 241 | 214 | 16,431 | 14,715 | 14 | 1 | 30,720 | 964 | 188 |
| 0x33 | Lion Transport | 162 | 149 | 16,432 | 14,715 | 14 | 2 | 9,000 | 648 | 192 |
| 0x3f | Tiercel Freighter | 162 | 149 | 16,446 | 14,715 | 14 | 2 | 48,750 | 648 | 248 |
| 0x45 | Thargoid Transporter | 15,000 | 13,000 | 16,453 | 10,900 | 14 | 0 | 229,376 | 60,000 | 276 |

`hull = mass * 4`; `shields = (shield word & 0xff) * 4`, the port's documented
simplification of F773's `random * 16`.

The 0x800e drive word means drive class 14, and `DATA_006266[14]` is zero, so
the port gives these ships no starting laser energy (F773 would too). They
chase, but the player's shots are the evidence here. An owner decision records
whether to model F929's `+0x119` fuel tokens or a loadout.

## Frames

| file | shows |
|---|---|
| `thargoid-before.png` | model 0x31 pinned from `DATA_006197`, 500 m ahead; readout `Hull 100.0% / Shields 100.0% / Shield Strength 16430` |
| `thargoid-hit-shields.png` | first 1MW-pulse hit; F1536 model-0x9d shield bubble, shields 72.8 % |
| `thargoid-hit-hull.png` | first hull hit after four shield hits; hull 98.2 %, shields 0 % |
| `thargoid-kill.png` | 23rd hit: hull 0.0 %, shields 0.0 %; the ship model is gone |
| `thargoid-death.png` | F942's model-0x9b explosion seen from about 20 m |

The 23-hit arithmetic is 184 shields / 50 damage = four shield hits, then 928
hull / 50 = nineteen hull hits. Shots are 38 frames apart at 60 Hz
(`0x795c` refire), matching the other laser tests.
