# Source gameplay parity audit — 26 October 2026

The owner requires the original JJFFE 2.8a7 gameplay, with Port Options as the only general extension. Earlier experimental deviations are implementation history, not acceptance of different gameplay. The explicit intro Space release guard remains recorded as a separately requested input exception; PAR-06 must keep that decision visible rather than call it original code. No runtime behaviour is changed by this audit.

This review covers the live launch/docking/input/speed/fuel/death/new-game paths and a cross-system scan of NPCs, hyperspace, economy, missions, randomness and persistence. It is not an exhaustive proof of the entire assembly or port. Confirmed differences are separated from missing evidence and suspected causes. Current uncommitted death/rendering work was read, not overwritten; hashes and HEAD are in [the review snapshot](../evidence/parity-audit/review-snapshot.json).

## Immediate findings

1. **Fuel and acceleration:** the source F152/F151 event-6 payload is whole seconds; F1007 hands it to F938. The live port feeds 994 physics ticks every 20 ms into thrust consumption and recharge. With Saker mass 28 and up-thrust 3270, the source cost is 178 reserve units per second; the port cost is 8,846,600. A fresh 0x20000000 reserve then lasts about 60.7 s of continuous up-thrust instead of about 34.9 days under that constant-thrust assumption. The warning itself is original (F929, text 0x9907); early exhaustion is not. It zeroes available thruster limits and can explain dead acceleration. See [numeric evidence](../evidence/parity-audit/fuel-time-units.json). Exact warning time still depends on actual thrust, shots, time acceleration and refuelling.

2. **Takeoff:** matching 0x24/0x26 constants is not enough. Source F1013 changes phase before the force update and calls F602 unconditionally. The live pad tick uses the generic engines-on capped integration and updates takeoff phase afterwards. Stored-seat altitude, 120-rendered-frame landing grace and inhibited frame selection are additional differences.

3. **Docking:** the live orbital station contact still uses the port corridor/speed/harmless-push rule. It is not the original pad collision and crash/lift/rotating-frame path.

4. **Starting view:** the supplied original image establishes a visible mismatch, despite matching Gateway / Hope / Old Blackelk names. The correct fix requires an actual type-0 source dump of bay, seat, rows, rotating flag, cockpit eye, city/model instances, lighting and viewport. F994 explicitly sets rotating flag +0x57=1; the playable builds a linear pad world. Do not infer a new coordinate or move the camera until this is compared. The reference image is the user-provided `codex-clipboard-9f7ceb1e-194a-48a7-bb38-cf87e02f1b83.png`; it is not committed/copied here. Earlier probes or screenshots for Ross 154/type 1 do not certify this view.

5. **Destruction:** source F1104/F1105 changes the camera reference to the explosion object and sets distance 0xf9000000. Merely selecting External in the port leaves the old ship/reference/distance and ordinary own-ship draw path. Explosion growth inputs and lifetimes are substituted. The exact blue primitive is not identified yet; model/primitive traces are required before calling that symptom fixed. Uncommitted tombstone changes do not establish the preceding explosion sequence.

6. **Hyperjump arithmetic is not wholly wrong:** F903 itself reads model mass, so the current model-mass range/fuel calculation is consistent with source; do not replace it with guessed cargo-loaded mass. Initial internal reserve 0x20000000 also matches F998. The problems include event units, split drive authority after damage, military reserve waste accounting and instant synthetic arrival/transit.

## Priority order and acceptance

Implement PAR-01 through PAR-08 first, in order with their listed dependencies. PAR-01 is the first fix: acceleration results cannot be trusted with the wrong fuel clock. Restore full source takeoff/contact before removing grace heuristics. Capture the source type-0 start and destruction scene before adjusting their presentation. A P0 may require a focused prerequisite slice of the listed P1 item; the priority does not waive that dependency. The remaining items cover gameplay differences found by the broader scan.

Every item needs executed-original **end-to-end** evidence for its affected path, a failing regression of the live call order and a passing corrected trace. Numeric helper tests and screenshots alone do not close an integration item. Same input/seed/clock/assets/settings must be used. Preserve original sources/assets unchanged and store only allowed numeric evidence, adapter code and hashes.

