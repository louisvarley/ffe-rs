# Autopilot: homing, arrival, station docking and cancel (2026-10-25)

Source: `jjffe-master/ffe.asm` (`FUNC_000NNN` is `FNNN`). Cache v29
(`data.bin`, 1813 assets), branch `autopilot` from `127db08`.

## Source map

| Routine | What it does | Port |
|---|---|---|
| F1020 (75359) | engage: stores the target slot in the player ship's `+0x100`/`+0xec`, sets the ship's AI state `+0xff = 8`, player mode `+0xe8 = 2`, F1116, cue 0x14 | `Navigator::engage` / `engage_locked` |
| F1018 (75157) | cancel: `DATA_008918 = DATA_008866` (set speed 0), `DATA_008870 = 0`, F1116, cues 0x14/0xe | `Navigator::cancel`, `OriginalFlight::halt`, Backspace in `main` |
| F741 (48840) | state 8 set-up: destination `+0x102..+0x106` = `[0,0,0xfa0]` (station) or `[0,0x1770,0]` (starport) in the target's frame, words of 0x400 mm; AI state 0; F742 | `long_leg_offset`, `STATION_APPROACH` 4,096,000 mm, `STARPORT_APPROACH` 6,144,000 mm |
| F742 (48912) | F610 distance/direction. Distance byte > 6: F599 clamp + F587 turn rates. Else F761/F741 retarget/F745 | `autopilot` + `command`; F610's `+0x10e` is `distance_byte`, so its band 6 is the 1,024,000 mm `APPROACH_STOP` |
| F610 (41722) | relative position/velocity, direction, speed envelope, F597 | `autopilot`: braking curve, `turn_toward` |
| F597 (39933) | near target at 1x: `v² < 4as` frame switch/velocity match; otherwise thrust; contains the shipped `FIX: Autopilot acceleration fudge` | port's real-time braking and terminal standoff |
| F599 (40373) | clamp body-axis thrust demand to the ship's limits | `OriginalFlight` thrust model |
| F587 (39263) | turn rates `+0x110/+0x114` from the desired direction dotted with the axes | `OriginalFlight::turn_toward` |
| F745 (49168) | F655 clearance; on grant stores F650's pad vector `>> 10`, AI state 0x23, cue 0x0c | `command` (F655 at approach), `final_leg` |
| F746 (49220) | state 0x23: F596/F597 to the pad, F589 alignment, F744/F652 touch at the pad | orbital: `slot_leg` + `contact`/`begin_docking`; starport: hand-off to the autoland stream |
| F744/F652 | F652 touch if the record holds the ship | `Starport::touch`, `begin_docking` |
| F650 (44371) | free bay + pad vector | `ffe_core::pads`, `starport_pad_point` |

## Root causes found

1. **Circling instead of homing.** `Navigator::command` computed the long-leg
   transfer with a fixed-point loop: start from the body's position now, get the
   transfer time, look up the body at that time, repeat. For a fast planet the
   target's orbital displacement during the transfer makes the guess diverge; the
   loop returned a landing point far from the body and the autopilot transferred
   again on the next frame. `self.skips` reset to 0 on every non-skip frame, so
   the 4-transfer cap never stuck. Observed on Sirius' Lucifer: 60,000 steps of
   skips, range stuck at 18 Mm; Test-4 1 failed the same way.
2. **Cleared but never docks.** The final leg aimed at the station's centre with a
   fixed standoff. It did not follow the slot axis that F745/F746 re-read every
   tick, and the straight line to the approach point can cut the hull when the
   jump arrival is on the far side. A spinning station then pushed the ship clear
   (`Contact::Collision`), the autopilot flew back at the centre and repeated
   (Morgue's Mortuary, Eden Station). At an open-air starport the grant cleared
   `arrived` and `main` dropped the target, leaving no leg and no cancel.
3. **Cannot be disabled.** Backspace only cleared `nav.target`; it did not apply
   F1018's set-speed 0 (the ship coasted on at the autopilot's speed), and the
   prior `Click::AutopilotOff` path has no live hot area since the HUD cleanup,
   so Backspace is the only cancel input. It now clears the leg and halts.

