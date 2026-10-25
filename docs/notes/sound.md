# Sounds and music

Status: reference note for the `sound` stream. It documents the original
DOS/JJFFE sound effects and music, the HMP format, the player in
`ffe-core`/`ffe-audio`, the 16-voice effect mixer, and the exact interface the
ESC/preferences page uses. The 23 cached effect WAVs are now decoded and mixed;
the intro's song 0 (`QQFRONT`) starts with F919 and loops until the start key.

**Cue wiring update:** `crates/ffe-rs/src/sound.rs` posts the original's
effect cues and scene music from the game events, and the ESC Sound Effects
page drives `AudioSettings`/`SoundFlags`. The sample table, call sites, music
rules, tests and the remaining mixer work are recorded in
`docs/notes/sound-cues.md`. Effects are audible through WinMM `waveOut`; music is
audible through the WinMM MIDI player.

Sources read for this note:

- `jjffe-master/ffe.asm` (routine bodies and call sites; line numbers below are
  from that file).
- `jjffe-master/ffedat.asm` (sample/song menus, sound globals).
- `jjffe-master/win32/w32sound.cpp` (sample table, DirectSound/DirectMusic/MCI
  paths, volume conversion).
- `jjffe-master/win32/w32dir.cpp`, `ffewin.cfg` (sample/song directories).
- `jjffe-master/hmp.cpp` and FFED3D's copy (HMP -> SMF converter).
- `game/FX/*.RAW`, `game/MUSIC/*.HMP` (the player's files; not committed).
- `docs/INVENTORY.md`, `docs/GAME_FILES.md`, `docs/FORMATS.md`, and
  `CLAUDE.MD` §5 "Audio decision".

JJFFE differences from the DOS release are called out explicitly. The DOS
release's HMI driver (`HMIDRV.386`, `HMIMDRV.386`, `HMIDET.386`,
`SETUP.INI`, `HMISET.CFG`, `DRUM.BNK`, `MELODIC.BNK`) is not present in this
tree, so the DOS driver's own mixing/timbre cannot be executed here.

---

## 1. Sound effects

### 1.1 Files and format

`FX/` contains 23 `*.RAW` files plus the separate root `TEST.RAW`. Format:
headerless signed 16-bit little-endian mono PCM at 22,050 Hz. `NOWT.RAW` is
0 bytes and deliberately silent. The cache converts every `FX/*.RAW` losslessly
to canonical 44-byte-header WAV at `audio/fx/<name>.wav` (`AssetKind::EffectWav`).

Two files are present but not referenced by the 40-entry sample table below:
`CLUNKIT.RAW` (3,470 B) and `ROAR.RAW` (1,488 B). `TEST.RAW` is also not part of
the table. They are kept because the original installation ships them; they are
not used by this build.

### 1.2 The 40 sample slots

This is `pSoundSampleDescs` in `win32/w32sound.cpp`. `loop` is a repeat count
baked into the DirectSound buffer (`LoadSample` copies the data `loop` times);
it is not an infinite loop. `pitchmod` is added to the requested frequency after
the 22,050 Hz default. `voice` is a fixed DirectSound channel; `-1` means the
generic allocator chooses. The game's table is shared by DOS and JJFFE; the
wrapper file is the clearest transcription.

| Index | File | Loop | Pitch mod | Voice | Used by |
|---:|---|---:|---:|---:|---|
| 0x00-0x04 | `nowt` | 1 | 0 | - | silent placeholders |
| 0x05 | `shipexp` | 1 | 0 | - | ship destruction |
| 0x06 | `missexpl` | 1 | 0 | - | missile/mine explosion |
| 0x07-0x09 | `nowt` | 1 | 0 | - | silent placeholders |
| 0x0a | `laser1` | 1 | 0 | - | laser bank 1 |
| 0x0b | `laser2` | 1 | 0 | - | laser bank 2 |
| 0x0c | `airopen` | 1 | 0 | - | airlock/door/collision |
| 0x0d | `airopen` | 1 | 0 | - | airlock/door |
| 0x0e | `wind` | 1 | -0x4000 | 0 | atmosphere wind |
| 0x0f | `station` | 50 | 0 | 1 | engine noise (continuous) |
| 0x10 | `station` | 50 | +0x1000 | 2 | space-station ambience |
| 0x11 | `nowt` | 1 | 0 | - | silent placeholder |
| 0x12 | `airopen` | 1 | 0 | - | airlock/door |
| 0x13 | `airopen` | 1 | 0 | - | airlock/docking |
| 0x14 | `bip` | 1 | 0 | - | icon/UI beep |
| 0x15 | `bip` | 1 | 0 | - | icon/UI beep |
| 0x16 | `bong` | 1 | 0 | - | message/notification |
| 0x17 | `chime` | 1 | 0 | - | not called by this build |
| 0x18 | `nowt` | 1 | 0 | - | silent placeholder |
| 0x19 | `ecm` | 2 | 0 | 3 | ECM |
| 0x1a | `necm` | 2 | 0 | 4 | naval ECM |
| 0x1b | `shipexp` | 1 | 0 | - | ship destruction |
| 0x1c | `missexpl` | 1 | 0 | - | missile explosion |
| 0x1d | `damage` | 1 | 0 | - | hull/damage |
| 0x1e | `siren` | 3 | 0 | 5 | battle alert/player hit |
| 0x1f | `boom` | 1 | 0 | 6 | large explosion |
| 0x20 | `launch` | 1 | 0 | - | launch/escape pod |
| 0x21 | `hype` | 1 | 0 | - | hyperspace |
| 0x22 | `nowt` | 1 | 0 | - | silent placeholder |
| 0x23 | `laser3` | 1 | 0 | - | not called by this build |
| 0x24 | `laser4` | 1 | 0 | - | not called by this build |
| 0x25 | `wind` | 1 | -0x4000 | 7 | atmosphere wind (second voice) |
| 0x26 | `nowt` | 1 | 0 | - | silent placeholder |
| 0x27 | `camera` | 1 | 0 | - | view change |

