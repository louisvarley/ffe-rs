# Runtime fixes (2026-10-25)

Frames from the reported bugs on `127db08`, captured with
`--root <cache> --capture-start <dir>` (640x480, the presented frame). The
scripted start flow presses Escape three times, P (resume), opens F4 (services)
at 50, closes with F1 at 60, launches with L at 70 and sets the `0` (10000x)
meter step at 75. The before frames are from the same script on the unmodified
build.

| frame | before | after |
|---|---|---|
| services | `before-services-cleared-console.png`: the board replaced the view and the dynamic console/date were gone | `after-services-over-landed.png`: services over the landed view, console icons and green date/clock kept, hint under the log |
| F1 return | the mode had switched to Docked and the original model was dropped | `after-f1-back-to-pad.png`: back on the same pad, still in flight mode, `Set`/`Act` live |
| clock | `before-clock-frozen.png`: `12:00:00` after 170 frames; the clock only ticked inside the flight-model branch | `after-clock-at-10000x.png`: the date/time has advanced (the `0` step) while the launch/free flight ran |
| set speed | — | `after-set-speed.png`: `Set:` and `Act:` both live after launch (`F1099`) |

The pad's flight model is built at start-up now (F1011/F1014 read F1037/F1041),
so the first frame already shows `Set:0.0`; the pad world follows the
navigator's planet/starport each frame, because F994 parents the player to the
planet and the clock now moves the bodies. The landed `0x34` fix (Enter/Shift/
W/S after a surface landing) has no single frame here: the scripted flow leaves
the pad before the fall-back landing. It is covered by
`tests/navigation.rs::landed_set_speed_runs_f1037_and_accelerates` (local cache),
which drives the same `step_landing` path `main` now calls.

Regression guards: `tests/runtime_regressions.rs` (pause menu gone, F3 map not a
preset toggle, one clock advance, pad services keeps `Flight`, landed branch runs
`step_landing`), `input::tests::f3_is_the_source_map_key_and_not_a_preset_switch`,
`tests/prefs.rs::the_port_page_flips_the_preset_and_shows_it`,
`navigation::the_clock_advances_with_and_without_a_flight_model`, and
`navigation::pad_set_speed_runs_in_the_pad_world_before_launch` (local cache).
