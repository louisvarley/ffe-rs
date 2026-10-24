"""Compare ffe_core::market with the executed original.

--market (jjffe-smoke or jjffe-commands): F619 (price) and F620 (stock) with the game's own
tables and generator, on 8,192 random commodities, populations, dangers and flags. The Rust test
(`executed_reference_market`) must reproduce every result and the generator state after each call;
altered fields and an empty trace must be rejected. Needs data.bin (cache v20) for the tables.

The adapter's current sources need the Linux build (tools/reference/build-linux.sh); pass its
jjffe-smoke or jjffe-commands.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

LINES = 8192
BANNER = b"JJFFE market-v1"


def run_twice(binary, directory):
    outputs = []
    for _ in range(2):
        result = subprocess.run([str(binary), "--market"], cwd=directory, capture_output=True,
                                timeout=120)
        if result.returncode:
            raise RuntimeError(f"--market reference failed ({result.returncode}): "
                               + result.stderr.decode(errors="replace"))
        if BANNER not in result.stderr or len(result.stdout) > (32 << 20):
            raise RuntimeError("wrong or unbounded --market reference")
        outputs.append(result.stdout)
    if outputs[0] != outputs[1] or len(outputs[0].splitlines()) != LINES:
        raise RuntimeError("--market traces differ or have the wrong line count")
    return outputs[0]


def first(rows, predicate):
    for index, row in enumerate(rows):
        if predicate(row):
            return index
    raise RuntimeError("no row for a tamper case")


def flip(value):
    return value ^ 1


def tampers():
    def field(name, change):
        def apply(rows):
            i = first(rows, lambda r: r[name] != 0)
            rows[i][name] = change(rows[i][name])
        return apply

    def high_price(rows):
        i = first(rows, lambda r: r["price"] > 1000)
        rows[i]["price"] += 1

    return [
        ("price", high_price),
        ("price generator a", field("price_a", flip)),
        ("price generator b", field("price_b", flip)),
        ("stock", field("stock", flip)),
        ("stock generator a", field("stock_a", flip)),
        ("stock generator b", field("stock_b", flip)),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--reference", required=True, type=Path,
                        help="jjffe-smoke or jjffe-commands built by build-linux.sh")
    parser.add_argument("--cache", required=True, type=Path, help="data.bin (cache v20)")
    args = parser.parse_args()
    reference = args.reference.resolve(strict=True)
    root = Path(__file__).resolve().parents[2]
    cargo = shutil.which("cargo") or str(Path.home() / ".cargo" / "bin" / "cargo.exe")
    with tempfile.TemporaryDirectory(prefix="ffe-market-") as directory:
        output = run_twice(reference, directory)
        trace = Path(directory) / "market.jsonl"
        trace.write_bytes(output)
        env = dict(os.environ, FFE_MARKET_VECTORS=str(trace),
                   FFE_MARKET_CACHE=str(args.cache.resolve(strict=True)))
        command = [cargo, "test", "--locked", "-p", "ffe-rs", "--test", "market_reference",
                   "executed_reference_market", "--", "--ignored", "--exact"]
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
            raise RuntimeError("empty market trace accepted")
    print(json.dumps({
        "scenario": "jjffe-market-v1",
        "cases": LINES,
        "repeat_matches": True,
        "rust_results_match": True,
        "all_tamper_fields_and_empty_traces_rejected": True,
        "trace_sha256": hashlib.sha256(output).hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
