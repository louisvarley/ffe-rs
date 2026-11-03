# Mission scripts (F31/F41)

Status (2026-11-03): **script graph imported (cache v32); interpreter
ported (`ffe_core::script::engine`), game hooks not yet wired.** Tracked
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
