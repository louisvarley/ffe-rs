# Game-over screen evidence

- `game-over-320.png` — the port's tombstone, GAME OVER face (model 172), from
  `crates/ffe-rs/tests/gameover_view.rs`.
- `obituary-320.png` — the port's tombstone, RIP / `COMMANDER JAMESON DIED
  1-JAN-3250` face, same test.
- `original-game-over-990.png`, `original-obituary-800.png` — the executed
  original (JJFFE 2.8a7 through `tools/reference/headless.c --death-probe`),
  frames 990 and 800 after the player's F953 destruction. Capture these locally;
  the compared model, strings and face layout are the same as the port's.
- `port-live-capture.png` — the live port loop, `--capture-gameover` (tombstone
  with the new game's date, mid-spin).

The frames are 320×200 originals except `port-live-capture.png`, which is the
window's 640×480 upscaled shot. The exact commands are in
`docs/notes/gameover.md` §5.