## PAR-01 — P0 — Repair F938/F929 event units for fuel, shields and hull repair

**Status:** todo. **Evidence:** Confirmed runtime mismatch.

**Finding:** main.rs passes combat::frame_tics into spend_thrust_energy/recharge. F152 divides elapsed tics by 0xc22e and dispatches event 6 through F151 to F1007; F1007 passes the whole-second count to F938. At 1x the port charges about 49,700 times the source rate. This explains early internal-fuel warnings and disabled acceleration.

**Source:** F152/F151/F1007/F938/F929. **Port locations:** crates/ffe-rs/src/main.rs:4892; crates/ffe-rs/src/flight.rs:460,615; crates/ffe-rs/src/combat.rs:149.

**Done when:** Trace one reserve from fresh type-0 boot through idle, 0x24 climb, full thrust, firing and 1x/10x/10000x. Match source event counts, energy, warning time, limit restoration, shields and repair; preserve wrapping arithmetic.

**Dependencies / existing scope:** gameplay-3, flight-6, flight-8.

## PAR-02 — P0 — Make reserve refuelling and cargo fuel bookkeeping match F929/F903

**Status:** todo. **Evidence:** Confirmed bookkeeping mismatch; reserve initialisation is correct.

**Finding:** The internal reserve is object +0x11e and correctly starts at 0x20000000. It is distinct from cargo fuel, although thrust and lasers use the same source reserve. Flight::burn_fuel removes military fuel without adding radioactive waste; the jump helper does add it. Do not invent separate rechargeable laser and engine pools.

**Source:** F929/F930/F903/F998. **Port locations:** crates/ffe-rs/src/flight.rs:565; crates/ffe-rs/src/navigation.rs:1252; crates/ffe-core/src/hyperspace.rs.

**Done when:** Compare hydrogen and military auto-refill with/without Auto Refueller, full/empty reserve, zero-cost call, full hold, waste and cargo-used words, including after save/load. Verify jump cargo is debited exactly once.

**Dependencies / existing scope:** PAR-01, gameplay-3.

## PAR-03 — P0 — Use one authoritative fitted-drive and failure state

**Status:** todo. **Evidence:** Confirmed runtime mismatch.

**Finding:** Damage/service code changes combat.drive while drive_state prefers nonzero loadout.drive. Thus a damaged/destroyed drive can still supply the old hyperdrive class, and reserve fuel selection can disagree with the jump report.

**Source:** F947/F948/F934/F408/F410/F903. **Port locations:** crates/ffe-rs/src/flight.rs:532,645; crates/ffe-rs/src/main.rs:6309; crates/ffe-rs/src/outfitting.rs:62.

**Done when:** Destroy, degrade, service, remove and fit each drive; compare engine limits, gauge, fuel type, range, jump refusal and reload with the original object +0xd0.

**Dependencies / existing scope:** PAR-01, PAR-02, gameplay-2.

## PAR-04 — P0 — Translate the complete F1013 takeoff handler and same-frame mode order

**Status:** todo. **Evidence:** Confirmed runtime mismatch.

**Finding:** F1013 runs F1037, changes 0x24 to 0x26 before F1041 when its tests pass, and explicitly calls F602. The port updates Takeoff after its physics tick and Player::step_in normally chooses capped F601 when engines are on. The fixed up-thrust is therefore not sufficient evidence of source launch behaviour.

**Source:** F1013/F1037/F1041/F602/F1046/F1033. **Port locations:** crates/ffe-rs/src/main.rs:3975,4123; crates/ffe-rs/src/original_flight.rs:303; crates/ffe-core/src/player.rs:371; crates/ffe-core/src/traffic.rs:320.

**Done when:** Replay type-0 and type-1 launches: no keys, held Return/Shift or selected port control mapping, speed change at threshold, engine-off and failure cases. Compare every frame mode, set/actual speed, thrust, cap, velocity and position.

**Dependencies / existing scope:** PAR-01, PAR-03, flight-13.

