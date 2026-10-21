# Model execution plan — compose depth commands/groups into the executor

Status: **implemented in v0.11; extended in v0.12 (composed-order oracle) and
v0.13 (point/line emission).** Retained as the design record and follow-up list.
The final API differs slightly from the proposal below (see §8). Decisions marked
**[DECIDED]** are resolved here (with rationale); **[CONFIRM]** items are flagged
but have a chosen default so work is not blocked.

## 1. Goal

Extend `ffe-core::model_execute` so the real station programs (models 74/75) no
longer stop at the stateful depth-order opcode 6. After this change the bounded
executor must:

1. Decode and execute F1696 depth commands (opcode 6) on the composed stream.
2. Resolve the referenced vertices through the **live, lazy** view-space slots in
   `ffe-core::model_instance` (F1470/F1478/F1471), never through pre-resolved
   static points.
3. Maintain the group state from `ffe-core::model_order` (F1780 group open/close,
   F1788 sort) and produce an **ordered** primitive stream.
4. Preserve the existing transactional guarantee: any error leaves the caller's
   context, vertex slots and queue unchanged.

## 2. Existing pieces (all verified, do not reimplement)

| Piece | Path | What it already does |
|---|---|---|
| Physical decode | `model_commands::decode` | Walks record boundaries; recognises opcode 6 via `model_depth::decode`; validates branch targets and face vertex references. |
| Depth decode | `model_depth::decode` / `Command` | Parses F1696 modes 0..7, operand-pair continuation (high byte 255), list min/max, position/first/offset forms; returns consumed size. |
| Depth resolve | `model_depth::Command::resolve` | Produces `Option<u32>` (None = close group) from **already-resolved** view-space points. |
| Depth key | `model_depth::key` | F1777: unsigned key from view point + key scale, with the packed overflow encoding. |
| Group ordering | `model_order::Order<T>` | F1780/F1788: open/close groups (empty groups valid), absorb members into the active group, sort groups/standalone by descending unsigned key, ties by allocation order. |
| Live slots | `model_instance::Vertices` / `project` | F1478 memoized object evaluation, F1470 view translation, F1471 projection+stamp, generation short-circuit. |
| Executor | `model_execute::execute` | Composed control flow for return/arithmetic/branches/rotation/face emission; currently errors on opcode 6. |

## 3. Resolved decisions

- **D1 [DECIDED] Primitive store: keep abstract `Order<Emission>`, do not emulate the byte arena.**
  `docs/FORMATS.md:273-286` already sanctions abstract owned tokens and explicitly
  says the native queue is "not an executable function pointer or serialized
  cache bytes". The 50,000-word arena, forced headers, and terminator function
  pointers are a later fidelity concern, not a prerequisite for correct order.
  If a future milestone needs the exact allocation size, add it as a separate,
  tested module rather than entangling it with control flow.

- **D2 [DECIDED] Executor state.**
  Add a small context struct rather than growing `execute`'s parameter list:

  ```rust
  pub struct InstanceContext {
      pub position: [i32; 3],   // instance +0x4
      pub generation: u32,      // instance +0xf0 (0 is invalid)
      pub key_scale: i32,       // instance +0x130
      pub operand_scale: i32,   // instance +0x148
  }
  ```

  `execute` keeps `bytes, vertex_count, context: &mut VertexContext, visibility,
  table, budget`, and additionally takes the vertex records plus this
  `InstanceContext` so it can own a fresh `Vertices` per invocation. The exact
  signature below is a starting point; align it with `F1772` dispatch when
  wiring the reference.

- **D3 [DECIDED] Opcode 6 behaviour.**
  For each depth command, resolve the referenced vertex/vertices with
  `Vertices::resolve(index, &context, position, generation, /* projected */ false)`
  (F1470, view only), then compute the key with `model_depth::key(view, key_scale)`.
  `Close` -> `order.group(None)`; any keyed form -> `order.group(Some(key))`.
  `Position` uses `InstanceContext.position`; `First` uses `u32::MAX`; `Offset`
  with a vertex uses that vertex's view, otherwise `[0,0,0]`, then adds
  `amount << operand_scale` to z. Do **not** project during opcode 6.

- **D4 [DECIDED] Standalone face key.**
  From F1708/F1712: a face emitted while no group is active is allocated with a
  key computed by `F1777` on the **view-space coordinate of the emitted face's
  first vertex** (mirror order preserved), then pushed via `order.push(key,
  emission)`. When a group is active, the member's own key is ignored (the group
  key applies) — `Order::push` already does this. Resolving that first vertex may
  require `Vertices::resolve(..., projected: true)` (F1471) since face emission
  needs screen coordinates; see D6.

- **D5 [DECIDED] Transactionality.**
  `execute` already clones `VertexContext` and commits only on a real return.
  Extend the same rule to the new state: build the `Vertices` cache and the
  `Order<Emission>` on working copies, and only fold them into the returned
  `Execution` (or a new `ExecutionState`) after an unconditional/conditional
  return. On any error, drop the working copies so nothing leaks.

- **D6 [DECIDED] Output shape.**
  Keep `Execution.emissions` in **command order** (diagnostics/tests depend on
  it) and add `Execution.ordered: Vec<Emission>` for the depth-sorted stream.
  Faces still become `Emission { face, context }` events; the ordered list is the
  same emissions re-sequenced by `Order::finish()` plus group membership.

