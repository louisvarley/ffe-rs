# Rank, reputation and the F2 profile / fines office

This slice adds the commander's military and Elite standing, the F2 profile page
(F312) and the police/fines contact (F543/F544/F545/F552). The owner's anchor is
the F2 page's `LEGAL STATUS` block and the police line **"Pay $600.00 before
midnight to go free."** The words, thresholds and messages below are byte
decoded from JJFFE 2.8a7 (`jjffe-master/ffe.asm`, `ffedat.asm`), not invented.

## 1. Source map

| What | Where |
|---|---|
| F2 page selector | `F286` (`ffe.asm:22279`); F292's actions 0xf6..0xfc (`ffe.asm:22866`) open pages 0,1,2,4,3,5,6. Page 1 is F312. |
| F2 profile page | `F312` (`ffe.asm:24770`): `DATA_005136` title, `DATA_005189` status block, `DATA_005190` rank line, then `DATA_009093`'s record and `DATA_009091`'s medals. |
| F2 page keys | F6 action 0xf6 -> page 0 (F283's ship equipment status); F7 action 0xf7 -> page 1 (F312). The port accepts F6/F7 only while the F2 page is open. |
| Page title | `DATA_005136` = `PROFILE FOR COMMANDER <name>`, drawn at (30, 5) by `F318` (`ffe.asm:25609`). |
| Status template | `DATA_005189` (`ffedat.asm:5806`); `0xfd <n>` sets the cursor x to `2*n` (`F1348` -> `F1587`): 114 for the right column, 150 for its values. |
| Rank line | `DATA_005190` (`ffedat.asm:5809`), drawn at y 30 under the header. |
| Elite words | `DATA_005191`-`DATA_005200`, selected by `DATA_005110` (`ffedat.asm:5491`). |
| Legal words | `DATA_005201`-`DATA_005205`, selected by F311 (`ffe.asm:24749`) from `DATA_005109` (`ffedat.asm:5488`). |
| Federal ranks | `DATA_005206`-`DATA_005218` (13 words, `None`..`Admiral`); promotion by F304/F306 (`ffe.asm:24395`, `24459`). |
| Imperial titles | `DATA_005219`-`DATA_005231` (14 words, `Outsider`..`Prince`); F305/F306. |
| Military thresholds | `DATA_005114` (`ffedat.asm:5514`): 1, 16, 81, 256, 625, 1296, 2401, 4096, 6561, 10000, 14641, 20736, 65535. `F306` ranks by the count of reached thresholds. |
| Elite score | `DATA_008911`; F942 (`ffe.asm:70650`) adds the destroyed ship statistics' `+0x16` word and prints `DATA_005861` when `score >> 16` changes. |
| Military points | `DATA_008912` (Federal), `DATA_008913` (Imperial); F300/F301 (`ffe.asm:23830`..) call F304/F305 on mission reward. |
| Medals/permits | `DATA_009091`, awarded by F307 (`ffe.asm:24584`); names `DATA_005242`-`DATA_005257`. |
| Fine ledger | `DATA_009095`-`DATA_00909A` (six dwords at `DATA_008804 + 0x50f8`); F236 adds `fine*10` (`ffe.asm:18823`). |
| Ledger index | `DATA_008996`, set by F869 (`ffe.asm:42692`) from the system word's `>> 6` **allegiance**, and used by F236/F655/F312. |
| Offence records | `DATA_009093` (six dwords at `DATA_008804 + 0x50e0`); F236 sets bit `offence`. |
| Record headings | `DATA_005235`-`DATA_005240`: `INTERPOL`, `IMPERIAL`, `FEDERAL`, `ALLIANCE`, `THARGOID`, empty. |
| Fines office entry | F552 (`ffe.asm:35294`): current fine > 0 gives the `Pay fine` prompt and `DATA_005983`'s police greeting. |
| Pay fine | F544 (`ffe.asm:35028`): `cash -= fine`, clear the slot, `DATA_005984` success row, then `DATA_005983`'s "Hang up." |
| Refusal | F545 (`ffe.asm:35077`): `DATA_005985` "You've until midnight to pay ... or be put in jail..." and the fine remains. |
| Short messages | F366 (`ffe.asm:27854`): `DATA_005990` "You don't have enough cash." / `DATA_005991` "Fine paid by remote transfer." |
| Police spawn response | F238/F239 (`ffe.asm:18939`, `19041`); the port still only provokes existing police/military traffic (F783 clears the alert). |

### The ledger index is the system allegiance, not a starport ordinal

`DATA_008996` is `F869`'s `[system record +0x2c] >> 6`; `F655` checks
`DATA_009095[DATA_008996]`. The earlier legal note called the six slots
per-starport. The data makes the real mapping clear:

- `DATA_009095/96/97` are entries 0/1/2 of one six-dword array, and
  `DATA_009097`'s `resb 16` covers entries 2..5.
- F312 prints `DATA_009097` under `Federal:`, `DATA_009096` under `Imperial:`
  and `DATA_009095` under `Interpol:`.
- The heading is `DATA_005235 + DATA_008996`, whose words are
  `INTERPOL, IMPERIAL, FEDERAL, ALLIANCE, THARGOID`.
- The executed default start's legal probe records the fine in slot 3, and the
  status heading there is `ALLIANCE`; Ross 154's executed F2 frame is
  `FEDERAL` (slot 2).

So the six slots are jurisdictions: 0 Interpol/Independent, 1 Imperial,
2 Federal, 3 Alliance, 4 Thargoid, 5 empty. F236 records against the current
system's slot, and F655 refuses clearance when that slot is >= 10000. The port's
`Navigator::jurisdiction` implements this; `starport_in_range` is the F234
witness/range test only.

### Rank words and caps

- Elite rating: `DATA_005110` is walked from its highest entry down. The rating
  word is `eax + 1`, so `<1024` is Harmless and `>=1536000` is ELITE.
- Military rank: `F306` walks `DATA_005114` upward and returns the count of
  reached thresholds. 0 points is `None`; 65535 and above is index 13. The
  **Imperial** table has 14 words, so index 13 is `Prince`. The **Federal**
  table has 13 words, so index 12 (`Admiral`) is the cap; the source's index 13
  would read one word past `DATA_005218` into `DATA_005219` (`Outsider`), which
  is a data-table overrun, not a rank. The port caps Federal at `Admiral`.
- F306's promotion/demotion strings are `DATA_005186`/`DATA_005187` ("Your have"
  is the source's typo).

## 2. What the port implements

- `ffe_core::status`: every word and threshold above, `Career`, `Profile`, F311's
  five legal-status bands, F307's medal names and the F544/F545/F552 messages.
- `ffe_core::npc::Legal`: adds `records[6]` (`DATA_009093`), `record()`,
  `status()` and `pay_fine(cash, jurisdiction)`.
- `ffe_core::npc::Career`: `federal`/`imperial` points, `elite` score and
  `medals`. `award_federal`/`award_imperial` mirror F306's signed add and
  promotion/demotion result; `award_kill` mirrors F942; `award_medal` mirrors
  F307.
- `ffe-rs::navigation`: `jurisdiction` from F869's allegiance; `fine_here`,
  `pay_fine`, `profile`; F236 records against the jurisdiction; F655 reads it;
  F942 adds a destroyed ship's `rating` (`+0x16`) to the Elite score and
  reports `rating_changed`.
- `ffe-rs::ships`: the ship statistics' `+0x16` word (`ShipStats::rating`),
  carried through `npc::Spec`/`NpcShip`/`Persisted`.
- `ffe-rs::screen`: F312's layout at the original positions (`draw_profile`).
  The F2 page's message window is hidden while the profile is open, as the
  original's upper-view page covers it.
- `ffe-rs::main`: F2 opens the ship page; F7 selects the profile, F6 returns;
  the console's equipment page is only F2's page 0. F942's rating change writes
  `DATA_005861`'s "Message from Elite Federation of Pilots: Right on, Commander!".
- `ffe-rs::station`: `Screen::Police` (F552) reached from the Shipyard's
  "Contact Local Police" row. It shows `Pay fine` with the current fine (or
  `Hang up.` when clean). Paying calls F544: enough cash deducts the debt and
  clears the ledger slot; the `DATA_009093` offence bits are permanent.
  Otherwise F545's refusal leaves the debt in place.

## 3. Evidence

- `crates/ffe-core/tests/status.rs`: the threshold/word tables, signed military
  awards and messages, Elite score/rating changes, medals, F236's record bit,
  F311's bands, F544/F545 payment/refusal and F312's three named slots.
- `crates/ffe-rs/tests/status_view.rs` (ignored/local cache): Ross 154 start
  (`jurisdiction == 2`, `FEDERAL`), clean profile -> fire (`fines[2] = 6000`,
  `Federal: Criminal`, the offence in the record) -> police contact -> refusal
  at 1000 tenths -> pay at 10000 -> clean. Frames with `FFE_SHOT_DIR`:
  `status-profile-clean.ppm`, `status-profile-offender.ppm`,
  `status-police-fine.ppm`, `status-police-clean.ppm`.
- `docs/evidence/status/`:
  - `original-profile-federal.png`: the executed JJFFE 2.8a7 F2 page at the
    Ross 154 start (`jjffe-smoke.exe --game-frame 400 0x3c 0x41`, the 400th
    frame). It shows `PROFILE FOR COMMANDER Jameson`, `Federal Rank: None`,
    `Imperial Title: Outsider`, `Elite Rating: Harmless`, `Federal: Clean`,
    `Imperial: Clean`, `Interpol: Clean`, `FEDERAL CRIMINAL RECORD` and
    `MEDALS AND PERMITS`.
  - `profile-clean.png`, `profile-offender.png`: the port's page in the same
    state and after one unlawful-discharge offence.
  - `police-fine.png`, `police-clean.png`: F552's contact before and after F544
    pays the fine. The console line carries F983/F984's text; the station
    screen is the port's list layout (the original's conversation box is not
    drawn).
