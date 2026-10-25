# Time-driven events (F152, F648/F633, F622, F675/F680/F927)

Status (2026-10-25): the day tick is ported as one marker
(`ffe_core::calendar::Calendar`, `Navigator::tick_days`), wired into the game
clock, and the periodic updates the port has state for run exactly once per
day. Mission/contract deadlines get a small API for the missions stream. The
remaining source periodic work is either script-driven (`F48(0x18)`, the
mission table) or belongs to a stream that does not exist yet (news history,
service reminders); each is listed below instead of being invented.

Rust: `crates/ffe-core/src/calendar.rs`, `ffe_rs::navigation::{tick_days,
schedule_deadline, take_expired, DayTick, Deadline}`. Evidence:
`crates/ffe-rs/tests/timed_events.rs` (local-only, cache v29) and the
`timed-events.jsonl` capture from its long-run test.

## The clock and the day marker

The original keeps two clock words: `DATA_008804` (the day fraction, one day =
`0xc22e` = 49,710 ticks) and `DATA_008807` (the whole-day counter; a new game
is `0x121cf7` days). Every frame `F152` (`ffe.asm:15176`) does:

- `F152:15196-15221`: `days = (DATA_008804 - DATA_008808) / 0xc22e`, where
  `DATA_008808` is the last fraction handed to the day handlers. The marker
  then moves by `days * 0xc22e`, so a slow frame, a time-accelerated frame and
  a multi-day jump all deliver each day exactly once.
- `F152:15204-15214`: for every module whose `DATA_008623` bit 1 is set,
  dispatch `F144(module, 0xa, days)` — the module's "day passed" event.
- `F152:15216-15222`: dispatch `F151(object, days)` for the physics objects;
  each object with `+0x87` set gets `F144(code, 6, &object)`.

`F144` (`ffe.asm:14828`) is the event-table dispatch through `DATA_004892`.
The mission/script module (21, `DATA_001700`) receives the day event in its
table as `F60` (`ffe.asm:1453c`): it resets the three duration counters when
`DATA_008807` changes and re-arms them through `F58` (the F48 query that
scripts watch). The service-date reminders are `F59` (`ffe.asm:144ac`), which
uses `F58` to raise events when `DATA_008554..7` (8, 9, 0x1c and 0x16d-day
intervals) come round. Neither has port state yet.

The port stores game time as `Navigator::clock` (world seconds) and derives
`Navigator::day()` as whole 86,400-second days from `start_clock`. The
`Calendar` is the equivalent of `DATA_008808`'s day: `Navigator::advance`,
`Navigator::transfer` and `Navigator::hyperspace` call `tick_days`, which walks
`calendar.advance(nav.day())` in order. A direct `Navigator::jump` is the
source's "enter at once, no time" path and does not tick.

## F633: the daily starport update

`F633` (`ffe.asm:43279`) is reached through the station module's action
handler `F648` (`ffe.asm:44299`, action `0xd`; action `2` is the F622
rebuild) and does, once per day:

1. `F633:43285-43300`: prune the 25-entry recent-systems/news buffer
   `DATA_009004` of records older than four days (`[entry+8] < day + 4`).
2. `F633:43301-43308`: three `random(0x19) <= DATA_009001` attempts to add a
   news/mission record through `F623`/`F624`.
