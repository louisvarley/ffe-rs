# FFED3D replacement models: provenance and attribution

Status 2026-10-28. Inventory of the owner-supplied FFED3D "full download"
(`ffed3d-master/full-download/`, git-ignored, FFED3DAJ build by Andy Jones on
the FFED3D wrapper by DreamZzz and Tehnomag). **Nothing from it is used by the
port yet** (the "Used" column). This note is the place to record each model the
port starts to use, where it came from and who made it.

## What it is

- `MODELS/<n>/` holds a replacement for original model slot `n` (the slot
  numbers of `+Information/FFE Models.txt`, the same numbering as the port's
  model table): 154 folders.
- Meshes are DirectX `.X` files: almost all `xof 0303bzip0032`
  (MSZIP-compressed **binary** `.X`), a few plain text. They carry a frame
  hierarchy, materials, texture names and `AnimationSet`s (the Viper's 49 parts
  and two animation sets, for example: gear and doors are animated in the mesh,
  not by the original's model program).
- Skins are PNG (`skin.png`, `skin1.png`… selected by `skins.ini`).
- These are **fan-made replacements**, not the original game's models. The
  original models stay the port's reference (Classic graphics); a replacement
  can only ever be a Modern-graphics presentation option.

## Using your own copy (2026-10-29)

Nothing from FFED3D is part of this project. To use the replacement models,
download FFED3D yourself (the FFED3DAJ "full download") and put it in a folder
named `ffed3d` beside the game (`ffed3d/MODELS/...`, or the download's own
`ffed3d/full-download/MODELS/...`). `/ffed3d/` and `/full-download/` are
git-ignored. To check what the game can read:

```
ffe-tools inspect-ffed3d --dir ffed3d
```

Status: the reader (`ffe_convert::xfile`: text/binary, MSZIP-compressed `.X`,
frames, meshes, normals, UVs, materials) and the folder lookup
(`ffe_convert::ffed3d`) are done; on the FFED3DAJ download 144 of 150 folders
read. Three folders have no mesh (131, 218, 219) and three files are damaged
MSZIP that no decoder reads (65, 66, 454: block 4 does not inflate). Drawing
them in Modern graphics is not done yet (see below).

### How FFED3D places a model (`ffed3d-master/win`)

- `w32video.cpp` ~1030: `models/<n>/model.x` and an optional
  `models/<n>/tris.ini` `[MODEL]` with `scale`, `scale_x/y/z` (default 1),
  `offset_x/y/z`, `cullmode`, `skip`, `notdrawtext`, `notdrawsubmodels`.
- `w32video.cpp` ~2921: world = `scale_xyz * scale * translate(offset) *
  modelList.world`; `gfx.cpp` ~5549: `modelList.world` = the original object's
  matrix (Q31 / `0x80009fff`) then its view position / 10000 (`DIVIDER`). So one
  `.X` unit is 10000 original view units, on the original's axes (x right,
  y up, z forward), row-vector convention.
- Skins (`ffe3d.h` ~295): `skin.png`, `skin1.png`..`skin9.png`; ships (models
  14..70) choose by the object's unique id & 255 in the shader, the rest use
  skin 0. A material's own `TextureFilename` wins.