## PAR-05 — P0 — Replace launch altitude substitution and collision grace with source frame/contact rules

**Status:** todo. **Evidence:** Confirmed runtime mismatch.

**Finding:** The runtime substitutes climb from a stored seat for DATA_008867, suppresses landing scans for 120 presented frames, and inhibits the source frame selector in its small pad world. Launch safety consequently depends on frame rate and adapters rather than source contacts.

**Source:** F1031/F1032/F1033/F569/F572/F573/F604-F606. **Port locations:** crates/ffe-rs/src/main.rs:1184,3917,4127,4162; crates/ffe-rs/src/original_flight.rs:206,621.

**Done when:** Prove pad release, planet retained, rotating/linear transforms, re-touch, high acceleration and return-to-pad on real ports; compare F1033 altitude and flags. Remove grace only when those traces pass.

**Dependencies / existing scope:** PAR-04, flight-9, flight-10.

## PAR-06 — P0 — Verify original set-speed controls, input gates and speed persistence end to end

**Status:** todo. **Evidence:** Confirmed differences; reported dead acceleration also depends on PAR-01.

**Finding:** Modern W/S is default while source Return/Shift is selectable. HeldControls adds release gates. Landed F1039 correctly excludes throttle, but launch must enable F1037 immediately. OriginalFlight reconstruction derives nonnegative set speed from actual forward velocity, losing negative or independently selected speed; cancel has both halt-to-zero and actual-speed paths.

**Source:** F1037/F1038/F1039/F1018/F1121. **Port locations:** crates/ffe-rs/src/input.rs:283; crates/ffe-rs/src/original_flight.rs:86,284,557,590; crates/ffe-rs/src/main.rs:2188,2690.

**Done when:** Record physical key -> mapped key -> active/mode -> F1121 set speed -> thrust -> actual velocity. Test fresh key after intro, held intro key, launch, autopilot cancel, engines off, reverse speed, menus and reload. Source controls are default; optional mappings belong only in Port Options.

**Dependencies / existing scope:** PAR-01, PAR-04, flight-5.

## PAR-07 — P0 — Match the original Hope starting state and cockpit view to the supplied image

**Status:** todo. **Evidence:** Confirmed visual mismatch; exact spatial cause not yet established.

**Finding:** Correct system/planet names do not prove matching bay, seat, rotating frame, eye offset, city model, orientation or lighting. Current source F994 copies station rows, sets +0x57=1 and calls F650; the port uses a fresh bay allocation and a linear pad world. The screenshot shows the expected broad city/runway view, unlike the close building face. Do not guess a new coordinate or camera angle.

**Source:** F829/F880/F879/F998/F994/F650/F1050/F1054/F1536. **Port locations:** crates/ffe-rs/src/navigation.rs:1965; crates/ffe-core/src/pads.rs:109; crates/ffe-rs/src/camera.rs:281; crates/ffe-rs/src/screen.rs:2672.

**Done when:** Dump actual default type 0, not the Ross 154 type-1 probe: clock/RNG, station slot/model, bay table, ship rows/frame, seat, eye, visible objects and projection. Compare identical 320x200 front frames at 12:00:01 against executed original and the user image.

**Dependencies / existing scope:** PAR-05, PAR-17, ui-7, ui-11.

## PAR-08 — P0 — Restore the complete third-person player explosion before GAME OVER

**Status:** todo. **Evidence:** Confirmed lifecycle/camera mismatches; exact blue-mesh primitive needs capture.

**Finding:** F1104/F1105 select the explosion object as camera reference and set external distance 0xf9000000. The port switches view but retains the old ship as reference, keeps ordinary own-ship rendering and its existing external distance. Effects use a fixed lifetime and forced global 4=0x4000 rather than source object evolution. This is not equivalent to F942 followed by the external explosion hold.

**Source:** F953/F942/F926/F764/F1104/F1105/F1054/F1107/F917. **Port locations:** crates/ffe-rs/src/main.rs:6390,4674; crates/ffe-rs/src/death.rs:232,288; crates/ffe-rs/src/screen.rs:1784,2641; crates/ffe-core/src/effects.rs:120.

