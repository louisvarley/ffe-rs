# Source-parity implementation, 26 October 2026

The source remains JJFFE 2.8a7. This entry supersedes earlier claims that launch,
starting view and destruction already matched the source. PAR-19 is complete;
other touched audit items remain in progress.

## Confirmed causes and corrections

- F938/F929: 192 executed original cases now match reserve energy, shield/hull
  recharge, normal/military cargo fuel and waste, including signed/overflow
  boundaries. Run `python tools/reference/verify-recharge.py --reference
  .cache/jjffe-reference/jjffe-smoke.exe`. Trace SHA-256:
  `1d6e1a670b089bb38ac59f6984cdfce06a69906ab752fdd3c5dbca8993417903`.
- F947/F948/F934: loadout, gauge, fuel selection and hyperspace disagreed after
  drive damage. Current combat drive is authoritative, including zero; fitting,
  degradation, service failure and save migration synchronize the equipment copy.
- F1677: navigation treated F879 surface-port coordinates as linear despite the
  rotating-frame flag. Transform both position and rows by the parent matrix.
  Before correction the first Hope contact reported about 680 m and prematurely
  handed off the source launch. The first-contact regression now checks <200 m.
- F1013: test previous altitude and current signed selected speed before F1041;
  use F602 even on the handoff frame. Do not run F1031 in that same frame. The
  live handler no longer uses a seat-climb measurement for its altitude decision.
- F994: temporarily reserve station byte +0xd1 during F650 allocation, then clear
  it. Both checked starts receive source bay 0 instead of the port's bay 1.
- F998/F997: the Hope default start reverses the right/forward rows after placing
  the ship. Missing this call pointed the camera into the nearby hangar. The
  source-frame probe reproduces the owner's broad city view; the port now has the
  same city arrangement, but sky/ground colours still differ.
- F1104/F1105: camera distance is 0xf9000000 and reference is the destruction
  object. The camera follows that object's axes/movement and the dead hull is
  hidden. Explosion shader inputs and lifetime still require original tracing.
- New commander after death now uses the same initializer as initial boot.
- Signed selected speed survives save/reconstruction; actual speed is independent.
- Starting free hold is 1 t again. Classic controls default for new configuration;
  persisted preferences remain honored. No new controls preference is forced on
  existing saved configuration.
- Source initialization/flight errors propagate instead of silently changing to
  the prototype flight simulation.

## Remaining acceptance gates

Keep PAR-01/02/03/04/05/06/07/08/23/24 open for their full acceptance scenarios.
The 120-frame landing grace and suppressed frame selector still need removal
against a faithful moving/rotating body world. Exact F152 scheduling, starting
atmosphere/lighting, full autopilot and source effect evolution remain unfinished.
Legacy cache-dependent navigation tests also expose three existing developer
controller failures; the six source autopilot tests pass. Those failures are
not grounds to mark autopilot complete.

Original frames and binaries are local-only in `.cache`; no assets or captures
are included in this change.
