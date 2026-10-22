"""Compare the polygon helpers (F1467, F1564, F1557, F1554, F1547) with Rust."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def capture(reference, directory):
    result = subprocess.run([str(reference), "--polygon-helpers"], cwd=directory,
                            capture_output=True, timeout=60)
    if result.returncode or b"JJFFE polygon-helpers-v1" not in result.stderr:
        raise RuntimeError("wrong reference output for --polygon-helpers")
    if len(result.stdout) > 16 << 20:
        raise RuntimeError("unbounded reference output")
    return result.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-polygon-") as directory:
        first = capture(reference, directory)
        if first != capture(reference, directory):
            raise RuntimeError("polygon helper traces differ between runs")
        trace = Path(directory) / "polygon-helpers.jsonl"
        trace.write_bytes(first)
        env = dict(os.environ, FFE_POLYGON_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "polygon_reference",
                   "executed_reference_polygon_helpers", "--", "--ignored", "--exact",
                   "--nocapture"]
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=600)
        summary = [line for line in result.stderr.decode(errors="replace").splitlines()
                   if line.startswith("compared")]
        if result.returncode:
            raise RuntimeError("Rust/original mismatch:\n"
                               + result.stderr.decode(errors="replace")[-2000:])
        rows = [json.loads(line) for line in first.splitlines()]
        kinds = sorted({row["k"] for row in rows})
        for kind in kinds:
            altered = [dict(row) for row in rows]
            number = max(n for n, row in enumerate(altered) if row["k"] == kind)
            row = altered[number]
            row["out"] = [row["out"][0] + 1] + row["out"][1:]
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=600).returncode == 0:
                raise RuntimeError(f"altered {kind} row {number} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=600).returncode == 0:
            raise RuntimeError("empty polygon helper reference accepted")
    print(json.dumps({
        "scenario": "jjffe-polygon-helpers-v1",
        "summary": summary,
        "rows": len(rows),
        "repeat_matches": True,
        "all_tamper_kinds_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(first).hexdigest(),
        "rasterisation_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
