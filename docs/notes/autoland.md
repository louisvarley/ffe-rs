# Starport auto-landing and the pad touch (2026-10-25)

Status: the **pad contact path** is fixed and the autopilot's hand-off, clearance,
pad target and `0x2e` -> `0x30` state flow are ported. The automatic **final
lateral capture** on the pad is still the controller's limit (see "What remains").

## Source map

The DOS FFE routines, in `jjffe-master/ffe.asm` (functions `FUNC_000NNN`):

| Routine | What it does | Port |
|---|---|---|
| F741 | state 8's set-up: the target is `[0, 0x1770, 0]` in the starport's frame (6,144,000 mm above the port), `[0,0,0xfa0]` for an indoor station; stores `+0x102..+0x106` and calls F742 | `Navigator::command`, `STARPORT_APPROACH` |
| F742 | F610's distance; far: F599 thrust + F587 turn rates; at the target: F761 (no target), F741 (re-aim) or F745 | `command` (approach, F655 on arrival) |
| F745 | F655 clearance; on success stores F650's pad offset `>> 10` in `+0x102..`, plays sound 0x0c, sets AI state `0x23` | `command`/`request_docking`, `final_leg` |
| F746 | state `0x23`: F596/F597 distance and speed, F599 thrust, F589 turn; at distance zero F744, then copies the target's orientation (the port's nine rows) and F671 | `starport_descent` (F599/F600 request), `OriginalFlight::align_to` |
| F744 | F652 when the port's record has the ship, else F745 | `Starport::touch`, `Navigator::command`'s pad touch |
| F652 | the pad touch: mode 3, timeout `0x7080` | `ffe_core::traffic::Starport::touch` |
| F655 | fines, F654, F650; the messages 0x98b7/0x98b8/0x98b9/0x98ba/0x98bc | `Starport::request_docking`, `navigation::refusal` |
| F650 | a free bay and the model's pad vector (`DATA_006175..6179`) | `ffe_core::pads`, `Navigator::starport_pad_point` |
| F1019 | the player's frame: F569/F1033's flags; a 0x20 starport region and a 0x40 landing set `0x2e`, F995/F1116 | `Navigator::land_at_starport` (`0x2e`) |
| F1048 | `0x2e` -> `0x30`, F653 resets the record, F1903 stops the song | `Landing::at_starport().frame()`, `landing_frame` |
| F576 | the ship's spheres against a complex shape's collision stream; F580 children | `ffe_core::surface::Scene::hit` |
| F575 | the landing test (`speed <= 0x258`, gear `0xffff`, `dot >= 0x78000000`), F578's bounce, the pad flags (`0x40` landing, `0x20` region) | `ffe_core::landing::contact` |

The AI state table (`ffedat.asm` `DATA_006255..6258`) is read with a four-byte
stride: state n's set-up is the dword at `DATA_006255 + 16 n`, its step the dword
at `DATA_006256 + 16 n` (= base + 4 + 16 n). State 8 is set-up F741, step F727;
state `0x23` is step F746. The comments beside `DATA_006258` are offset by one
group (the table starts 12 bytes before them).

## The blocker: the F576 stream and the cache

At the exact pad, `F569` runs `F576` against the **starport** model. The ship must
be a *sibling* of the starport under its planet (F994's frame), not the starport's
child: the two-body `step_landing` makes the starport the parent and F575 takes
the sphere branch, which never reads the stream.

With the sibling frame, model `0x59` faulted: its collision stream is an F580
child at vertex 0, and `0x59`'s vertex/normal assets were **not in the cache**.
The stream pointer is correct; the model-data import was wrong. The converter
skipped every model whose **command** program did not decode, and `0x59`'s last
command record (opcode 14 at offset 0x48) reads the next model block, so its
72-byte vertex block and 6-byte normal block were discarded. F576 only needs the
raw vertex and normal records; commands are for drawing. The cache builder
(`ffe-tools/src/cache.rs`) now imports
vertices/normals for every model that decodes them and the command asset
independently. The cache gains model `0x59`'s geometry (and two other models'
command programs) and has 1815 assets, still v29 (`docs/FORMATS.md`, README).

`step_starport_landing` builds F994's three-body world for the tick (the player
relative to the planet, the starport a sibling) and restores the absolute
position after it, so the playable keeps its system-frame position while the
scan sees F576/F580.

## Verification

`crates/ffe-rs/tests/navigation.rs`, all local-only (need `FFE_WORLD_CACHE`):

- `starport_autopilot_lands_on_the_pad`: the approach, F655's grant and F650's
  bay, the descent request, F746/F744/F652's pad touch and F1048's `0x30`, then
  F381/F654's launch. Frames (`FFE_SHOT_DIR`): `autoland-0000-approach`,
  `autoland-9999-pad`.
- `manual_pad_contact_reaches_0x30`: the F994 three-body world and the F569/F576
  scan at the pad (no model-data fault), then `0x2e` -> `0x30`. Frames:
  `manual-0000-above`, `manual-9999-pad`. The F575 `0x40` pad flags are **not**
  yet observed through this path: the starport's F576 stream reports the `0x20`
  region but no landing surface at the seat/height probed, so the touch's state
  path is exercised directly. This is the same remaining item as the lateral
  capture below.
- `starport_model_0x59_collision_geometry_is_imported`: `0x59`'s vertex/normal
  records and a scan through its child model (the old gap).

`docs/evidence/autoland/` holds the converted frames.

## What remains

- The automatic **final lateral capture** on the port's pad is not reliable: the
  port's controller (F599's body-axes request plus the navigator's one-second
  `follow` drift) passes the pad ~100 m to the side on some runs and can clip a
  building (F578's bounce). The test's pad touch is scripted at F650's seat; the
  manual collision path is proven separately. The descent should use F596/F597's
  exact distance/speed law and the F570 rotating frame's velocity, not the
  port's position-only follow.
- F652's mode-3 handshake is called by the autopilot's touch; the player's
  F1019 path ignores its return for a starport, as the source does.
- The sounds F745/F1019 play (0x0c, 0x13) and F671 are not wired to the audio
  layer for this path.
