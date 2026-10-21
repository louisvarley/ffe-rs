"""Compare every natively executed model slot with the original F1772 run of it."""
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


def capture(reference, directory):
    rows, crashed = [], []
    for index in range(SLOTS):
        result = subprocess.run([str(reference), "--station-model", str(index)],
                                cwd=directory, capture_output=True, timeout=30)
        if result.returncode:
            crashed.append(index)
            continue
        if b"JJFFE station-model-v1" not in result.stderr or len(result.stdout) > 1 << 20:
            raise RuntimeError(f"wrong/unbounded reference output for slot {index}")
        rows.append(result.stdout)
    return b"".join(rows), crashed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--game", required=True, type=Path, help="installed game directory")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin with the sine table")
    parser.add_argument("--minimum", type=int, default=180,
                        help="fewest models that must be compared")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    exe = next(p for p in args.game.resolve(strict=True).iterdir()
               if p.name.upper() == "FIRSTENC.EXE")
    table = TABLES.get(hashlib.sha256(exe.read_bytes()).hexdigest())
    if table is None:
        raise RuntimeError("unrecognised FIRSTENC.EXE")
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-station-models-") as directory:
        first, crashed = capture(reference, directory)
        second, crashed_again = capture(reference, directory)
        if first != second or crashed != crashed_again:
            raise RuntimeError("station-model traces differ between runs")
        trace = Path(directory) / "station-models.jsonl"
        trace.write_bytes(first)
        env = dict(os.environ, FFE_STATION_MODELS_VECTORS=str(trace), FFE_STATION_EXE=str(exe),
                   FFE_STATION_TABLE_RVA=table, FFE_STATION_CACHE=str(args.cache.resolve(strict=True)),
                   FFE_STATION_MODELS_MINIMUM=str(args.minimum))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test",
                   "station_models_reference", "executed_reference_station_models",
                   "--", "--ignored", "--exact", "--nocapture"]
        result = subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=600)
        summary = [line for line in result.stderr.decode(errors="replace").splitlines()
                   if line.startswith(("compared", "  model"))]
        if result.returncode:
            raise RuntimeError("Rust/original mismatch:\n" + "\n".join(summary))
        rows = [json.loads(line) for line in first.splitlines()]
        position = {row["model"]: number for number, row in enumerate(rows)}
        for model, field in [(74, "emissions"), (75, "faces"), (86, "nested"), (380, "nested")]:
            altered = [dict(row) for row in rows]
            row = altered[position[model]]
            if field == "emissions":
                row[field] = ("0" if row[field][0] != "0" else "1") + row[field][1:]
            elif field == "faces":
                row[field] += 1
            else:
                row[field] = row[field][:-1]
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=600).returncode == 0:
                raise RuntimeError(f"altered model {model} {field} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=600).returncode == 0:
            raise RuntimeError("empty station-model reference accepted")
    print(json.dumps({
        "scenario": "jjffe-station-model-v1",
        "slots": SLOTS,
        "original_crashed_slots": crashed,
        "summary": summary,
        "repeat_matches": True,
        "all_tamper_fields_and_empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(first).hexdigest(),
        "render_order_glyph_child_execution_or_pixels_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
