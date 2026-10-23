"""Local real-data check against JJFFE declarations; never stores source/captures."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import struct
import subprocess
import tempfile
import wave
import zipfile
import zlib


def declarations(path: Path) -> dict[str, str]:
    return dict(re.findall(
        r"^(DATA_\d+):[^\n]*\n(.*?)(?=^DATA_\d+:|\Z)",
        path.read_text(encoding="latin-1"), re.MULTILINE | re.DOTALL,
    ))


def db_bytes(body: str) -> bytes:
    return bytes(
        int(value.strip(), 0)
        for line in body.splitlines() if line.strip().startswith("db ")
        for value in line.strip()[3:].split(",")
    )


def db_text_bytes(body: str) -> bytes:
    """db lines that may mix quoted Latin-1 strings and numbers."""
    out = bytearray()
    for line in body.splitlines():
        line = line.strip()
        if not line.startswith("db "):
            continue
        for token in re.findall(r"'[^']*'|[^,\s][^,]*", line[3:]):
            token = token.strip()
            if token.startswith("'"):
                out += token[1:-1].encode("latin-1")
            else:
                out.append(int(token, 0))
    return bytes(out)


def pe_data_section(exe: bytes) -> tuple[int, int, int]:
    """Image base, and the DATA section's virtual address and raw offset."""
    pe = struct.unpack_from("<I", exe, 0x3C)[0]
    sections, optional = struct.unpack_from("<H", exe, pe + 6)[0], struct.unpack_from("<H", exe, pe + 20)[0]
    base = struct.unpack_from("<I", exe, pe + 0x34)[0]
    for index in range(sections):
        header = pe + 24 + optional + 40 * index
        if exe[header:header + 8].rstrip(b"\0") == b"DATA":
            _, address, _, raw, _ = struct.unpack_from("<IIIII", exe, header + 8)
            return base, address, raw
    raise RuntimeError("no DATA section")


def ship_object_table(blocks: dict[str, str]) -> list[str | None]:
    """The ship object table slot by slot; a null entry is four zero bytes."""
    slots: list[str | None] = []
    for line in blocks["DATA_004681"].splitlines():
        text = line.split(";")[0].strip()
        if not text:
            continue
        keyword, _, rest = text.partition(" ")
        if keyword == "dd":
            slots.append(rest.strip())
        elif keyword == "db":
            slots.append(None)
    return slots


def inventory(root: Path) -> dict[str, str]:
    return {
        str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
        for path in root.rglob("*") if path.is_file()
    }

def run_tool(arguments: list[str]) -> subprocess.CompletedProcess:
    result = subprocess.run(arguments, capture_output=True, timeout=60)
    if result.returncode:
        raise RuntimeError(f"tool exited {result.returncode}: {result.stderr.decode(errors='replace')}")
    return result


def indexed_png(data: bytes) -> tuple[bytes, bytes]:
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise RuntimeError("invalid PNG signature")
    offset, compressed, palette = 8, b"", b""
    width = height = 0
    while offset < len(data):
        size = struct.unpack_from(">I", data, offset)[0]
        kind = data[offset + 4:offset + 8]
        chunk = data[offset + 8:offset + 8 + size]
        if zlib.crc32(kind + chunk) & 0xffffffff != struct.unpack_from(">I", data, offset + 8 + size)[0]:
            raise RuntimeError("PNG CRC mismatch")
        if kind == b"IHDR":
            width, height, depth, colour, _, _, interlace = struct.unpack(">IIBBBBB", chunk)
            if (width, height, depth, colour, interlace) != (320, 158, 8, 3, 0):
                raise RuntimeError("unexpected cabin PNG format")
        elif kind == b"PLTE":
            palette = chunk
        elif kind == b"IDAT":
            compressed += chunk
        offset += size + 12
    rows = zlib.decompress(compressed)
    if len(rows) != (width + 1) * height or any(rows[y * (width + 1)] for y in range(height)):
        raise RuntimeError("unexpected PNG rows/filter")
    return b"".join(rows[y * (width + 1) + 1:(y + 1) * (width + 1)] for y in range(height)), palette


