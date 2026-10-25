# Default start: Escape, services and takeoff (2026-10-25)

The DOS default new game (F829 record 0: Gateway, Hope, Old Blackelk, Saker Mk
III, 10,000 CR, state `0x30`) compared with the executed original through
`jjffe-smoke --game-frame`, and with the port's new `--capture-start` script.

## Original (`jjffe-smoke --game-frame`)

The reference adapter sends the intro's "any other key" (`0x39`) at frame 160,
then one scripted key every 40 frames from frame 200. `game-frame.bin` holds
the final 320x200 frame and palette; the PNGs are that frame expanded 2x.

| Frame | What | Command |
|---|---|---|
| `original-escape-1.png` | one Escape: the landed view is frozen; no page | `jjffe-smoke --game-frame 500 0x01` |
| `original-prefs.png` | two Escapes: the preference pages | `jjffe-smoke --game-frame 620 0x01 0x01` |
| `original-escape-3.png` | three Escapes: back to the landed view, still stopped | `jjffe-smoke --game-frame 660 0x01 0x01 0x01` |
| `original-services.png` | plain F4: `Old Blackelk Services` over the view | `jjffe-smoke --game-frame 500 0x3e` |
| `original-services-back.png` | F4 then F1: back to the landed view | `jjffe-smoke --game-frame 620 0x3e 0x3b` |

`--game-frame` cannot hold a key, and only its scripted key edge is sent, so the
takeoff's set-speed input cannot be scripted in the reference. The source map
instead pins the action: `DATA_007688`'s plain-F4 row byte is `0xf4` (station
module event 1, `F343`, which tests `0x2a`/`0x30` and calls `F333`), plain Tab
is action `0x09` (`F1056`'s launch, `JUMP_004417`) and plain L is action `0x6c`
(the label toggle). `verify-takeoff.py` checks all three.

The smoke adapter fails on the station's AVI (`DirMakeAVIName`); the services
frame used a local, ignored build of `tools/reference/headless.c` with
`DirMakeAVIName` returning a missing path, as `docs/notes/station.md` describes.

## Port (`--capture-start`)

```
cargo run --locked -p ffe-rs -- --root . --capture-start DIR --skip-intro
```

The script presses Escape at frames 10/20/30 (pause, pages, back), P at 40
(resume), F4 at 50 (services), F1 at 60 (back), L at 70 (launch) and holds the
set speed from frame 80, writing every tenth frame.

| File | What |
|---|---|
| `ours-escape-1.png` | Escape 1: paused; the log says "Paused. Escape again for the preference pages." |
| `ours-prefs.png` | Escape 2: the original pages (page 5 is the documented Port Options addition) |
| `ours-escape-3.png` | Escape 3: back to the landed view (still paused) |
| `ours-services.png` | F4: `Old Blackelk Services`, the same rows/status/hint |
| `ours-services-back.png` | F1: back to the landed view |
| `ours-takeoff-launch.png` | L at the pad: `0x24` (F1013's climb) |
| `ours-takeoff-climb.png` | W held: `0x24` -> `0x26`, `Set`/`Act` in the log |
| `ours-takeoff-free.png` | past F1031's `0x20` region: free flight, still accelerating |

The port's landed view at boot still differs from the original's (the starport
and horizon drawing is the `intro-fix2` stream); these frames are about the
input/state path.

## Tests

- `tests/prefs.rs::escape_pauses_then_opens_the_pages_then_returns` - F224's
  order.
- `tests/station_view.rs::default_start_services_board_is_old_blackelk` - the
  services board at the default start's starport.
- `tests/navigation.rs::default_start_takes_off_from_hope` - `0x30` -> `0x24` ->
  `0x26` -> `0x00` with F654/F651 and the bay released.
- `tests/navigation.rs::pad_takeoff_hands_over_and_falls_back_under_gravity` -
  Hope's F603 gravity and the planet as the ship's parent.
- `verify-prefs.py` checks `F224`, scancode 1 -> action 7 -> module 10;
  `verify-takeoff.py` checks F343, the F4/Tab/L table bytes and both starts.
