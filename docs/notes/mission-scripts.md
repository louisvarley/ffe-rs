# Mission scripts (F31/F41)

Status (2026-11-03): **script graph imported (cache v32); interpreter
ported (`ffe_core::script::engine`) and running in the playable
(`ffe_rs::scripts`): a new game (or an older save) starts them, each new
day runs F59's day ticks and F90's issues, docking runs F90's docking
message, and their state is saved with the flight (about 280 KB of JSON).
Every statement and target the real scripts use now reaches the game: the
station conversations, the journals, the ships and objects they make,
place, steer, damage and name, their contracts, fines, ranks and cargo,
and the game's signals (F953 damage, F316 contract rows, F383 prices, F633
station days, F60 time ticks). See "Mission ships and objects" below for
what is still a stand-in.** The message subtypes F59/F90 react to (15 a new day, 5
docking) are inferred: their senders were not found in JJFFE's code. Tracked
as dashboard item `gameplay-41`.

On a new game with no game hooks answered, F55 starts 20 instances
(missions 0, 64, 2-11, five of 19, 65, 72, 73) holding 94 subscriptions;
the first statements create the five journals: Imperial Herald (masthead
1), Federal Times (2), RIG (3), Frontier News (5) and Universal Scientist
(4), each with a first issue now, 28-day issues and 600-day retention
(`tests/mission_scripts.rs`).

**Deviation:** the evaluator cases the shipping game leaves empty for
event tokens (0x1c, 0x1d, 0x1e, 0x21, 0x26-0x29, 0x2c) return an
uninitialised stack word in the original. The port returns 1 while F48 is
dispatching the token's own event (with its keys) and 0 otherwise, which is
what the scripts are written to expect.

The storyline and special missions (Turner's Quest, the Thargoid campaign,
the special bulletin-board jobs, journal subscriptions, adverts), the ship
library's journals and several station conversations are not fixed code in
the original. They are scripts that a small interpreter runs.

## Where the scripts are

- `DATA_001697` ("Mission struct table", `ffebmp.asm:10433`) has 105
  pointers to mission structs. In the player's floppy 1.02 `FIRSTENC.EXE`
  the table is at RVA 0x8e0f8; the script region runs from RVA 0x801c4 to
  the table's end (0x8e29c), 57,560 bytes.
- **JJFFE's copy differs.** Its `ffebmp.asm` matches the executable only up
  to `DATA_000316` and its table sits at 0x8e28c; JJFFE's scripts were
  changed. The port imports the player's own executable.
- Every pointer in the region is in the executable's base relocations. All
  but ten point inside the region; those ten point at ten BSS words from
  RVA 0x20bde0, the missions' static words (`DATA_008540..` in JJFFE,
  which has eight).
- The asset (`base:script/missions`, `ffe_core::script::Image`) is the
  region with each pointer rewritten as `0x4000_0000 | offset` (region) or
  `0x8000_0000 | word` (static words); null stays 0.

## Records

- **Mission struct**: `+0x00` parameter target stream, `+0x08` parameter
  count, `+0x0c` kill state (-1 none), `+0x10` local word count, `+0x14`
  static words pointer, `+0x18` static word count, `+0x1c..` state
  pointers.
- **State**: `+0x00` entry stream, `+0x04` condition count, `+0x08..` one
  stream per condition: an F31 expression, then the F41 statements it runs.