`w32sound.cpp` has 16 mixing channels (`NUMCHANNELS`). A sample with a fixed
`voice` reuses that channel; index 16 is special-cased so a repeat call only
changes volume/pitch while it is playing. Otherwise a new call restarts the
sample at the first free channel. `CLUNKIT`, `ROAR`, `chime`, `laser3` and
`laser4` have no call site in `ffe.asm`, so they are table entries for the
original data set, not active triggers in this build.

### 1.3 Volume and pitch semantics

| Routine | Meaning |
|---|---|
| `F1906_SoundPlaySample(index)` (line 132685) | Plays at the default linear level 63. Calls `F1908`. |
| `F1907_SoundPlaySampleLogVol(index, logvol)` (line 132705) | Clamps `logvol` to >= 0, shifts right by 2, then `level = 0x3f >> shift`; calls `F1908`. So 0 -> 63, 4 -> 31, 8 -> 15, 12 -> 7, 16 -> 3, 20 -> 1, >= 24 -> 0. |
| `F1908_SoundPlaySampleLinVol(index, level)` (line 132732) | Gates on `DATA_009028` (effects), `DATA_007902` (digital available) and `DATA_007900` (all sound). Applies the per-sample switches below. Converts `level << 9` and caps at 0x7fff before calling the platform `SoundPlaySample(index, volume, pitch)`. `pitch` = -1 means the 22,050 Hz default. |
| `F1909_SoundStopAllSamples` (line 132816) | Stops all 16 channels. |
| `F1910_SoundModifyStationNoise` (line 132831) | Re-sends sample 0x10 with `pitch + 0x10000` and the supplied volume/pitch pair; this is the continuous station-noise update. |

`F1908` switches, in order:

- `0x14` (`bip`) requires `DATA_009036` ("Icon beeps").
- `0x10` (`station`, voice 2) requires `DATA_009030` ("Space station").
- `0x0f` (`station`, voice 1) requires `DATA_009032` ("Engine sound").
- `0x0e`/`0x25` (`wind`) require `DATA_009034` ("Wind noise"). They additionally
  randomise a second wind: index 14 or 37 from `random(4)`, volume
  `random(0x4000)`, pitch `random(0x4000)+0x6000`, before the normal wind call.
- All other effects require only the master effects/sound flags.

`w32sound.cpp::SoundPlaySample` converts the linear level to DirectSound
centibels with `2217 * log10(level)`, rounded to 16, minus 10000. That is the
original's logarithmic loudness curve; the port should preserve the relative
levels, not the exact DirectSound number.

### 1.4 Trigger sites in `ffe.asm`

Line references are the call instruction. "Default" means `F1906` (level 63);
"log" means `F1907` with an object's `+0x88` volume byte.

| Routine (line) | Call | Game situation |
|---|---|---|
| `F150` (15092) | `0x14` bip | GUI button/hot-area click in the first menu page. |
| `F209` (17217) | `0x14` bip | SysMenu accelerator click (event 3). |
| `F216` (18025) | `0x14` bip | Any SysMenu action (event 4) acknowledgement. |
| `F236` (18894) | `0x26` nowt | Mission/combat event; silent in this table. Then song 14 if `player+0x3f63` (music in battles). |
| `F291` (22850) | `0x20` launch | Missile/object launch path. |
| `F351` (27171) | `0x16` bong | Message/notification on an object event. |
| `F361` (27706) | `0x14` bip | Row/selection change. |
| `F372` (28080) | `0x26` nowt | Target/selection change; silent placeholder. |
| `F379` (28315) | `0x26` nowt | Object state change; silent placeholder. |
| `F380` (28329) | `0x15` bip | Selection change. |
| `F714` (47315) | `0x11` nowt | Collision with object type 0x99/0x6a; silent placeholder. |
| `F719` (47608) | `0x06` missexpl, log | Missile/mine explosion. |
| `F720` (47648) | `0x06` missexpl, log | Missile/mine explosion (destroyed object). |
| `F745` (49201) | `0x0c` airopen, log | Player collision with an object flagged `+0x14c & 0x10` (airlock/door). |
| `F750` (49579) | `0x1e` siren | Battle start/alert. |
| `F750` (49585) | song 14 | Battle music, if `DATA_009035` and `DATA_009029`; otherwise `F750` stops the song (49589). |
| `F770` (50460) | stop song | Player death/combat end. |
| `F908` (67583) | `0x21` hype | Hyperspace jump. |
| `F919` (68420, 68429) | song 0 | Front end/main menu; replays when `F1904` says done (68425). Stops on leaving (68484). |
| `F930` (69472) | `0x0a` laser1, log | Player laser, bank index `< 4`. |
| `F930` (69512) | `0x0b` laser2, log | Player laser, bank index `>= 4`. |
| `F935` (70001) | `0x1a` necm, log | Naval ECM activation. |
| `F935` (70015) | `0x19` ecm, log | ECM activation. |
| `F937` (70086) | `0x1b` shipexp, log | Ship destroyed by energy damage. |
| `F941` (70530) | `0x1e` siren | Player ship hit/explosion setup. |
| `F941` (70557) | `0x20` launch, log | Explosion/escape launch. |
| `F942` (70713) | `0x05` shipexp, log | Ship damage/destruction. |
| `F942` (70761) | `0x1f` boom, log | Large explosion. |
| `F953` (71271) | `0x1c` missexpl, log | Missile impact absorbed by shields. |
| `F953` (71294, 71327) | `0x1d` damage, log | Hull/energy damage. |
| `F983` (73162) | `0x15` bip | Cursor/proximity icon change. |
| `F1014` (75054) | `0x0e` wind | Atmospheric flight, when `player+0x3f62` and `+0x3f5c` are set. |
| `F1018` (75166) | `0x14` bip | Leave station/launch transition. |
| `F1019` (75253) | `0x13` airopen | Docking/airlock approach. |
| `F1020` (75359) | `0x14` bip | Docking/launch computer state. |
| `F1024` (75757) | `0x13` airopen | Docking/airlock approach. |
| `F1025` (75848) | `0x22` nowt | Docking transition; silent placeholder. |
| `F1026` (75989) | `0x12` airopen | Door/airlock. |
| `F1029` (76037) | `0x13` airopen | Door/airlock. |
| `F1033` (76245) | `0x11` nowt | Docking message/state; silent placeholder. |
| `F1042` (76992), `F1043` (77145), `F1046` (77257) | `F1910` | Continuous station-noise volume/pitch updates. |
| `F1048` (77305) | stop song | Docking/launch transition. |
| `F1056` (78237, 78247, 78257, 78268, 78275, 78285, 78317, 78463, 78534, 78615, 78687, 78709, 78728) | `0x14` bip | Station/menu screen navigation and button presses. |
| `F1056` (78611) | `0x0c` airopen | Station door/launch. |
| `F1059` (78897), `F1061` (78982), `F1063` (79063), `F1065` (79151) | `0x15` bip | Selection/icon changes. |
| `F1088` (79844, 79847) | `0x22` nowt + `0x13` airopen | Docking/landing transition. |
| `F1092` (80089) | `0x0f` station, linvol 8 | Engine noise, when `player+0x3f60` is set. |
| `F1094` (80278) | song -2 (random) | Docking/launch sequence, when `player+0x3f5d` and `+0x3f5f`. |
| `F1098` (80465) | `0x20` launch | Undock/launch. |
| `F1104` (81475, 81518) | `0x1f` boom | Atmospheric entry/exit explosion conditions. |
| `F1125` (82748) | `0x27` camera | View/camera change. |
| `F1759` (109664) | `F1908` dynamic | Generic spatial-volume helper: volume from the sum of the vector's absolute components, sample index from the caller. Not called from `ffe.asm`; a platform/other module entry point. |
| `F1900` (132543) | init | Clears the current song, calls `SoundCheckInit` with `DATA_007900/7901/7902`. |
| `F1902` (132560) | play song | Explicit index or random selection (section 2). |
| `F1903` (132626) | stop song | Stop and clear current song. |
| `F1904` (132643) | done | Returns done, clears current song. |
| `F528` (90894) | stop song | System cleanup. |