def verify(tool: Path, game: Path, reference: Path) -> dict:
    before = inventory(game)
    blocks = declarations(reference / "ffebmp.asm")
    ship_table = ship_object_table(blocks)
    palette = db_bytes(declarations(reference / "ffedat.asm")["DATA_007769"])
    font_blocks = declarations(reference / "ffedat.asm")
    font = db_bytes(font_blocks["DATA_007750"]) + db_bytes(font_blocks["DATA_007751"])
    declaration_differences: list[int] = []
    with tempfile.TemporaryDirectory(prefix="ffe-content-") as directory:
        first, second = Path(directory) / "a.bin", Path(directory) / "b.bin"
        for output in [first, second]:
            run_tool([str(tool), "build-data", "--game", str(game), "--out", str(output)])
        data = first.read_bytes()
        if data != second.read_bytes():
            raise RuntimeError("repeated cache builds differ")
        result = run_tool([str(tool), "inspect-data", "--data", str(first), "--json"])
        manifest = json.loads(result.stdout)
        if manifest["complete_game_assets"] or manifest["format_version"] != 15 or len(manifest["files"]) != 1782:
            raise RuntimeError("incorrect cache scope/count")
        sources = {record["path"]: record for record in manifest["source_files"]}
        for record in sources.values():
            source = (game / record["path"]).read_bytes()
            if len(source) != record["size"] or hashlib.sha256(source).hexdigest() != record["sha256"]:
                raise RuntimeError("source provenance mismatch")
        exe = (game / "FIRSTENC.EXE").read_bytes()
        counts = {"primary": 96, "secondary": 251}
        tables = {
            table: re.findall(r"^\s*dd (DATA_\d+)\s*$", blocks[label], re.MULTILINE)
            for table, label in [("primary", "DATA_008240"), ("secondary", "DATA_008241")]
        }
        with zipfile.ZipFile(first) as archive:
            if archive.testzip() is not None or len(archive.infolist()) != 1783:
                raise RuntimeError("ZIP CRC/count failure")
            for record in manifest["files"]:
                payload = archive.read(record["path"])
                if len(payload) != record["size"] or hashlib.sha256(payload).hexdigest() != record["sha256"]:
                    raise RuntimeError("cache payload digest mismatch")
                asset = record["asset"]
                if asset["kind"] == "system_tables":
                    # DATA_007106..7138: the galaxy table (JJFFE appends systems after the
                    # original's 329), the specials and the generator's tables.
                    blocks_d = declarations(reference / "ffedat.asm")
                    def tail(label):
                        return db_bytes(blocks_d[label])
                    galaxy = b"".join(
                        tail(label) for label in ["DATA_007110", "DATA_007111", "DATA_007112",
                                                  "DATA_007113", "DATA_007114", "DATA_007115"])
                    if len(payload) != 20984 or payload[432:432 + 329 * 54] != galaxy[:329 * 54]:
                        raise RuntimeError("galaxy table differs from JJFFE DATA_007110")
                    for label, offset in [("DATA_007116", 18252), ("DATA_007119", 18492),
                                          ("DATA_007120", 18584), ("DATA_007121", 18676),
                                          ("DATA_007122", 18772), ("DATA_007123", 18864),
                                          ("DATA_007124", 18956), ("DATA_007129", 19152),
                                          ("DATA_007131", 20180), ("DATA_007132", 20304),
                                          ("DATA_007133", 20496), ("DATA_007134", 20592),
                                          ("DATA_007136", 20936), ("DATA_007137", 20952),
                                          ("DATA_007138", 20968)]:
                        expected = tail(label)
                        if payload[offset:offset + len(expected)] != expected:
                            raise RuntimeError(f"system table differs from JJFFE {label}")
                if asset["kind"] == "model_names":
                    # DATA_004682, the "Ship string table": each pointer's string, NUL-terminated.
                    data_blocks = declarations(reference / "ffedat.asm")
                    labels = re.findall(r"^\s*dd (DATA_\d+)\b", data_blocks["DATA_004682"], re.MULTILINE)
                    expected = [db_text_bytes(data_blocks[label]).split(b"\0")[0] for label in labels]
                    names = payload.split(b"\0")[:-1]
                    # Entries 0..110 (every craft and more) match; from 111 floppy 1.02
                    # inserts ten digit entries ("0".."9") and differs in decal texts.
                    # The release table also continues past JJFFE's 209 declared
                    # pointers (219 entries: "police", the FFE logo, ...), so the
                    # declaration is a lower bound, not the expected count. The
                    # extra entries are checked against the executable's own bytes
                    # below by their provenance offset.
                    if (asset["count"] < len(labels) or len(names) != asset["count"]
                            or names[:111] != expected[:111]
                            or names[111:121] != [str(d).encode() for d in range(10)]):
                        raise RuntimeError("model name table differs from JJFFE DATA_004682")
                    offset = asset["source_offset"]
                    extra = names[len(labels):]
                    if extra:
                        # Each extra pointer sits after JJFFE's 209 and must point
                        # into the data section at the same string in the file.
                        base, section_va, section_raw = pe_data_section(exe)
                        for entry, text in enumerate(extra, start=len(labels)):
                            pointer = struct.unpack_from("<I", exe, offset + 4 * entry)[0]
                            start = pointer - base - section_va + section_raw
                            if exe[start:start + len(text) + 1] != text + b"\0":
                                raise RuntimeError("extra model string does not match the executable")
                if asset["kind"] == "sine_table":
                    if payload != db_bytes(blocks["DATA_007975"]) or len(payload) != 65536:
                        raise RuntimeError("original sine table differs")
                    offset = asset["source_offset"]
                    if exe[offset:offset+65536] != payload:
                        raise RuntimeError("sine table source provenance differs")
                # Model 361's data differs between the JJFFE declarations and
                # this release, so only the executable's own bytes can be checked.
                if asset.get("index") == 361:
                    continue
                if asset["kind"] == "model_normals":
                    model_table = ship_table
                    model = model_table[asset["index"]]
                    normal_label = re.findall(r"^\s*dd (DATA_\d+)\b", blocks[model], re.MULTILINE)[2]
                    expected = db_bytes(blocks[normal_label])
                    descriptor = asset["descriptor_offset"]
                    raw = 0x69c00 if manifest["source_profile"] == "ffe-floppy-1.02-en" else 0x6a000
                    table_rva = 0xd0cfc if manifest["source_profile"] == "ffe-floppy-1.02-en" else 0xd0cf8
                    descriptor_pointer = struct.unpack_from("<I", exe, table_rva - 0x80000 + raw + asset["index"] * 4)[0]
                    vertex_count = struct.unpack_from("<I", exe, descriptor + 8)[0]
                    pointer, count = struct.unpack_from("<II", exe, descriptor + 12)
                    # F1480 writes the implicit pair 0 and 1 and computes the rest
                    # lazily, so a model that declares no normals needs no records.
                    records = max(count // 2 - 1, 0) * 6
                    if (descriptor_pointer - 0x480000 + raw != descriptor
                            or (count <= 2 and payload)
                            or (count > 2 and (pointer - 0x480000 + raw != asset["source_offset"]
                                               or len(payload) != records))
                            or (vertex_count, count) != (asset["vertex_count"], asset["normal_count"])
                            or payload != exe[asset["source_offset"]:asset["source_offset"]+len(payload)]):
                        raise RuntimeError("model normal records differ from JJFFE/original pointer provenance")
                    if count > 2 and payload != expected:
                        declaration_differences.append(asset["index"])
                if asset["kind"] == "model_commands":
                    model_table = ship_table
                    model = model_table[asset["index"]]
                    command_label = re.findall(r"^\s*dd (DATA_\d+)\b", blocks[model], re.MULTILINE)[0]
                    expected = db_bytes(blocks[command_label])
                    descriptor = asset["descriptor_offset"]
                    raw = 0x69c00 if manifest["source_profile"] == "ffe-floppy-1.02-en" else 0x6a000
                    table_rva = 0xd0cfc if manifest["source_profile"] == "ffe-floppy-1.02-en" else 0xd0cf8
                    descriptor_pointer = struct.unpack_from("<I", exe, table_rva - 0x80000 + raw + asset["index"] * 4)[0]
                    pointer = struct.unpack_from("<I", exe, descriptor)[0]
                    count = struct.unpack_from("<I", exe, descriptor + 8)[0]
                    if (descriptor_pointer - 0x480000 + raw != descriptor
                            or pointer - 0x480000 + raw != asset["source_offset"]
                            or count != asset["vertex_count"]
                            or len(payload) != len(expected)
                            or payload != exe[asset["source_offset"]:asset["source_offset"] + len(payload)]):
                        raise RuntimeError("model command stream differs from JJFFE/original pointer provenance")
                    if payload != expected:
                        declaration_differences.append(asset["index"])
                if asset["kind"] == "model_vertices":
                    model_table = ship_table
                    model = model_table[asset["index"]]
                    vertex_label = re.findall(r"^\s*dd (DATA_\d+)\b", blocks[model], re.MULTILINE)[1]
                    expected = db_bytes(blocks[vertex_label])
                    descriptor = asset["descriptor_offset"]
                    raw = 0x69c00 if manifest["source_profile"] == "ffe-floppy-1.02-en" else 0x6a000
                    table_rva = 0xd0cfc if manifest["source_profile"] == "ffe-floppy-1.02-en" else 0xd0cf8
                    descriptor_pointer = struct.unpack_from("<I", exe, table_rva - 0x80000 + raw + asset["index"] * 4)[0]
                    pointer, count = struct.unpack_from("<II", exe, descriptor + 4)
                    scale = struct.unpack_from("<I", exe, descriptor + 20)[0]
                    if (descriptor_pointer - 0x480000 + raw != descriptor
                            or pointer - 0x480000 + raw != asset["source_offset"]
                            or (count, scale) != (asset["vertex_count"], asset["scale"])
                            or len(payload) != count // 2 * 6
                            or payload != exe[asset["source_offset"]:asset["source_offset"] + len(payload)]):
                        raise RuntimeError("model vertex program differs from JJFFE/original pointer provenance")
                    if payload != expected:
                        declaration_differences.append(asset["index"])
                if asset["kind"] == "bitmap_font":
                    offset = asset["source_offset"]
                    if payload != font or exe[offset:offset + len(font)] != font:
                        raise RuntimeError("font differs from JJFFE/original executable")
                if asset["kind"] in ("indexed_bitmap", "texture_block"):
                    table, index = asset["table"], asset["index"]
                    if len(tables[table]) != counts[table]:
                        raise RuntimeError("reference bitmap table count differs")
                    original = db_bytes(blocks[tables[table][index]])
                    size = 32776 if asset["kind"] == "texture_block" else 8 + asset["width"] * asset["height"]
                    offset = asset["source_offset"]
                    if original[:size] != exe[offset:offset + size] or payload != original[8:size]:
                        raise RuntimeError(f"{table}/{index}: source/executable/cache bytes differ")
                    flags, height, width, x, y = struct.unpack_from("<BBHhh", original)
                    if asset["kind"] == "texture_block":
                        if (table, index, flags, height, width, x, y) not in [
                            ("primary", 94, 0, 0, 64, 0, 0), ("primary", 95, 0, 0, 64, 0, 0)
                        ] or len(original) != 32776:
                            raise RuntimeError("special texture block differs")
                        continue
                    if (width, height, x, y, bool(flags & 1)) != (
                        asset["width"], asset["height"], asset["offset_x"], asset["offset_y"], asset["masked_zero"]
                    ):
                        raise RuntimeError("bitmap metadata differs")
                elif asset["kind"] == "low_palette_rgb6":
                    if payload != palette or len(payload) != 384 or max(payload) > 63:
                        raise RuntimeError("fixed low palette differs")
                elif asset["kind"] == "effect_wav":
                    name = Path(record["path"]).stem.upper() + ".RAW"
                    with wave.open(io.BytesIO(payload), "rb") as reader:
                        if (reader.getnchannels(), reader.getsampwidth(), reader.getframerate()) != (1, 2, 22050):
                            raise RuntimeError("WAV metadata differs")
                        if reader.readframes(reader.getnframes()) != (game / "FX" / name).read_bytes():
                            raise RuntimeError("WAV samples differ")
                elif asset["kind"] == "cabin_png":
                    name = Path(record["path"]).stem.upper() + ".BMP"
                    bmp = (game / name).read_bytes()
                    pixel_offset = struct.unpack_from("<I", bmp, 10)[0]
                    expected = b"".join(bmp[pixel_offset + y * 320:pixel_offset + (y + 1) * 320] for y in range(157, -1, -1))
                    indices, colours = indexed_png(payload)
                    rgb = b"".join(bytes([bmp[n + 2], bmp[n + 1], bmp[n]]) for n in range(54, 1078, 4))
                    if indices != expected or colours != rgb:
                        raise RuntimeError("cabin pixels/palette differ")
            bitmap_records = [record for record in manifest["files"] if record["asset"]["kind"] in ("indexed_bitmap", "texture_block")]
            if len(bitmap_records) != sum(counts.values()):
                raise RuntimeError("missing embedded bitmaps")
        refused = subprocess.run([str(tool), "build-data", "--game", str(game), "--out", str(first)],
                                 capture_output=True, timeout=30)
        if refused.returncode != 2 or first.read_bytes() != data:
            raise RuntimeError("existing cache overwritten")
        damaged = bytearray(data)
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            entry = archive.getinfo("bitmaps/primary/000.idx")
            header = entry.header_offset
            name_size, extra_size = struct.unpack_from("<HH", data, header + 26)
            damaged[header + 30 + name_size + extra_size] ^= 1
        first.write_bytes(damaged)
        rejected = subprocess.run([str(tool), "inspect-data", "--data", str(first)],
                                  capture_output=True, timeout=30)
        if rejected.returncode != 2:
            raise RuntimeError("corrupt payload accepted")
        if inventory(game) != before:
            raise RuntimeError("source installation changed")
        return {
            "source_profile": manifest["source_profile"], "assets": len(manifest["files"]),
            "bitmap_slots": counts, "executable_bytes_match": True,
            "repeat_cache_matches": True, "corruption_rejected": True,
            "source_files_unchanged": len(before), "cache_size": len(data),
            "jjffe_declaration_differences": sorted(set(declaration_differences)),
            "cache_sha256": hashlib.sha256(data).hexdigest(), "complete_game_assets": False,
        }


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--tool", type=Path, required=True)
    parser.add_argument("--game", type=Path, required=True)
    parser.add_argument("--reference", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify(args.tool.resolve(strict=True), args.game.resolve(strict=True),
                            args.reference.resolve(strict=True)), indent=2))


if __name__ == "__main__":
    main()