- **Instance** (`DATA_008561`, 150 x 0x24): `+0x00` status (0 free, 1
  waiting, 2 running a condition, 3 entering a state, 4 starting, 5 being
  killed, 6 to free), `+0x04` id word and `+0x06` generation (the instance
  id is the dword), `+0x08` current state, `+0x0c` its index, `+0x10` next
  state, `+0x14` its index, `+0x18` mission number, `+0x1c` mission
  struct, `+0x20` the local words (`[0]` the instance id, `[1]` the
  creator's argument).

## Expressions (F31)

Prefix trees. Tokens 2-6 carry a dword: 2 a constant, 3 one of F21's
player values (ship id, rating, cash, ... date and time), 4 a global word
(`DATA_008548`, 71 words), 5 a mission static word, 6 an instance local
word. Tokens 7-9 are negate, not, logical not; 0x0a-0x1b the binary
operators (* / % + - << >> < <= > >= == != & ^ | && ||); the rest query the
game (ships by id through F1532, systems F461-F463/F857/F858, cargo,
ranks, random numbers F1476).

Tokens 0x26-0x2c (and others) are cases the shipping game leaves empty:
they evaluate their operands and return an uninitialised local. For
0x28-0x2c F31 reads two operands while the token table (and the scripts)
give one. The one place a script uses one of them (mission 83, state 4)
misparses in the original: the condition ends on a token with no case and
its "goto state 7" never runs. The port reproduces the exact reading.

## Statements (F41)

61 opcodes, read until opcode 0. 1 starts another mission (F52, with its
parameters), 2 if/else jump, 3 jump, 7 assign, 8 kill an instance (F38),
9 go to a state, 10-18 the journals (F101, F102, F103, F97, F98, F99, F100,
F104, F105), and the rest drive the game: station menus and console pages
(F326, F342, F345-F360), bulletin-board adverts (F331), schedules (F332),
ship spawning (F40, F664-F668, F297), damage and effects (F953, F942,
F926), fines (F236), ranks (F304/F305), cargo and mission records (F290,
F291), and object fields. F37 is the assignment: tokens 3-6 and the
object/cargo/rank targets, each raising its change event.

## Events

F44/F45 keep a binary tree of subscriptions keyed by `(event, a, b)`; F48
calls every match in order, then runs the scheduler F54. F53 subscribes a
state's conditions through F32, which walks each condition's tree with the
token records at `DATA_001701` (child count, event, key modes): reading a
variable subscribes the condition to that variable's change event, so a
condition is re-checked whenever one of its inputs changes. F47 is the
callback: it evaluates the condition and, when true, runs its statements.

- Event 1: F21 values; F58/F59/F60 raise it with `b` = 5..11 on the
  time ticks (0xc22e, 0x2d82c8, 0xaaaa6e0 tics) and on day, 7-day, 28-day
  and 365-day boundaries.
- Events 2, 3, 4: a global, static or local word changed (F34, F37).
- Event 7: an issue was published (F89, `a` journal, `b` article).
- Others (0x0a, 0x11, 0x12, 0x15-0x18) come from F37's object targets and
  from game code; the full list of F48 callers is still to be traced.

F57 handles game events: 1 frees every instance, 4 (new game) runs F55,
which clears the timers, starts mission 0 and runs F68 (the journals).

## Census (floppy 1.02, every reachable stream)

`tests/mission_scripts.rs census_of_statements_and_tokens` counts the
statements and tokens the real scripts use, to order the game hooks:

- **Statements:** assignment 7 (715), go to state 9 (320), if 2 (225),
  F345 response entries 0x17 (172), kill 8 (162), start mission 1 (124),
  F352 console 0x1d (101), jump 3 (94), F349 pages 0x16 (66), F346 0x18
  (65), text building 0x24 (64), F326 6 (62), F347 0x20 (42), F331 adverts
  4 (38), F342 5 (24), F360 0x36 (28), and a long tail. Journals: 10 (5),
  11-13 (4), 14 (273), 16 (1), 17 (5).
- **Tokens:** constants and variables dominate; then `==` (632), `||`
  (234), `&&` (172), the journal issue test 0x20 (266), random 0x32 (113),
  cargo 0x25 (71), 0x31 (51), 0x38 (24), 0x36 (19). The event-only tokens
  0x1c-0x1e, 0x21 and 0x26-0x29 do not occur, so the uninitialised-value
  deviation above never applies to the real scripts.

So after the journals the next hooks are the station conversation UI the
scripted adverts use (F331 adverts, F342/F326 menus, F345-F349 responses
and pages, F352 console lines, the 0x24/0x23/0x2d text buffer).

## Station menus (wired 2026-11-03)

The engine keeps the station menu the scripts drive (`script::menu`):
F331/F332 open a screen (`DATA_009054`, title `9052`, words
`9056/9057/9061`), F345/F346 add entries (F346 at F921's index seeded by
the code), F347 sets the header, F349/F352 post console lines, F360 selects
(codes 0x9800-0x9873 go to the station's own handlers, everything else is
written to globals 1-3 and 0), and F326 rebuilds the current screen: the
bulletin board (F453) clears the menu (F331/F342's signal 2), takes the
station's adverts and signals 1 so the scripts add theirs; the military
office (F493) signals 3; screens 1 and 11 (F541) only redraw.

In the playable the bulletin board lists the station's adverts and then the
scripts'; choosing a script advert runs F360, and a screen the scripts open
is the new `Screen::Script` page (layout row 1, their title and responses);
their console lines use the log source `Script`. "Hang up." (0x983f) goes
back to the bulletin board (its handler in `DATA_005288` is inferred from
the neighbouring entries). Leaving the station closes the scripts' screen
(F339). `tests/station_view.rs script_adverts_and_conversation_frames`
draws the board with the new game's script adverts and a journal
subscription conversation.

Not yet: the station's own entries are not passed to the scripts
(`Host::native_entries`) and the other station screens do not signal the
scripts.

## Text macros and script queries (2026-11-03)

F1343 runs F77 on every expanded string, and F77 hands each `[` to F76
(`ffe_core::script::brackets`). `[Mi<seed>(name)` is F64's query: global 7
gets the seed, the name's id from `DATA_001698` (cache v34,
`base:script/queries`, 68 names) pulses global 6, and the text the scripts
write with statements 35, 36 and 45 replaces the macro. While a code's
macros expand, `DATA_008568[0]` holds its first word (F1343), so
`[MiL0(...)` reaches the instance that owns the record. Ported macros: `Gr`,
`Mi`, `Ca`, `Fm`, `Ff`, `Cr`, `Co`, `Dr`, `Nu`, `Pn`; the journals' others
are kept as written for now. F31's arithmetic tokens 0x2f, 0x36 (F23:
seconds to a day number), 0x37 (F24) and 0x3b (a system code) are in the
engine. The port's date codes read a value of 0x121cf7 or more as the
original's absolute day number. On a new game the hitch-hikers' advert reads
"... by 25-Feb-3251" and the Valhallan job "$50000 paid to the right
Commander" (`tests/station_view.rs script_advert_macros_ask_the_scripts`).

