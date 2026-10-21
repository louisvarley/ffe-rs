"""Compare whole real station programs run by the original F1772 with Rust."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

CASES = 2
# (row, field, label); row 0 is model 74, row 1 model 75.
TAMPERS = [
    (0, "faces", "face count"), (1, "emissions", "face stream"),
    (0, "points", "point"), (0, "nested", "text call"), (1, "nested", "submodel call"),
    (1, "locals", "final locals"), (0, "basis", "final basis"),
    (1, "colour", "final colour"), (0, "handedness", "final handedness"),
    (1, "alive", "alive"),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--cache", required=True, type=Path, help="data.bin with station programs and the sine table")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-station-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--station"], cwd=directory,
                                    capture_output=True, timeout=30)
            if result.returncode:
                raise RuntimeError(f"station reference failed ({result.returncode}): "
                                   + result.stderr.decode(errors="replace"))
            if b"JJFFE station-v1" not in result.stderr or len(result.stdout) > 65536:
                raise RuntimeError("wrong/unbounded station reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != CASES:
            raise RuntimeError("station traces differ or wrong line count")
        trace = Path(directory) / "station.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_STATION_VECTORS=str(trace),
                   FFE_STATION_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                   "station_reference", "executed_reference_station_programs",
                   "--", "--ignored", "--exact"]
        discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=300)
        names = [line for line in discovered.stdout.decode().splitlines()
                 if line.endswith(": test")]
        if names != ["executed_reference_station_programs: test"]:
            raise RuntimeError("station comparison not discovered")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
        for row, field, label in TAMPERS:
            rows = [json.loads(line) for line in outputs[0].splitlines()]
            value = rows[row][field]
            if field == "emissions" or field == "locals":
                rows[row][field] = ("0" if value[0] != "0" else "1") + value[1:]
            elif field in ("points", "nested"):
                value[-1][1] ^= 1
            elif isinstance(value, list):
                value[0] ^= 1
            else:
                rows[row][field] ^= 1
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in rows))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=300).returncode == 0:
                raise RuntimeError(f"altered {label} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=300).returncode == 0:
            raise RuntimeError("empty station reference accepted")
        trace.write_bytes(outputs[0])
        subprocess.run(command, cwd=root, env=env, check=True, timeout=300)
    print(json.dumps({
        "scenario": "jjffe-station-v1",
        "cases": CASES,
        "repeat_matches": True,
        "rust_station_programs_match": True,
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(outputs[0]).hexdigest(),
        "render_order_glyph_child_execution_or_pixels_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
