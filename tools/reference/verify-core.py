"""Compare executed JJFFE numeric/state traces with native Rust, keeping no traces."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile


def run(arguments: list[str], directory: str) -> bytes:
    result = subprocess.run(arguments, cwd=directory, capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError(
            f"command failed ({result.returncode}): {result.stderr.decode(errors='replace')}"
        )
    return result.stdout


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--tool", type=Path, required=True)
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    tool = args.tool.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="ffe-core-reference-") as directory:
        traces = {}
        for name, arguments, expected_rows in [
            ("vectors", ["--vectors"], 1596),
            ("state", ["--state", "400"], 400),
        ]:
            # Separate working directories prevent one run from influencing the next.
            runs = []
            for index in range(2):
                working = Path(directory) / f"{name}-{index}"
                working.mkdir()
                runs.append(run([str(reference), *arguments], str(working)))
            if runs[0] != runs[1]:
                raise RuntimeError(f"{name}: repeated reference traces differ")
            if len(runs[0].splitlines()) != expected_rows:
                raise RuntimeError(f"{name}: unexpected number of rows")
            path = Path(directory) / f"{name}.jsonl"
            path.write_bytes(runs[0])
            traces[name] = path
        arguments = [
            str(tool), "verify-reference", "--vectors", str(traces["vectors"]),
            "--state", str(traces["state"]), "--json",
        ]
        report = json.loads(run(arguments, directory))
        for name in ["vectors", "state"]:
            if report[f"{name}_sha256"] != hashlib.sha256(traces[name].read_bytes()).hexdigest():
                raise RuntimeError(f"{name}: Rust report hash differs")
        report["repeat_traces_match"] = True
        # F152's whole-second marker (DATA_008808) on every in-game clock frame.
        if report["second_event_transitions"] != report["game_clock_transitions"]:
            raise RuntimeError("state: F152 marker missing from the reference trace")
        # A marker moved by one event must fail.
        state_bytes = traces["state"].read_bytes()
        rows = state_bytes.splitlines()
        tampered = json.loads(rows[300])
        tampered["second_marker"] = (tampered["second_marker"] + 0xC22E) & 0xFFFFFFFF
        rows[300] = json.dumps(tampered, separators=(",", ":")).encode()
        traces["state"].write_bytes(b"\n".join(rows) + b"\n")
        rejected = subprocess.run(arguments, cwd=directory, capture_output=True, timeout=30)
        if rejected.returncode != 2 or b"F152" not in rejected.stderr or rejected.stdout:
            raise RuntimeError("incorrect F152 marker was not explicitly rejected")
        report["modified_second_marker_rejected"] = True
        traces["state"].write_bytes(state_bytes)
        # A changed return value must fail, even with valid JSON and all rows present.
        lines = traces["vectors"].read_bytes().splitlines()
        altered = json.loads(lines[0])
        altered["value"] ^= 1
        lines[0] = json.dumps(altered).encode()
        traces["vectors"].write_bytes(b"\n".join(lines) + b"\n")
        rejected = subprocess.run(arguments, cwd=directory, capture_output=True, timeout=30)
        if rejected.returncode != 2 or b"RNG mismatch" not in rejected.stderr or rejected.stdout:
            raise RuntimeError("incorrect reference vector was not explicitly rejected")
        report["modified_vector_rejected"] = True
        print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
