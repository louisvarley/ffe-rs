# Display options (port extension)

The original DOS release has no resolution menu: it composes a 320x200 screen
and the video mode shows that screen with non-square 4:3 pixels. The port keeps
the same authoritative framebuffer (`screen::Screen::logical_pixels`, 320x200)
and adds presentation choices after the frame is finished. These are
**port-only extensions**, marked yellow in the preference pages and recorded as
`EXP-017` in `docs/DEVIATIONS.md`; they are not original DOS behaviour and they
never change the simulation, a save or a capture byte.

## What the presenter did before

`main` opened a resizable minifb window at 640x480 (`screen::WIDTH`/`HEIGHT`)
with `Scale::X2` and `ScaleMode::AspectRatioStretch`; `Screen::present` had
already made that 640x480 copy from the 320x200 frame with a 4:3 height
correction (2x across, 2.4x down). minifb then stretched that 4:3 image to the
client area with black bars. So the old default was a 1280x960 window with
nearest sampling and 4:3 pixels, but a resize produced a fractional, unspecified
scale, there was no integer/pixel-perfect fit, no filter choice and no
full-screen key.

## Options

| Constant | Values | Default | Meaning |
|---|---|---|---|
| `DisplaySettings::scale` | 1..=6 | 4 | initial window size: 320x240 (4:3) or 320x200 (square) times this |
| `FitMode` | `PixelPerfect`, `FillWindow` | `PixelPerfect` | largest integer multiple with black bars, or the whole frame fitted by aspect |
| `Filter` | `Nearest`, `Linear` | `Nearest` | linear only differs when the fit is fractional |
| `PixelAspect` | `Dos43`, `Square` | `Dos43` | 320x200 shown as 320x240, or as a square-pixel 16:10 image |
| `Prefs::fullscreen` | on/off | off | borderless monitor-sized window via `Alt+Enter`; session-only |

The defaults give 1280x960, the port's previous initial window and the
original's 4:3 pixels.

## Fit maths (`display::fit_rect`)

`base_height` is 240 for `Dos43` and 200 for `Square`; the logical unit is
320 x `base_height`.

- **Pixel-perfect**: `scale = min(window_width / 320, window_height / base_height)`.
  If `scale >= 1`, the destination is `320*scale` x `base_height*scale`, centred
  with `(window - destination)/2` black bars. If the window is smaller than the
  unit, the fractional fit below is used so the frame stays visible instead of
  overflowing.
- **Fill window**: compare the window with the target aspect `320 : base_height`.
  A wider window keeps the full height (`height = window_height`,
  `width = window_height * 320 / base_height`); a narrower one keeps the full
  width. The result is centred and never exceeds the window.

Examples (all tested in `display::tests`):

| Window | Settings | Destination |
|---|---|---|
| 1280x960 | default 4x 4:3 | 1280x960 at (0, 0) |
| 1920x1080 | default | 1280x960 at (320, 60) |
| 1000x700 | default | 640x480 at (180, 110) |
| 1920x1080 | square pixels | 1600x1000 at (160, 40) |
| 1920x1080 | fill window, 4:3 | 1440x1080 at (240, 0) |
| 1920x1080 | fill window, square | 1728x1080 at (96, 0) |

Nearest sampling maps a destination pixel to `dx * 320 / width` and
`dy * 200 / height`, so at an integer scale every source pixel becomes an exact
block and the 4:3 correction repeats rows the way the old 640x480 copy did.
Linear sampling maps pixel centres in 16.16 fixed point and blends the four
neighbouring source pixels with clamped edges.

`display::logical_point` maps a client position through the same rectangle, so
mouse steering and console/preference clicks follow the chosen fit and ignore
the black bars. It replaces `Screen::logical_point` in `main`; the old helper
and its tests remain for the fixed 4:3 mapping.

## Window limits and `Alt+Enter`

minifb 0.28 has no `set_size`, borderless or full-screen call. The game therefore
compares `display::window_request(settings, fullscreen)` with the request that
opened the current window and, when it differs, opens the new window first and
drops the old one. That happens when a display row changes while windowed, when
`Alt+Enter` toggles full-screen, or on the frame after `Alt+Enter` while a page
is open. The presenter buffer is reset and the next frame fills the bars.

Full-screen is a borderless `WS_POPUP` window at the monitor size. The size
comes from Win32 `GetSystemMetrics(SM_CXSCREEN/SM_CYSCREEN)` on Windows; other
platforms keep the configured window size (documented as a limitation, not a
claim of full-screen support). `Alt` is read with `GetAsyncKeyState(VK_MENU)`
because minifb 0.28's Windows scan-code table does not report the ordinary left
Alt; minifb's Alt/Menu keys are accepted too. Both calls are the only `unsafe`
in `ffe-rs` (scalar arguments, no retained pointers) and are recorded in
`docs/ARCHITECTURE.md` §2.2. On the toggle frame the loop `continue`s, so the
same Enter cannot also activate a page row, raise the set speed or launch.

## Persistence

`Escape` -> `Port Options` -> `Display Options` (or `Alt+Enter` for full-screen).
Scale, fit, filter and pixel aspect are written to `saves/display.cfg` as soon
as they change:

```text
scale=4
fit=pixel-perfect
filter=nearest
aspect=dos-4:3
```

`settings::load_display` rejects a symlinked, oversized (> 256 bytes) or
malformed file and keeps the default. Unknown keys are ignored so an older
build can read a newer file; a known key with a bad value rejects the file.
Full-screen is deliberately session-only so a launch always starts in a window.
The controls preset stays in `saves/controls.cfg` (`docs/CONTROLS.md`).

## Captures, tests and evidence

- The 320x200 frame, `Screen::present`'s 640x480 copy, every `--capture*` PPM
  and `--smoke-frames` keep the same pipeline and pixel format. The display
  options only affect the window and the new `--capture-display DIR` images.
- `--capture-prefs DIR` now also writes `prefs-display.ppm`; the original pages
  and space-colour frames are byte-identical to the baseline, and `prefs-port`
  changes only by the new port-only `Display Options` row.
- Unit tests: `display::tests` (fit rectangles, nearest/linear scaling, mouse
  mapping, config round trip), `prefs::tests::display_page_...`, `settings`'
  display round trip, and `tests/prefs.rs`'s display-page test.
- Evidence: `docs/evidence/display/` with real `PrintWindow` captures of a 2x
  and 3x window and of `Alt+Enter` full-screen at 1920x1080, plus
  `--capture-display`'s deterministic 2x/3x/square/linear/full-screen images.

## Out of scope (would need art/UI rework)

- Re-rendering the original art, models, fonts or HUD at a non-320x200 internal
  resolution. The 320x200 frame is authoritative; only the finished frame is
  scaled.
- Widescreen framing that shows more of the 320x158 view than the original, or
  moving the console/HUD. The original composition and hit areas stay fixed.
- Enhanced graphics, HD art packs, texture filtering, MSAA, refresh-rate or
  vsync selection (the port targets 60 Hz presentation in minifb).
- Per-monitor DPI/refresh handling and true platform full-screen on non-Windows
  builds; those are future platform work, not display preferences.
