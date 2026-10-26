# Hope launch ground — 26 October 2026

The missing planet in the owner's screenshot was reproduced without station
geometry: the lower terrain band contained only 88 non-space pixels (HUD and
stars). The original starting place remains Gateway / Hope / Old Blackelk,
F998 type 0, F994/F650 seat and station rows. This fix does not move the start.

## Causes and correction

The world renderer inferred a model-to-world scale by dividing the longest
surface axis by an object's inner collision bound (and by the outer bound in
landed views). Neither bound defines the terrain's base radius. For Hope the
ship was drawn about 1.39 base radii from the planet despite standing on its
physical terrain. F1533 and F579 use descriptor scale plus scale offset;
vertices already contain descriptor scale, leaving `2^scale_offset` world
millimetres per post-scale geometry unit. Hope's offset is 4, so the conversion
is 16 mm per model unit. The renderer now reads that field from model records.

The port also introduced an inside-sphere centroid/normal cull and bypassed
F1868/F1869's coarse horizon test there. That extra cull removed the ground
when the eye was near or inside the terrain base sphere. Removed both changes
to restore the original recursion. Removed the related suppression of F1877's
atmosphere strength; the original routine now receives its record unchanged.

## Evidence and limits

`verify-surface-near.py --reference JJFFE_COMMANDS --cache DATA_BIN` executes
original F1876 for four model-131 near-pad scenes, including eyes inside the
base sphere. All 168 emitted triangles, nodes, edges, normals, depth words,
colours, atmosphere state and order queues match Rust. The Windows adapter
runs each case in its own process and streams its result to stdout, because
this platform has neither fork nor open_memstream. No supplied source changed.

`hope_start_and_launch_keep_the_physical_ground_visible` uses the actual Saker
statistics and pad physics, advances the system clock, and excludes buildings
from rendering. Ground remains visible at frames 0, 50 and 150. Height above
the station rises from 3,664 mm to 22,354 mm at 1 s and 169,715 mm at 3 s. Thus
the regression checks both rendered ground and physical rise, including the
first orbital refresh. The final 240-frame `--capture-start` smoke also passes with hull at 100%.
The capture script previously forced 10000x at frame 75 and forward speed
during the automatic climb; those stress actions were removed so this evidence
reproduces an ordinary 1x launch. Player acceleration controls are unchanged.

This establishes the reported Hope start/first-three-seconds regression and
near-pad source arithmetic. It is not a pixel-identical whole-game claim;
whole-frame lighting/depth and other planets still require broader traces.
