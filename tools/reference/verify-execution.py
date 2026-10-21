"""Compare composed native command execution with the original dispatcher and handlers."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path)
    parser.add_argument("--data", required=True, type=Path)
    args = parser.parse_args()
    reference, data = args.reference.resolve(strict=True), args.data.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-execution-") as directory:
        outputs = []
        for _ in range(2):
            result = subprocess.run([str(reference), "--execution"], cwd=directory,
                                    capture_output=True, check=True, timeout=30)
            if b"JJFFE execution-v1" not in result.stderr or len(result.stdout) > 512*1024:
                raise RuntimeError("wrong/unbounded execution reference")
            outputs.append(result.stdout)
        if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != 900:
            raise RuntimeError("execution results differ or wrong case count")
        trace = Path(directory) / "execution.jsonl"
        trace.write_bytes(outputs[0])
        env = dict(os.environ, FFE_EXECUTION_CACHE=str(data), FFE_EXECUTION_VECTORS=str(trace))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "execution_reference", "--", "--ignored"]
        discovered = subprocess.run(command+["--list"], cwd=root, env=env,
                                    capture_output=True, check=True, timeout=120)
        names = [line for line in discovered.stdout.decode().splitlines() if line.endswith(": test")]
        expected = {
            "executed_reference_composed_command_programs: test",
            "executed_reference_station_depth_composition_advances: test",
        }
        if set(names) != expected:
            raise RuntimeError(f"execution comparisons not discovered: {names}")
        subprocess.run(command, cwd=root, env=env, check=True, timeout=120)
        rows = outputs[0].splitlines()
        for field in ["faces", "emissions", "locals", "matrix", "alive"]:
            row = json.loads(rows[0])
            row[field] = "0"*16 if isinstance(row[field], str) else row[field] ^ 1
            trace.write_bytes(json.dumps(row).encode()+b"\n"+b"\n".join(rows[1:])+b"\n")
            if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
                raise RuntimeError(f"altered execution {field} accepted")
        trace.write_bytes(b"")
        if subprocess.run(command, cwd=root, env=env, capture_output=True, timeout=120).returncode == 0:
            raise RuntimeError("empty execution reference accepted")
    print(json.dumps({"scenario":"jjffe-execution-v1","composed_programs":3,"contexts":900,
                      "repeat_matches":True,"rust_operand_emission_order_and_states_match":True,
                      "all_five_tamper_fields_and_empty_trace_rejected":True,
                      "real_station_advances_past_opcode_6":True,
                      "trace_sha256":hashlib.sha256(outputs[0]).hexdigest(),
                      "full_station_execution_geometry_or_pixels_validated":False},indent=2))


if __name__ == "__main__":
    main()