**Done when:** Capture non-capsule player death at source event-relative times: removed old ship, correct explosion reference/distance, animated fragments, debris, palette/depth, sounds and 0x3cae6 wait, then tombstone. Identify the blue mesh by model/primitive trace; test crash, laser and missile death separately.

**Dependencies / existing scope:** PAR-17, PAR-21, gameplay-25, ui-12.

## PAR-09 — P1 — Replace orbital docking corridor tests and harmless pushes with source contacts

**Status:** todo. **Evidence:** Confirmed gameplay substitute.

**Finding:** Port::contact accepts a 15%-of-face corridor (minimum 60 m), 250 m/s entry limit and teleports/rezeros bad approaches without source damage. Navigator::contact calls this in normal player flight. Original collision/clearance/pad tests must decide docking and crashes.

**Source:** F575/F576/F580/F652/F655/F1019/F1024. **Port locations:** crates/ffe-rs/src/docking.rs:12,95,100; crates/ffe-rs/src/navigation.rs:4107; crates/ffe-rs/src/main.rs:4197.

**Done when:** For every station model compare cleared/uncleared entry, bay, hull/slot edge, angular mismatch, moving station and entry speed; same damage, bounce, law/traffic and dock transition. No guessed corridor or silent position reset.

**Dependencies / existing scope:** PAR-05, flight-10.

## PAR-10 — P1 — Use original indoor departure placement and moving lift/door state

**Status:** todo. **Evidence:** Confirmed gameplay substitute.

**Finding:** Port::launch_point places the ship front + 2*radius + 2 km away. Launch-grace suppresses hull collision until clear, and the ship is held by port sequence adapters. Timed lift stages alone do not verify release position/velocity/orientation.

**Source:** F1088/F1091/F1094/F650/F651/F671/F573. **Port locations:** crates/ffe-rs/src/docking.rs:95; crates/ffe-rs/src/navigation.rs:2210; crates/ffe-rs/src/main.rs:4290,4338,4740.

**Done when:** Compare source door stages, bay, object placement and velocity on every release frame for rotating and nonrotating stations; remove spatial grace/teleport substitutions.

**Dependencies / existing scope:** PAR-09, flight-11, flight-9.

## PAR-11 — P1 — Complete source autopilot world and target adapters

**Status:** todo. **Evidence:** Confirmed missing source branches.

**Finding:** Live player ordinary F597/F587/F589 arithmetic is verified, but F610/F802 avoidance, accelerated near-offset physical-frame matching, rotating-parent F589, ship targets and object-event cadence are missing. Prototype helpers remain developer-only and their arrival tests are not player parity evidence.

**Source:** F741-F746/F594/F597/F610/F802/F589/F706/F732. **Port locations:** crates/ffe-rs/src/navigation.rs:4646; crates/ffe-core/src/autopilot.rs; docs/notes/autopilot-source-audit.md.

**Done when:** Replay full planet approach, obstruction, moving ship pursuit, all station docking models and 1x through 10000x without direct turns or long-leg teleports; same event/state/contact sequence.

**Dependencies / existing scope:** PAR-01, PAR-05, PAR-09, flight-14.

## PAR-12 — P1 — Unify source event clocks, time acceleration and physical integration

**Status:** todo. **Evidence:** Confirmed scheduler mismatch.

**Finding:** The frame loop, fixed prototype accumulator, whole-second events, AI updates and 0.1 s physics clamp use different scheduling paths. TICS_PER_SECOND is 49700 while F152 uses 0xc22e=49710. High acceleration can advance clocks/events without equivalent collision/frame work.

**Source:** F152/F151/F706/F727/F1031/F1046. **Port locations:** crates/ffe-rs/src/main.rs:1458,4255,4892; crates/ffe-rs/src/original_flight.rs:21,803; crates/ffe-core/src/player.rs:371.

**Done when:** Same scripted input and RNG consumption at 20/50/60/144 fps, pauses, focus loss and acceleration boundaries; distinguish source timer domains and reproduce event ordering, not merely elapsed calendar time.

