# Removed help and debug UI evidence

The player build has no `?` help page, no scenario menu and no F11/F12 debug
keys. F1 is the original view-cycle key (F6-F9 direct, F10 missile); F12 is the
source escape capsule. The developer commands moved behind `--dev` /
`FFE_DEV=1` (`Ctrl+F11` screenshot, `Ctrl+F12` viewer, `Ctrl+Shift+F12` lab, `O`
prototype flight).

## F1 frame pair

`f1-front.png` and `f1-rear.png` are the same free-flight state before and after
one F1 view cycle. The solid rectangle and text of the retired help page are
gone; the original camera change is visible (the station is ahead in front view
and behind in rear view). The old `STATION`/`STATION BEHIND / OFF VIEW`
diagnostic text was also removed.

Reproduce:

```powershell
$env:FFE_WORLD_CACHE = '<checkout>\data.bin'
$env:FFE_RS_CLEANUP_CAPTURE = '<out-dir>'
cargo test -p ffe-rs --locked --test views f1_cycles_the_view_without_painting_the_removed_help_page
```

The test asserts `input::view_command(Key::F1) == ViewCommand::Cycle`, advances
`ViewMode::Front` to `ViewMode::Rear`, and writes `f1-front.ppm` / `f1-rear.ppm`.
The source view mapping and the absent `/` binding are unit-tested in
`crates/ffe-rs/src/input.rs`; `crates/ffe-rs/tests/playable.rs` draws the same
state twice and asserts the removed help page paints nothing.

## Removed bindings and dev gate

`crates/ffe-rs/src/input.rs` covers:

- F1/F6/F7/F8/F9/F10 map to the six source view commands; `/` (the old help),
  F11 and F12 are not view commands.
- `dev_command` returns `None` for every chord unless `--dev` is set; enabled it
  accepts only `Ctrl+F11`, `Ctrl+F12`, `Ctrl+Shift+F12` and `O`. Plain F11/F12
  are left for the source combat computer and escape capsule.

## Capture/oracle paths

The CLI inputs are unchanged and remain the reproducible harness:

```powershell
target\release\ffe-rs.exe --root . --smoke-frames 30
target\release\ffe-rs.exe --root . --capture DIR
target\release\ffe-rs.exe --root . --capture-start DIR
target\release\ffe-rs.exe --root . --capture-prefs DIR
```

`--capture*`, `--smoke-frames`, `--skip-intro` and `--start 0..5|sol` are
developer inputs; they add no player-facing key or menu.
