# Agent streams

Record of the parallel work streams (one branch each), what each owns, the
commits that carry it and how it was integrated. Read with `docs/STATUS.md` and
`docs/PORT_LOG.md`: this file is the branch/merge ledger, not a second status
document.

The integrator owns `docs/PORT_LOG.md`, `docs/STATUS.md`, `dashboard/burndown.js`
and this file. A stream must not edit them; it records its own work in its
`docs/notes/*.md` and leaves the integration entries to the integrator.

## Integration rule

- Merge **one branch at a time** into `main`, never stack two on each other.
- If the branch is behind, integrate it by merging `main` into it or by merging
  it into `main` and resolving there; do not force-push a stream branch.
- Resolve a conflict by combining both intents. Never drop another stream's work
  to make a build green.
- After each merge: `cargo test --workspace --locked`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo fmt --check`, plus that stream's local cached-frame tests
  (`FFE_WORLD_CACHE=<repo>/data.bin cargo test -p ffe-rs --test <name> -- --ignored`),
  then commit the merge and push. `main` stays green.

## Streams merged on 2026-10-25

All five were based on older `main`s; each was merged in queue order, verified
and pushed before the next one started.

| Stream | Branch commits | Merge commit | Scope | Notes |
|---|---|---|---|---|
| intro plan | `b6c0e16` | `5fc9992` | `docs/notes/intro-todo.md` only | No code; no dashboard item. |
| planets | `1bd1b71`, `a1e96f6` | `4eb7478` | Vanishing-planet fix, surfaces to the horizon, surface starports/buildings, F575 landing wiring | `navigation::Body` gained `inner_radius`/`parent`; one `tests/navigation.rs` initialiser on `main` was updated. `world-5` done (v0.54). |
| lift fix | `204d717` | `ebacdb3` | Launched ship stays in the station until F1089's door opens, hull skipped until clear | Adds `SystemStation.door`; planet tests set it to `0xffff`. DEVIATIONS launch entry renumbered **EXP-011** (EXP-010 was taken). `flight-11` done (v0.55). |
| station UI | `f3cb3d8`, `d93b1ad` | `89581bc` | Original docked services, shipyard, stockmarket, upgrades and ships screens; market becomes one screen | Rewrote the docked screen and the docked input; conflicts in `main.rs`, `screen.rs`, `STATUS.md` resolved keeping the planets/lift guards and the console icons. `ui-4` done (v0.56), EXP-014. |
| prefs | `d37cb84`, `bf80d08` | `7e40a46` | Escape preference pages, space colour, Modern/Classic, Quit | Conflicts in `indexed.rs`, `main.rs`, `screen.rs`, `STATUS.md`; EXP renumbered to EXP-012/013. `ui-9` done (v0.48p). Followed by the owner's space-colour decision (`a780418`): default `#0c0c4c`, Black stays `#000000`. |

## Branches not in this round

`nav-buttons`, `ship-ui`, `sound` and `claude/code-commit-capabilities-k4hm9l`
were integrated earlier; see `docs/PORT_LOG.md` for their entries.

## Housekeeping conventions

- Screenshots and `.ppm` frames are never committed. A stream's evidence goes to
  `docs/evidence/<feature>/` as PNG, and `--capture-*` output directories
  (`shots-*`, `shots/`, `screenshots/`) are ignored.
- Each stream's milestone row belongs in `docs/STATUS.md`; the integrator adds
  the matching `PORT_LOG.md` entry, ticks `dashboard/burndown.js` and appends the
  dated history point for the stream with the new remaining count.
