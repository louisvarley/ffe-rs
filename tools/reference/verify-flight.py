"""Compare scripted flight scenarios (idle boot, launch, retained planet) with the executed original.

`--flight idle|takeoff|planet 900` (jjffe-smoke) runs the real game from boot
with a scripted Y and start key, then (for the last two) the Tab launch action at
frame 170 and the set-speed/faster keys from frame 260. Each frame emits one
bounded JSONL row: the player index, view mode, F1013's altitude and F1121's set
speed, the player planet (`+0x56`: index, model, parent and position), the ship's
position and velocity, and an FNV-1a chain over exactly those integers. No
framebuffer, image or memory dump is written.

The Rust tests (`flight_reference.rs`) must reproduce the hash chain and show:

* `idle`: modes 0x00 (intro) then 0x30 on the pad, with position, velocity and
  planet unchanged;
* `takeoff`/`planet`: 0x30 -> 0x24 -> 0x26 -> 0x00, the F1013 hand-off above
  0x30d40 or with a non-zero set speed, no teleport-sized frame step, and the
  same planet retained through free flight (`+0x56` never changes);
* the port's `Flight` idle and `Navigator`/`OriginalFlight` takeoff reach the same
  observable states and the same planet model on start record 1.

Repeated runs must be byte-identical. Altered fields and an empty trace must be
rejected by the Rust test.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

SCENARIOS = ["idle", "takeoff", "planet"]
FRAMES = 900
FIRST_FRAME = 140
ROWS = FRAMES - FIRST_FRAME
BANNER = b"JJFFE flight-v1"


def run_reference(reference, scenario, directory):
    result = subprocess.run(
        [str(reference), "--flight", scenario, str(FRAMES)],
        cwd=directory, capture_output=True, timeout=180,
    )
    if result.returncode:
        raise RuntimeError(
            f"--flight {scenario} failed ({result.returncode}): "
            + result.stderr.decode(errors="replace")
        )
    if BANNER not in result.stderr or len(result.stdout) > (8 << 20):
        raise RuntimeError(f"wrong or unbounded --flight {scenario} reference")
    if len(result.stdout.splitlines()) != ROWS:
        raise RuntimeError(f"--flight {scenario}: unexpected number of rows")
    return result.stdout


def bump(row, field):
    def change(value):
        value = json.loads(json.dumps(value))
        if field == "mode":
            value["mode"] = (value["mode"] + 1) % 256
        elif field == "start_type":
            if value["player"]:
                value["start_type"] += 1
        elif field == "player":
            if value["player"]:
                value["player"] += 1
        elif field == "frame":
            value["frame"] += 1
        elif field == "position":
            value["position"][0] += 1
        elif field == "velocity":
            value["velocity"][1] += 1
        elif field == "planet_model":
            value["planet"]["model"] += 1
        elif field == "planet_parent":
            value["planet"]["parent"] += 1
        elif field == "planet_position":
            value["planet"]["position"][2] += 1
        elif field == "planet_index":
            value["planet"]["index"] += 1
        elif field == "prev_hash":
            digits = list(value["prev_hash"])
            digits[0] = "0" if digits[0] != "0" else "1"
            value["prev_hash"] = "".join(digits)
        elif field == "state_hash":
            digits = list(value["state_hash"])
            digits[-1] = "0" if digits[-1] != "0" else "1"
            value["state_hash"] = "".join(digits)
        elif field == "scenario":
            value["scenario"] = "takeoff" if value["scenario"] != "takeoff" else "idle"
        else:
            value[field] += 1
        return value

    return change(row)


FIELDS = [
    "frame", "mode", "tics", "player", "start_type", "altitude", "set_speed",
    "planet_index", "planet_model", "planet_parent", "planet_position",
    "position", "velocity", "prev_hash", "state_hash", "scenario",
]


def main():
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--reference", required=True, type=Path,
                        help="jjffe-smoke (the plain new-game adapter)")
    parser.add_argument("--cache", required=True, type=Path,
                        help="data.bin for the port side's Navigator takeoff")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    cache = args.cache.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")

    def rust_test(name, env):
        command = [cargo, "test", "--release", "--locked", "-p", "ffe-rs",
                   "--test", "flight_reference", name, "--", "--ignored", "--exact"]
        return subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=1800)

    with tempfile.TemporaryDirectory(prefix="ffe-flight-") as directory:
        traces = []
        for scenario in SCENARIOS:
            runs = []
            for index in range(2):
                working = Path(directory) / f"{scenario}-{index}"
                working.mkdir()
                runs.append(run_reference(reference, scenario, working))
            if runs[0] != runs[1]:
                raise RuntimeError(f"{scenario}: repeated reference traces differ")
            traces.append(runs[0])
        output = b"".join(traces)
        trace = Path(directory) / "flight.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_FLIGHT_VECTORS=str(trace),
                   FFE_WORLD_CACHE=str(cache))

        result = rust_test("executed_reference_flight_scenarios", env)
        if result.returncode:
            raise RuntimeError("reference flight trace rejected:\n"
                               + result.stderr.decode(errors="replace")[-3000:])
        result = rust_test("port_flight_scenarios_match_the_reference", env)
        if result.returncode:
            raise RuntimeError("Rust/original flight scenario mismatch:\n"
                               + result.stderr.decode(errors="replace")[-3000:])

        rows = [json.loads(line) for line in output.splitlines()]
        for field in FIELDS:
            altered = [bump(row, field) for row in rows]
            trace.write_bytes(b"".join(json.dumps(row).encode() + b"\n"
                                       for row in altered))
            if rust_test("executed_reference_flight_scenarios", env).returncode == 0:
                raise RuntimeError(f"altered flight field {field} accepted")
        trace.write_bytes(b"")
        if rust_test("executed_reference_flight_scenarios", env).returncode == 0:
            raise RuntimeError("empty flight trace accepted")

    modes = {}
    for scenario in SCENARIOS:
        seen = []
        for row in rows:
            if row["scenario"] != scenario or not row["player"]:
                continue
            if not seen or seen[-1] != row["mode"]:
                seen.append(row["mode"])
        modes[scenario] = seen
    print(json.dumps({
        "scenario": "jjffe-flight-v1",
        "scenarios": SCENARIOS,
        "rows": len(rows),
        "rows_per_scenario": ROWS,
        "modes": modes,
        "planet": {"index": rows[-1]["planet"]["index"],
                   "model": rows[-1]["planet"]["model"],
                   "parent": rows[-1]["planet"]["parent"]},
        "repeat_traces_match": True,
        "rust_results_match": True,
        "tamper_fields_rejected": FIELDS,
        "empty_trace_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
