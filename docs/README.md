# Documentation map

Start with [STATUS.md](STATUS.md) (the current state and the work list), then
[BURNDOWN.md](BURNDOWN.md) (what is left). [../CLAUDE.MD](../CLAUDE.MD) holds the
owner's decisions, scope and working rules; [../README.md](../README.md) is how to
build, play and test.

## Records

| Document | What it is |
|---|---|
| [STATUS.md](STATUS.md) | The handoff: the two workstreams and the ordered work list. |
| [BURNDOWN.md](BURNDOWN.md) | Open work by area; generated from [../dashboard/burndown.js](../dashboard/burndown.js). |
| [PORT_LOG.md](PORT_LOG.md) | Every change with its evidence and trace hashes, newest first. |
| [DEVIATIONS.md](DEVIATIONS.md) | The register of intentional differences from the source (SIM/EXP rows). |
| [KNOWN_ISSUES.md](KNOWN_ISSUES.md) | Known source and reference issues, and the bug policy. |
| [audits/](audits/gameplay-parity-2026-10-26.md) | The source-parity audit (PAR-nn items) and its JSON progress record. |

## Reference, formats and conventions

| Document | What it is |
|---|---|
| [REFERENCE_HARNESS.md](REFERENCE_HARNESS.md) | How to add a differential test against the executed JJFFE. |
| [../tools/reference/README.md](../tools/reference/README.md) | The local oracles and how to run them. |
| [MODEL_COMMANDS.md](MODEL_COMMANDS.md) | The F1772 model opcode map and the Rust coverage. |
| [FORMATS.md](FORMATS.md) | The content cache contract and the model helper semantics. |
| [ARCHITECTURE.md](ARCHITECTURE.md) | The workspace design and determinism rules (partly the original proposal). |
| [GAME_FILES.md](GAME_FILES.md) | The supplied releases: every file, size and hash. |
| [CONVENTIONS.md](CONVENTIONS.md) | Code conventions: errors, numerics, budgets, process. |
| [CONTROLS.md](CONTROLS.md) | The Modern and Classic bindings and the input routing audit. |
| [NAVIGATION.md](NAVIGATION.md) | Systems, maps and the autopilot as the player uses them. |
| [LEGAL.md](LEGAL.md), [TAKEDOWN.md](TAKEDOWN.md) | The project's legal stance and the rights-holder contact. |
| [evidence/](evidence/) | Captures and runs that back the notes, one folder per topic. |

## Subsystem notes (`notes/`)

Each note maps the original routines to the Rust and records what is verified.

**Flight and physics:**
[physics](notes/physics.md),
[player](notes/player.md),
[thrust](notes/thrust.md),
[frames](notes/frames.md),
[collision](notes/collision.md),
[collision-shapes](notes/collision-shapes.md),
[docking](notes/docking.md),
[autopilot-source-audit](notes/autopilot-source-audit.md),
[hyperspace](notes/hyperspace.md),
[time](notes/time.md),
[dust](notes/dust.md),
[views](notes/views.md).

**The galaxy and its systems:**
[galaxy-table](notes/galaxy-table.md),
[sysgen](notes/sysgen.md),
[name-generation](notes/name-generation.md),
[objects](notes/objects.md),
[terrain](notes/terrain.md),
[rings](notes/rings.md),
[galaxy-map](notes/galaxy-map.md).

**Models and drawing:**
[animation](notes/animation.md),
[beams](notes/beams.md),
[child-models](notes/child-models.md),
[curved-surfaces](notes/curved-surfaces.md),
[fills](notes/fills.md),
[lines](notes/lines.md),
[polygons](notes/polygons.md),
[text](notes/text.md),
[textures](notes/textures.md),
[thrusters](notes/thrusters.md),
[palette](notes/palette.md),
[display](notes/display.md).

**Cockpit and screens:**
[cockpit-dashboard](notes/cockpit-dashboard.md),
[console](notes/console.md),
[labels](notes/labels.md),
[scanner](notes/scanner.md),
[message-log](notes/message-log.md),
[prefs](notes/prefs.md),
[station](notes/station.md),
[intro](notes/intro.md),
[death](notes/death.md),
[gameover](notes/gameover.md).

**Gameplay:**
[combat](notes/combat.md),
[npc](notes/npc.md),
[equipment](notes/equipment.md),
[market](notes/market.md),
[mining](notes/mining.md),
[missions](notes/missions.md),
[mission-scripts](notes/mission-scripts.md),
[journal](notes/journal.md),
[legal](notes/legal.md),
[rank-and-fines](notes/rank-and-fines.md),
[ship-names](notes/ship-names.md),
[persistence](notes/persistence.md).

**Sound, media and tools:**
[sound](notes/sound.md),
[sound-cues](notes/sound-cues.md),
[music](notes/music.md),
[cd-media](notes/cd-media.md),
[launcher](notes/launcher.md).

## Archive

[archive/](archive/README.md) keeps superseded and out-of-scope documents
verbatim: the Phase 0 study, the renderer and graphics-pack plans, one-off
incident notes from October and the old README.
