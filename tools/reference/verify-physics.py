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
no cache. --flight-step: one whole free-flight tick (F1041, F601/F602 and F1512 in
the order F1021 runs them). --steer: F1037/F1038/F1121 (set speed and heading input). --gravity: F603 (the parent body's pull, with the ffp helpers F1482/F1483/F1496 and F1658). --frames: F590, F604, F605 and F606 (frames of reference, with the cached chain F1677/F1678 and F595). --world-step: a whole tick in a world of bodies (F1041 with gravity F603, auto-roll F609 and the F606 countdown, then F601/F602 and F1512). --collide: F569/F570 (the distance to the parent body and the linear/rotating frame switch F572/F573; F575, F577), contact-free cases.
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
    ("step", "--flight-step", b"JJFFE flight-step-v1", "FFE_STEP_VECTORS",
     "executed_reference_flight_step",
     ["time", "position", "matrix", "drift", "rate", "limits", "velocity", "model", "craft",
      "player", "scanner", "counter", "tics", "mode", "flags", "combat", "combat_velocity",
      "set_speed", "keys", "after"]),
    ("steer", "--steer", b"JJFFE steer-v1", "FFE_STEER_VECTORS", "executed_reference_steer",
     ["active", "keys_on", "time", "boost", "flags", "view", "speed", "previous", "mouse", "flip",
      "roll", "turret", "gun_mounts", "rate", "return", "shift", "hold", "after"]),
    ("frames", "--frames", b"JJFFE frames-v1", "FFE_FRAMES_VECTORS",
     "executed_reference_frames",
     ["stamp", "a", "b", "live", "bodies", "vector", "shift"]),
    ("world", "--world-step", b"JJFFE world-step-v1", "FFE_WORLD_VECTORS",
     "executed_reference_world_step",
     ["time", "stamp", "live", "matrix", "drift", "rate", "limits", "model", "craft", "player",
      "scanner", "counter", "tics", "altitude", "mode", "flags", "combat", "combat_velocity",
      "set_speed", "gravity", "keys", "bodies", "after"]),
    ("collide", "--collide", b"JJFFE collide-v1", "FFE_COLLIDE_VECTORS",
     "executed_reference_collide",
     ["stamp", "tics", "high", "bodies", "result", "flags", "distance", "object", "pad",
      "register"]),
    ("gravity", "--gravity", b"JJFFE gravity-v1", "FFE_GRAVITY_VECTORS",
     "executed_reference_gravity",
     ["parent", "mass", "time", "position", "velocity", "fraction", "after"]),
]


def run(reference, flag, tag, directory):
    result = subprocess.run([str(reference), flag], cwd=directory,
                            capture_output=True, timeout=60)
    if result.returncode or tag not in result.stderr or len(result.stdout) > 1 << 26:
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
    elif field == "bodies":
        def tamper(b):
            if "before" in b:
                return dict(b, before=tamper(b["before"]))
            return dict(b, position=[v + (1 << 40) for v in b["position"]],
                        velocity=[v + 1 for v in b["velocity"]])
        row[field] = [tamper(b) for b in value]
    elif field == "altitude":
        row[field] = 0 if value > 0x2625a00 else 0x7fffffff
    elif field == "live":
        row[field] = [1 - v for v in value]
    elif field in ("keys", "flip"):
        row[field] = [1 - v for v in value]
    elif field in ("keys_on", "roll", "return", "shift"):
        row[field] = 1 - value
    elif field == "active":
        row[field] = value ^ 0x80
    elif field == "boost":
        row[field] = value ^ 0x7f
    elif field == "view":
        row[field] = value ^ 1
    elif field == "hold":
        row[field] = [v + 40 for v in value]
    elif field in ("previous", "mouse"):
        row[field] = (value + (1 << 22)) & 0xffffffff
    elif field == "gun_mounts":
        row[field] = value + 3
    elif field == "speed":
        row[field] = value // 2 + 1000
    elif field in ("mode", "flags"):
        row[field] = value ^ 1
    elif field == "combat":
        row[field] = value ^ 2
    elif isinstance(value, list):
        # Only a matrix coordinate's high 16 bits matter.
        step = {"matrix": 0x10000, "combat_velocity": 300000}.get(field, 1)
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
        "scenario": "jjffe-accelerate-v1 + jjffe-orient-v1 + jjffe-player-orient-v1 + jjffe-advance-v1 + jjffe-flight-step-v1 + jjffe-steer-v1 + jjffe-gravity-v1 + jjffe-frames-v1 + jjffe-world-step-v1 + jjffe-collide-v1",
        "repeat_matches": True,
        "empty_traces_rejected": True,
        **summary,
    }, indent=2))


if __name__ == "__main__":
    main()
