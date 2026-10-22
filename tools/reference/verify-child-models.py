"""Compare every natively executed model scene, child models included, with the
original F1772 run of the slot in which child calls really execute (--child-model)."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

SLOTS = 466
# Model table RVA by exact executable SHA-256 (see ffe-tools model_layout).
TABLES = {
    "d919566140abfc298bc9929bc9141d92526586821f017be1336253a04bca1e72": "0x0d0cfc",
    "9957dc34aa9ef93d9a69c5f5be8e13d243046e758ad83c8a35241e31b7e6e616": "0x0d0cf8",
}


def capture(reference, directory, depth):
    rows, crashed = [], []
    extra = [] if depth == 2000 else [str(depth)]
    for index in range(SLOTS):
        result = subprocess.run([str(reference), "--child-model", str(index)] + extra,
                                cwd=directory, capture_output=True, timeout=30)
        if result.returncode:
            crashed.append(index)
            continue
        if b"JJFFE child-model-v1" not in result.stderr or len(result.stdout) > 4 << 20:
            raise RuntimeError(f"wrong/unbounded reference output for slot {index}")
        rows.append(result.stdout)
    return b"".join(rows), crashed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--game", required=True, type=Path, help="installed game directory")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin with the sine table")
    parser.add_argument("--minimum", type=int, default=400,
                        help="fewest models that must be compared")
    parser.add_argument("--depth", type=lambda v: int(v, 0), default=2000,
                        help="instance position z (2000 by default; e.g. 0x50000 for thrusters)")
    parser.add_argument("--verbose", action="store_true",
                        help="print skip reasons and first differences")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    exe = next(p for p in args.game.resolve(strict=True).iterdir()
               if p.name.upper() == "FIRSTENC.EXE")
    table = TABLES.get(hashlib.sha256(exe.read_bytes()).hexdigest())
    if table is None:
        raise RuntimeError("unrecognised FIRSTENC.EXE")
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-child-models-") as directory:
        first, crashed = capture(reference, directory, args.depth)
        second, crashed_again = capture(reference, directory, args.depth)
        if first != second or crashed != crashed_again:
            raise RuntimeError("child-model traces differ between runs")
        trace = Path(directory) / "child-models.jsonl"
        trace.write_bytes(first)
        env = dict(os.environ, FFE_CHILD_VECTORS=str(trace), FFE_CHILD_EXE=str(exe),
                   FFE_CHILD_TABLE_RVA=table, FFE_CHILD_CACHE=str(args.cache.resolve(strict=True)),
                   FFE_CHILD_MINIMUM=str(args.minimum), FFE_CHILD_DEPTH=str(args.depth))
        if args.verbose:
            env["FFE_CHILD_VERBOSE"] = "1"
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                   "child_models_reference", "executed_reference_child_models",
                   "--", "--ignored", "--exact", "--nocapture"]
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=900)
        summary = [line for line in result.stderr.decode(errors="replace").splitlines()
                   if line.startswith(("compared", "children", "thrusters", "not executed", "  model",
                                       "  skipped"))]
        if result.returncode:
            raise RuntimeError("Rust/original mismatch:\n" + "\n".join(summary))
        rows = [json.loads(line) for line in first.splitlines()]
        position = {row["model"]: number for number, row in enumerate(rows)}
        # Tamper: a child face's depth, a child set-up field, a cull outcome, a thruster
        # glow point, ramp colour and line point, a queue key.
        tampers = []
        for row in rows:
            nested = row.get("nested", [])
            for kind, field in [(22, 1), (24, 1), (19, 1), (25, 2), (42, 2), (45, 3), (48, 1)]:
                hit = next((i for i, e in enumerate(nested)
                            if e[0] == kind and (kind != 22 or e[1] > 0)), None)
                if hit is not None and not any(t[1] == kind for t in tampers):
                    tampers.append((row["model"], kind, hit, field))
        queued = next(row["model"] for row in rows
                      if any(e[0] == 21 for e in row.get("nested", [])) and row.get("queue"))
        for model, kind, hit, field in tampers + [(queued, "queue", None, None)]:
            altered = [json.loads(json.dumps(r)) for r in rows]
            row = altered[position[model]]
            if kind == "queue":
                row["queue"][-1][2] ^= 1
            else:
                row["nested"][hit][field] ^= 1
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=900).returncode == 0:
                raise RuntimeError(f"altered model {model} event {kind} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=900).returncode == 0:
            raise RuntimeError("empty child-model reference accepted")
    print(json.dumps({
        "scenario": "jjffe-child-model-v1",
        "slots": SLOTS,
        "depth": args.depth,
        "original_crashed_slots": crashed,
        "summary": summary,
        "repeat_matches": True,
        "tampered_events": [str(t[1]) for t in tampers] + ["queue"],
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(first).hexdigest(),
        "glyphs_or_pixels_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
