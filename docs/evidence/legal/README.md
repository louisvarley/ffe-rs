# Legal evidence

- `legal-hope.png` — the port's 320x200 frame from
  `firing_at_hope_is_fined_and_shows_f236s_message` (new game at Ross 154,
  starport Hope): the F236 message in the original ten-entry log,
  "Unlawful discharge of weapon is not permitted here. Pay a $600 fine.".
  Regenerate with
  `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test npc_ai firing_at_hope -- --ignored --nocapture`.
- `original-legal-probe.jsonl` — the executed JJFFE 2.8a7
  `--legal-probe 166` rows at frames 164/165: the illegal-fire flag 0→4
  (`DATA_008860` bit 2), wanted 2→3 (`DATA_008646` bit 0), the fine ledger
  0→6000 at starport index 2, cash unchanged at 1000, and the original message
  text.
