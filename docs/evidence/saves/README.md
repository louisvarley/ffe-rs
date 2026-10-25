# Save/load evidence

The native version-2 format (`docs/notes/persistence.md`) is exercised by the
in-process capture:

```powershell
cargo run -p ffe-rs -- --root <checkout-with-data.bin> --capture-save docs/evidence/saves
```

That run creates a flight commander, writes `roundtrip.json` atomically, drops
the live navigator/state objects, loads the bytes back, regenerates the system
from its code and clock, and compares both halves. It writes
`save-before.ppm`, `save-after.ppm` and `save-roundtrip.txt`; the checked-in
PNGs are those two PPMs converted for viewing. The two frames are identical
because the capture does not advance the simulation (the pre-save and resumed
states compare equal); the text log is the substantive evidence, together with:

```powershell
cargo test -p ffe-rs --lib saves
cargo test -p ffe-rs --lib settings
$env:FFE_WORLD_CACHE = "<checkout>/data.bin"
cargo test -p ffe-rs --test saves -- --ignored
# after the capture above, before deleting its roundtrip.json:
cargo run -p ffe-rs -- --root <checkout> --load docs/evidence/saves/roundtrip.json --smoke-frames 2
```

The first two suites also cover corrupt/truncated/newer files, oversized input,
version-1 migration, atomic replace, numbered/quick naming and config migration.
The ignored suite covers the real system regeneration, docked pad, landed body,
and bad-index/stage rejection. The `--load` run is the real binary's front-end
load path.

`roundtrip.json` itself is not committed (it is player data); it is recreated by
the capture command.
