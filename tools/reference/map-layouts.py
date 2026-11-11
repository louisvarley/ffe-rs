"""Find floppy 1.02 data locations in another FIRSTENC executable.

How the CD 1.01 and Elite Club shareware 1.06 layouts in
`crates/ffe-tools/src/cache.rs` were found. For each floppy RVA:

* `find`: take a window of the table's bytes and look for it in the other
  executable, with dwords that point into the image as wildcards (pointer
  values differ between builds). The match must be unique.
* `find_pointers`: for a pure pointer table, match the pattern of
  differences between consecutive pointers; it reports the shift of the
  pointed-to data.

A unique match is a candidate only. The importer's decoders then check each
table (counts, terminators, pointer targets), and the CD cache was compared
asset by asset with the floppy one. Zero-filled regions can match falsely
(the script statics were found from the scripts' own pointers instead).

Run (RVAs on stdin):

    echo 0xe71d8 0xe094c | python tools/reference/map-layouts.py         game/FIRSTENC.EXE cd <CD>/FIRSTENC.EXE sw <shareware>/FIRSTENC.DAT
"""
import re
import struct
import sys
from pathlib import Path


class PE:
    def __init__(self, path):
        self.data = Path(path).read_bytes()
        d = self.data
        pe = struct.unpack_from("<I", d, 0x3c)[0]
        assert d[pe:pe + 4] == b"PE\0\0"
        sections = struct.unpack_from("<H", d, pe + 6)[0]
        opt = struct.unpack_from("<H", d, pe + 20)[0]
        self.base = struct.unpack_from("<I", d, pe + 24 + 28)[0]
        self.size = struct.unpack_from("<I", d, pe + 24 + 56)[0]
        self.sections = []
        at = pe + 24 + opt
        for i in range(sections):
            name = d[at:at + 8].rstrip(b"\0").decode()
            vsize, va, rsize, raw = struct.unpack_from("<IIII", d, at + 8)
            self.sections.append((name, va, vsize, raw, rsize))
            at += 40

    def off(self, rva):
        for name, va, vsize, raw, rsize in self.sections:
            if va <= rva < va + max(vsize, rsize) and rva - va < rsize:
                return raw + rva - va
        return None

    def rva(self, off):
        for name, va, vsize, raw, rsize in self.sections:
            if raw <= off < raw + rsize:
                return va + off - raw
        return None

    def is_pointer(self, value):
        return self.base <= value < self.base + self.size or 0x1000 <= value < self.size


def find(src, dst, rva, length=96):
    o = src.off(rva)
    if o is None:
        return "no file bytes (bss?)", None
    window = src.data[o:o + length]
    words = len(window) // 4
    mask = []
    for i in range(words):
        v = struct.unpack_from("<I", window, i * 4)[0]
        mask.append(src.is_pointer(v) and v > 0x10000)
    # regex: exact bytes except pointer dwords
    pattern = b""
    for i in range(words):
        chunk = window[i * 4:i * 4 + 4]
        pattern += b"...." if mask[i] else re.escape(chunk)
    exact = sum(1 for m in mask if not m) * 4
    if exact < 16:
        return f"only {exact} literal bytes", None
    hits = [m.start() for m in re.finditer(pattern, dst.data, re.DOTALL)]
    if len(hits) != 1:
        return f"{len(hits)} matches ({exact} literal bytes)", None
    cand = dst.rva(hits[0])
    deltas = set()
    for i in range(words):
        if mask[i]:
            a = struct.unpack_from("<I", window, i * 4)[0]
            b = struct.unpack_from("<I", dst.data, hits[0] + i * 4)[0]
            deltas.add(b - a)
    return f"ok ({exact} literal bytes, pointer deltas {sorted(hex(x) for x in deltas)})", cand


def find_pointers(src, dst, rva, count=24):
    """A pointer array: match the differences between consecutive pointers."""
    o = src.off(rva)
    vals = [struct.unpack_from("<I", src.data, o + 4 * i)[0] for i in range(count)]
    diffs = [b - a for a, b in zip(vals, vals[1:])]
    d = dst.data
    hits = []
    first = vals[0]
    for pos in range(0, len(d) - 4 * count, 4):
        v0 = struct.unpack_from("<I", d, pos)[0]
        if not dst.is_pointer(v0):
            continue
        ok = True
        prev = v0
        for i, diff in enumerate(diffs):
            v = struct.unpack_from("<I", d, pos + 4 * (i + 1))[0]
            if v - prev != diff:
                ok = False
                break
            prev = v
        if ok:
            hits.append((pos, v0 - first))
    if len(hits) != 1:
        return f"{len(hits)} pointer-pattern matches", None
    pos, delta = hits[0]
    return f"ok (pointer pattern, target delta {delta:#x})", dst.rva(pos)


if __name__ == "__main__":
    floppy = PE(sys.argv[1])
    others = {name: PE(path) for name, path in zip(sys.argv[2::2], sys.argv[3::2])}
    rvas = [int(x, 16) for x in sys.stdin.read().split()]
    for rva in rvas:
        line = [f"{rva:#08x}"]
        for name, pe in others.items():
            note, cand = find(floppy, pe, rva)
            if cand is None:
                note, cand = find_pointers(floppy, pe, rva)
            line.append(f"{name}: {cand:#08x} {note}" if cand is not None else f"{name}: - {note}")
        print("  ".join(line))