The cache contains all 23 WAVs and the mixer decodes them. The events that need
wiring are therefore the rows above with a non-`nowt` sample:
weapons (`F930`, `F935`, `F719`, `F720`, `F941`, `F942`, `F953`), collisions
(`F745`, `F953`), docking/launch (`F1018`-`F1098`), station/engine ambience
(`F1042`/`F1043`/`F1046`, `F1092`), wind (`F1014`), hyperspace (`F908`), camera
(`F1125`) and the UI/notification family (`F150`, `F209`, `F216`, `F351`,
`F361`, `F380`, `F983`, `F1056`, `F1059`-`F1065`). `nowt` rows should stay
no-ops, not "missing sample" errors.

### 1.5 The port mixer (`ffe_audio::mixer`)

`Mixer` is the software equivalent of the DirectSound channel array. It is
pure (no device) and is driven by the effect sink: `MixerSampleSink` renders
256-frame blocks on a WinMM `waveOut` thread, or exposes `mix_into` headlessly in
tests. The sample set is decoded from the cache's `base:effect/<name>` WAVs at
startup (`GameAudio::from_cache`); a v27 cache still has all 23.

| Rule | Implementation |
|---|---|
| Voices | 16 (`NUMCHANNELS`). |
| Same sample already playing | Reuse its voice. If the descriptor has a fixed `voice` (`>= 0`) the repeat is ignored, except index `0x10`, which only updates volume/pitch (the station bed's `F1910`). |
| Generic sample already playing | A second call allocates another voice, as `SoundPlaySample` does. |
| No free voice | Steal a `laser1` (`0x0b`) voice; with none, drop the new sample. This is the original's explicit choice, not oldest-voice eviction. |
| Loop count | `LoadSample` copies the PCM `loopcount` times (`0` means once), so `station` is 50 copies and `ecm`/`necm` 2, `siren` 3. |
| Pitch | `pitch == -1` is 22,050 Hz; add the descriptor's `pitchmod` (wind is -0x4000, station 0x10 is +0x1000), clamp to DirectSound's 100..=200,000 Hz and resample by that ratio. |
| Volume | `F1908` passes `level << 9` (capped 0x7fff). `SoundPlaySample` computes `(long)(2217 * log10(volume))`, rounds up to a multiple of 16 and subtracts 10,000 centibels. `level_centibels`/`level_gain` reproduce that exactly; level 63 is 0 centibels, level 31 is -688, level 0 is silence. |
| Mixing | Sum the 16 voices in floating point, then clamp to `i16`. The original leaves this to DirectSound; the port's hard cut at the sample end is the same behaviour. |
| Preferences | `AudioEngine` applies `effects_on` before the sink and `effects_volume` through `settings.effect_level` (the original level is scaled before the curve). `set_settings` stops all voices when effects are switched off. |

The mixer does not implement positional audio. The original's `F1759` spatial
helper is present in `ffe.asm` but not called by the DOS game; the port has no
per-voice pan or distance attenuation. The fixed-voice markers are honoured as
the source's repeat rules, not as a forced channel allocation: `w32sound.cpp`
allocates the first free DirectSound channel and uses `fixednum` only to suppress
a repeat.

---

## 2. Music

### 2.1 The 17 songs

`ppSoundSongNames` (`w32sound.cpp`) and `DATA_004965` (the menu titles). The HMP
files are not in `data.bin`; they live in the player's `MUSIC/` directory.
Durations are the parsed event timeline at the original fixed tempo; the
"advertised" column is the header dword at `0x3c` (section 3). Two songs
disagree; see the note below.

| Index | File | Title | Advertised | Parsed | Direct start |
|---:|---|---|---:|---:|---|
| 0 | `QQFRONT.HMP` | First Encounters Theme - Quality Quartet | 223 s | 117.5 s | `F919` front end |
| 1 | `QQATMOSP.HMP` | Atmosphere - Quality Quartet | 222 s | 223.2 s | random pool |
| 2 | `QQTRAVEL.HMP` | Travel - Quality Quartet | 149 s | 148.3 s | random pool |
| 3 | `QQDRAMA.HMP` | Drama - Quality Quartet | 200 s | 200.9 s | random pool |
| 4 | `QQSUSPEN.HMP` | Suspense - Quality Quartet | 130 s | 163.8 s | random pool |
| 5 | `QQROCK.HMP` | Rock - Quality Quartet | 140 s | 141.3 s | random pool |
| 6 | `QQESCAPE.HMP` | Escape - Quality Quartet | 199 s | 200.3 s | random pool |
| 7 | `QQPARADI.HMP` | Paradise - Quality Quartet | 148 s | 149.0 s | `F306` page/scroll |
| 8 | `QQFRONT2.HMP` | Alien Encounters - Quality Quartet | 87 s | 88.0 s | random pool |
| 9 | `HALLKING.HMP` | In the Hall of the Mountain King - Grieg | 122 s | 123.2 s | random pool |
| 10 | `INTRO.HMP` | Frontier Main Theme - Lowe | 190 s | 191.6 s | `F307` message/video |
| 11 | `BABAYAGA.HMP` | Baba Yaga - Mussorgsky | 57 s | 57.8 s | random pool |
| 12 | `BAREM.HMP` | Night on the Bare Mountain - Mussorgsky | 156 s | 157.2 s | random pool |
| 13 | `JUPALT.HMP` | Frontier Second Theme - Lowe | 166 s | 166.8 s | random pool |
| 14 | `VALKRIES.HMP` | Ride of the Valkyries - Wagner | 157 s | 158.0 s | `F750` battle |
| 15 | `BLUEDAN.HMP` | Blue Danube - Strauss | 103 s | 103.9 s | `F656` after hyperspace |
| 16 | `GGOFKIEV.HMP` | Great Gates of Kiev - Mussorgsky | 66 s | 67.2 s | never chosen by `F1902(-1)` |

`F1902(-1)` (and `F1094`'s `-2`) calls `FUNC_000922(0x10)`, which returns
`0..=15`; index 16 is therefore in the table and the "Choose Music From" menu,
but the random selector never picks it. A caller can still pass 16 explicitly.

Only indices 0, 7, 10, 14 and 15 have explicit scene starts in `ffe.asm`. The
other Quality Quartet tracks are reached through the random selector (or through
a save/song-enable combination); the code does not attach them to individual
scene names, so their titles describe the music rather than a hard-coded screen.

The `QQFRONT`/`QQSUSPEN` discrepancies: every other file's parsed duration is
within ~1% of the header value. `QQFRONT` is 0.53x and `QQSUSPEN` is 1.26x.
There are no CC110/111 loop markers in either file; `QQFRONT` may be authored as
an intro plus a repeated theme and the header may include the repeat. The port
uses the event timeline (what the player actually sends), not the advisory
header.

### 2.2 Start/stop/update routines

- `F1902_SoundPlaySong` (line 132560, `FUNC_001902`): the only routine that starts
  a song. It requires `player+0x3f5d` ("music on"), `DATA_007901` (music
  available) and `DATA_007900` (all sound). An explicit index must be `< 0x11`
  (17), must be enabled in `player+0x3f66[index]`, and must differ from the
  current song `player+0x5334`. Negative indices choose a random enabled song:
  ten attempts over 0..15, skipping the current song. It then calls the platform
  `SoundPlaySong(index)` and stores `player+0x5334`.
- `F1903_SoundStopSong` (line 132626, `FUNC_001903`): if music is available and
  `DATA_009116 >= 0`, calls `SoundStopSong` and sets `DATA_009116 = -1`.
- `F1904_SoundSongDone` (line 132643, `FUNC_001904`): returns 1 immediately when
  `DATA_009116 == -1`, or when `DATA_007900`/`DATA_007901` are off. Otherwise
  asks the platform `SoundSongDone`; if it is 1, clears the current song.
- `F232_SysMenuUpdateSong_Event10` (line 18585): if continuous music
  (`DATA_009037`) is on, the current song is `-1` or done, it calls
  `F1902(-1)`. This is the "continuous music" event, not a per-frame loop.
- `F919` (line 68164): the front end explicitly plays song 0, then plays it
  again whenever `F1904` reports done (lines 68425-68429). This is why the main
  menu theme loops even when "Continuous music" is off.
- `F216` (line 17549): the SysMenu action handler. Toggling "Continuous music"
  off (action `0x8a`, line 17864) stops the song if it is playing; toggling it
  on plays `F1902(-1)` (line 17871). Toggling "Music on" (action `0x81`, line
  17858) stops the song when turned off.
- `F215_SysMenuDisableSong` (line 17530): toggles
  `DATA_009038[songindex]` and stops the current song if it was that song.
- `F306` (line 24459) starts song 7 (`qqparadi`) when a page/row advances
  (line 24542). `F307` (line 24584) starts song 10 (`intro`) once per message
  bit (`DATA_009091`), line 24622; this is the message/video sequence path.
- `F656` (line 44730) starts song 15 (`bluedan`) when the hyperspace event
  string is `0x98ba`, `DATA_009033` (music after hyperspace) is set, and the
  current song is not already 15 (line 44749).
- `F750` (line 49480) starts song 14 (`valkries`) at battle start when
  `DATA_009035` and `DATA_009029` are set (line 49585); otherwise it stops the
  song (line 49589).
- `F236` (line 18761) also starts song 14 for its combat event if
  `player+0x3f63` is set (line 18909), otherwise stops the song (line 18913).
- `F1094` (line 80162) starts a random song (`-2`) at the end of a docking/launch
  sequence when `player+0x3f5d` and `player+0x3f5f` (docking music) are set
  (line 80278).
- Stops: `F185` load-commander (16282), `F215` (17539), `F216` (17862/17867),
  `F236` (18913), `F750` (49589), `F770` player death (50460), `F917`
  restart/quit path (68056), `F919` cleanup (68195/68422/68484), `F1048`
  docking/launch (77305), `F1091` docking (80056), `F528` cleanup (90894).

### 2.3 Loop and completion semantics

The original does not loop inside a song. `F1904` reports done when the platform
player finishes; the caller decides what happens next:

- Front end (`F919`): play song 0 again.
- "Continuous music" (`F232`): choose another random song.
- Everywhere else: silence until the scene starts another cue.

`hmp.cpp`/DirectMusic play the segment once (`PlaySegment` with
`DMUS_SEGF_DEFAULT`, no loop flags). The HMP files contain no CC110/111 loop
markers (checked all 17), so there is no hidden loop point to reproduce. The
port's `LoopMode::Once` is therefore the game-faithful default; `LoopMode::Loop`
exists only for a caller that explicitly wants a loop (tests, front end).

`F1902` does not stop a still-playing song before starting an explicit new one;
the DirectMusic path's `DMPlaySong` refuses to start while the previous segment
is playing, but `F1902` still updates the current index. The MCI path stops
first. The port stops the previous song before starting the new one; that is a
deliberate, documented deviation that removes the DirectMusic race.

### 2.4 Settings and globals

Master availability (set by `F1900`/`SoundCheckInit`, not user menu flags):

| Global | Meaning |
|---|---|
| `DATA_007900` | All sound available (`pAll`). |
| `DATA_007901` | Music available (`pMidi`; win32 always sets 1). |
| `DATA_007902` | Digital effects available (`pDigi` when DirectSound opened). |
| `DATA_009116` | Current song index; `-1` means none. |

User switches from the ESC "Sound Effects" page (`DATA_009028..009037`; the
action numbers are the ones in `F216`'s dispatch):

| Action | Menu label | Global | Effect |
|---:|---|---|---|
| 0x81 | Sound effects on | `DATA_009028` | Gates all `F1908` effects. Turning it off stops all samples. |
| 0x82 | Music on | `DATA_009029` | Music master; turning it off stops the song. |
| 0x83 | Space station | `DATA_009030` | Sample 0x10 station ambience. |
| 0x84 | Docking music | `DATA_009031` | Docking/launch music (`player+0x3f5f`). |
| 0x85 | Engine sound | `DATA_009032` | Sample 0x0f engine noise. |
| 0x86 | Music after hyperspace | `DATA_009033` | Song 15 after a jump. |
| 0x87 | Wind noise | `DATA_009034` | Samples 0x0e/0x25. |
| 0x88 | Music in battles | `DATA_009035` | Song 14 at battle start. |
| 0x89 | Icon beeps | `DATA_009036` | Sample 0x14 bip. |
| 0x8a | Continuous music | `DATA_009037` | `F232` starts another song when done. |

`DATA_009038[0..17]` is the "Choose Music From" table. `F215` toggles
`DATA_009038[songindex]`. `F1902` reads a copy at `player+0x3f66[index]`. The
player struct also has scene copies: `+0x3f5d` music on, `+0x3f5f` docking
music, `+0x3f60` engine sound, `+0x3f62` wind, `+0x3f63` battle music,
`+0x5334` current song. The DOS/JJFFE save state carries these; the port does
not need the exact struct layout, only the meanings.

The original has no user volume sliders. Music loudness is the device/system
volume; effects have fixed per-call levels. The port's `music_volume` and
`effects_volume` are explicit extensions (0..=100) so the preferences page can
persist them without changing the original relative levels.

### 2.5 Configuration

`ffewin.cfg`:

```ini
[DIR]
SamplePath=FX
SongPath=MUSIC
AVIPath=DATA
CmmdrPath=COMMANDR

[SOUND]
softsynth=0
forcemci=0
```

- `SongPath` is where the 17 HMPs (and optional `.mid` overrides) live. It must
  be writable in JJFFE because the converter writes a `.mid` next to the HMP.
- `SamplePath` is `FX`.
- `softsynth=1` forces the Microsoft soft synth in `DMInit`; `forcemci=1` skips
  DirectMusic and uses the MCI sequencer; `win2khack` is read by `SoundInit`
  but unused in this source.
- The DOS installer's `SETUP.INI`/`HMISET.CFG` select the HMI driver and the
  `.BNK` patch banks. Those files are installation material, not content for
  the port.
- `jjffe.cfg`'s `[TIMER] maxfps` is unrelated to audio.

---

## 3. The HMP format and the original player

### 3.1 Container

All 17 files start with `HMIMIDIP013195` (the 31 Jan 1995 HMP revision). The
old `HMIMIDIP` and the later `HMI-MIDISONG061595` are different formats;
`hmp.cpp` accepts only the `P` variant. Observed header (all 17 files agree on
the fixed fields):

| Offset | Size | Meaning |
|---:|---:|---|
| 0x00 | 14 | ASCII `HMIMIDIP013195`. |
| 0x20 | 4 | LE file-length-ish value (observed file size minus 8..15). Not used by `hmp.cpp`. |
| 0x30 | 1 | Track count, including the conductor (7..17 in the shipped set). `hmp.cpp` reads the byte. |
| 0x34 | 4 | 96 in every shipped file; the HMI "tempo/ticks" field. Ignored by `hmp.cpp`. |
| 0x38 | 4 | 120 in every shipped file; the HMI division. Ignored by `hmp.cpp` (it writes 192). |
| 0x3c | 4 | Advertised duration in seconds. Advisory; used only for diagnostics. |
| 0x40.. | | Instrument and track designation area (WHS's 4-byte-per-instrument block). `hmp.cpp` skips it. |

The first music chunk is found by scanning from `0x30` for `FF 2F` (the same
byte pair as an SMF end-of-track). `hmp.cpp` then advances seven bytes to the
chunk length.

### 3.2 Track chunks

Each chunk is:

```
FF 2F 00            marker (three bytes)
u32 LE chunk number
u32 LE chunk length  (includes the 12-byte chunk header)
u32 LE track number
data                 (chunk length - 12 bytes)
```

`hmp.cpp` reads the length, skips the length and track number (eight bytes),
converts `length - 12` data bytes, then skips the four bytes after the data.
The parser in `ffe_core::hmp` follows that exact layout (`chunk_position += size`
after each track). The first header count includes the conductor, so a file with
header count 15 has 14 data tracks; `hmp.cpp` synthesizes the missing one.

### 3.3 Delta and event encoding

HMP deltas are **least-significant group first**, and the **high bit marks the
last** byte (the opposite convention to Standard MIDI):

```
value 200 = 0x48 0x81   (0x48 + (1 << 7))
value 0   = 0x80
```

At most four bytes are valid. After the delta, every event has an explicit
status byte (there is no running status in the input):

- `0x8n`, `0x9n`, `0xAn`, `0xBn`, `0xEn`: status + two data bytes.
- `0xCn`, `0xDn`: status + one data byte.
- `0xFF`: meta type, then a **standard MIDI** variable-length length, then the
  payload. `FF 2F 00` ends the track. The shipped files contain only end-of-track
  meta events (156 of them; one per data track).
- `0xF0..0xF7` (SysEx) are not supported by `hmp.cpp` and are a parse error in
  the port. None of the shipped files use them.

### 3.4 The original conversion and timing

`hmp.cpp::load_hmp` writes a type-1 Standard MIDI File:

- `MThd`, format 1, `ntrks = HMP track count`, division `0xC0` (192).
- A synthesized conductor `MTrk` with tempo `0x188000` microseconds per quarter
  note (1,605,632 us) and an immediate end-of-track.
- One `MTrk` per HMP data track. Deltas are rewritten as standard MIDI VLQs,
  channel running status is suppressed when the status repeats, meta events are
  copied, and the track stops at `FF 2F`.
- The output is a valid SMF; the converter is deterministic.

The fixed tempo/division are the original playback grid: at 22,050 Hz the song
is not resampled, the event times are. Computed duration =
`ticks * 1,605,632 / (192 * 1,000,000)`. This matches the advertised duration
for 15 of 17 files (section 2.1).

The port reproduces this in `ffe_core::hmp`:

- `HmpSong::parse` accepts only `HMIMIDIP...P`, validates every chunk bound,
  decodes the LSB-first deltas, keeps absolute ticks and meta payloads, and
  keeps the EOT marker.
- `HmpSong::to_smf` emits the same type-1 file, including the conductor track
  and running-status suppression.
- `duration_seconds`, `duration_ticks`, `event_count`, `data_track_count`.

`docs/FORMATS.md` says 480 PPQN for the future music cache. That is
aspirational and conflicts with `hmp.cpp`; the port follows the source (192).
If a cache asset is added later, it should record 192 or resample consistently.

### 3.5 System MIDI path and driver selection

DOS original:

- The game's `F1902`/`F1903`/`F1904` call the platform `SoundPlaySong`,
  `SoundStopSong`, `SoundSongDone`. In this tree only the Win32, SDL and Linux
  wrappers are present; the DOS wrapper's HMI calls are not.
- The DOS installer selects the HMI driver (`HMIDRV.386`/`HMIMDRV.386`) and the
  `DRUM.BNK`/`MELODIC.BNK` patches. HMP is HMI's native event format, so the DOS
  path did not need to convert to SMF.

JJFFE Win32 (`win32/w32sound.cpp`):

- Default: DirectMusic performance/port. `DMInit` enumerates output ports and
  skips external and software-synth ports unless `softsynth` is set; `forcemci`
  bypasses DirectMusic entirely.
- `DMPlaySong` prefers a `.mid` file next to the `.hmp`; if absent it converts
  HMP to `.mid` in `SongPath` and plays that. The MCI path does the same
  conversion and uses `MCI_DEVTYPE_SEQUENCER`.
- `SoundCheckInit` sets `DATA_007901 = 1` (music always "available" on win32)
  and `DATA_007900`/`DATA_007902` from whether DirectSound opened.
- The fallback is WinMM MCI; the direct WinMM `midiOut*` API is not used by this
  wrapper, but it is the same system-synth destination.

FFED3D:

- Adds BASS streams and a custom music folder (`music/custom/...`), plus
  pre-rendered MP3. That is not original DOS behaviour and is excluded.

The port (`ffe-audio`):

- Parses HMP itself and does not write a `.mid` file.
- Uses WinMM `midiOutOpen`/`midiOutShortMsg`/`midiOutReset` on Windows. This is
  dependency-free, works with the Microsoft GS Wavetable Synth, and is the
  closest available equivalent of the original's system-MIDI destination. There
  is no DirectMusic/MCI/BASS layer.
- On non-Windows or when no device opens, the `NullMidiSink` is used. Tests and
  CI never require a device. The effect path is independent: `waveOut` when it
  opens, otherwise the same `Mixer` headless (silent but deterministic for tests).
- No `.mid` override: the port always reads `<SongPath>/<NAME>.HMP`. JJFFE's
  `.mid`-first preference is a documented difference; adding it later only needs
  an SMF reader in front of the same `MidiSink`.
- Song selection, the enable table, the current-song check, the 0..15 random
  range, the ten retries and the `F1904`-style done state are reproduced. The
  random generator is the player's own LCG (the game's random state is not part
  of the audio API); callers that need the original game RNG should pass an
  explicit index instead of `-1`.

### 3.6 What can and cannot be verified locally

Verified against the 17 shipped files (test output is in section 5):

- Signature, header track count, chunk boundaries, track count and event count.
- Delta/meta decoding and the absolute tick timeline.
- The fixed 192 division / 1,605,632 us tempo is consistent with the advertised
  duration for 15/17 files.
- SMF export headers/track counts and the conductor tempo bytes.
- Start/volume/stop/loop behaviour with a recording backend.

Cannot be verified here:

- The DOS HMI driver's actual timbre, patch bank selection, and whether it
  handled any HMI-specific loop/expression messages.
- DirectMusic port selection and MCI availability on a given Windows install.
- The exact General MIDI instrument mapping of the system synth versus the
  original HMI patches; the notes and timing match, the timbre will not be
  bit-identical.
- The QQFRONT/QQSUSPEN advertised-duration discrepancy (no loop markers).
- Any `MUSIC/*.MID` overrides a user may have; the port does not read them.

---

## 4. Interface for the ESC/preferences page

The preferences stream can depend on `crates/ffe-audio` and must not edit its
files. `crates/ffe-rs/src/audio.rs` re-exports the same API as `ffe_rs::audio`,
including `open_game_audio(game_dir)`, so a page inside `ffe-rs` needs no
additional dependency. The shared types are in `ffe_audio::settings`; the player
is in `ffe_audio::audio`/`player`.

### 4.1 Settings types

```rust
use ffe_audio::{AudioSettings, SoundFlags, SONG_COUNT};

#[derive(Clone)]
struct Preferences {
    audio: AudioSettings,
    sound: SoundFlags,
}

let audio = AudioSettings {
    music_on: true,
    music_volume: 70,   // 0..=100; port extension
    effects_on: true,
    effects_volume: 80, // 0..=100; port extension
};
let sound = SoundFlags {
    space_station: true,
    docking_music: true,
    engine_sound: true,
    music_after_hyperspace: true,
    wind_noise: true,
    music_in_battles: true,
    icon_beeps: true,
    continuous_music: false,
    songs: [true; SONG_COUNT], // "Choose Music From"
};
```

Both types derive `serde::Serialize`/`Deserialize` with defaults. Persist them in
the preferences file the other stream owns; `AudioSettings::clamped()` enforces
volumes `0..=100` after loading. `SoundFlags` maps element-for-element to the
original menu labels in section 2.4; the `songs` array maps to
`DATA_009038[0..17]` and the same order as `SONG_NAMES`.

### 4.2 Player API

```rust
use std::time::Duration;
use ffe_audio::{Audio, AudioEngine, AudioError, MusicLibrary};
// Inside `ffe-rs`, `use crate::audio::{...}` also works.

let library = MusicLibrary::from_game_dir(game_dir); // <game>/MUSIC
let mut audio: Box<dyn Audio> = Box::new(AudioEngine::system(library));
// In the game use `GameAudio::from_cache(&game_dir, &cache)`: it also opens the
// wave device and installs the cache's `base:effect/*` WAVs and intro song.
// Or: let mut audio = crate::audio::open_game_audio(game_dir);
audio.set_settings(settings);
audio.set_sound_flags(flags);

// Per frame, after advancing the game clock:
audio.tick(elapsed);

// Scene hooks:
audio.play_sample(0x14, 63, -1);          // icon beep, F1906 default level
audio.play_sample(0x1e, 63, -1);          // battle siren
audio.play_song(0)?;                      // front end (F919)
audio.play_song(10)?;                     // message/video (F307)
audio.play_song(-1)?;                     // F1902(-1) random selection
if audio.song_done() {
    audio.play_song(0)?;                  // F919 loop the front end
}
audio.stop_song();                        // F1903

// The F232 continuous-music hook (event 10), not every frame:
audio.update_song()?;
```

Exact trait (object-safe, so `Box<dyn Audio>` works):

```rust
pub trait Audio {
    fn play_song(&mut self, index: i16) -> Result<(), AudioError>;
    fn stop_song(&mut self);
    fn song_done(&self) -> bool;
    fn play_sample(&mut self, code: u8, volume: u8, pitch: i32);
    fn tick(&mut self, elapsed: Duration);
    fn settings(&self) -> AudioSettings;
    fn set_settings(&mut self, settings: AudioSettings);
    fn sound_flags(&self) -> SoundFlags;
    fn set_sound_flags(&mut self, flags: SoundFlags);
}
```

`play_song` semantics: `0..=16` is an explicit table index; a negative value is
the original random selection. If `music_on` is false, or the index is disabled
in `SoundFlags::songs`, the call is a no-op (matching `F1902`). A missing file
returns `AudioError::SongMissing`; the original only logs it, so the prefs/game
layer may choose to ignore that variant. `song_done()` is true when nothing is
playing (the `F1904` contract). `play_sample` takes the original sample-table
code, not a filename; it multiplies the supplied level by
`effects_volume / 100` and does nothing when `effects_on` is false.

Concrete constructors:

- `AudioEngine::system(library)`: Windows system MIDI and wave output when
  available, otherwise silent/headless. This is the normal game choice and never
  fails. It has an empty effect set; use `system_with_effects` or
  `GameAudio::from_cache` to install the cache WAVs.
- `AudioEngine::system_with_effects(library, effects)`: the normal game engine
  with decoded `EffectLibrary` samples.
- `AudioEngine::silent(library)`: no device, for CI/headless.
- `AudioEngine::silent_with_effects(library, effects)`: headless but still
  renders the effect path (useful for captures that must not open a device).
- `AudioEngine::windows(library, None)`: explicit WinMM MIDI open, returns
  `AudioError::MidiUnavailable` on failure. The effect device is independent.
- `AudioEngine::new(library, midi_sink, sample_sink)`: custom backends (also how
  tests attach `RecordingMidiSink`/`RecordingSampleSink`/`MixerSampleSink`).
- `AudioEngine::set_effects(effects)`: install WAVs after construction.

The preferences page should not spin a thread. Call `tick` from the same loop
that advances the game clock; 20 ms or the frame time is fine. The player uses
microsecond integers and does not drift with variable frame times.

### 4.3 What the prefs branch must do

1. Add `ffe-audio = { path = "../ffe-audio" }` to its crate, or use the existing
   `crate::audio` re-export in `ffe-rs`.
2. Store `AudioSettings` and `SoundFlags` in its preferences structure and
   persist them with serde.
3. Create one `GameAudio::from_cache(&game_dir, &cache)` (or
   `AudioEngine::system_with_effects`) at start-up and keep it for the process
   lifetime.
4. On the ESC page's "Sound Effects" controls, update the matching `SoundFlags`
   field or `AudioSettings` value and call `set_sound_flags`/`set_settings`.
   `music_volume`/`effects_volume` are new sliders; the original had only the
   on/off switches.
5. On "Choose Music From", toggle `SoundFlags::songs[index]` and call
   `set_sound_flags`.
6. Call `audio.tick(elapsed)` once per frame. Call `audio.play_song(0)` again on
   the front end when `song_done()`, and `audio.update_song()` from the
   continuous-music hook.
7. Do not call `AudioEngine::windows` unconditionally in CI. Use `system` or
   `silent`.

No file in the preferences stream is edited by this stream, and no dashboard or
`docs/PORT_LOG.md` file is touched.

---

## 5. Implementation, tests and remaining work

New code:

- `crates/ffe-core/src/hmp.rs`: HMP parser, absolute-tick events, duration, SMF
  export, errors and unit tests.
- `crates/ffe-audio`: `MusicLibrary` (17-name table, runtime `MUSIC/` load and a
  cache HMP override), `MusicPlayer` (tick-driven MIDI scheduler, once/loop, CC7
  scaling, all-notes-off), `AudioEngine`/`Audio` trait, WinMM MIDI sink,
  `mixer` (WAV decode, sample table, 16 voices, centibel curve), WinMM `waveOut`
  sink, recording/null sinks, settings/flags, and a runnable `examples/play.rs`.
- Workspace member `crates/ffe-audio`; no new external runtime dependencies
  (`winmm` is a system library; `serde` was already in the workspace).

Evidence (run on the reference checkout with the player's `MUSIC` folder):

```
cargo test -p ffe-audio -p ffe-core
  19 ffe-audio unit tests passed (mixer, WAV, voices, settings, player)
  3 ffe-core hmp tests passed
  1 player_smoke test passed (null backend)
  FFE_MUSIC_DIR=<game>/MUSIC: all 17 HMPs parsed; 2 asset tests passed
cargo test -p ffe-rs --test sound_mixer
  3 passed (real PCM mixed, mute stops voices, 50% volume curve)
```

The 17-file test prints the table in section 2.1 and asserts:
track count = data tracks + 1, >0 data tracks/events/ticks, division 192, tempo
0x188000, SMF header/track count/division, and a plausible duration ratio. The
recording tests assert the startup CC7 volume, note on/off, loop restart, stop
(all-notes-off plus reset), and volume changes without a restart.

The mixer tests assert actual PCM output (`Mixer::render`), one-shot end,
`ecm`'s two baked copies, the `0x0f` ignore/`0x10` update rules, the laser1
steal, the `nowt` no-op, and the exact centibel values for levels 0/31/63.

Remaining work, in order:

1. **Continuous beds.** `Engine`, `Station` and `Wind` are still one-shot cue
   calls from the game. The mixer implements the 50-copy station loop and
   `0x10`'s volume/pitch update, but the flight state does not yet drive
   `F1910`'s envelope or `F1908`'s randomised second wind voice. This is the
   next audible improvement.
2. **Device support.** WinMM system MIDI and `waveOut` are in; test on a machine
   without the GS Wavetable Synth (a MIDI port that only exposes external
   hardware) and on a machine whose default wave device is exclusive-mode. Add a
   device picker if needed. A bundled SoundFont/software synth is a separate
   licensing decision (CLAUDE.MD §5).
3. **`.mid` override.** If a user has `MUSIC/*.mid`, JJFFE prefers it. Add a
   small SMF reader and route it through the same `MidiSink`.
4. **Cache asset decision.** V28 caches only `MUSIC/QQFRONT.HMP`
   (`base:music/qqfront`, byte-for-byte) because the intro needs it at boot; the
   other 16 stay player-supplied runtime files. Extending the same `music_hmp`
   kind to all 17 would remove the runtime folder dependency but raise the cache
   to 1813 assets. Do not commit the HMPs; they are copyrighted game data.
5. **Timbre.** The system synth is not the HMI patch set. Notes/timing/volume
   are faithful; instrument sound is not. Record the synth/SoundFont identity in
   any future cache provenance.
6. **QQFRONT/QQSUSPEN duration.** Confirm whether the advertised header includes
   a repeat. There are no loop markers, so the port plays the event timeline and
   F919 restarts it on completion.
7. **Positional audio.** `F1759` (the spatial-volume helper) is not called by
   the DOS game, and the port has no pan/distance path. Add it only if a caller
   needs it; the original effect calls are mono.

No dashboard, burndown or `docs/PORT_LOG.md` edits were made.