## Game actions the scripts can refuse (2026-11-03)

The original sets a script global before some actions and refuses the
action when a script has cleared it (`Engine::permits`): F381's launch
(global 4 = 1), F908's hyperspace jump (0xf = 1), F650's docking request
(0x17 = the ship's id; the port's player is id 1), F931's laser shot
(5 = 0x7cf + the laser byte's type, only when the shot is ready) and F291's
jettison (5 = 0x3e8 + the commodity). The playable asks before each of
them. F383's price multiplier, F316's contract rows and F633's daily
station signal are wired too (see below).

## Mission ships, first slice (2026-11-03)

- **Ids.** The scripts know the player as object 1 and each navigator ship
  as `ship.id + 0x10` (`ffe_rs::scripts::SHIP_IDS`).
- **Created ships.** Statements 41, 42 and 43 (F665/F666/F668 -> F669) make
  a ship from F772's type list 0xc, 4 or 2 that attacks the player; the
  port places it 25 km from the player in a direction from the shared pair
  (its stand-in for F702) and returns its id to the script.
- **Removal.** F924 sends module message 11 when an object leaves the game,
  and F61 raises event 0x16 with `DATA_008560` set to it (token 0x34). The
  playable raises it for every ship that is gone since the last frame
  (destroyed, jumped out or cleared).
- **Queries.** Token 0x24 (an object's model) and 0x3e (its AI state, from
  the port's NPC state) read the player and the navigator's ships; 0x31,
  0x1f, 0x22 and 0x23 read the allegiance, offences, military points and
  fines.

Superseded by the next section: every object statement is now wired.

## Mission ships and objects (2026-11-03)

The census (`census_by_mission`) lists which missions use each statement;
`tests/mission_scripts.rs list_a_mission` with `FFE_SCRIPT_MISSION=all`
prints every mission's listing.

- **Ids.** The player is 1, a navigator ship `ship.id + 0x10`, a system
  body `0x1000 + index` and a script canister from `0x2000`
  (`ffe_rs::scripts`). F27's searches (tokens 0x33, a starport by its
  `+0xa0` code, `+0x14c & 0x10`; 0x3c, a planet, models 0x76..=0x85) walk
  the bodies from the top as F27 walks the slots.
- **Actions.** The hooks queue what the statements ask for
  (`scripts::Action`), and `scripts::apply` carries it out after each run,
  in order; the hooks keep a view of each new ship so the same run reads
  back its AI state and target.
- **19 (F772)** makes a ship of F772's type list (F658/F635's model,
  `npc::model_for_type`), where the player is until **54 (F702)** places it
  F1507's packed distance (`mantissa * 2^(exponent - 15)` mm) from the
  parent in a direction from the shared pair. **57** stops it. **60** puts
  it in AI state 0xb behind its leader at `DATA_001705`'s place
  (`npc::SCRIPT_FORMATIONS`, scaled by `npc::FORMATION_SCALE` as F695's
  escorts are). **39 (F664/F672)** makes a ship of a type named from its
  seed (F791 kind 0x12) waiting in a starport's bay until its departure
  time (F24/F23 of the script seconds); with no starport it starts far out
  in transit. **41-43 (F669)** keep their attackers, now in AI state 9.
