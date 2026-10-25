# Classic control-routing evidence

Two layers, both reproducible from a checkout:

1. The pure routing tests:
   `cargo test -p ffe-rs --locked input::` and
   `cargo test -p ffe-rs --locked --test controls --test original_flight --test prefs`.
2. A 60-frame flight/capture with the Classic route: Enter held (thrust) and the
   right mouse button held with 20 px right / 10 px up each frame (yaw and
   pitch).

```powershell
$dir = Join-Path $env:TEMP 'opencode\controls-capture'
$env:FFE_RS_CONTROLS_CAPTURE = $dir
cargo test -p ffe-rs --locked --test controls classic_input_strip_capture
```

The test writes `classic-00.ppm` .. `classic-60.ppm` (320x200, the flight view
plus console) and `input.txt` to `$dir`. The run on 2026-10-25 produced:

```
frame,thrust,yaw,pitch,speed
0,1.000,0.400,0.200,0.000
15,1.000,0.400,0.200,8.993
30,1.000,0.400,0.200,17.946
45,1.000,0.400,0.200,26.820
60,1.000,0.400,0.200,35.576
```

That is the routed prototype stick: Enter gives full positive thrust, the held
right button gives +0.4 yaw and +0.2 pitch after the 50-pixel full-deflection
scale (`input::flight_controls`), and the ship accelerates from rest. The same
route with `Preset::Modern` is asserted to leave speed, yaw and pitch at zero,
and the original flight model's F1038 path is covered by
`tests/original_flight.rs::the_classic_mouse_word_steers_and_a_disabled_mouse_never_does`.

The PPMs stay in the temporary directory (frames are not committed); the numbers
and the test assertions are the stable record.