**Dependencies / existing scope:** PAR-01, flight-4, flight-6, flight-8.

## PAR-13 — P1 — Complete hyperjump initiation, transit, misjump and source arrival

**Status:** todo. **Evidence:** Confirmed gameplay substitute; core fuel formula not shown wrong.

**Finding:** F903 standalone range/fuel/time arithmetic uses model mass, as does current drive_state: do not replace that with guessed wet mass. Navigator::hyperspace instantly advances time and chooses an offset from the first planet, skipping the source transition and arrival placement.

**Source:** F899-F909/F903/F908/F606. **Port locations:** crates/ffe-core/src/hyperspace.rs:46; crates/ffe-rs/src/navigation.rs:4326,4433; crates/ffe-rs/src/main.rs:2136,6309.

**Done when:** Replay normal/military drives, exact and insufficient cargo, out of range, mode/altitude refusal, cancellation/failure/misjump, clock/cargo/waste and source arrival RNG/position. Compare consumption as a transaction with no debit on refusal.

**Dependencies / existing scope:** PAR-02, PAR-03, PAR-12, world-7.

## PAR-14 — P1 — Verify source fuel scooping gates and capacity changes

**Status:** todo. **Evidence:** Confirmed adapter requiring parity trace.

**Finding:** The playable uses nearest-star surface distance, floating speed conversion, a cooldown and port cargo-capacity accounting. Helper threshold tests do not establish F1033 contact/class, atmosphere, heat, cargo-used and timing behaviour.

**Source:** F1033/F929/F903. **Port locations:** crates/ffe-rs/src/main.rs:4923; crates/ffe-rs/src/damage.rs:214; crates/ffe-rs/src/flight.rs:675.

**Done when:** Sweep contact flags, altitude, speed, fitted/mounting state, fuel species, full hold, pause and acceleration; compare exact increment events and heat/damage with the original.

**Dependencies / existing scope:** PAR-01, PAR-02, PAR-12.

## PAR-15 — P1 — Replace NPC speed caps and simplified flight state transitions

**Status:** todo. **Evidence:** Confirmed gameplay substitute.

**Finding:** NPC Spec derives floating top speeds and an acceleration ramp; Ship::tick moves toward a chosen speed instead of integrating original object thrust/caps through every source state. The cagent/AI done labels therefore do not establish source combat difficulty.

**Source:** F706/F725/F727/F752/F770/F773 and DATA_006258. **Port locations:** crates/ffe-core/src/npc.rs:665,1714; crates/ffe-rs/src/navigation.rs:3328.

**Done when:** Trace traders, escorts, pirates, police and heavy freighters under the same objects/seed; match per-state target, thrust, turn, velocity and event time at 1x and accelerated time.

**Dependencies / existing scope:** PAR-11, PAR-12, PAR-17, gameplay-18.

## PAR-16 — P1 — Restore system-wide encounter, station traffic and police placement rules

**Status:** todo. **Evidence:** Confirmed gameplay substitute.

**Finding:** F688 counts/model tables are present, but spawn placement, anchor orbit, station selection, police reconstruction and reduced traffic give-up/cruise rules change who appears and how encounters unfold.

**Source:** F688/F691/F702/F762/F783/F238. **Port locations:** crates/ffe-core/src/npc.rs:760,2510,2671,2804; crates/ffe-rs/src/navigation.rs:3197.

**Done when:** For a fixed source seed compare all live object slots/roles/positions, station bay occupancy, launch times, arrivals, police responses and persistence through a jump; include mission-gated Thargoids.

**Dependencies / existing scope:** PAR-15, PAR-17, gameplay-7, gameplay-11.

## PAR-17 — P1 — Restore source RNG state and call order for gameplay

**Status:** todo. **Evidence:** Confirmed gameplay deviation.

**Finding:** Markets, effects, dust and NPC adapters initialise independent streams from system/position/clock. The original shares DATA_008818/8820 calls. Identical formulas cannot reproduce stocks, encounters, damage or missions with a different draw order.

