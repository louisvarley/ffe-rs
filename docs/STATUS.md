# Status and handoff

Authoritative, current status of the reimplementation. This file supersedes the
status snippets in `README.md`, `CLAUDE.MD` and `docs/PORT_LOG.md` where they
disagree with it. Read this first, then `docs/MODEL_EXECUTION_PLAN.md` for the
v0.11 design record and follow-ups.

## How to read this repo

- `CLAUDE.MD` — owner decisions, scope, phases/exit gates, and per-milestone narrative.
- `docs/PORT_LOG.md` — append-only record of decisions, discrepancies, trace hashes, newest first.
- `docs/FORMATS.md` — the implemented partial `data.bin` cache contract (v1..v6) and the model/command/vertex/normal/depth helper semantics.
- `docs/ARCHITECTURE.md` — proposed workspace, crate responsibilities, determinism rules.
- `docs/INVENTORY.md` — reference-source inventory: formats, numeric model, timing, gotchas.
- `docs/KNOWN_ISSUES.md` — FFED3D-vs-JJFFE differences and port gaps.
- `docs/DEVIATIONS.md` — register of intentional deviations (approved / implemented / rejected).
- `docs/CONVENTIONS.md` — code conventions for this workspace.
- `docs/REFERENCE_HARNESS.md` — how to add a differential test against the executed JJFFE reference.
- `docs/MODEL_EXECUTION_PLAN.md` — design record for the implemented v0.11–v0.13 depth/point/line work and its follow-ups.
- `docs/MODEL_COMMANDS.md` — full F1772 opcode map and current Rust coverage.

## Verdict on handoff readiness

**READY for the next model task.** The v0.10 projection/live-slot work, v0.11
depth composition, v0.12 composed-order oracle and v0.13 point/line emission are
implemented, tested and recorded. The next task (text, submodels, other transform
forms and colour) is scoped in `docs/MODEL_EXECUTION_PLAN.md` §8 and
`docs/MODEL_COMMANDS.md`. Remaining gaps are explicit below.

## Milestone status

"Code" = implemented in `crates/`. "Tests" = synthetic tests that run in `cargo
test --workspace --locked`. "Reference" = a differential comparison against the
executed JJFFE binary, driven by a `tools/reference/verify-*.py` script and an
`#[ignore]`d Rust test. Trace hashes are the SHA-256 of the disposable JSONL
capture produced by two repeat runs of the local reference.

