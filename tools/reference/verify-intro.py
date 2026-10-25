"""Compare the intro timeline with the executed original.

--intro (jjffe-smoke): the real F961/F962 at the 18 credit thresholds and at
0, 10.0 s, the model switch, the primary-bitmap boundary and the last tics
before the intro restarts. The model draw (DATA_007807) and the text draw
(DATA_007644) are captured; the real F1343 expands each string and it is printed
plain. The Rust test (`executed_reference_intro`) must reproduce the model,
animation, credit windows, positions, wrap widths and strings from
`ffe_rs::front_end::IntroTimeline`; altered fields and an empty trace must be
rejected.

The JJFFE legal splash is not part of this oracle (docs/notes/intro.md): it is
the mod's, and the DOS release is the source of truth.

The adapter's current sources need the local Windows or Linux build
(tools/reference/build-smoke.ps1 or build-linux.sh).
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

LINES = 76
BANNER = b"JJFFE intro-v1"


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--intro"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--intro reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError("wrong or unbounded --intro reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--intro traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def first_text(row):
    return row["texts"][0]


def tampers():
    def tics(rows):
        row = rows[first(rows, lambda r: r["texts"])]
        row["tics"] = 0x181534

    def text_field(name, change):
        def apply(rows):
            first_text(rows[first(rows, lambda r: r["texts"])])[name] = change(
                first_text(rows[first(rows, lambda r: r["texts"])])[name])
        return apply

    # The trace's plain `text` field is not compared: the port draws the DOS
    # bytes from the v27 cache, and `intro_reference` checks the codes, colour,
    # position and wrap only (see the note in that test).
    return [
        ("tics", tics),
        ("model", lambda r: r[first(r, lambda x: x["tics"] == 0)].update(model=176)),
        ("animation", lambda r: r[first(r, lambda x: x["tics"] == 0)].update(
            animation=r[first(r, lambda x: x["tics"] == 0)]["animation"] + 1)),
        ("version flag", lambda r: r[first(r, lambda x: x["version"] == 0)].update(version=1)),
        ("text x", text_field("x", lambda v: v + 1)),
        ("text y", text_field("y", lambda v: v + 1)),
        ("text wrap", text_field("wrap", lambda v: v + 1)),
        ("text code", text_field("code", lambda v: v + 1)),
        ("text colour", text_field("colour", lambda v: v + 1)),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path,
                        help="jjffe-smoke built by build-smoke.ps1 or build-linux.sh")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-intro-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "intro.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_INTRO_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "intro_reference",
                   "executed_reference_intro", "--", "--ignored", "--exact"]
        subprocess.run(command, cwd=root, env=env, check=True, timeout=600)
        rows = [json.loads(line) for line in output.splitlines()]
        for label, change in tampers():
            altered = json.loads(json.dumps(rows))
            change(altered)
            trace.write_bytes(b"".join(json.dumps(r).encode() + b"\n" for r in altered))
            if subprocess.run(command, cwd=root, env=env, capture_output=True,
                              timeout=600).returncode == 0:
                raise RuntimeError(f"altered {label} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True,
                          timeout=600).returncode == 0:
            raise RuntimeError("empty intro trace accepted")
        trace.write_bytes(output)
        subprocess.run(command, cwd=root, env=env, check=True, timeout=600)
    print(json.dumps({
        "scenario": "jjffe-intro-v1",
        "cases": LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