- **F37's object targets.** 0x3e sets the AI state (`NpcShip::set_ai`,
  `npc::State::from_original`; 0x17, the scripts' parked state, holds the
  ship in place) and raises event 0x17; 0x41 the target; 0x24/0x3d the
  model (on the player, F441 changes the ship); 0x38 the `+0xa0` word;
  0x30 `DATA_008908`'s offence points (`Legal::points`, which F236 now
  adds to); 0x25 the cargo (commodities `kind - 1000` in the player's hold
  within its free space, equipment records `kind - 100` fitted or removed
  one at a time by F390/F391 at no charge, counted by F392's
  `Loadout::count`); 0x42 a station's price (F649).
- **Queries.** Tokens 0x24/0x3d (model), 0x3e (AI), 0x38 (`+0xa0`), 0x40
  (F1680's distance class), 0x41 (target), 0x25, 0x30, 0x33/0x3c and 0x35
  (F658: a model for a type). F21's cases 1 (Elite score), 3 (reputation),
  13 (the body the player is docked or landed at) and 14 (`DATA_008870`:
  0x2a docked, 0x30 on a pad, the landed words, 0x2c dying, 4 in
  hyperspace).
- **Damage.** **21 (F953)** damages a ship or the player (a negative amount
  is the most, so `21 P0 -1` kills the player through the usual death
  path); **51 (F942)** destroys an object. F953's signal runs before every
  laser hit on a ship and every hit on the player: globals 0xd (the
  object), 0xe (the kind) and 0xc (the damage) are set, each raising its
  event, and the damage dealt is what the scripts leave in 0xc (missions 2
  and 10 count their buildings' hits this way).
- **Names.** **26/47** set an object's name (`+0x124`) from a message code
  and word.
- **Canisters.** **52 (F926)** makes a canister of commodity `a` at an
  object; **58 (F291)** jettisons a tonne from the player's hold. Both give
  their ids back; a script canister is not lost to range and its removal
  (scooped, destroyed) reaches the scripts as event 0x16.
- **Objects.** **48 (F297)** puts a building of a model on a planet's
  surface at `DATA_007130/7131`'s latitude and longitude; **50 (F40)** puts
  an object of a model on an orbit at a packed distance round a body. Both
  are fixed, shootable objects (`npc::script_object`, `ships::object_spec`)
  that `apply` holds to their body each frame.
- **Contracts.** **37/38 (F314/F315)** add and remove the scripts' own
  records in the contracts list (six at most with the removal jobs); the
  contracts page asks the scripts for each row (F316: global 8 the record,
  then the code in 9 and its words in 1-3) and draws it across the page.
  **49 (F290)** adds a delivery contract.
- **Fines and ranks.** **40 (F236)** records an offence in a jurisdiction;
  **53** is F305 (Imperial) or F304 (Federal) with F306's points and F307's
  medal, posting their notices.
- **Signals.** F60's time ticks run every frame on the game time passed;
  F383 sets global 0xa to the tech multiplier and signals 0xb when the
  Upgrades screen is built, and the second-hand records (`+0x14`) are
  priced by what the scripts leave in 0xa (`equipment::fit_price_with`;
  this replaces the port's earlier assumption that the branch was random);
  F633 signals 0x10 with each station's id on every new day.

Still stand-ins (deviation 2026-11-03 in `docs/DEVIATIONS.md`): F702/F1507's
directions come from the shared pair rather than F1506's orbit phase;
F669's attackers start 25 km from the player; F879's surface point is the
latitude/longitude direction at the planet's radius, not the terrain
height; F40's orbit does not move. The scripts' objects, canisters and the
fields they set (`+0xa0`, the AI code, the target) are not kept in a
save, and the station's own entries are not passed to the scripts.