- Existing `npc_ai::firing_at_hope_is_fined_and_shows_f236s_message` still
  covers F236/F238/F783's response and the `legal-hope.png` frame.

## 4. Differences and owner decisions

- **F2 page 0 text.** The port's F2 page 0 is the console equipment page
  (`F1120`); the original also draws `DATA_005133`'s "SHIP EQUIPMENT STATUS"
  block over the view. This slice adds the profile page; the page-0 text block
  remains unmapped (the equipment icons are the current stand-in).
- **Police spawns.** F238/F239's extra police objects still need F927's slot
  allocator and the F239 object template. The port provokes every existing
  police/military ship and sets `alert`; it does not spawn a new patrol. This is
  the one remaining item of the original's police response.
- **Military awards.** F300/F301's mission rewards are not ported (mission
  completion is still absent), so `Career::award_federal/imperial` has tests but
  no live caller yet. The mission layer should call it when it lands.
- **Police officer name.** `DATA_005983` inserts a person name
  (`0xff 0x92 0x04`); the port has no police-person name source, so the greeting
  says "I'm Sergeant of the ... Police". The rank word and the rest of the
  source sentence are exact.
- **Station conversation box.** F552/F544/F545 are a conversation screen in the
  original, with the officer's line and response rows in the station panel. The
  port uses its existing list screen (one `Pay fine`/`Hang up.` row) and the
  console message for the line. The text and the cash/record effects are the
  source's.
- **Federal rank index 13.** Capped to `Admiral` as discussed above; keeping
  the source's raw index would print `Outsider` for a Federal commander at
  65535+ points.
- **Legal-note correction.** `docs/notes/legal.md` still calls `DATA_009095` a
  per-starport ledger and `DATA_008996` a starport ordinal. The code now follows
  the allegiance mapping above; that note's table/route descriptions should be
  read with this correction.
