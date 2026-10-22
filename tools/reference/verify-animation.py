"""Compare the original animation inputs with Rust (ffe_core::model_animation).

--animation (jjffe-smoke or jjffe-commands): F1675 MatBuildYZT and F866, the
orbital update (F865 door word, rotation step). --object-instance
(jjffe-commands only): the F1537 frame loop (global word 0, light), F1680's
set-up of each object and F1683's display set-up, captured at F1773.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

ANIMATION_LINES = 2640 + 4096
OBJECT_LINES = 96 + 4096


def run_twice(binary, mode, banner, lines, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), mode], cwd=directory, capture_output=True, timeout=120)
        if result.returncode:
            raise RuntimeError(f"{mode} reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if banner not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError(f"wrong/unbounded {mode} reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != lines:
        raise RuntimeError(f"{mode} traces differ or wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def flip(value):
    return value ^ 1


def animation_tampers(rows):
    """(label, function changing a copy of the rows)."""
    def yzt(rows):
        i = first(rows, lambda r: r["k"] == "yzt" and r["a"] == 0x2020 and r["b"] == 0x1234)
        rows[i]["m"][3] = flip(rows[i]["m"][3])

    def rebuilt(rows):
        i = first(rows, lambda r: r["k"] == "orbital" and r["m"] is not None)
        rows[i]["m"][0] = flip(rows[i]["m"][0])

    def not_rebuilt(rows):
        i = first(rows, lambda r: r["k"] == "orbital" and r["m"] is None and r["flags"] & 0x20 == 0
                  and r["speed"] not in (0,))
        rows[i]["m"] = [2147483647, 0, 0, 0, 2147483647, 0, 0, 0, 2147483647]

    def door(rows):
        i = first(rows, lambda r: r["k"] == "orbital" and r["flags"] & 0x30 and r["out_word"] != r["word"])
        rows[i]["out_word"] = flip(rows[i]["out_word"])

    def state(rows):
        i = first(rows, lambda r: r["k"] == "orbital" and r["flags"] & 0x30 and r["out_state"] != r["state"])
        rows[i]["out_state"] = rows[i]["state"]

    return [("F1675 matrix", yzt), ("F866 rebuilt matrix", rebuilt),
            ("F866 rebuild where none", not_rebuilt), ("F865 door word", door),
            ("F865 door state", state)]


def object_tampers(rows):
    def drawn(rows, field, index=None):
        for row in rows:
            if row["k"] != "frame":
                continue
            for obj in row["objects"]:
                if obj["outcome"] == 1:
                    value = obj["instance"][field]
                    if index is None:
                        obj["instance"][field] = flip(value)
                    else:
                        value[index] = flip(value[index])
                    return
        raise RuntimeError("no drawn object")

    def object_field(rows, predicate, field):
        for row in rows:
            if row["k"] != "frame":
                continue
            for obj in row["objects"]:
                if predicate(obj):
                    obj[field] = flip(obj[field])
                    return
        raise RuntimeError("no object for a tamper case")

    def display(rows, field, index=None):
        i = first(rows, lambda r: r["k"] == "display" and r["outcome"] == 1)
        if index is None:
            rows[i]["instance"][field] = flip(rows[i]["instance"][field])
        else:
            rows[i]["instance"][field][index] = flip(rows[i]["instance"][field][index])

    def light(rows):
        rows[0]["light"][1] = flip(rows[0]["light"][1])

    return [
        ("F1534 light", light),
        ("global word 0", lambda r: object_field(r, lambda o: o["sleep"] == 0, "out_word0")),
        ("sleeping object", lambda r: object_field(r, lambda o: o["sleep"] != 0, "out_sleep")),
        ("distance class", lambda r: object_field(r, lambda o: True, "distance")),
        ("outside byte", lambda r: object_field(r, lambda o: o["outside"] != 0, "outside")),
        ("outcome", lambda r: object_field(r, lambda o: o["outcome"] == 1, "outcome")),
        ("instance position", lambda r: drawn(r, "position", 2)),
        ("instance frame", lambda r: drawn(r, "matrix", 4)),
        ("instance light +0x38", lambda r: drawn(r, "direction", 0)),
        ("observer", lambda r: drawn(r, "observer", 1)),
        ("observer scale", lambda r: drawn(r, "observer_scale")),
        ("key scale", lambda r: drawn(r, "key_scale")),
        ("operand scale", lambda r: drawn(r, "scale")),
        ("local 0", lambda r: drawn(r, "local0")),
        ("display position", lambda r: display(r, "position", 0)),
        ("display observer scale", lambda r: display(r, "observer_scale")),
        ("display light", lambda r: display(r, "light", 2)),
    ]


def check(name, trace, output, env, command, root, tampers):
    trace.write_bytes(output)
    discovered = subprocess.run(command + ["--list"], cwd=root, env=env,
                                capture_output=True, check=True, timeout=600)
    names = [line for line in discovered.stdout.decode().splitlines() if line.endswith(": test")]
    if names != [f"{name}: test"]:
        raise RuntimeError(f"{name} not discovered")
    subprocess.run(command, cwd=root, env=env, check=True, timeout=600)
    rows = [json.loads(line) for line in output.splitlines()]
    for label, change in tampers(rows):
        altered = json.loads(json.dumps(rows))
        change(altered)
        trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=600).returncode == 0:
            raise RuntimeError(f"altered {label} accepted")
    trace.write_bytes(b"")
    if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=600).returncode == 0:
        raise RuntimeError(f"empty {name} trace accepted")
    trace.write_bytes(output)
    subprocess.run(command, cwd=root, env=env, check=True, timeout=600)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path,
                        help="the instrumented jjffe-commands binary")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin with the sine table")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-animation-") as directory:
        animation = run_twice(reference, "--animation", b"JJFFE animation-v1", ANIMATION_LINES, directory)
        objects = run_twice(reference, "--object-instance", b"JJFFE object-instance-v1",
                            OBJECT_LINES, directory)
        animation_trace = Path(directory) / "animation.jsonl"
        object_trace = Path(directory) / "object-instance.jsonl"
        animation_trace.write_bytes(animation)
        object_trace.write_bytes(objects)
        env = dict(os.environ, FFE_ANIMATION_VECTORS=str(animation_trace),
                   FFE_OBJECT_INSTANCE_VECTORS=str(object_trace),
                   FFE_ANIMATION_CACHE=str(args.cache.resolve(strict=True)))
        for name, trace, output, tampers in [
            ("executed_reference_animation", animation_trace, animation, animation_tampers),
            ("executed_reference_object_instances", object_trace, objects, object_tampers),
        ]:
            command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "animation_reference",
                       name, "--", "--ignored", "--exact"]
            check(name, trace, output, env, command, root, tampers)
    print(json.dumps({
        "scenario": "jjffe-animation-v1",
        "matrix_cases": 2640,
        "orbital_cases": 4096,
        "object_frames": 96,
        "display_cases": 4096,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "animation_trace_sha256": hashlib.sha256(animation).hexdigest(),
        "object_instance_trace_sha256": hashlib.sha256(objects).hexdigest(),
        "model_execution_or_rotating_frames_validated": False,
    }, indent=2))


if __name__ == "__main__":
    main()