**Source:** F1476/F1530/F922/F622/F633/F942/F1035. **Port locations:** crates/ffe-rs/src/navigation.rs:2370,3197,3926,4015; crates/ffe-rs/src/death.rs:296; crates/ffe-rs/src/flight.rs:105; docs/DEVIATIONS.md.

**Done when:** Record a baseline draw ledger including object-event and rendering dependencies; match both source words and downstream outcomes. Optional port presentation must not silently choose a different gameplay seed.

**Dependencies / existing scope:** flight-6, world-4.

## PAR-18 — P1 — Persist and restore the complete live simulation state

**Status:** todo. **Evidence:** Confirmed gameplay deviation.

**Finding:** Navigator::from_save clears ships, missiles/effects and respawns traffic; RNG is skipped/reseeded and the integer OriginalFlight/control state is reconstructed from floating Flight. This changes threats, speed selection, bays and other outcomes on reload. Native file encoding itself need not change gameplay.

**Source:** F65/F66/F67 and object/state records. **Port locations:** crates/ffe-rs/src/saves.rs:100; crates/ffe-rs/src/navigation.rs:2185; crates/ffe-rs/src/flight.rs:93; crates/ffe-rs/src/original_flight.rs:86.

**Done when:** Save at parked pad, climb, rotating dock, combat/missile flight, jump transition and landed surface; compare continuation with uninterrupted execution including RNG, integer fractions, caps, signed set speed and bay ownership.

**Dependencies / existing scope:** PAR-06, PAR-12, PAR-17, ui-22.

## PAR-19 — P1 — Remove the non-source 7-tonne starting hold and reconcile fitted capacity

**Status:** todo. **Evidence:** Confirmed explicit prior deviation, now superseded by owner parity requirement.

**Finding:** DEFAULT_CARGO=7 bypasses the six tonnes of fitted items; the verified original default status shows 1 t free. Cash numeric 10000 is in tenths and is not evidence that starting cash is wrong. Keep the same Saker, Gateway/Hope/Old Blackelk and source loadout.

**Source:** F998/F994/F413/F383/F558. **Port locations:** crates/ffe-rs/src/navigation.rs:2060; crates/ffe-core/src/equipment.rs; docs/DEVIATIONS.md EXP-019.

**Done when:** Compare fresh F2/services cargo/cabins, fitted item mass, used/free hold, cash units, internal reserve and cargo fuel with the type-0 original before any trade or launch.

**Dependencies / existing scope:** PAR-07, gameplay-26.

## PAR-20 — P1 — Replay market, shipyard, BBS and daily mission events using source state

**Status:** todo. **Evidence:** Confirmed independent RNG/lazy-generation deviation; individual formulas may match.

**Finding:** Markets are first-visit generated from station seeds and are not restored with traffic; offered mission templates do not prove complete source scripts, deadlines or event hooks. Source outcome parity requires event chronology and the authoritative saved world, not formula tests alone.

**Source:** F622/F633/F635/F619/F620/F301-F306 and mission tables. **Port locations:** crates/ffe-rs/src/navigation.rs:2370; crates/ffe-rs/src/trade.rs; crates/ffe-core/src/missions.rs; docs/notes/time.md.

**Done when:** Compare first visit, day boundaries, timewarp, accepted missions, cargo/passengers, legal witnesses/fines, rewards, failure and reload in the same source trace; enumerate any absent script before marking done.

**Dependencies / existing scope:** PAR-17, PAR-18, gameplay-1, gameplay-9.

## PAR-21 — P1 — Drive all explosion, debris and hit effects from source object states

**Status:** todo. **Evidence:** Confirmed lifecycle substitute.

**Finding:** Effect::tick integrates floating velocity and removes objects after port-chosen lifetimes. Renderer supplies a fixed explosion input instead of F764/F769 growth/state evolution. This affects the destruction scene, visibility and object recycling/RNG as well as appearance.

**Source:** F939/F942/F926/F764/F769/F927/F1536. **Port locations:** crates/ffe-core/src/effects.rs:44,120; crates/ffe-rs/src/main.rs:5262; crates/ffe-rs/src/screen.rs:2641.