- **D7 [CONFIRM, default chosen] Generation 0 and scale.**
  Keep the current `model_instance` behaviour: `generation == 0` and
  `context.scale > 31` are explicit errors. If a composed test later needs a
  valid startup generation, set it explicitly in the fixture; do not invent an
  implicit default.

- **D8 [CONFIRM, default chosen] Reference oracle.**
  Extend the existing command probe (`--execution` / `jjffe-commands.exe`) or add
  a new mode that runs a synthetic composed program *through* F1696/F1778/F1780/
  F1788 and emits the ordered primitive stream. See `docs/REFERENCE_HARNESS.md`.

## 4. Suggested implementation order

1. Add `InstanceContext` and the vertex/order parameters to `model_execute`
   (D2), keeping the old signature as a thin wrapper for the existing 900-case
   oracle until the new oracle lands.
2. Implement opcode 6 (D3) inside the executor loop, using `model_depth::decode`
   + `Vertices::resolve(..., false)`.
3. Emit faces through `Order<Emission>` (D4), building `Execution.ordered` via
   `Order::finish()` (D6).
4. Apply transactional working copies (D5).
5. Add synthetic tests (below). Keep the existing opcode-6 rejection test but
   change its expectation to "proceeds past opcode 6 and stops at the next
   unsupported command".
6. Add the reference mode and `verify-*.py` (D8), record the trace hash.
7. Update `docs/PORT_LOG.md` (newest-first), `docs/STATUS.md`, `docs/FORMATS.md`
   model section, and `CLAUDE.MD` next-step paragraph.

## 5. Synthetic test plan (in `crates/ffe-core/tests/model_execute.rs`)

- A stream whose depth commands precede grouped faces orders faces by group key.
- Standalone faces (no active group) order by their own first-view-vertex key.
- Empty groups and replaced groups drop nothing unexpectedly.
- Equal-key ties preserve allocation order.
- Reusing a slot across generations recomputes view; projection advances the
  stamp; view-only resolution does not (regression against F1470/F1471).
- Unresolved/negative/external vertex indices error without mutating slots/queue.
- Arithmetic, branch, visibility, rotation and depth errors leave context, slots
  and queue unchanged (transactional).
- Exact 1..4096 budget boundary; budget exhaustion leaves state unchanged.
- The real station streams (74/75) now advance past opcode 6 and fail explicitly
  at the next unsupported command (do not assert a full render).

## 6. Non-goals for this task

- No point/line/text (opcode 1/2/10), submodels (26), colour state, or other
  transform forms.
- No normal evaluation, culling, shading, clipping or rasterization.
- No change to the F4 lab, synthetic flight, cache schema (stays v6), or input.
- No "real" byte/word arena (D1).

## 7. What still needs a human/owner decision

None strictly blocking this task. The only genuinely open items are: whether to
eventually emulate the exact F1778 arena (deferred by D1), and whether to bump
the product version label to 0.11 (cosmetic; the window title currently says 0.9).

## 8. Result (v0.11–v0.13)

Implemented in `crates/ffe-core/src/model_execute.rs`:

- `execute_live(bytes, vertices: &mut Vertices, context: &mut VertexContext,
  visibility, table, InstanceContext { position, generation, key_scale,
  operand_scale }, budget)` is the composed path. The vertex count comes from the
  live cache, so there is no separate `vertex_count` argument.
- `execute(bytes, vertex_count, context, visibility, table, budget)` is unchanged
  and still rejects opcode 6 explicitly for the earlier no-live-vertex checks.
- `Execution.ordered` is the depth-sorted face stream; `Execution.emissions` stays
  in command order.
- Opcode 6 modes, lazy F1470 view resolution, F1777 keys, F1780 groups, F1788
  sorting, standalone face keys and transactional commit are as specified above.
- Eight synthetic tests cover ordering and rollback; the local-only real-station
  check (`executed_reference_station_depth_composition_advances`) shows models
  74/75 advance past opcode 6 and stop at `0x000a`/`0xf4` and `0x1c6e`/`0x1c8`.
- No fully dispatched/face-handler ordered trace was captured yet; the existing
  depth/order/executor traces are unchanged.
- v0.12 adds `Execution.ordered_keys` / `Order::finish_with_keys` and a
  `--composed-order` adapter mode that drives the original F1696/F1777/F1778/
  F1780/F1788 with marker callbacks. Rust matches order `[2,3,1]`, keys
  `[9,9,0]` (`ce0d1b4f…`). This is a helper composition, not F1772/face
  handlers/pixels.

- v0.13 decodes/emits point (1) and line (2) operands. `Execution.ordered` is now
  `Vec<Primitive>` (`Face`/`Point`/`Line`) with aligned `ordered_keys`;
  `verify-point-line.py` matches original F1699/F1703 arguments (trace
  `afa955be…`). Point/line are operand events, not geometry or pixels.

Follow-ups (next tasks, in order):

1. Extend the executor through text (10/14), child models (21/26), other
   transform forms (28) and colour state. For each, first add an original
   handler probe and document the payload in `docs/MODEL_COMMANDS.md`; see
   `docs/REFERENCE_HARNESS.md`.
2. Capture a fully dispatched/face-handler ordered primitive trace and replace
   the helper-composition-only evidence.
3. Decide later whether exact F1778 arena allocation is needed; the abstract
   `Order<Emission>` queue remains the approved default (D1).
