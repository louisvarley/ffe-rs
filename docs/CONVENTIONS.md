# Conventions

Working rules for this workspace, gathered from the code and the existing docs.
Where the docs are silent, these are the de facto rules the code already follows.

## Errors

- Use `Result<T, String>` in `ffe-core`/`ffe-content`/`ffe-convert` for fallible
  helpers. No `anyhow`/`thiserror`/custom error enums unless a change justifies
  them and updates this file.
- Fail explicitly on unsupported input; never return a guessed/zero fallback.
- Operations are **transactional**: on error, leave the caller's context, caches
  and queues unchanged (see `ffe-core::model_execute`, `model_instance`,
  `model_order`).
- Corrupt/oversized input fails cleanly (no panic). `docs/DEVIATIONS.md` PLT-006.

## Numeric model

- Little-endian 8/16/32/64-bit; explicit widths; `wrapping_*`/`saturating_*` for
  overflow; no accidental `usize` math on serialized data.
- No `f32`/`f64` in `ffe-core` simulation code. The experimental `ffe-rs` flight
  uses floats and is an explicit, recorded deviation (EXP-001).
- Fixed-point: 1.31 matrices (native identity `0x7fffffff`, not exact `+1`),
  16.16 values, 16-bit binary angles, FFP `{mantissa, exponent}`. Preserve x87
  rounding where the original uses it (F1519_fp etc.).
- Angles: `0x10000` = full turn. Sine via the imported table, never `f64::sin`.

## Bounds and budgets

- All decoders bound input size (streams, caches, traces) and reject malformed/
  truncated/out-of-range data with a specific error.
- Model executor and order queue use a deliberate **1..4096** operation budget,
  distinct from the legacy 50,000-word arena. Document any change to this.

## Safety and determinism

- No `unsafe` unless justified and documented (`docs/ARCHITECTURE.md` §2.2). The
  crate set currently contains none.
- `ffe-core` must not depend on `wgpu`, `winit`, `std::time`, `std::fs`, or any
  OS crate. No wall clock, no I/O, no unseeded randomness.

## Naming and layout

- Standard Rust: `snake_case` modules/functions, `CamelCase` types, one module
  per concern under `ffe-core` (`model`, `model_commands`, `model_depth`,
  `model_execute`, `model_instance`, `model_math`, `model_normals`,
  `model_order`).
- Tests live in `crates/<crate>/tests/<module>.rs`; local reference tests are
  named `<x>_reference.rs` and are `#[ignore]`.

## Process

- Every new milestone must append a `docs/PORT_LOG.md` entry (newest first),
  update `docs/STATUS.md`, and extend `docs/FORMATS.md`/`CLAUDE.MD` where the
  contract changed. Update the test count and build/version strings in
  `README.md` in the same change.
- Every intentional behaviour change goes in `docs/DEVIATIONS.md` before it
  becomes the default, with a failing test first.
- Verification commands: `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked`, `python -m unittest ci.test_check_forbidden`,
  `python ci/check_forbidden.py`.
