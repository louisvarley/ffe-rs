"""Compare the physics routines with the executed original.

--accelerate (jjffe-commands): F601 and F602 (velocity from thrust over a time
step) on 4096 synthetic objects in the small, medium and large time-step
regimes; the velocity dwords and fraction words are compared. --orient: F585
(angular rates spent as small rotations, with F584, F611 and F1520/F1519/F1552)
on 4096 synthetic objects; the matrix, drift counter, rates and scanner word
are compared. --player-orient: F1041 (the player's free-flight orientation and
thrust request, with F607, F1043 and F599) in free space; the orientation, rates,
thrust words and caps are compared. --advance: F1512/F1511 (velocity over a
64-bit time added to the 64-bit position); the position after is compared. Needs
no cache.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

SCENARIOS = [
    ("accelerate", "--accelerate", b"JJFFE accelerate-v1", "FFE_ACCELERATE_VECTORS",
     "executed_reference_accelerate",
     ["function", "tics", "matrix", "thrust", "fraction", "velocity", "cap", "after"]),
    ("orient", "--orient", b"JJFFE orient-v1", "FFE_ORIENT_VECTORS",
     "executed_reference_orient",
     ["matrix", "drift", "rate", "autopilot", "model", "craft", "player", "scanner", "time",
      "ai", "ai_time", "after"]),
    ("player", "--player-orient", b"JJFFE player-orient-v1", "FFE_PLAYER_VECTORS",
     "executed_reference_player_orient",
     ["matrix", "drift", "rate", "limits", "velocity", "model", "craft", "player", "scanner",
      "counter", "tics", "mode", "flags", "combat", "combat_velocity", "set_speed", "keys",
      "after"]),  # (altitude is only a guard, so it is not tampered)
    ("advance", "--advance", b"JJFFE advance-v1", "FFE_ADVANCE_VECTORS",
     "executed_reference_advance", ["time", "position", "velocity", "after"]),
]


def run(reference, flag, tag, directory):
    result = subprocess.run([str(reference), flag], cwd=directory,
                            capture_output=True, timeout=60)
    if result.returncode or tag not in result.stderr or len(result.stdout) > 1 << 25:
        raise RuntimeError(f"{flag} failed or wrong/unbounded output")
    return result.stdout


def alter(row, field):
    """Change one field of every row (one changed input need not bind one case)."""
    value = row[field]
    if field == "function":
        row[field] = 601 if value != 601 else 602
    elif isinstance(value, dict):
        key = next(iter(value))
        inner = value[key]
        value[key] = [v + 1 for v in inner] if isinstance(inner, list) else inner + 1
    elif field == "keys":
        row[field] = [1 - v for v in value]
    elif field in ("mode", "flags"):
        row[field] = value ^ 1
    elif field == "combat":
        row[field] = value ^ 2
    elif isinstance(value, list):
        # Only a matrix coordinate's high 16 bits matter.
        step = 0x10000 if field == "matrix" else 1
        row[field] = [v + step for v in value]
    elif field == "time":
        row[field] = value + (1 << 20)
    elif field == "tics":
        row[field] = value + 0x2000
    elif field == "set_speed":
        row[field] = value + 100000
    else:
        row[field] = value + 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path, help="jjffe-commands")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    summary = {}
    with tempfile.TemporaryDirectory(prefix="ffe-physics-") as directory:
        for name, flag, tag, variable, test, tampers in SCENARIOS:
            trace_bytes = run(reference, flag, tag, directory)
            if trace_bytes != run(reference, flag, tag, directory):
                raise RuntimeError(f"{flag} traces differ between runs")
            trace = Path(directory) / f"{name}.jsonl"
            trace.write_bytes(trace_bytes)
            env = dict(os.environ, **{variable: str(trace)})
            command = [cargo, "test", "--release", "--locked", "-p", "ffe-rs", "--test",
                       "physics_reference", test, "--", "--ignored", "--exact", "--nocapture"]
            result = subprocess.run(command, cwd=root, env=env, capture_output=True,
                                    timeout=1200)
            lines = [line for line in result.stderr.decode(errors="replace").splitlines()
                     if line.startswith("compared")]
            if result.returncode:
                raise RuntimeError(f"Rust/original {name} mismatch:\n" +
                                   result.stderr.decode(errors="replace")[-2000:])
            rows = [json.loads(line) for line in trace_bytes.splitlines()]
            for field in tampers:
                altered = [dict(row) for row in rows]
                for row in altered:
                    if isinstance(row[field], dict):
                        row[field] = dict(row[field])
                    alter(row, field)
                trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
                if subprocess.run(command, cwd=root, env=env, capture_output=True,
                                  timeout=1200).returncode == 0:
                    raise RuntimeError(f"altered {name} field {field} accepted")
            trace.write_bytes(b"")
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=1200).returncode == 0:
                raise RuntimeError(f"empty {name} trace accepted")
            summary[name] = {
                "summary": lines,
                "tamper_fields_rejected": tampers,
                "trace_sha256": hashlib.sha256(trace_bytes).hexdigest(),
            }
    print(json.dumps({
        "scenario": "jjffe-accelerate-v1 + jjffe-orient-v1 + jjffe-player-orient-v1 + jjffe-advance-v1",
        "repeat_matches": True,
        "empty_traces_rejected": True,
        **summary,
    }, indent=2))


if __name__ == "__main__":
    main()
