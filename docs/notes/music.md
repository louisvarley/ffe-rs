# Music catalogue and scene mapping

Status: all 17 `MUSIC/*.HMP` songs are catalogued, cached byte-for-byte in
cache v29 (`base:music/<name>`), indexable by song number and wired to the
original's start/stop/loop/random rules. The intro (`F919`, song 0) plays at
boot; combat (14), hyperspace exit (15), docking/launch (random) and continuous
music (random) follow `ffe.asm`. See `docs/notes/sound.md` for the HMP format and
player internals and `docs/notes/sound-cues.md` for the effect cues.

The machine-readable table is `ffe_audio::SONGS` (`SongInfo` in
`crates/ffe-audio/src/library.rs`); the tests assert every row below.

---

## 1. The 17 songs

`index` is `player+0x5334` / the `ppSoundSongNames` order. `trigger` is the
direct `F1902` call site; `random` means `F1902(-1)` may choose it. Durations are
the advertised header dword and the parsed event timeline from
`docs/notes/sound.md`.

| Index | File | Original title | Length | Trigger | Random | Cache asset |
|---:|---|---|---:|---|---|---|
| 0 | `QQFRONT.HMP` | First Encounters Theme - Quality Quartet | 117.5 s | `F919` front end | yes | `base:music/qqfront` |
| 1 | `QQATMOSP.HMP` | Atmosphere - Quality Quartet | 223.2 s | — | yes | `base:music/qqatmosp` |
| 2 | `QQTRAVEL.HMP` | Travel - Quality Quartet | 148.3 s | — | yes | `base:music/qqtravel` |
| 3 | `QQDRAMA.HMP` | Drama - Quality Quartet | 200.9 s | — | yes | `base:music/qqdrama` |
| 4 | `QQSUSPEN.HMP` | Suspense - Quality Quartet | 163.8 s | — | yes | `base:music/qqsuspen` |
| 5 | `QQROCK.HMP` | Rock - Quality Quartet | 141.3 s | — | yes | `base:music/qqrock` |
| 6 | `QQESCAPE.HMP` | Escape - Quality Quartet | 200.3 s | — | yes | `base:music/qqescape` |
| 7 | `QQPARADI.HMP` | Paradise - Quality Quartet | 149.0 s | `F306` message page | yes | `base:music/qqparadi` |
| 8 | `QQFRONT2.HMP` | Alien Encounters - Quality Quartet | 88.0 s | — | yes | `base:music/qqfront2` |
| 9 | `HALLKING.HMP` | In the Hall of the Mountain King - Grieg | 123.2 s | — | yes | `base:music/hallking` |
| 10 | `INTRO.HMP` | Frontier Main Theme - Lowe | 191.6 s | `F307` message/video | yes | `base:music/intro` |
| 11 | `BABAYAGA.HMP` | Baba Yaga - Mussorgsky | 57.8 s | — | yes | `base:music/babayaga` |
| 12 | `BAREM.HMP` | Night on the Bare Mountain - Mussorgsky | 157.2 s | — | yes | `base:music/barem` |
| 13 | `JUPALT.HMP` | Frontier Second Theme - Lowe | 166.8 s | — | yes | `base:music/jupalt` |
| 14 | `VALKRIES.HMP` | Ride of the Valkyries - Wagner | 158.0 s | `F750` battle | yes | `base:music/valkries` |
| 15 | `BLUEDAN.HMP` | Blue Danube - Strauss | 103.9 s | `F656` hyperspace exit | yes | `base:music/bluedan` |
| 16 | `GGOFKIEV.HMP` | Great Gates of Kiev - Mussorgsky | 67.2 s | — | **no** | `base:music/ggofkiev` |

`F1902(-1)` calls `FUNC_000922(0x10)` and therefore samples `0..=15`; index 16 is
in the song table and the "Choose Music From" menu but is never chosen randomly.
An explicit `play_song(16)` still works.

No `MUSIC/*.HMP` is unreferenced: every file maps to a table row and every table
row maps to a file. The installation's root `TEST.HMP` is not a `MUSIC/` song and
is not part of `ppSoundSongNames`; it stays uncached. No table row lacks a file.

---

## 2. Scene rules

`F1902` is the only start routine. It requires music on (`player+0x3f5d`),
music available (`DATA_007901`) and all sound available (`DATA_007900`); an
explicit index must be `< 17`, enabled in `player+0x3f66[]` and different from the
current `player+0x5334`. A negative index makes up to ten random attempts over
`0..=15`, skipping disabled songs and the current one.

