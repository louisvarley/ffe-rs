# Display options evidence

Port-only presentation settings (`EXP-017`, `docs/notes/display.md`). The
authoritative framebuffer stays 320x200; these images show only how the finished
frame is fitted into a window. They are not original DOS behaviour.

## Real windows (`PrintWindow`)

Captured from the release binary on a 1920x1080 desktop with
`Alt+Enter` delivered through the window's key messages. The window content is
the port's own frame (Sirocco Station services), not an original screenshot.

| File | Client size | Settings |
|---|---|---|
| `window-2x.png` | 640x480 (656x519 with borders) | 2x, pixel-perfect, nearest, 4:3 |
| `window-3x.png` | 960x720 (976x759 with borders) | 3x, pixel-perfect, nearest, 4:3 |
| `window-4x-default.png` | 1280x960 (1296x999 with borders) | default 4x |
| `window-fullscreen-1080p.png` | 1920x1080 | `Alt+Enter`; the 1280x960 frame is centred at (320, 60) with black bars |
| `ui-port-options.png` | 1280x960 | `Escape` -> Port Options shows the port-only `Display Options` row |
| `ui-display-options.png` | 1280x960 | the Display Options page and its current defaults |
| `ui-3x-applied.png` | 960x720 | after selecting 3x with the keyboard; the window reopened immediately |

The full-screen capture used the automation's `Menu`+`Enter` message pair as the
Alt stand-in; the product path reads the physical Alt state
(`GetAsyncKeyState(VK_MENU)`) and also accepts minifb's Alt/Menu keys. The
screenshot is therefore the real borderless 1920x1080 window, not a composite.

The three `ui-*.png` captures are the same real window after keyboard
navigation (`Escape`, four Downs, Enter, two Downs, Enter, two Downs, Enter);
the 3x selection reopened the window at 960x720 and wrote `saves/display.cfg`
with `scale=3`.

## Deterministic scaling (`--capture-display`)

These are written by `ffe-rs --capture-display DIR` from one 320x200 game frame
with the production `display::compose` scaler, so they show the exact fit maths
at sizes that do not depend on this desktop.

| File | Canvas | Settings |
|---|---|---|
| `display-2x.png` | 640x480 | 2x, pixel-perfect, nearest, 4:3 |
| `display-3x.png` | 960x720 | 3x |
| `display-default-4x.png` | 1280x960 | default |
| `display-fullscreen-1080p.png` | 1920x1080 | simulated full-screen: 1280x960 at (320, 60) |
| `display-square-3x.png` | 960x600 | square pixels at 3x (16:10, not original) |
| `display-linear-fit.png` | 1000x600 | fill-window + linear filter (fractional fit) |
| `display-options-2x.png` | 640x480 | the port-only Display Options page at 2x |

`display-fullscreen-1080p.png` matches the real `window-fullscreen-1080p.png`
fit; two different scenes are shown because the deterministic frame comes from
`--start sol`/free flight.

## Reproduce

```text
cargo build --release --locked -p ffe-rs
target\release\ffe-rs.exe --root . --capture-display out\display
target\release\ffe-rs.exe --root . --capture-prefs out\prefs
```

The real-window captures need a desktop and a focused window; the repository
test playable script (`tools/test-playable.ps1`) has the same `PrintWindow`
helper. Only Windows is fully supported for `Alt+Enter` full-screen (the monitor
size comes from `GetSystemMetrics`; other platforms keep the configured window
size).

## Capture regression

The presentation change does not touch composition or capture writers. Comparing
`--capture-prefs`, `--capture-intro` and `--capture-traffic` against the release
build of this branch's starting commit:

- `prefs-main.ppm`, `prefs-graphic.ppm`, `prefs-sound.ppm`, `prefs-others.ppm`,
  `prefs-maps.ppm`, `space-black.ppm` and `space-light-blue.ppm` are
  byte-identical;
- `prefs-port.ppm` gains only the port-only `Display Options` row;
- all 17 `--capture-intro` frames and all 7 `--capture-traffic` frames are
  byte-identical;
- `--smoke-frames 30` still prints
  `PASS: native window presented 30 frames using verified cabin artwork.`

`--capture-prefs` also writes one new file, `prefs-display.ppm`
(`docs/evidence/prefs/prefs-display.png`).
