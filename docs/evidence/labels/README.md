# Ship label evidence frames

Before/after frames for the F989/F1108 label anchor fix (2026-10-25). The label
was drawn at the object's projected point `+(1, 0)`; F989 passes `(x + 3, y - 4)`
to F1108, which adds one to x, so the original pen is `+(4, -4)`. The missing
four-row lift made labels hang below the object, and the top edge had no signed
clipping. The glyph size was already the original's main font (verified against
the executed F1584/F1598; see `docs/notes/labels.md`).

- `single-before.png`, `single-after.png` — F694's pirate from the NPC AI
  sequence (`crates/ffe-rs/tests/npc_ai.rs`, `npc-ai-before.ppm`, 80 km), the
  locked `Moray Starboat 3`. The before frame's pen is the ship's projected
  centre + (1, 0); the after frame's is + (4, -4), so the name is vertically
  centred on the hull. `single-before.png` is the committed
  `docs/evidence/npc-ai/before.png`.
- `live-before.png`, `live-after.png` — the real game
  (`ffe-rs.exe --root . --skip-intro --capture-traffic <dir>`, frame 79):
  the starport's label and the locked target's range text at the launch point.
  Rebuild with the same command; `traffic-79.ppm` is the source frame.
- `several-before.png`, `several-after.png` — three traffic ships, a locked
  target, and two labels at the view edges in one frame
  (`crates/ffe-rs/tests/npc_view.rs::several_ship_labels_and_a_label_at_the_view_edge`,
  `FFE_SHOT_DIR`). The "TOP EDGE" label is clipped to rows 0.. in the after
  frame (pen y -1) and "RIGHT EDGE" starts at pen x 312.
- `label-ship-after.png` — one ship's label with the models left out, so the
  ink box can be measured against the ship's projected rect
  (`system_ship_label_sits_at_the_projected_centre_in_the_fixed_font`).

The `.png`s are 2x logical (640x400); the source frames are local-only and not
committed. The names are the port's model-name-plus-serial strings, not the
original's F703/F791 registrations, so they are wider than the original's labels
(see "Known differences" in `docs/notes/labels.md`).