**Done when:** Match birth records, age/global words, fragment calls, movement, duration and slot removal for player/NPC/missile destruction; source reference must choose the rendered input rather than forcing fragments visible.

**Dependencies / existing scope:** PAR-08, PAR-12, PAR-17, ui-12.

## PAR-22 — P1 — Replay missile, ECM, escape and legal outcomes in the complete event loop

**Status:** todo. **Evidence:** Integration fidelity still unproven; source constants alone are insufficient.

**Finding:** Existing missile/ECM and escape helpers cover several numeric/state branches, but runtime cadence, object ownership/recycling, shared RNG and the simplified NPC world can change fusing, evasion, fines and survival. F1098 derelict behaviour is explicitly absent.

**Source:** F754/F757/F758/F718/F721/F935/F1098/F931/F941/F953. **Port locations:** crates/ffe-core/src/missile.rs; crates/ffe-rs/src/missile.rs; crates/ffe-rs/src/death.rs; crates/ffe-rs/src/navigation.rs.

**Done when:** Whole encounter traces: player/NPC launches, both ECM types, decoys/mines, witness reporting, subsystem failure and capsule/Stowmaster escape; preserve old-ship AI, cargo/equipment and exact source event order.

**Dependencies / existing scope:** PAR-15, PAR-16, PAR-17, gameplay-5.

## PAR-23 — P1 — Remove silent fallback to the synthetic playable from normal starts/loads

**Status:** todo. **Evidence:** Confirmed reachable fallback path.

**Finding:** Navigation/model creation converts errors to None and original=None permits Flight::step synthetic physics. That alternative must remain developer-only; a missing asset/frame adapter must not silently change player movement, safety or docking rules.

**Source:** F998/F994/F1031/F1021 and source module dispatch. **Port locations:** crates/ffe-rs/src/main.rs:874,986,4268; crates/ffe-rs/src/flight.rs:335.

**Done when:** Inject missing/invalid essential cache records and unsupported saves; the normal game must report a concrete error or source-equivalent recovery, never enter prototype physics. Dev tools remain explicitly gated.

**Dependencies / existing scope:** PAR-07, PAR-18, flight-8.

## PAR-24 — P1 — Use the same source new-game setup after death and on every resume path

**Status:** todo. **Evidence:** Confirmed restart mismatch.

**Finding:** After GAME OVER the new-game branch sets cash/ship and Mode::Docked, original=None, but does not apply the initial pad seat/rows/model setup used by the first boot. This can restart in a different view/state and expose the synthetic fallback.

**Source:** F917/F998/F994/F1098/F1011. **Port locations:** crates/ffe-rs/src/main.rs:1284-1323 versus 1004-1064.

**Done when:** Compare initial Enter boot, post-death Enter boot, alternate start, intro load and escape/resume paths: identical source fields, seat/rows/frame, HUD, model, input release, fuel and services before the first tick.

**Dependencies / existing scope:** PAR-07, PAR-08, PAR-18, PAR-23.

## Limits of the scan

Some original arithmetic has meaningful differential tests: the ordinary autopilot guidance branch, collision helpers, F903 hyperdrive maths, parts of model execution and world generation. Those tests remain useful; they do not validate the runtime adapters above. Historical deviation descriptions can be stale, so this audit follows current call sites rather than treating every old EXP entry as still active. Developer-only prototype autopilot tests were not counted as live player defects; the reachable synthetic flight fallback was. No gameplay bug is marked fixed merely by adding these work items.


### Implementation progress — 2026-10-26

PAR-01 and PAR-02 are in progress. The live F938 call site now uses whole
seconds from the existing cooling accumulator rather than per-frame physics
tics. Thrust IMUL/SAR arithmetic retains the source low 32 bits. F929 military
internal refuelling now creates one tonne of radioactive waste. The 27 fuel
and combat tests pass, including multi-rate/time-acceleration reserve checks.
Executed-source event traces, exact scheduler alignment and remaining recharge
numeric boundaries are still required; neither work item is closed.
