# Reference harness — adding a differential test against executed JJFFE

The reimplementation is verified by comparing bounded Rust helpers against the
executed original JJFFE 2.8a7 routines, through a locally built headless adapter.
This file is the recipe for adding a new comparison. Follow it exactly; the
supplied `jjffe-master/` sources are **never edited**.

## 1. The pieces

- `tools/reference/headless.c` — the adapter `main`; one `--<mode>` per scenario,
  printing JSONL to stdout and a `JJFFE <mode>-v1:` tag to stderr.
- `tools/reference/exports.inc` — NASM `GLOBAL`/`equ` aliases that name the
  original `FUNC_*`/`DATA_*` labels. Preincluded during assembly, never patched
  into `ffe.asm`.
- `tools/reference/build-smoke.ps1` — builds `jjffe-smoke.exe` (and the
  instrumented `jjffe-commands.exe`) from the unchanged sources plus `headless.c`.
- `tools/reference/win-compat.h` and `tools/reference/winstub/` — forced include and empty headers that let
  `headless.c` build with MSVC again: `sigsetjmp`, `setitimer` and `personality` are shimmed, and `fork` and
  `open_memstream` report failure, so the scenarios that need a forked child (`--surface-draw`, and the fault
  catchers) are Linux-only. 35 of the 36 oracles in `verify-all.py` pass on Windows; `verify-world.py` does not,
  because `--worldgen` reads uninitialised stack words that hold addresses, which Linux pins by disabling ASLR
  (`personality`) and Windows cannot.
- `tools/reference/build-linux.sh` — the same two binaries as 32-bit ELF
  (`nasm`, `gcc -m32`) under `.cache/jjffe-reference-linux/`. Linux traces are
  LF-terminated; hash them with CRLF endings to compare with recorded hashes.
- `tools/reference/make-probe.py` — writes the instrumented `ffe-command-probe.asm`
  for both builds: whole-file emitter hooks plus hooks scoped to one routine's
  range (each with an exact expected site count).
- `tools/reference/verify-<x>.py` — runs the mode twice, checks repeat/line
  count, drives the ignored Rust test, tampers each result field, and rejects
  empty traces.
- `crates/ffe-rs/tests/<x>_reference.rs` — an `#[ignore]`d test that reads a
  `FFE_*_VECTORS` env var pointing at the JSONL trace and compares Rust output.

## 2. Add an adapter mode

1. In `headless.c`, find the real routine's label in `jjffe-master/ffe.asm` (or
   `ffedat.asm`/`ffebmp.asm`), e.g. `FUNC_001479`.
2. In `exports.inc`, add `GLOBAL _ref_xxx` and `_ref_xxx equ FUNC_00xxxx` (or the
   `DATA_*` label for a buffer).
3. In `headless.c`, declare `extern` for the aliased symbol and add a
   `static int xxx_vectors(void)` that:
   - prints `fprintf(stderr, "JJFFE xxx-v1: <what is and is not validated>\n");`
   - loops over synthetic inputs (never original assets), calling the reference
     routine through the alias;
   - prints one JSON object per case to stdout (fields = what Rust must match);
   - returns 0.
4. Register it: `if (argc == 2 && !strcmp(argv[1], "--xxx")) return xxx_vectors();`.
5. In `build-smoke.ps1`, add `xxx_adapter = 'xxx-v1'` to the manifest object.

## 3. Add the Rust comparison

1. Add `crates/ffe-rs/tests/xxx_reference.rs` with:

   ```rust
   #[test]
   #[ignore = "Local-only original <what>; use tools/reference/verify-xxx.py"]
   fn executed_reference_xxx() { /* read FFE_XXX_VECTORS, compare, assert count */ }
   ```

   Reconstruct the input ordering from the trace (like the other `*_reference.rs`
   files do with fixed value tables), and assert the exact expected case count.
2. Keep the test read bound (e.g. `< 8 MiB`) and `deny_unknown_fields` on the
   deserialized struct.

## 4. Add the verify script

Copy an existing `verify-*.py` (e.g. `verify-depth.py` for a key helper,
`verify-instance.py` for a two-mode helper) and adapt:

1. Build the `--mode` list with `(mode, env_var, expected_count, test_name,
   [tamper_fields])`.
2. Run the reference twice in a temp dir; fail if the two traces differ, the
   stdout is too large, or the stderr lacks the `JJFFE <mode>-v1` tag.
3. Write the trace to a file, set the env var, and run
   `cargo test --locked -p ffe-rs --test <file> <name> -- --ignored --exact`.
4. For each tamper field, alter a reachable row and assert the Rust test **fails**.
5. Assert an **empty** trace is rejected.
6. Re-run the untouched trace and assert it **passes**.
7. Print a summary including `trace_sha256`; copy that hash into `docs/PORT_LOG.md`
   and `docs/STATUS.md`.

## 5. Rules

- Never edit `jjffe-master/` sources. The command probe that patches emission
  calls does so in an **ignored generated copy** (`ffe-command-probe.asm`), not
  in `ffe.asm`.
- Reference binaries and traces stay local and ignored; never commit them.
- Public CI runs only synthetic tests; all `*_reference.rs` tests are `#[ignore]`.
- The alias table (`exports.inc`) is adapter-only; do not add game logic there.
- Prefer a fresh `--mode` over overloading an existing mode's scenario.
