#!/usr/bin/env bash
# Linux (32-bit ELF) counterpart of build-smoke.ps1. Builds the same local-only
# JJFFE 2.8a7 reference adapter from the unchanged supplied sources.
#
# Requirements: nasm, gcc with -m32 support (Debian/Ubuntu: gcc-multilib).
# Outputs go to .cache/jjffe-reference-linux/ (ignored). Supplied sources are
# never edited: the original Linux name converter (jjffe-master/linux/lnxconv.c)
# writes a generated copy, and `abs` is escaped in that copy because newer NASM
# treats it as a keyword. Traces are LF-terminated; the Windows build emits CRLF,
# so recorded Windows trace hashes are reproduced by hashing with CRLF endings.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
source="$root/jjffe-master"
output="$root/.cache/jjffe-reference-linux"
harness="$root/tools/reference/headless.c"
mkdir -p "$output"
cd "$output"

gcc -O2 -w -o lnxconv "$source/linux/lnxconv.c"
./lnxconv "$source/ffe.asm" ffelnx.asm >/dev/null
sed -i -E 's/^(\s*EXTERN\s+)abs\b/\1$abs/; s/(call\s+)abs\b/\1$abs/' ffelnx.asm
sed -E 's/\b_ref_/ref_/g' "$root/tools/reference/exports.inc" > exports-linux.inc

nasm -f elf32 -w-all -i"$source/" -p exports-linux.inc -o ffe.o ffelnx.asm
nasm -f elf32 -w-all -i"$source/" -o ffebmp.o "$source/ffebmp.asm"

cflags=(-m32 -O2 -w -D__FFELNXSDL__ -I"$source")
gcc "${cflags[@]}" -c "$source/ffecfg.c" -o ffecfg.o
gcc "${cflags[@]}" -c "$source/ffemisc.c" -o ffemisc.o
gcc "${cflags[@]}" -c "$harness" -o headless.o
gcc -m32 -no-pie -o jjffe-smoke headless.o ffecfg.o ffemisc.o ffe.o ffebmp.o -lm 2>/dev/null

# Instrumented copy: redirect exactly two call sites per primitive emitter.
python3 - "$output/ffelnx.asm" "$output/ffe-command-probe.asm" <<'PY'
import re, sys
text = open(sys.argv[1], encoding="latin-1").read()
hooks = [("FUNC_001708", "trace_triangle"), ("FUNC_001712", "trace_quad"),
         ("FUNC_001699", "trace_point_args"), ("FUNC_001703", "trace_line_args")]
for original, probe in hooks:
    pattern = re.compile(r"(?m)^(\s*call )" + original + r"(\s*)$")
    if len(pattern.findall(text)) != 2:
        sys.exit(f"Unexpected primitive hook sites: {original}")
    text = pattern.sub(lambda m: m.group(1) + probe + m.group(2), text)
header = "".join(f"EXTERN {probe}\n" for _, probe in hooks)
open(sys.argv[2], "w", encoding="latin-1").write(header + text)
PY
nasm -f elf32 -w-all -i"$source/" -p exports-linux.inc -o ffe-command-probe.o ffe-command-probe.asm
gcc "${cflags[@]}" -DFFE_COMMAND_PROBE -c "$harness" -o headless-probe.o
gcc -m32 -no-pie -o jjffe-commands headless-probe.o ffecfg.o ffemisc.o ffe-command-probe.o ffebmp.o -lm 2>/dev/null

echo "Built local-only reference: $output/jjffe-smoke and $output/jjffe-commands"
echo "The linked binaries contain legacy data and must not be distributed."
