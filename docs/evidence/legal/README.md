# Legal evidence

- `legal-hope.png` — the port's 320x200 frame from
  `firing_at_hope_is_fined_and_shows_f236s_message` on the **default start**
  (F829 record 0: Gateway, planet Hope, starport Old Blackelk, state `0x30`,
  Saker Mk III, 10,000 CR). It shows F236's message in the original ten-entry
  log: "Unlawful discharge of weapon is not permitted here. Pay a $600 fine.".
  Regenerate with
  `FFE_WORLD_CACHE=<data.bin> FFE_SHOT_DIR=<dir> cargo test -p ffe-rs --test npc_ai firing_at_hope -- --ignored --nocapture`.
- `original-legal-probe-default.jsonl` — the executed JJFFE 2.8a7
  `--legal-probe 166 0x39` (key `0x39` = "any other key" → F147(0), record 0)
  rows at frames 164/165: `start_type 0`, `player_state 48`, `player_index 87`,
  `ship 31`, `cash 10000`, illegal-fire 0→4 (`DATA_008860` bit 2), wanted 2→3
  (`DATA_008646` bit 0), the fine ledger 0→6000 at starport index 3, and the
  original message text. **This is the actual default-start frame the owner
  described, and it still prints $600 / owes 6000.**
- `original-legal-probe.jsonl` — the same probe at start type 1 (Ross 154,
  Eagle, 1000 CR), for comparison: same message and 6000 at starport index 2.
