# Sound cues and music wiring

Status: the game posts the original's effect cues and scene music, and the
16-voice mixer now plays them. The cue table, the preference page switches, the
music rules and the event sites are wired and tested. `AudioEngine`'s device
engine owns a WinMM `waveOut` mixer fed by the cache's `base:effect/*` WAVs; the
recording/headless sinks still see the original sample-table calls. Song 0
(`QQFRONT`) starts with the intro per F919 and is cached in v28
(`base:music/qqfront`). Captures and the smoke test construct a silent engine,
so no test or capture path opens a device.

This note is the implementation record for `crates/ffe-rs/src/sound.rs` and the
call sites in `crates/ffe-rs/src/main.rs`. The source of truth is the DOS
`jjffe-master/ffe.asm` (line numbers below are from that file) and the sample
table in `jjffe-master/win32/w32sound.cpp`.

---

## 1. The cue API

`GameAudio` owns the `ffe_audio::Audio` player (the interface the preferences
stream designed) and exposes:

| Method | Original | Meaning |
|---|---|---|
| `play(Cue)` | `F1906` | sample at the default linear level 63 (`push dword 0x3f`) |
| `play_level(Cue, level)` | `F1908` | explicit linear level |
| `play_log(Cue, logvol)` | `F1907` | `0x3f >> (logvol >> 2)` |
| `front_end_music()` / `leave_front_end()` | `F919` | song 0 and its done-replay; stop on leaving |
| `start_battle()` / `end_battle()` | `F750` / `F770` | siren, song 14 if enabled, stop at combat end |
| `hyperspace_music()` | `F656` | song 15 if "Music after hyperspace" |
| `sequence_end_music()` | `F1094` | random enabled song if "Docking music" |
| `stop_for_sequence()` | `F1048`/`F1091` | a docking/launch transition stops the song |
| `apply_prefs(&Prefs)` | `F216`/`F1908`/`F1902` | the ten Sound Effects switches |
| `tick(elapsed)` | `F1904`/`F232`/`F919` | advance MIDI, continuous-music/done handling |

`Cue::sample(SoundFlags)` reproduces `F1908`'s per-sample switches:

| Gate | Global | Cues |
|---|---|---|
| `SoundFlags::icon_beeps` | `DATA_009036` | `0x14` `bip` only (`Cue::UiBeep`) |
| `SoundFlags::space_station` | `DATA_009030` | `0x10` `station` (`Cue::Station`) |
| `SoundFlags::engine_sound` | `DATA_009032` | `0x0f` `station` (`Cue::Engine`) |
| `SoundFlags::wind_noise` | `DATA_009034` | `0x0e`/`0x25` `wind` (`Cue::Wind`) |
| `AudioSettings::effects_on` | `DATA_009028` | all effects (applied by the player) |

`0x15` `bip` is deliberately **not** gated by "Icon beeps": `F1908` only tests
`DATA_009036` for index `0x14`.

---

## 2. Source map and cue table

### 2.1 Effects

`F1908` (`ffe.asm:132732`) checks `DATA_009028`, `DATA_007902`, `DATA_007900`,
then the per-sample switches, then calls `_SoundPlaySample(code, level << 9,
-1)`. `F1906` passes 63; `F1907` (`132705`) halves the log volume by two bits
and shifts 63 right. The cue table keeps the original indices.