| Milestone | Code | Tests | Reference (cases) | Trace SHA-256 | Notes |
|---|---|---|---|---|---|
| v0.0 tools: inspect/export/cache | `ffe-tools`, `ffe-convert`, `ffe-content` | `inspect.rs`, `reference.rs`, `cache.rs`, `decoders.rs`, `embedded.rs`, `sine.rs` | `verify-assets.py` | — | exact floppy 1.02 + CD 1.01 profiles; cache v6 / 385 assets |
| v0.0 core: RNG/timer/clock | `ffe-core` (lib.rs) | `primitives.rs` | `verify-core.py` (1536 RNG, 60 timer, 238 clock) | `accbe92f9…` | day fraction wraps at 2^32, not 49,710 |
| v0.1 experimental playable | `ffe-rs` | `playable.rs`, `indexed.rs`, `input.rs` | none (synthetic gameplay) | — | synthetic flight/market/save; **not faithful** |
| v0.2 font/console | `ffe-rs::indexed`/`screen` | 5 + 3 | `verify-font.py` (696 glyph + 28 blit) | `5642024b…`, `0cd9357e…` | cache v2 |
| v0.3 vertex interpreter | `ffe-core::model` | `model.rs` | `verify-models.py` (13,632) | `7432278a…` | models 74/75; cache v3 |
| v0.4 command decode + helpers | `ffe-core::model_commands` | `model_commands.rs` | `verify-commands.py` (1315/360/65536/32) | `2c0c1a67…` | cache v4 |
| v0.5 normals | `ffe-core::model_normals` | `model_normals.rs` | `verify-normals.py` (26,712) | `55b2060d…` | cache v5 |
| v0.6 trig/transforms/visibility | `ffe-core::model_math` | `model_math.rs` | `verify-transforms.py`, `verify-rotation-operands.py`, `verify-visibility.py` | `b7ea38a8…`, `08ce2914…`, `d7e8cc0b…` | cache v6 |
| v0.7 bounded executor | `ffe-core::model_execute` | `model_execute.rs` | `verify-execution.py` (900) | `d2a76e9c…` | `execute` (no live vertices) still rejects opcode 6 |
| v0.8 depth key | `ffe-core::model_depth::key` | `model_depth.rs` | `verify-depth.py` (17,303) | `61fffc80…` | F1777 |
| v0.9 depth commands + order | `ffe-core::model_depth`, `model_order` | `model_depth.rs`, `model_order.rs` | `verify-order.py` (11,700 + 128) | `ab5da346…`, `4ab5453e…` | F1696/F1780/F1788; abstract tokens |
| v0.10 projection + live slots | `ffe-core::model_instance` | `model_instance.rs` (5), `instance_reference.rs` (2, ignored) | `verify-instance.py` (2,197 + 8,192) | `c794945388b5bae086ceb6cb8ad615d17d954a85e2b200bd89367166c1890f70` (projection), `6fe8a6470d42d884d11e54f1cc9db26107794b5220cbfbb051a6ebf826e422cc` (live vertices) | F1479 + F1470/F1471/F1478 |
| v0.11 depth composition | `ffe-core::model_execute::execute_live` | `model_execute.rs` (12), `execution_reference.rs` (+1, ignored) | local `executed_reference_station_depth_composition_advances`; existing depth/order/executor traces | — (no new composed trace) | composes F1696/F1780/F1788 with `model_instance`; `Execution.ordered`; real stations advance past opcode 6 |
| v0.12 composed-order oracle | `model_execute::ordered_keys`, `model_order::finish_with_keys` | `order_reference.rs` (+1, ignored) | `verify-order.py` composed-order (1 program, order `[2,3,1]`, keys `[9,9,0]`) | `ce0d1b4fc55e2ac5533aa848dbe4642da8d0c00161de7959b561e184af021d2a` | composition of original F1696/F1777/F1780/F1788 with markers; not F1772/face handlers/pixels |
| v0.13 point/line emission | `model_commands` (1/2), `model_execute::Primitive` | `model_commands.rs` (6), `model_execute.rs` (14), `point_line_reference.rs` (1, ignored) | `verify-point-line.py` (2 events) | `afa955be148192eaf2be79fb0b818386eafbc8d008e67e3fc56c3b7dd3e758a3` | operand events from original F1698/F1702; ordered with faces; not geometry/pixels |

Facts verified on 2026-10-21:

- `cargo test --workspace --locked` passes. 145 `#[test]` functions total; 17 are
  `#[ignore]` local-only reference tests; one more is `#[cfg(unix)]`. On Windows
  that is **127 runnable synthetic tests**.
- `python tools/reference/verify-instance.py --reference .cache/jjffe-reference/jjffe-smoke.exe`
  passes both scenarios (repeat/tamper/empty all behave as expected).
- The local-only real-station composition test advances past opcode 6: model 74
  stops at unsupported `0x000a`/`0xf4`, model 75 at `0x1c6e`/`0x1c8`; context,
  vertex slots and queue are unchanged on those errors.
- The local reference build (`jjffe-smoke.exe`, `jjffe-commands.exe`) and its
  `build-manifest.json` exist under `.cache/jjffe-reference/`.
- `game/` has 71 files; `jjffe-master/` and `ffed3d-master/` are present; root
  `data.bin` is cache v6; `.cache/data-v1..v5-backup.bin` are retained.

## Next task

Extend the composed executor through **text (opcodes 10/14), child models
(21/26), other transform forms (28) and colour state**. Point (1) and line (2)
are now operand events; see `docs/MODEL_COMMANDS.md`. Before implementing each
remaining family, extend the local probe to capture the original handler
arguments and document the payload; do not infer fields from record size.
Acceptance: at least one synthetic program per added command family; an
executed-JJFFE ordered trace once the face/primitive handlers are probed; errors
remain transactional; F4, synthetic flight and cache v6 stay unchanged. The
abstract `Order` queue remains the approved default; exact F1778 arena emulation
and real child-model content (cache v7) are later, separate steps.

## Known stale spots

- None of the earlier v0.10/v0.11 discrepancies remain; they were corrected on
  2026-10-21. `README.md` and the executable title still say "0.9" because the
  product version label has not been bumped (cosmetic).
- `tests/golden/` still does not exist; it is now labelled "planned" in
  `README.md`.
