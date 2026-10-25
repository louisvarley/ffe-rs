# Rank/legal status evidence

- `original-profile-federal.png`: JJFFE 2.8a7's F2 page, page 1 (F7), at the
  Ross 154 start. Captured locally with
  `.cache/jjffe-reference/jjffe-smoke.exe --game-frame 400 0x3c 0x41`
  (F2 then F7); 320x200, palette applied and scaled 3x for viewing.
- `profile-clean.png`: the port's `status_view` test after `Navigator::new_game`
  at Ross 154, before any offence.
- `profile-offender.png`: the same page after one unlawful discharge at the pad
  (`fines[2] = 6000`, `Federal: Criminal`, the offence in the record).
- `police-fine.png`: the port's `Contact Local Police` screen with the 600.0
  credit debt and F552's greeting on the console line.
- `police-clean.png`: after F544 pays the debt (F984's message).

The port frames are PPM captures from
`cargo test -p ffe-rs --test status_view -- --ignored` with `FFE_WORLD_CACHE`
and `FFE_SHOT_DIR`, converted to PNG locally. The executable original was run
for the profile frame only; the police conversation box is the port's station
list layout, not an original frame.
