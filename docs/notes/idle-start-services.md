# Enter at intro, idle pad, and services — 26 October 2026

Start remains F998 type 0: Gateway / Hope / Old Blackelk. The owner reported
falling without launching: press Enter at the intro and then nothing.

## Reproduced causes

The new game created its pad flight model but left its mode as free flight.
The runtime then called step_pad on every idle frame. F603 gravity and movement
therefore ran on a ship which F994 had parked. The regression reproduced this
on frame 0: -6 mm drift and velocity [-3,-1,2] with no input.

The old runtime comments and regression claimed F1014/F1015 ran set-speed and
flight physics. Inspection of ffe.asm proves otherwise: F1011/F1012 zero the
altitude and call F1039 (turret steering only); F1014/F1015 handle atmosphere,
heat and drag, not F1037/F1041 movement. Added step_landed, initialised the
model's 0x30 mode, and routed both the new-game pad and landed records through
the parked handler. The clock, planet following, screens and launch action
remain active. An idle or held speed key does not start flight physics.

A second regression appeared at frame 253: incrementing the u8 frame stamp
aliased the stationary body's old cached absolute position and produced a
system-scale jump. Explicitly invalidate the copied player body before F1677
resolves its live motion. A parked pad also follows the port's relative drift;
the takeoff world remains planet-relative during a climb.

Services were displayed via pad_board without changing Flight mode (correct),
but dock_click accepted only Docked mode (incorrect). Accept clicks when the
pad services board is open as well. This restores the original menu hot areas,
including the Launch Request; no menu geometry was changed.

## Verification

idle_pad.rs checks Enter selects type 0, the exact system/planet/port names,
600 idle frames (12 s, multiple u8 stamp wraps) with zero velocity and at most
2 mm relative drift, and an actual launch/climb after that idle period.
Boot-safety and source-autopilot tests pass. The old source-level regression
which required landed flight integration was replaced with a guard for the
actual parked handler.

Native mouse verification opens Services, Shipyard, Stockmarket and Bulletin
Board, then clicks Launch Request and observes the climb (12.3 km/h, hull
100%) in the
rebuilt main release executable; the idle HUD remains at zero speed and hull
100%. Keyboard injection through the computer-use helper did not reach the
intro, so the intro choice is established by the shared Intro::key test rather
than falsely claiming a successful injected Enter. The game idle verification
starts with --skip-intro, which reaches the same new-game initialisation.
