# The game-over screen (F1104/F917)

Researched from `jjffe-master/ffe.asm`/`ffedat.asm` and compared with the
executed original through the headless adapter's new `--death-probe` mode. The
screen is **not a menu**: it is FFE's spinning tombstone, model 172, with text
engraved by the model's opcode-10 glyph records. There are no options, cursor
hot areas or input in this state.

## 1. Source map

| What | Where |
|---|---|
| Player destruction, no capsule | `F1104` (`ffe.asm:81458`, called from `F1102` at 81426) |
| Death-state entry, clear/flip, 0x3cae6 timeout | `F1104` 81466–81519 |
| Death-state first handler (`F1044` -> `F1045` -> `F1512`) | `ffedat.asm:18619`; `ffe.asm:77164` |
| Death-state second handler (`F1107`), timeout expiry -> `F1106` -> `F918(1)` | `ffe.asm:81557` |
| `F918(1)` -> `F917` (tombstone loop) | `ffe.asm:68148`, `68047` |
| Tombstone model choice | `F917` `ffe.asm:68069`: model 0xac (172) normally, 0xaf (175) when `DATA_008885 == 0xeb6973d` |
| Spin matrix, draw | `F1675_MatBuildYZT` (`ffe.asm:101537`), `F1681`/`F1683` (`102181`) |
| Model 172 text records | `models/172.cmd`; text codes `0x9c00`, `0x9c01`, `0x4085..0x4087` |
| Model 175 text records | `models/175.cmd`; text codes `0x9c02`, `0x9c03`, `0x4088`, `0x4089` |
| Screen strings | `DATA_005129` `"\rGAME\rOVER"`; `DATA_005130` `"\r…COMMANDER\r <name>\r DIED\r <date>"` (`ffedat.asm:5626`) |
| Engine text expansion | `F1344` (`ffe.asm:83896`), FF9C -> `F277` -> `DATA_005104` (`ffedat.asm:5243`) |
| Music/sound | `F1104` calls `F1002(0)` and sample `0x1f` (BOOM) before/after `F1098` |
| Return to front end | `F918(1)` -> `F917` -> `F914`/`F915` -> longjmp `DATA_009125`, back in `_asmmain` |

The string table is at `FILE_OFFSET 0xC0208` in `FIRSTENC.EXE`; `DATA_005104`
starts at `0xBF94C` (VA 0x4D5D4C) and its first four entries are the game-over
strings. Model 172's text records select them through the FF9C handler.

## 2. Screen and flow

1. **Destruction** (`F1104`): if the player has no escape capsule (`F1098`
   returns 0), `DATA_008870 = 0x2c`, the buffer is cleared and flipped twice,
   the console hot areas are cleared, view `DATA_008872 = 4` (external), the
   timeout `DATA_008884 = 0x3cae6` (248,550 tics ≈ 5.0 s), and sample 0x1f is
   played. `F1044`/`F1045`/`F1512` keep the death camera rotating. There is no
   HUD text or player input in this phase.
2. **Timeout** (`F1107`): after 5.0 s, `F918(1)` runs `F917`.
3. **Tombstone** (`F917`): black 320×200 frame with model 172 rotating about
   its vertical axis. The loop starts at `0x3d090`, subtracts `DATA_009155`
   every frame and passes `remaining >> 1` as F1675's yaw; it ends at
   `-0xAAE60` (about 19.1 s at 1x). The box has two engraved faces:
   - **obituary face** (text normal 2, vertex 0): `RIP` (`0x4086`) and
     `COMMANDER <name> DIED <date>` (`0x9c01`), with the cross/flower strings
     `0x4085`/`0x4087`;
   - **GAME OVER face** (text normal 4, vertex 3): `GAME OVER` (`0x9c00`).
   The date is `F1356`/`F1358`'s `1-JAN-3250` form, using `DATA_008807`.
4. **Front end**: at the end `F918(1)` longjmps to `_asmmain`, which restarts
   the intro/title. The title's choices 1–5 (including the Commander Load
   screen) are the only "options" after death; the tombstone itself has none.

