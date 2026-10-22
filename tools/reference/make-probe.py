"""Write the instrumented command-probe copy of ffe.asm. Never edits the input.

Usage: make-probe.py <ffe.asm or converted copy> <output.asm> [--prefix _]

Whole-file hooks redirect exactly two call sites per primitive emitter. Scoped
hooks redirect calls only inside one original routine range, so the probe can
capture the text (F1747) and submodel (F1749/F1750) handlers' decoded arguments
while those handlers still compute their own record advance.
"""
import re
import sys

GLOBAL_HOOKS = [
    ("FUNC_001708", "trace_triangle", 2),
    ("FUNC_001712", "trace_quad", 2),
    ("FUNC_001699", "trace_point_args", 2),
    ("FUNC_001703", "trace_line_args", 2),
]

# (first routine label, label that ends the range, [(call target, probe, sites)])
SCOPED_HOOKS = [
    ("FUNC_001747", "FUNC_001748", [
        ("near [DATA_007729]", "trace_text_vertex", 3),   # F1470 view vertex
        ("near [DATA_007757]", "trace_text_font", 1),     # F1539 glyph model set
        ("FUNC_001740", "trace_text_frame", 1),           # child basis set-up
        ("FUNC_001745", "trace_text_push", 1),            # nested buffer frame push
        ("near [DATA_007639]", "trace_text_string", 1),   # F1344 string expansion
        ("FUNC_001746", "trace_text_pop", 1),             # nested buffer frame pop
    ]),
    ("FUNC_001696", "FUNC_001697", [
        ("FUNC_001780", "trace_group", 8),                # depth-group open/close
    ]),
    ("FUNC_001697", "FUNC_001698", [
        ("FUNC_001781", "trace_nested_push", 1),          # nested block start
        ("FUNC_001782", "trace_nested_pop", 1),           # nested block end
    ]),
    ("FUNC_001708", "FUNC_001715", [
        ("FUNC_001778", "trace_alloc", 5),                # face queue allocations
        ("FUNC_001691", "trace_face_colour", 2),          # flat face colour (F1708/F1712)
    ]),
    ("FUNC_001721", "FUNC_001722", [
        ("FUNC_001777", "trace_polygon_key", 1),          # polygon depth key
        ("FUNC_001778", "trace_polygon_alloc", 1),        # polygon queue allocation
        ("near [DATA_007629]", "trace_polygon_colour", 1),  # F1326 after the outline is queued
        ("FUNC_001779", "trace_polygon_abandon", 1),      # flag abandonment (F1813 entry)
    ]),
    ("FUNC_001759", "FUNC_001760", [
        ("FUNC_001908_SoundPlaySampleLinVol", "trace_sound", 1),  # opcode 15 sample
    ]),
    ("FUNC_001764", "FUNC_001765", [
        ("FUNC_001777", "trace_label_key", 1),            # label depth key
        ("FUNC_001778", "trace_label_alloc", 1),          # label queue allocation
    ]),
    ("FUNC_001737", "FUNC_001738", [
        ("FUNC_001777", "trace_spline_key", 1),           # curve depth key
        ("FUNC_001778", "trace_spline_alloc", 1),         # curve queue allocation
        ("near [DATA_007629]", "trace_spline_colour", 1),  # F1326 after the entry is filled
    ]),
    ("FUNC_001680", "FUNC_001681", [
        ("FUNC_001773", "trace_object_run", 1),           # top-level model run (captured)
        ("FUNC_001700", "trace_object_dot", 1),           # distant dot
    ]),
    ("FUNC_001683", "FUNC_001684", [
        ("FUNC_001773", "trace_display_run", 1),          # display object model run
    ]),
    ("FUNC_001749", "FUNC_001751", [
        ("near [DATA_007727]", "trace_submodel_vertex", 5),  # F1471 projected vertex
        ("near [DATA_007756]", "trace_submodel_model", 1),   # F1538 model descriptor
        ("FUNC_001687", "trace_submodel_cull", 1),           # child screen cull
        ("FUNC_001740", "trace_submodel_frame", 1),          # child basis set-up
        ("FUNC_001773", "trace_submodel_run", 1),            # nested execution
    ]),
]


def redirect(text, target, probe, sites):
    pattern = re.compile(r"(?m)^(\s*call )" + re.escape(target) + r"([ \t]*(;[^\r\n]*)?\r?)$")
    if len(pattern.findall(text)) != sites:
        sys.exit(f"Unexpected hook sites for {target} -> {probe}")
    return pattern.sub(lambda m: m.group(1) + probe + m.group(2), text)


def main():
    if len(sys.argv) not in (3, 5) or (len(sys.argv) == 5 and sys.argv[3] != "--prefix"):
        sys.exit(__doc__)
    prefix = sys.argv[4] if len(sys.argv) == 5 else ""
    with open(sys.argv[1], encoding="latin-1", newline="") as source:
        text = source.read()
    probes = []
    for original, probe, sites in GLOBAL_HOOKS:
        text = redirect(text, original, prefix + probe, sites)
        probes.append(prefix + probe)
    for start, end, hooks in SCOPED_HOOKS:
        begin = re.search(r"(?m)^" + start + r":", text)
        finish = re.search(r"(?m)^" + end + r":", text)
        if not begin or not finish or finish.start() <= begin.start():
            sys.exit(f"Missing routine range {start}..{end}")
        body = text[begin.start():finish.start()]
        for target, probe, sites in hooks:
            body = redirect(body, target, prefix + probe, sites)
            probes.append(prefix + probe)
        text = text[:begin.start()] + body + text[finish.start():]
    newline = "\r\n" if "\r\n" in text else "\n"
    header = "".join(f"EXTERN {probe}{newline}" for probe in probes)
    with open(sys.argv[2], "w", encoding="latin-1", newline="") as output:
        output.write(header + text)


if __name__ == "__main__":
    main()