## Fixes

- `Navigator::intercept_time`: bracket the first `t = transfer_seconds(range(t))`
  crossing and bisect it. The landing point is the body's position at the advanced
  clock, so one transfer lands at `stop + APPROACH` and the final leg flies. A
  sticky `MAX_SKIPS` cap flies the remainder if a transfer cannot be solved.
- `slot_leg`: F746's final leg tracks the current axis. Off-axis/behind the face
  it lines up on a gate outside the face; in the corridor it enters along the
  axis and brakes so the face crossing is under 250 m/s.
- `station_route`: if the segment to the approach point would pass inside the
  station's safety sphere, steer around it on the sphere before lining up.
- `ARRIVAL_SLOP` (10 m): the braking curve approaches the standoff asymptotically;
  the last millimetres were below the set-speed step and left the star at
  `stop + a few mm` with `arrived == false`.
- Starport grant sets `final_leg` and keeps the target; `notice` says
  "clearance granted (bay N). Descending to the pad." The hand-off fields are
  unchanged (`final_leg`, `cleared`, `bay_of(PLAYER)`, `starport_pad_point`,
  `land_at_starport`) for the autoland stream.
- Cancel: `Navigator::cancel` clears target, final leg, memory, skip count and
  clearance timer; `OriginalFlight::halt` sets `DATA_008918` to 0; `main` plays
  the UI beep and shows "Autopilot off." Clearance is left to time out (F867).

## Scripted runs

All from `crates/ffe-rs/tests/navigation.rs`, cache v29, release, on this branch.
Logs in this directory; frames from `FFE_SHOT_DIR` (PPM converted to PNG).

| Run | Command | Result |
|---|---|---|
| Body + every system body | `cargo test -p ffe-rs --test navigation --release -- --ignored --nocapture --exact autopilot_after_every_jump` | 50 body kinds over 12 systems: all arrived, 0 hull pushes, 3.6 s. Includes Sirius/Lucifer and Test-4 1 (the old failures) and the 2,145 Mm star standoff. `every-jump.log` |
| Station approach and dock | `... --exact autopilot_docks_at_stations` | Daedalus, Abraham Lincoln, Galileo: docked at 23 m/s, 0 pushes. `station-dock.log`; frames `dock-*.png` (far/near/slot) |
| Stations from every bearing | `... --exact autopilot_docks_at_stations_from_every_bearing` | Eight systems, every orbital station, three bearings: all docked, 0 pushes. `station-bearings.log` |
| Cancel mid-flight | `... --exact cancelling_the_autopilot_stops_the_ship_and_clears_the_leg` | target/leg cleared, set 0, velocity braked 59 -> 0 units in 50 s. `cancel.log` |
| Unit tests | `cargo test -p ffe-rs --lib navigation::tests original_flight::tests` | intercept rendezvous, slot law, homing/arrival, halt |

Frames (`320x200`): `dock-Daedalus-{far,near,slot}.png`,
`dock-Abraham_Lincoln-{far,near}.png`, `dock-Galileo-{far,near,slot}.png`.
Abraham Lincoln contacts before the 0.6 km shot mark, so it has no `slot` frame.

## Interactions and remaining owner decisions

- **Autoland stream.** It owns the open-air final descent and pad contact. This
  branch keeps the hand-off stable at F745's grant (`final_leg`, `cleared`,
  `starport_pad_point`, `land_at_starport`) and does not add a `thrust`/`align`
  command variant, so the two changes overlap only in `Navigator::command`'s
  starport arm and the `clearance_at` throttle. Its `starport_descent` and
  three-body pad scan remain the descent implementation.
- The long-leg transfer itself is a port convenience, not source behaviour. The
  intercept now matches the body at the advanced clock; if an owner prefers no
  teleport, the source's model is real-time flight at the manual time
  acceleration, and `MAX_SKIPS` already falls back to flying.
- No DOS oracle was run for this stream (the reference harness is a separate
  executed-original path); the constants are the source's `0xfa0*0x400`,
  `0x1770*0x400` and F610's exponent-6 band, and the final entry speeds are
  22-23 m/s.