| Cue | Sample | Original call sites | Wired in the port? |
|---|---|---|---|
| `UiBeep` | `0x14` bip | `F150` 15092, `F209` 17217, `F216` 18025, `F361` 27706, and the `F1056`/`F1059` families (78237, 78611, ...) | **yes** – prefs pages (open/close/change), station screens, console buttons |
| `UiBeep2` | `0x15` bip | `F380` 28315, `F983` 73162, `F1059`..`F1065` 78897/78982/79063/79151 | **yes** – locking an NPC ship |
| `Message` | `0x16` bong | `F351` 27171 | **yes** – every new console-log entry (`F271` point) |
| `Laser` | `0x0a` laser1 / `0x0b` laser2 | `F930` 69472 / 69512 | **yes** – successful `combat.fire`; bank `(DATA_008836 & 0x78) >> 3` is the mount, 0..3 → `0x0a` |
| `ShieldHit` | `0x1c` missexpl | `F953` 71271 | **yes** – `Damage::Shields` |
| `HullDamage` | `0x1d` damage | `F953` 71294/71327 | **yes** – `Damage::Hull`, body hit and landing crash |
| `ShipExplosion` | `0x05` shipexp | `F942` 70713 (also `F937` 70086 → `0x1b`) | **yes** – `Damage::Destroyed` |
| `LargeExplosion` | `0x1f` boom | `F942` 70761, mass word `+0x6 >= 0xa` | **yes** – destroyed ship whose `ShipStats::mass >= 10` |
| `Siren` | `0x1e` siren | `F750` 49579, `F941` 70530 | **yes** – first hit of a fight (`start_battle`) |
| `Docking` | `0x13` airopen | `F1019` 75253, `F1024` 75757, `F1029` 76037, `F1088` 79847 | **yes** – auto-dock, `Traffic::Docked`, landing |
| `Door` | `0x13` airopen | same family; `F1026` 75989 uses `0x12` | **yes** – traffic "opening/closing the door" stages, launch-out |
| `Launch` | `0x20` launch | `F1098` 80465, `F941` 70557 (also `F291` 22850) | **yes** – `Traffic::LaunchDone` at the end of an undock |
| `Hyperspace` | `0x21` hype | `F908` 67583 | **yes** – successful jump |
| `Camera` | `0x27` camera | `F1125` 82748 | **yes** – view change (F1/F6/F7/F8/F9 and console view buttons) |
| `Engine` | `0x0f` station | `F1092` 80089 (linvol 8) | **yes, one-shot** – `Traffic::LaunchDone`; the continuous envelope is the mixer's job |
| `Station` | `0x10` station | `F1910` 76992 and `F1042`/`F1043`/`F1046` | **yes, one-shot** – `Traffic::Docked` |
| `Ecm` | `0x19` ecm | `F935` 70015 | API + test only; the port has no ECM control |
| `NavalEcm` | `0x1a` necm | `F935` 70001 | API + test only |
| `MissileLaunch` | `0x20` launch | `F291` 22850 | API + test only; the port has no missile firing |
| `MissileExplosion` | `0x06` missexpl | `F719` 47608, `F720` 47648 | API + test only |
| `Wind` | `0x0e` wind | `F1014` 75054 | API + test only; the port has no atmosphere-entry state |

The original's `0x26` (`nowt`) event markers are deliberately **not** a cue:
they are silent placeholders in the sample table, so no "missing sample" is
reported.

### 2.2 Music

Only `F1902` starts a song. It requires `player+0x3f5d` ("Music on"),
`DATA_007901` and `DATA_007900`; an explicit index must be `< 0x11`, enabled in
`player+0x3f66[]` and different from `player+0x5334`; a negative index chooses a
random enabled index from 0..=15, skipping the current one (ten attempts). The
port keeps those rules in `ffe_audio::AudioEngine::play_song`; `GameAudio` only
decides **when** to call it.

| Scene | Original | Port call |
|---|---|---|
| Front end | `F919` 68420/68429 play song 0, replay when `F1904` says done | `front_end_music()` + `tick` replay in `run_intro`; `leave_front_end()` on the start key |
| Battle | `F750` 49585 / `F236` 18909 play song 14 if `DATA_009035 & DATA_009029`; otherwise stop (`49589`/`18913`) | `start_battle()` on the first ship hit |
| Combat end / death | `F770` 50460 stops | `end_battle()` when the last pirate dies |
| Hyperspace | `F656` 44749 plays song 15 if `DATA_009033` | `hyperspace_music()` after a successful jump |
| Launch sequence end | `F1094` 80278 plays `-2` (random) if `player+0x3f5f` | `sequence_end_music()`; a station dock/landing is F1903's stop (`stop_song`), not this |
| Docking/launch transition | `F1048` 77305, `F1091` 80056 stop | `stop_for_sequence()` on `begin_launch`/`begin_docking` |
| Continuous music | `F232` 18597 checks `F1904` and plays `-1` | `tick` → `update_music()` |
| Leaving the front end | `F919` 68484 stops | `leave_front_end()` |

`AudioEngine::play_song` stops the previous song before starting an explicit
one; the original's DirectMusic path could race (documented deviation in
`docs/notes/sound.md` §2.3). Loop mode stays `Once`, matching the HMP event
timeline and the absence of CC110/111 markers; the front end's loop is an
explicit replay on `song_done`, not `LoopMode::Loop`.

---

## 3. Preference page integration

`Prefs::sound[0..10]` is the original's `DATA_004965` order:

| Index | Label | Target |
|---:|---|---|
| 0 | Sound effects on | `AudioSettings::effects_on` |
| 1 | Music on | `AudioSettings::music_on` |
| 2 | Engine sound | `SoundFlags::engine_sound` |
| 3 | Music after hyperspace | `SoundFlags::music_after_hyperspace` |
| 4 | Space station | `SoundFlags::space_station` |
| 5 | Docking music | `SoundFlags::docking_music` |
| 6 | Wind noise | `SoundFlags::wind_noise` |
| 7 | Music in battles | `SoundFlags::music_in_battles` |
| 8 | Icon beeps | `SoundFlags::icon_beeps` |
| 9 | Continuous music | `SoundFlags::continuous_music` |

All ten rows now toggle and are drawn as applied (the grey footer is gone).
`main` calls `sound.apply_prefs(&prefs)` at start-up and after any page change;
turning "Sound effects on" off calls `set_settings`, which stops all samples,
and turning "Music on" off stops the song. Turning "Continuous music" on
chooses a song (`F216` action `0x8a`). The "Choose Music From" table itself is
still text-only (the original page's music list has no hot areas in this port's
prefs implementation); `SoundFlags::songs` is preserved across `apply_prefs`.