The rare model 175 is selected when `DATA_008885 == 0xeb6973d`; it is the
underlined `__name` / `__date` memorial variant (`0x9c02`/`0x9c03`), not the
normal slab. New games take the model 172 branch.

## 3. Escape capsule

`F1098` is called before the screen. It looks at `ship+0xc8` bits 3 (`0x8`) and
27 (`0x8000000`), allocates object model `0x4f`/`0x4e`, moves the player into
it and returns non-zero. `F1104` then returns without setting state 0x2c: the
game continues in the capsule, so the tombstone is not shown. The `death`
stream owns F1098; the port's `GameOver` is only entered when its outcome is
"no capsule".

## 4. Port implementation

- `crates/ffe-rs/src/gameover.rs`: pure `Phase`/`GameOver` state machine
  (`DEATH_VIEW_TICS`, `TOMBSTONE_START_TICS`, `TOMBSTONE_END_TICS`), model
  choice, F917 angle, and `announce` (stop music + sample 0x1f).
- `GameOver::draw` sets the commander/date on `ModelViewer` and calls
  `Screen::draw_game_over`.
- `ModelViewer::set_game_over_text` expands FF9C00..`03` for the model text
  records. `Screen::draw_game_over` renders model 172/175 on black, applies the
  external view's 30°/-20° camera, and uses a full 320×200 clip
  (`ViewHeightGuard`) because the slab reaches below the flight view's y=158.
- The port's glyph normals for model 172's text are degenerate (the culled
  obituary normal never flips), so the screen selects the face from the spin's
  half-turn: yaw bit 0x8000 -> obituary face (normal 2), otherwise GAME OVER
  (normal 4). This is the one documented reconstruction, not an invented
  screen.
- This branch is rebased on the `death` stream's F1104/F1098 port. Its
  `Death::Exploding` hold is kept; when `Death::is_game_over()` becomes true,
  `main` starts `GameOver::tombstone` and the screen owns the frame until its
  spin ends, then returns to the front end. The earlier text-page placeholder
  and its `N`/`Q` options are gone. `GameOver::announce` remains the standalone
  music-stop/boom helper; the merged death path does not call it twice because
  `death::play_destroyed_audio` already posts F942/F1104's cues.
- Scripted capture: `cargo run -p ffe-rs -- --root <repo> --capture-gameover
  <dir>` starts the tombstone with the new game's date and saves 640×480 shots
  every 20 frames.

## 5. Evidence and tests

- `docs/evidence/gameover/game-over-320.png`: port frame, model 172 GAME OVER
  face.
- `docs/evidence/gameover/obituary-320.png`: port frame, RIP/obituary face.
- `docs/evidence/gameover/original-game-over-990.png` and
  `original-obituary-800.png`: the executed original, `--death-probe` frames
  990 and 800 (the same model and strings).
- `docs/evidence/gameover/port-live-capture.png`: the live port loop, captured
  by `--capture-gameover`.
- Unit tests: `gameover::tests` (phase timing, F917 angle/end, model choice,
  one-shot music stop and sample 0x1f).
- Local render test: `crates/ffe-rs/tests/gameover_view.rs` (ignored; needs
  `FFE_GAMEOVER_CACHE`), checks the tombstone and both engraved faces.
- Original probe: `tools/reference/headless.c --death-probe N` calls the real
  `F953` on the player and writes PPM frames; the reference build is local-only.

## 6. Remaining owner decisions

- The commander name is passed by `main` from `death::COMMANDER_NAME`; it is
  still a constant until the save/commander stream adds a name field.
- The 5 s external-view hold is the `death` stream's `Death::Exploding` phase;
  the tombstone starts when it expires. `GameOver::new` retains the same timing
  for standalone callers/tests.
- The commander name is passed by `main` from `death::COMMANDER_NAME`; it is
  still a constant until the save/commander stream adds a name field.
- Model 175's rare branch is ported but its boulder geometry is only partly
  visible at the straight F917 pitch; it is unreachable in a normal new game.
- The exact obituary text scale/line positions are the glyph model's; the
  port's `size` handling reproduces the original's layout closely but not
  pixel-exactly (the original's font-0 line anchors are not used elsewhere).