| Scene | Rule | Port call |
|---|---|---|
| Boot/title/front end | `F919` plays 0 and replays it when `F1904` reports done; leaving stops it | `GameAudio::front_end_music`, `tick` replay, `leave_front_end` |
| Message page | `F306` starts 7 | `GameAudio::message_page_music` (not triggered yet: no FF-code renderer) |
| Message/video | `F307` starts 10 once per `DATA_009091` message bit | `GameAudio::message_video_music` (not triggered yet) |
| Battle | `F750`/`F236` start 14 when "Music in battles" and "Music on" are set; otherwise stop | `start_battle` |
| Combat end / player death | `F770` stops; `F941` stops on the player's destruction | `end_battle`, `player_death` |
| Hyperspace exit | `F656` starts 15 when "Music after hyperspace" is set and the current song is not 15 | `hyperspace_music` |
| Docking/launch transition | `F1048`/`F1091` stop the song | `stop_for_sequence` |
| Docking/launch complete | `F1094` starts a random enabled song when "Docking music" is set | `docking_music` |
| Continuous music | `F232` starts `-1` when `F1904` says done | `tick` → `update_music` |
| Restart/quit/cleanup | `F917`/`F528` stop | process exit / scene teardown |

There are no CC110/111 loop markers in any of the 17 files, so the player uses
`LoopMode::Once`; the front end's loop is the explicit `F919` replay. `F232`'s
continuous mode chooses a *new* random song after completion, never an in-song
loop. Docking/launch uses the same random pool and never plays a fixed track.

`SoundFlags::songs[0..17]` is the "Choose Music From" enable table. `music_on`
gates every start; `music_volume` scales MIDI CC7 (`127 * volume / 100`), and
turning music off stops the current song. `AudioSettings::music_on` is checked by
`AudioEngine::play_song`, so a scene call cannot bypass it.

---

## 3. Cache v29

`build-data` imports every inspected `MUSIC/<NAME>.HMP` as `base:music/<name>`,
path `music/<name>.hmp`, kind `music_hmp` with the table `index`. Validation
requires `index < 17`, the `HMIMIDIP` signature and a complete
`ffe_core::hmp::HmpSong::parse`. The cache is v29 with 1813 assets (1814 ZIP
entries including `manifest.json`); v28 is 1797.

Fallbacks:

- **v29:** all songs come from the cache; the runtime `MUSIC/` folder is not
  needed for playback.
- **v28:** only `base:music/qqfront` exists (index 0). `GameAudio::from_cache`
  inserts it and leaves 1..16 to `MusicLibrary::path_for`, i.e. `MUSIC/`.
- **v27 and older:** no cached songs; every index comes from `MUSIC/`. A missing
  file is `AudioError::SongMissing` and playback stays silent, as the original
  logs and continues.
- A malformed cached payload is rejected by the cache reader before playback.
  A cache payload always wins over a same-named file on disk.

The HMPs are copyrighted game data and are not committed; only the local
`data.bin` carries them.

---

## 4. Verification

- `crates/ffe-audio/src/library.rs` tests: 17-row table, names/titles, file
  names, direct trigger map, random pool 0..=15 and index 16 exclusion.
- `crates/ffe-audio/tests/hmp_assets.rs` (`FFE_MUSIC_DIR`): parses all 17 files
  from the folder and again from in-memory cache payloads with no folder.
- `crates/ffe-rs/tests/music_sequence.rs`: a synthetic 17-song library and the
  real `AudioEngine`; logs front end (0) → F919 replay (0) → leave (none) →
  continuous flight (random) → launch stop → combat (14) → combat end (none) →
  hyperspace (15) → dock (random) → player death (none). A second test asserts
  mute stops/gates every scene and 50% volume writes CC7 63.
- `crates/ffe-content/tests/cache.rs`: v28 intro-only and v29 all-song manifests,
  out-of-range index and malformed HMP rejection, and a legacy v28 unit
  `music_hmp` record that still deserializes as index 0.
- `tools/reference/verify-assets.py`: cache v29/1813 assets/1814 ZIP entries,
  every `music_hmp` payload byte-equal to `game/MUSIC/<name>.HMP`, 17 records.
- Manual WinMM check: `cargo run -p ffe-audio --example play -- game/MUSIC 0`
  opens the system MIDI device and plays song 0; any index 0..16 can be checked
  the same way. A machine without a MIDI port falls back to silence without
  affecting the game.