The port-extension volumes (`AudioSettings::music_volume`/`effects_volume`) stay
at 100 because the original page has no volume sliders. `GameAudio::set_volumes`
sets them for a future page; the effect volume scales an original level through
`AudioSettings::effect_level` (and the integration test checks 50% → 31).

---

## 4. Tests and manual check

Automated:

- `crates/ffe-rs/src/sound.rs` unit tests: every cue's table index, the
  `laser1`/`laser2` bank split, the four per-sample preference gates, the
  `F1907` log-volume table, battle/hyperspace/sequence-end music, the F919 replay and
  F232 continuous selection, and the ten preference rows.
- `crates/ffe-rs/tests/sound_cues.rs`: a headless `AudioEngine` with
  `RecordingSampleSink` (no MIDI device, no `MUSIC/`) records a combat/launch
  cue sequence and checks the codes, default level and pitch; it also checks
  the effects mute and the 50% volume scale.
- `crates/ffe-rs/tests/sound_mixer.rs`: the headless `MixerSampleSink` renders
  the waveform: a cue produces its decoded PCM (not just a record), the mute
  stops active voices and suppresses new ones, and 50% effects volume produces
  `level_gain(31)` of full scale.
- `crates/ffe-audio` unit tests: WAV decode/validation, one-shot/loop counts,
  the `0x0f`/`0x10` repeat rules, the laser1 steal, and the exact centibel
  values. `crates/ffe-content` tests cover the v28 `music_hmp` asset.
- `crates/ffe-rs/tests/prefs.rs` and `src/prefs.rs`: the Sound page rows apply
  and toggle.

Manual check with a device (Windows):

1. Put a `data.bin` built from a game folder and its `MUSIC/` under one root
   (`ffe-tools build-data --game <game> --out data.bin` writes `data.bin` there;
   the 17 HMPs stay in `<game>/MUSIC`, although v28 also caches QQFRONT).
2. `cargo run -p ffe-rs -- --root <game>`.
3. The intro should play song 0 through the system MIDI synth and restart it
   after its 117.5 s event timeline ends; the start key stops it.
4. Launch from the station (`L`). F1048 stops any song; F1098's launch cue
   follows at the end of the lift sequence, and F1094's random sequence-end
   song is only a launch-end choice. The system MIDI device should play the
   song. On a machine with no MIDI port the engine falls back to silence and
   the game is unaffected.
5. Fire (`Space`) at an NPC: `0x0a` `laser1` should be audible; the first hit
   plays `0x1e` and starts song 14 when "Music in battles" is on, and a kill
   plays `0x05` plus `0x1f` for a mass of 10 t or more.
6. Hyperspace from the galaxy map: `0x21`, then song 15 with "Music after
   hyperspace" on.
7. Escape → Sound Effects: toggling "Music on" stops the music immediately;
   "Sound effects on" stops all 16 voices and mutes new cues; the other switches
   take effect on the next cue.
8. If no sound is heard, check the Windows default wave device and that
   `waveOutOpen` succeeded (the engine falls back headless silently). The
   `logErr` path is not exposed; add a device picker if this proves common.

`--capture*` and `--smoke-frames` run `GameAudio::silent`, so they never open a
device and their output is unchanged.

---

## 5. What is missing

1. **Continuous beds.** `Engine`, `Station` and `Wind` are one-shot calls. The
   mixer implements `station`'s 50-copy loop and `0x10`'s volume/pitch update,
   but the flight state does not yet call `F1910`/`F1908`'s envelope, so the
   engine/station/wind beds are short bursts rather than a sustained bed. This
   is the next audible gap.
2. **Unreachable cues.** ECM/naval ECM, missile launch/explosion and wind have
   mapping/tests but no game event yet: the port has no ECM control, missile
   firing or atmosphere state. Player death (`F770`, `F941`) is not modelled;
   the only current `F770`-equivalent is the last pirate's death.
3. **`.mid` override.** The port reads `MUSIC/*.HMP` (or the v28 cached song 0).
   JJFFE prefers a `.mid` next to the HMP; that is still the documented
   difference.
4. **Volume UI.** The two port-extension volumes are API-only; the original
   page has no sliders.
5. **Positional audio.** `F1759` exists but is not called by the DOS game. The
   mixer is mono and has no pan/distance attenuation; add it only if a caller
   needs it.
6. **Other 16 songs in the cache.** Only QQFRONT is cached. The runtime `MUSIC/`
   path still supplies the rest; caching all 17 would raise v28 to 1813 assets
   and remove the folder dependency.

No dashboard, `docs/PORT_LOG.md`, `docs/STATUS.md` or
`docs/notes/agent-streams.md` edits were made by this stream.