3. For each of `DATA_008997` starport records in `DATA_008999` (18):
   - `F633:43309-43445`: for each of the 33 commodities, new price then new
     stock from F619/F620 and the quarter drift
     `(new + 3*old) / 4` (arithmetic shift, toward zero). After each commodity
     `F48(0x18, station object, 0x3e8 + commodity)` is the script hook that
     may raise a special-offer/mission event.
   - `F633:43446-43468`: the BBS draw `random(7) < population`; on success a
     random entry is replaced by `F621`'s generator (and its two service flags
     are cleared).
   - `F633:43470-43521`: shipyard stock. Retire the entry at
     `random(0xe)` when it is in range; then add a military hull when
     `random(9) < DATA_008993`, a civil hull when
     `random(9) < DATA_008994`, and a shuttle when
     `random(0x1b) < DATA_008997`.
   - `F633:43523-43527`: `F34(0x10, station)` re-queries the station screen
     (the original's news/state refresh).

`F622` (`ffe.asm:42654`) makes a station's records on first use: the market
(price then stock per commodity), the BBS (`population * 4 + 1` attempts,
`F621` at `ffe.asm:42635`) and the hull stock (`F634`/`F635` at
`ffe.asm:43543`/`43576`). The port does this lazily per visited station; a
station first seen on day 10 is generated then, exactly as F622 does on system
entry. `F648`'s action `2` also rotates the four remembered systems
`DATA_009006..9` before calling F622; the port has no such history.

Port behaviour: `Navigator::market`, `Navigator::station_offers` generate on
first use and then bring the station up to the current day; `tick_days` does
the same for every visited station as each day crosses. Both paths share the
per-station `day`, so a market can never drift twice for one day. The 30-day
lazy cap the port used to have is gone; one call can apply every missed day
(the source's loop has no cap either).

`F633`'s recent-systems buffer feeds `F636` (wanted in recent systems), which
the port's bulletin generator deliberately returns no entry for; the buffer is
therefore not implemented, and neither is the per-commodity `F48(0x18)` hook
(no script engine). The visible news the port has is the BBS replacement. The
special-offer commodity table `F641` and missing-person `F637` remain absent.

## Jumps, save/load and station records

`F674` (`ffe.asm:45338`) clears the AI traffic on leaving a system; `F675`
(`45374`) keeps hostile ships within `0x10` (the port's `PERSIST_RANGE =
2.0e6`) and escape pods; `F680` (`45536`) restores them at the arrival point
before `F688` spawns the new system's traffic. The port already implements
that in `npc::persist`/`npc::restore` (`Navigator::jump`/`spawn_traffic`) and
`F927` (`ffe.asm:69170`) recycling in `Navigator::tick_ships`.

The starport records are a single 18-record array for the *current* system:
`F622` rebuilds them on entry, and `F633` updates only that array. The port
therefore clears `markets`/`offers` in `Navigator::jump`, like the source, and
generates them at the arrival day. The day marker lives on `Navigator` and is
derived from the clock, so a jump does not replay the days it took; a load
that replaces only `Flight` (the current save format) leaves the navigator
and its marker untouched. A save format that carried the clock would need to
carry the navigator's marker too; that is an owner decision (see below).

## Cadence table

| source | interval | port |
|---|---|---|
| `F152:15196` day comparison | every frame, `days = (fraction difference)/0xc22e` | `Calendar::advance`, `Navigator::tick_days` (day, in order, once) |
| `F633:43309` market price/stock quarter drift | day | `StationMarket::daily`, `Market::daily` (F633 half already unit-tested) |
| `F633:43446` BBS replacement | day, `random(7) < population` | `bulletin::daily` |
| `F633:43470` shipyard retire/add | day | `shipyard::daily` |
| `F633:43285` news buffer prune + 3 adds | day | not ported (F636/F637/F641 absent) |
| `F633:43442` `F48(0x18)` per commodity | day | hook only: `DayTick`/missions API; no script engine |
| `F59:144ac` service-date reminders | day/interval | dates saved in `Flight::service_days`; reminder events not ported |
| `F60:1453c`/`F58` script time counters (mission table) | day | `Navigator::schedule_deadline`/`take_expired` for the missions stream |
| `F675/F680` NPC jump persistence | jump | `npc::persist`/`npc::restore` |
| `F927` NPC slot recycle | frame | `Navigator::tick_ships` |
| `F747` docked interval (2-3 days) | game seconds | `NpcShip::dock_timer` (`npc.md`) |
| `F867` clearance timeout | game seconds | `Starport::elapse` (`docking.md`) |
| `F866` station spin | frame | `Body::spin`/`refresh` |
| legal/rank decay | none found in the source | none (the only legal clock use is F783's alert clear on police removal) |

## The missions deadline API

The source can only run its mission table (module 21) through the script
engine; the port has no scripts yet, so the missions stream gets a typed
bridge instead of an invented mission format:

```rust
nav.schedule_deadline(mission_id, due_day); // replaces the same id
nav.cancel_deadline(mission_id);            // completed/abandoned
nav.deadlines();                            // pending list
nav.take_expired();                         // drained once, after advancing time
```

`due_day` is the same counter as `Navigator::day()` (whole days since the new
game/sandbox start). A deadline with `day <= current` expires on the day tick
that reaches it; `DayTick::expired` also lists the ids for a capture, and
`Navigator::last_day_tick()` exposes the marker. `advance`/`transfer`/
`hyperspace` run the tick, so the stream only has to drain `take_expired` and
then call `schedule_deadline` again if it wants a follow-up.

## Evidence

`cargo test -p ffe-core --lib calendar` runs the pure marker tests (boundary,
acceleration, no replay, backwards clock).

With the v29 cache:
`cargo test -p ffe-rs --test timed_events -- --ignored --test-threads=1`
runs six local tests:

- `the_day_boundary_runs_the_updates_once`: 86,399 s changes nothing; the
  next second drifts the market, replaces a board entry and changes the hull
  stock; the following sub-day frames change nothing.
- `acceleration_matches_stepwise_days`: four days in one step equals 96
  one-hour steps, market/board/hull state and marker included.
- `a_long_clock_jump_ticks_each_day_in_order`: days 3..7 exactly once.
- `mission_deadlines_expire_on_their_day`: one drain, once.
- `npcs_persist_across_a_jump_and_are_recycled`: a close pirate survives
  `jump`/`spawn_traffic`; an escaped and a far ship are recycled.
- `long_run_capture`: eight accelerated days, a real hyperspace jump to Alpha
  Centauri and a dock at its station; with `FFE_SHOT_DIR` it writes
  `timed-events.jsonl` (the committed capture was produced this way).

The capture (`docs/evidence/timed-events/timed-events.jsonl`) shows the
market stock moving from 15,005 to 13,877 over eight days at Sol, the jump
advancing the clock 140,310 s to day 9, the marker following, and the new
system's board/stock (9 posts, 3 hulls) replacing Sol's (18/13).

## Remaining owner decisions

1. **Save format.** The clock and the navigator are not in `Flight` today; if
   the commander save should restore the date, `calendar`/market days need a
   serialized navigator section (and then a version bump).
2. **News.** The `DATA_009004` four-day buffer and the F636/F637/F641
   generators need state the port does not keep; implementing the buffer alone
   would have no visible screen.
3. **Script hook.** `F48(0x18)` per commodity and module 21's mission table
   need the script VM; until then the deadline API is the bridge.
4. **Service reminders.** `F59`'s dated reminders would become dated F271
   messages once the repairs actions exist.
