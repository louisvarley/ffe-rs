# Modern graphics: the switch, the separation and the first technique

Status (2026-10-27): **first slice implemented.** `crates/ffe-rs/src/modern.rs`, the presenter in
`crates/ffe-rs/src/display.rs`, the Port Options rows in `prefs.rs`, `Ctrl+G` in `main.rs`. Deviation
ENH-021. Design background: `docs/RENDERING_DESIGN.md` (this is its "master button" and a first
presentation-only feature, without the wgpu backend yet).

## How the two engines are kept apart

```
game state ──► Screen (the original's drawing, unchanged) ──► 320x200 logical frame
                                                                   │
                                     ┌─────────────────────────────┴───────────────┐
                                     ▼                                             ▼
                        Classic: display::blit                   Modern: modern::ModernRenderer
                        (nearest/linear scale, 4:3)              (its own techniques, window res)
                                     └──────────────► window ◄─────────────────────┘
```

- The game always draws the original's frame, the same in both modes. Captures, the reference
  comparisons, input hit-testing (`display::logical_point`) and saves all use that frame and are the
  same in both modes.
- The modern engine receives only `&[u32]` (the finished frame) and a destination rectangle. It cannot
  reach the simulation, the RNG, the clock or input; it allocates its own buffers.
- The choice is `DisplaySettings::graphics` (`modern::Graphics::{Classic, Modern}`), checked once per
  presented frame. Flipping it is instant and needs no restart, reload or state change.
- Classic is the default (first run and any config without `graphics=`), and its output is
  byte-for-byte what the presenter produced before this change.

## Switching

- **Escape → Port Options → Graphics: Classic / Modern** (right-hand column; the icons are click
  targets like the original's, and the keyboard rows work too).
- **Ctrl+G** flips it from anywhere; a message names the new mode. (Plain G is still the galaxy map;
  with Ctrl held it is not.)
- Saved in `config.cfg` as `graphics=classic|modern` with the other display choices.

## First technique: edge-directed upscaling

The original's frame is 320x200, and every scaler so far either repeats pixels (blocky staircases on
every polygon edge, line, circle and letter) or blurs them. The modern engine instead enlarges the frame
4x with **Scale2x applied twice** (the AdvMAME rules: a pixel's corner takes a neighbour's colour only
where the two neighbours on that side agree and the other two do not), giving 1280x800. That turns
staircases into clean diagonals and rounds circles and letters, but leaves flat areas and one-pixel
details exactly as they were: no new colours and no blur. The 1280x800 image is then resampled
bilinearly into the window's fit rectangle, which also applies the DOS 4:3 stretch smoothly (Classic
repeats every fifth row).

Why first: it is presentation-only by construction, cheap (a few milliseconds a frame), needs no new
assets or GPU code, improves every screen at once (flight, HUD, menus, maps), and exercises the switch
and the separation that every later technique will plug into.

Limits: it works from the 320x200 pixels, so it cannot add detail the frame does not have. The
crosshair, the station's edges and text improve most; textures and the planet's surface mostly keep
their look. Tests: `modern::tests` (the Scale2x rules) and
`display::tests::the_engines_share_the_frame_and_only_modern_smooths_it` (same frame, frame untouched,
Classic unchanged and blocky, Modern smooth only on edges). The presentation evidence
(`--capture-display DIR`) now writes `display-modern-4x.png` beside the Classic images.

## What comes next (in the order `RENDERING_DESIGN.md` gives)

1. **Native-resolution 3D** (ENH-002), the step that adds real detail: the model executor already
   produces the scene as shapes; for Modern, carry the vertices' view-space positions (not only the
   original's integer screen points) and rasterise the 3D view at the window's resolution, then put the
   original HUD over it. The executor's results stay the same; only a second, higher-precision
   projection is added in the modern engine.
2. **Smooth shading and anti-aliased edges** on that native-resolution path (ENH-003/005 in software,
   or through wgpu once the backend is chosen).
3. **HD images** (ENH-001) from an owner-upscaled pack, resolved by stable asset ID.
4. A higher-quality starfield and glows (ENH-014/015) drawn from the same game data.

Each one is another technique inside `modern`, selected with the same switch, and Classic stays
exactly as it is.
