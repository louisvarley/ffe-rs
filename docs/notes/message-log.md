# Message log (`F271`/`F272`/`F273`) — layout, fix and evidence

Status 2026-10-25. This note corrects the `hud` stream's message-window model and
records the owner-reported "huge, overlapping log over the console" fix.

## 1. Source map (DOS FFE, `jjffe-master/ffe.asm`)

| Item | Source | Value |
|---|---|---|
| Ring | `F271` `20623-20790`; `DATA_008681` `ffedat.asm:27851` | 10 entries, stride `0x40e`, date/time `DATA_008678/79`, flag `DATA_008682` |
| Entry wrap | `F271` `20650-20656` (`push byte +0x78`) | **120 px** (`F1398_StringWrap`) |
| Display wrap | `F272` `21036-21043` (`push byte +0x7c`) | **124 px** |
| Header | `F272` `20804-20842` | current date + space + time + `\r`; colour escape `0x01 0x04` |
| Newest entry | `F272` `20844-20878` (`[ebp-0x4]` starts 1) | text only; the header is its date |
| Older entries | `F272` `20880-21018` | date + space + time + `\r`, then `0x01 0x06` + entry + `\r`; a date line per older entry |
| Window | `F272` `21054-21097` | text x **196**, y **159 − scroll**; shadow x **195**, y **158 − scroll** |
| Clip / line height | `F1586` `93785-93800`, call `21070-21097` | `ymin 158`, `ymax -1`, height **8 px**, five rows 159..199 |
| Scroll | `F273` `21114-21130`, `21140-21141` | actions `0xe/0xf`, integer `DATA_005096` clamped to **−0x18** (the port's positive "older" pixels), auto-scroll after **0x3cae6** tics `21178-21208` |
| Draw order | `F272` `21021-21097` | panel 43, scanner globe, console buttons, gauges, contacts, **then** the log text; the log is the top layer |
| Speed unit | `F1393` `85000-85048`; `DATA_007653/4` `ffedat.asm:23255-23258` | `km/h` with 1 dp below `0x4705d` units, `km/s` with 2 dp above; ` kmh`/`kms` glyphs |
| HUD speed strings | `DATA_007310/312` `ffedat.asm:18945-18953` | `Set:`<speed>`\rAct:`<speed>`\rAlt:`<m>`\r`registration` |

The window is **top-anchored**. `F272` draws line 0 (the header) at y 159 and
each later line 8 px lower; `y = 159 − scroll` moves the whole block up, so a
positive scroll reveals older entries from below. At boot one stored entry is
two rows: the dated header at y 159 and the undated text at y 167.

## 2. Root cause of the owner's frame

The port had four independent faults:

1. **Bottom anchoring** — `messages.rs::window_start` computed
   `content_height − window − scroll` and drew only the *last* five lines. With
   ten entries it showed the **oldest** messages and dropped the header; the
   source shows the header and the newest entries first.
2. **Every entry stamped** — `lines()` prefixed each entry with its date/time on
   the same line *and* drew the current `clock` as a separate header. The source
   puts the newest entry undated under the header and gives older entries a
   separate date line. The result was the reported `1-Jan-3250 12:26:56 Set
   speed 0` / `… Autopilot:` rows.
3. **Spam** — the autopilot range and `Set speed …` were pushed as new entries
   every time the text changed, filling the five-row window with near-identical
   dates. F271 has a replace flag (`ffe.asm:20776-20792`) for exactly this.
4. **Units/templates** — speed was printed in m/s (HUD `actual_speed` was in
   flight units, `set_speed` in m/s) and the log line repeated "speed" twice;
   F1393 prints `km/h`. The boot hint was three wrapped lines instead of one.

The font itself was already correct: `base:font/main` is the original 9-row,
8-px-advance font and `F1586`'s line height is 8. The "huge" look was the dense
five-line block of duplicated dates and the 120+ px lines, not a wrong font
size.

## 3. Fix

- `crates/ffe-rs/src/messages.rs`
  - `ENTRY_WRAP = 0x78`, `WRAP = 0x7c`, `VISIBLE_ROWS = 5`.
  - `lines()` returns `Line { text, header }` top-to-bottom: header, newest
    entry undated, then each older entry's date line + text.
  - `first_line()` / top-anchored drawing; `scroll_by` is unchanged externally
    but now moves the block up from the header.
  - `update()` implements F271's replace flag.
- `crates/ffe-rs/src/screen.rs`
  - `draw_messages` draws `line i` at `159 − scroll + 8 i`, clips 158..200,
    header colour 17, entry colour 15, shadow 4 at (195, y−1).
  - `messages_visible` lets the places/navigation list hide the log.
  - `scroll_messages(pixels)` exposes F273's step for Page Up/Down.
- `crates/ffe-rs/src/main.rs`
  - boot message is `Landed at <name>.`; autopilot updates no longer carry a
    range; `Set:`/`Autopilot…` lines replace the newest entry; Page Up/Down
    scroll the log while flying.
- `crates/ffe-rs/src/hud.rs`
  - `speed_text` implements F1393 (`m/s × 3.6`, 1 dp km/h, 2 dp km/s above
    99,490.9 km/h).
  - Set and Act are separate lines (`Set:…`/`Act:…`); Alt and registration
    follow, as `DATA_007310/312`.

## 4. Evidence (`docs/evidence/message-log/`)

| File | What |
|---|---|
| `before-boot.png` | old two-line boot: header plus `1-Jan-3250 12:26:56 Docked.` |
| `before-full.png` | old ten-entry frame: oldest entries, five rows of repeated stamps |
| `after-boot.png`, `after-boot-4x.png` | header y159 + `Docked.` y167 only; buttons at y177+ clear |
| `after-multi.png` | header, newest entry, then each older entry's date line |
| `after-scrolled.png` | F273 0x18 scroll: header off the top, older entries at y159.. |
| `after-places.png` | places list open; the log stays in x196..320, y158+ |

## 5. Tests

- `cargo test --workspace --locked`, `cargo clippy --workspace --all-targets
  --locked -- -D warnings`, `cargo fmt --all` green.
- Unit (`messages.rs`): boot is header + one undated entry; older entries get a
  separate date line; entry wrap ≤120 and display wrap ≤124; top anchoring;
  scroll clamp/auto-scroll; replace updates the newest entry.
- Unit (`hud.rs`): F1393 km/h/km/s strings; Set/Act/Alt lines.
- Local frames (`tests/messages_view.rs`, ignored): boot has no log pixels
  outside x195..320/y158..176; multi-entry and 0x18 scroll frames; the places
  list is pixel-identical with the log hidden or shown.
- `tests/dashboard_view.rs` still pins the log to x195..320/y158..200.

## 6. Remaining differences / owner decisions

- Header colour 17 and entry colour 15 are the port's approximations of the
  original's runtime `COLPAL` values (4/6); the shadow is palette 4.
- `F1393`'s exact constant is 0.342 km/h per original velocity unit; the port's
  `MM_PER_SECOND` (97.07 mm/s) makes that 0.349 km/h, so `m/s * 3.6` differs
  from the original display by about 2%. The switch to km/s at 99,490.9 km/h is
  the source threshold.
- The date/time string is the port clock's (`galaxy_map::date_text`), same
  format as `F1358/F1363` but not re-derived from the original routines.
- `F273`'s auto-scroll target is based on the newest entry's line count in the
  original; the port's `scroll_by` now uses `line_count * 8 - 8`, capped at
  0x18, and auto-scrolls after the same 0x3cae6 tics.
- The original draws the log after the right-hand console buttons, so a long
  multi-entry log can cover those icons; the port now does the same. At boot
  (two short rows) and with typical one-line messages the buttons stay clear.
- The docked screens keep their separate two-line note (`screen.rs` docked
  branch); the station screens have not been re-derived here.
- Page Up/Down are wired only while flying with the maps closed, since those
  keys already page the system/places lists and the ship list.
