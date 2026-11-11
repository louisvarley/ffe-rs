"""Extract a local FFE MODE1/2352 BIN into a NEW directory (never runs files).

Usage: python tools/extract-cd.py disc.bin new-install-directory
Only ISO9660 regular files/directories are supported; source media is read-only.
"""
import argparse
from pathlib import Path


def extract(source, destination):
    if destination.exists():
        raise ValueError("destination already exists; choose a new directory")
    with source.open("rb") as disc:
        disc_size = source.stat().st_size

        def read(extent, size):
            if size > 64 * 1024 * 1024 or (extent + (size + 2047) // 2048) * 2352 > disc_size:
                raise ValueError("invalid or oversized ISO extent")
            result = bytearray()
            while size:
                disc.seek(extent * 2352 + 16)
                count = min(size, 2048)
                chunk = disc.read(count)
                if len(chunk) != count:
                    raise ValueError("truncated disc image")
                result.extend(chunk)
                size -= count
                extent += 1
            return result

        pvd = read(16, 2048)
        if pvd[:7] != b"\x01CD001\x01" or int.from_bytes(pvd[128:130], "little") != 2048:
            raise ValueError("expected ISO9660 on MODE1/2352 sectors")
        destination.mkdir(parents=True)
        visited = set()
        count = 0

        def walk(record, directory, depth=0):
            nonlocal count
            extent = int.from_bytes(record[2:6], "little")
            size = int.from_bytes(record[10:14], "little")
            if depth > 8 or extent in visited:
                raise ValueError("cyclic or excessively deep ISO directory")
            visited.add(extent)
            data = read(extent, size)
            offset = 0
            names = set()
            while offset < len(data):
                length = data[offset]
                if not length:
                    offset = (offset // 2048 + 1) * 2048
                    continue
                entry = data[offset:offset + length]
                offset += length
                if len(entry) < 34 or len(entry) != length:
                    raise ValueError("invalid ISO directory record")
                raw_name = entry[33:33 + entry[32]]
                if raw_name in (b"\0", b"\1"):
                    continue
                name = raw_name.decode("ascii").split(";")[0]
                if not name or name in (".", "..") or any(c not in "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-" for c in name):
                    raise ValueError("unsafe ISO filename")
                if name.casefold() in names or entry[25] & 0x80:
                    raise ValueError("duplicate filename or unsupported multi-extent file")
                names.add(name.casefold())
                path = directory / name
                if entry[25] & 2:
                    path.mkdir()
                    walk(entry, path, depth + 1)
                else:
                    count += 1
                    if count > 4096:
                        raise ValueError("too many ISO files")
                    payload = read(int.from_bytes(entry[2:6], "little"), int.from_bytes(entry[10:14], "little"))
                    with path.open("xb") as output:
                        output.write(payload)

        walk(pvd[156:190], destination)
        print(f"Extracted {count} files to {destination}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    extract(args.source, args.destination)
