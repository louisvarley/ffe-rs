# Takeover issue report — 25 October 2026

Reviewed the current Rust port, JJFFE assembly, handoff, dashboard and supplied screenshots. Existing uncommitted documentation was preserved. Several handoff paragraphs describe older states; the current code and executed checks take precedence.

| Reported issue | Cause and result |
| --- | --- |
| Intro nebula in front of planets | Previously lost alignment of primitive depth keys. HEAD already contains v1.10's fix. The nebula/planet landmark render test passes. |
| Intro has no stars | F1766/F1814 chain particles were missing. HEAD already implements them; the test verifies a star reaches the framebuffer. This does not establish exact star density across the whole intro. |
| Ship/station lettering extends outside the model | The port substituted the full catalogue name for an absent object registration. Removed that substitution: source F1344/F1747 uses the object's string. Explicit supplied registrations still render. This does not finish 0x3022 word wrapping. |
| Game-over screen differs | The source tombstone and death hold exist and their tests pass. Font normals/face selection still use a workaround; obituary placement and rare-model geometry are not pixel-exact. Fidelity remains unfinished. |
| Destroyed on spawn | Earlier scaffolding forced pirates near the pad and ran combat against landed players. Current code already suppresses landed combat and no longer forces that spawn. Five boot-safety tests pass, including 60 seconds without attacks. The exact reported live incident was not reproduced. |
| Launch starts in space / planet vanishes | Earlier placement/frame and near-surface rendering paths were incomplete. Current takeoff check passes modes 0x24→0x26→0x00; 22 planet tests pass, including the boot planet and near-surface rendering. This is regression coverage, not reproduction of the reported intermittent live failure. |
| “Your ship is destroyed. GAME OVER.” | Port-authored log line, not original text. Removed it. |
| No moving space dust | Confirmed missing implementation: AI state 0x1b lists control F769/movement F715, but the playable world has no corresponding dust lifecycle/spawn path. Still outstanding. |
| Pause menu | Port-authored PAUSED overlay. Removed the overlay; preserved source Escape stopped-time/preferences sequencing and focus-loss input safety. |
| Intro Space fires after entry | Raw key-down bypassed HeldControls. Laser fire now uses the existing release-to-rearm latch, disarmed at game entry. A second Space press works and holding that press continues firing. Regression test passes. |

**Count:** of the ten reports, **two involve unfinished implementation/fidelity**: dust (missing) and game-over rendering (partial). Four received fixes in this pass; four have earlier fixes present and passing targeted checks. The intermittent launch/spawn reports still need an exact live reproduction before claiming complete resolution.

## Next ten outstanding work items, in priority order

1. Space-dust spawn, lifetime and rendering, respecting the original detail preference (new explicit backlog item).
2. `flight-12`: scripted flight scenarios against the reference; include idle boot, manual takeoff, launch frame continuity and retained planet.
3. `gameplay-13`: finish death/escape fidelity, especially tombstone font normals, obituary layout and rare variant.
4. `ui-11`: near/far clipping and planet approach fidelity.
5. `v0.48-14`: pixel-level whole-frame oracle, extended to the reported intro and game-over frames.
6. `flight-4`: finish fixed 50 Hz flight integration independent of presentation rate.
7. `flight-9`: finish entering/leaving rotating station frames.
8. `flight-10`: docking position/orientation and launch continuity.
9. `ui-5`: finish text rendering, including object-name wrapping and glyph normals.
10. `v0.48-8`: planet-scale surface quad oracle.

## Validation

Workspace tests pass; local-cache checks pass: intro 12, boot safety 5, planet 22, death view 2 and game-over 2. Takeoff source/scenario check passes all 24 assembly checks and the expected mode sequence. Format and warning-free Clippy checks pass. These checks do not prove whole-frame source parity. Use the rebuilt `target/release/ffe-rs.exe` or `play.bat` for retesting.

The rebuilt headless intro capture completes, but late model 176 reports the existing undefined-parent-basis child path (360/456/321). This is an additional intro fidelity limit, not fixed here.

## Follow-up: immediate death reproduced and fixed

The owner's retest exposed a different startup failure from the pirate spawn:
main calls `Flight::recharge` before `ensure_damage`. On a new ship `[0, 0]`,
recharge wrote `[0, full]`; ensure_damage then saw a matching maximum and left
zero hull intact, triggering death in the first frame. The new regression
`first_frame_recharge_does_not_destroy_a_new_ship` failed with `[0, 100]` before
the fix and passes after initialising both words to full. A second test confirms
existing damage and actual destroyed hulls remain unchanged. All 13 damage
and 8 death tests pass. The earlier boot tests bypassed recharge and therefore
missed this defect. This is a regression, not an unimplemented feature.