- Animation (`w32video.cpp` ~2940): ships 15..69 and the docking parts 227,
  234, 236 play animation set 0 at `landing gear / 65536` of its length (the
  game's gear/door word, global 1) when the gear is out or the model has only
  one set; otherwise set 1 runs on real time.

### Still to do

1. A full-colour texture path in the GPU renderer (`ffe-render` takes only
   palette-indexed maps today).
2. In Modern graphics, draw a slot's replacement in place of its executed
   faces, with the placement above; Classic stays original.
3. Animation sets (gear, doors), then the per-ship skin choice.
4. Fill in the "Used" column below as models are drawn.

## Licence and permission

- Neither `FFED3DAJ_readme.txt` nor the mod information grants a licence or
  permission to copy, convert or redistribute the models or textures. The only
  stated permission is nanite2000's, for including some textures in the
  FFED3DAJ core pack. The readme says downloads were for registered forum
  members of Elite-Games.ru / SpaceSimCentral.
- Treat every model and texture as all-rights-reserved by its author: **local,
  private use only**, never committed, never shipped, never uploaded. This
  matches the project rule for derived art (`CLAUDE.md` §5). Distribution would
  need each author's permission.

## Credits (from `FFED3DAJ_readme.txt`)

FFED3D: programmer DreamZzz; forum/project moderator HeadHunter.
Models and textures: Arn, Jahell, SparkS, NiK2009, KrIM, UncleBob, wandour,
potsmoke66, Gluck, sined, SteveT (Steve Tyler).
Textures and sprites: DreamZzz, KrIM, Eisfee, wandour, Darth NanoDesu,
Synchromesh and other participants. Planet/star textures: nanite2000, Ittiz,
Vampirette. Music: XL, SleepyEmp, RedLord, Disou, Mespotine. Sound effects:
mespotine, jbotor. Auxiliary programs: TexHoMar (Tehnomag), Magistr_AVSH.
FFED3DAJ and its configuration tool: Andy Jones. Original project forum:
http://www.elite-games.ru/conference/viewforum.php?f=43

## Attributed mods (`MODS/` folders and `ModInfo*.jsgme`)

| Mod folder | Author | Model slots |
|---|---|---|
| COCKPIT - SteveT - Cockpit mod (Base + v2-v3.texture) | SteveT | — |
| MODELS -  SteveT - Viper Defense Craft | SteveT | 30 |
| MODELS - Arn - Axeblade Station | Arn | 74, 77, 238 |
| MODELS - Arn - Eagle MK1 | Arn | 23 |
| MODELS - Arn - Eagle MK1 Missile | Arn | 3 |
| MODELS - Arn - Eagle MK2 | Arn | 24 |
| MODELS - Arn - Eagle MK3 | Arn | 25 |
| MODELS - ECM Antenna | not stated | 196 |
| MODELS - Jahell - Crater | Jahell | 162 |
| MODELS - Jahell - Hope Spaceport (older model) | Jahell | 271 |
| MODELS - Jahell - Lifter Cargo | Jahell | 153 |
| MODELS - Jahell - Merlin Attack Fighter | Jahell | 33 |
| MODELS - Jahell - Saker MK3 - New | not stated | — |
| MODELS - KrIM - Escape Pod | KrIM | 14 |
| MODELS - KrIM - Osprey Attack Fighter | KrIM | 19, 185, 186, 187 |
| MODELS - KrIM - Osprey X | KrIM | 32, 186, 187 |
| MODELS - New Hanger | not stated | — |
| MODELS - Panther Clipper | not stated | 217, 220, 221, 222 |
| MODELS - Potsmoke - IP Shuttle Stud-City | Potsmoke | 15 |
| MODELS - Sidewinder | not stated | 26 |
| MODELS - UncleBob - Lanner 1 | UncleBob | 41 |
| MODELS - Wandour - Building | Wandour | 392 |
| MODELS - Wandour - Landing Pad | Wandour | 288 |
| MODELS - Wandour - Terminal Hanger | Wandour | 302 |

Per-model authors below come from these folders and the `Mod Creator` lines of
the `*.jsgme` files. A model with no named author is covered only by the
collective credits above; its author is not known.

## Model folders

| Slot | Original model | Author | `.X` format | Images | Used |
|---|---|---|---|---|---|
| 3 | generic missile | Arn | bzip | 1 | not used |
| 6 | KL760 Homing Missile | — | bzip | 1 | not used |
| 14 | Escape Capsule | KrIM | bzip | 4 | not used |
| 15 | Interplanetary Shuttle | Potsmoke | bzip | 2 | not used |
| 18 | Lifter | — | bzip | 3 | not used |
| 19 | Osprey attack fighter | KrIM | bzip | 1 | not used |
| 20 | Falcon attack fighter | — | bzip | 1 | not used |
| 21 | Hawk airfighter | — | bzip | 5 | not used |
| 22 | Kestrel airfighter | — | bzip | 5 | not used |
| 23 | Eagle long range fighter | Arn | bzip | 10 | not used |
| 24 | Eagle Mk2 fighter | Arn | bzip | 5 | not used |
| 25 | Eagle Mk3 fighter | Arn | bzip | 5 | not used |
| 26 | Sidewinder | — | bzip | 1 | not used |
| 27 | Krait Assault Craft | — | bzip | 10 | not used |
| 28 | Gecko | — | bzip | 10 | not used |
| 29 | Adder | — | bzip | 5 | not used |
| 30 | Viper Defence Craft | SteveT | txt | 14 | not used |
| 31 | Saker III fighter | Jahell | bzip | 7 | not used |
| 32 | Osprey X attack fighter | KrIM | bzip | 1 | not used |
| 33 | Merlin attack fighter | Jahell | bzip | 1 | not used |
| 34 | Viper Defence MkII | — | bzip | 9 | not used |
| 36 | Cobra Mk I | — | bzip | 10 | not used |
| 38 | Cobra Mk III | — | bzip, txt | 3 | not used |
| 39 | Constrictor | — | bzip | 10 | not used |
| 40 | Asp Explorer | — | bzip | 7 | not used |
| 41 | Lanner | UncleBob | bzip | 6 | not used |
| 42 | Harris fighter | — | bzip | 1 | not used |
| 43 | Spar attack fighter | — | bzip | 2 | not used |
| 44 | Wyvern Explorer | — | bzip | 1 | not used |
| 45 | Skeet cruiser | — | bzip | 9 | not used |
| 46 | Turner Class | — | bzip | 2 | not used |
| 49 | "Unknown" (Piston Thargoid ship) | — | bzip | 1 | not used |
| 53 | Transporter | — | bzip | 5 | not used |
| 56 | Imperial Courier | — | bzip | 4 | not used |
| 60 | Puma Clipper | — | txt | 0 | not used |
| 61 | Boa Freighter | — | bzip | 1 | not used |
| 65 | Imperial Explorer | — | bzip | 1 | not used |
| 66 | IMRA Command Ship | — | bzip | 1 | not used |
| 67 | Mantis Transport | — | bzip | 2 | not used |
| 71 | Long-range Cruiser | — | bzip | 1 | not used |
| 74 | orbital station (Hammer with 4 arms) | Arn | txt | 1 | not used |
| 77 | orbital station (Hammer with 2 arms) | Arn | txt | 1 | not used |
| 78 | Tube Station | — | bzip | 1 | not used |
| 89 | 2 pad base #3 | — | bzip | 1 | not used |
| 112 | space rock (small asteroid model) | — | bzip | 1 | not used |
| 131 | living planet | — | — | 0 | not used |
| 153 | cargo object | Jahell | bzip | 1 | not used |
| 162 | crater | Jahell | bzip | 1 | not used |
| 172 | game over gravestone | — | bzip | 1 | not used |
| 179 | landing skis (sidewinder,anaconda) | — | bzip | 0 | not used |
| 181 | double landing skis | — | bzip | 0 | not used |
| 182 | lifter engine pod | — | bzip | 2 | not used |
| 183 | osprey wing | — | bzip | 0 | not used |
| 184 | cockpit | — | bzip | 0 | not used |
| 185 | tail fin | KrIM | bzip | 0 | not used |
| 186 | three landing wheels | KrIM | bzip | 0 | not used |
| 187 | nose wheel with doors | KrIM | bzip | 0 | not used |
| 188 | hawk body | — | bzip | 0 | not used |
| 189 | hawk wing | — | bzip | 0 | not used |
| 190 | hawk tail fin | — | bzip | 0 | not used |
| 191 | hawk engine | — | bzip | 0 | not used |
| 192 | kestrel wing | — | bzip | 0 | not used |
| 193 | kestrel engine | — | bzip | 1 | not used |
| 194 | pilot head | — | bzip | 0 | not used |
| 195 | fuel scoop | — | bzip | 0 | not used |
| 196 | ECM antenna (ship) | — | bzip | 1 | not used |
| 197 | naval ECM, double antenna (ship) | — | bzip | 1 | not used |
| 198 | scanner antenna (rotating radar dish) | — | bzip | 1 | not used |
| 199 | 3 spikes / gray lines - gun | — | bzip | 0 | not used |
| 201 | saker engine | Jahell | bzip | 5 | not used |
| 207 | wyvern engine (rotates with raised undercarrage) | — | bzip | 1 | not used |
| 208 | skeet dish | — | bzip | 8 | not used |
| 209 | thargoid warship centerpiece (piston) | — | bzip | 1 | not used |
| 213 | gun turret | — | bzip | 1 | not used |
| 215 | imperial courier/trader rotating engine/spike | — | bzip | 4 | not used |
| 217 | lynx engine | — | bzip | 1 | not used |
| 218 | lynx midsection | — | — | 0 | not used |
| 219 | lynx container | — | — | 0 | not used |
| 220 | panther engine | — | bzip | 1 | not used |
| 221 | panther between-engines | — | bzip | 1 | not used |
| 222 | panther combined engine | — | bzip | 1 | not used |
| 223 | imperial explorer - rotating engine/spike | — | bzip | 1 | not used |
| 227 | space station docking bay entrance area | — | bzip | 3 | not used |
| 233 | docking-bay ** final room ** | Jahell | bzip | 4 | not used |
| 234 | docking bay gates | — | bzip | 1 | not used |
| 235 | Tv screen - "LITE" banner etc | — | bzip | 1 | not used |
| 236 | docking gates elevator | — | bzip | 1 | not used |
| 238 | orbital base part - hammer arm | Arn | txt | 1 | not used |
| 248 | city block - grass poly, swimming pools, houses, Obelisk churches & trees & little man statue | — | bzip | 2 | not used |
| 251 | city block - offices | — | bzip | 1 | not used |
| 254 | small factory + 3 chimneys | — | bzip | 1 | not used |
| 260 | eiffel tower | — | bzip | 1 | not used |
| 263 | pillar | — | bzip | 1 | not used |
| 268 | 2 pad base with control tower & 2 hangars | — | bzip | 1 | not used |
| 269 | gray-white dome | — | bzip | 1 | not used |
| 271 | start base - black & blue textures, 1 & 2 text in white | Jahell | bzip | 1 | not used |
| 273 | city square + church/house/towers | — | bzip | 1 | not used |
| 274 | city square + watertower/house/towers | — | bzip | 1 | not used |
| 275 | city square | — | bzip | 1 | not used |
| 276 | city square | — | bzip | 1 | not used |
| 277 | city square | — | bzip | 1 | not used |
| 278 | city square | — | bzip | 1 | not used |
| 279 | city square - powerstation/stadium | — | bzip | 1 | not used |
| 283 | city square | — | bzip | 1 | not used |
| 284 | city square | — | bzip | 1 | not used |
| 287 | city square with church + houses | — | bzip | 1 | not used |
| 288 | 2 pad base with control tower & 2 hangars | Wandour | bzip | 1 | not used |
| 289 | hexagonal (250) factory with 3 3-chimney factories (254) | — | bzip | 1 | not used |
| 302 | hangar | Wandour | bzip | 1 | not used |
| 315 | background galaxy cube, game | — | bzip | 0 | not used |
| 316 | ball-array - grey (6,6,6) & whiter (10,10,10) | — | bzip | 2 | not used |
| 365 | factories | — | bzip | 1 | not used |
| 366 | control-tower | — | bzip | 1 | not used |
| 368 | billboard | — | bzip | 1 | not used |
| 369 | church + graves | — | bzip | 1 | not used |
| 370 | building | — | bzip | 1 | not used |
| 371 | pond (big blue square) | — | bzip | 1 | not used |
| 372 | tiny red-white house | — | bzip | 2 | not used |
| 373 | triangle Obelisk / spike | — | bzip | 1 | not used |
| 374 | tree-grove (ball-array) | — | bzip | 1 | not used |
| 377 | building | — | bzip | 1 | not used |
| 378 | building | — | bzip | 1 | not used |
| 379 | building | — | bzip | 1 | not used |
| 380 | building | — | bzip | 1 | not used |
| 381 | building | — | bzip | 1 | not used |
| 389 | angled house / tiny white house | — | bzip | 1 | not used |
| 390 | building | — | bzip | 2 | not used |
| 392 | building (replacement is tall rounded building with transparent dome on top containing park) | Wandour | bzip | 1 | not used |
| 393 | building - thin sky scraper with angled roof // | — | bzip | 2 | not used |
| 396 | building | — | bzip | 2 | not used |
| 397 | buildings | — | bzip | 1 | not used |
| 399 | water-tower | — | bzip | 2 | not used |
| 401 | building | — | bzip | 1 | not used |
| 403 | building | — | bzip | 2 | not used |
| 411 | building | — | bzip | 1 | not used |
| 413 | wind-turbine | — | bzip | 1 | not used |
| 414 | part of wind-turbine - blades | — | bzip | 1 | not used |
| 440 | intro turner ship! | — | bzip | 1 | not used |
| 441 | destroyed asteroid -space rocks | — | bzip | 2 | not used |
| 454 | IMRA command ship (imperial explorer) | — | bzip | 1 | not used |
| 455 | viper mk2 (intro) | — | bzip | 1 | not used |
| 457 | thargoid warship | — | bzip | 1 | not used |
| 458 | Clock face | — | bzip | 0 | not used |
| 459 | grave... | — | bzip | 1 | not used |
| 460 | Clock hand | — | bzip | 0 | not used |
| 465 | part of thargoid warship | — | bzip | 1 | not used |
| 466 | mirrored version of city area model #279 | — | bzip | 1 | not used |
| 470 | docking bay for cylindrical space stations (dublin citadel) | — | bzip | 1 | not used |
| 471 | undergound docking bay with animated lifter | — | bzip | 1 | not used |
| 472 | — | — | bzip | 1 | not used |
| 370_mast | — | — | bzip | 1 | not used |
| 392New | — | — | bzip | 2 | not used |
| 86notused | — | — | bzip | 1 | not used |
| _export | — | — | — | 0 | not used |
