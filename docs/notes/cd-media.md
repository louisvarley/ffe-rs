# Original CD media

The supplied English CD 1.01 contains 323 AVI videos and 323 matching BMP stills in DATA (533,312,426 bytes). Both formats are preserved byte-for-byte in an ignored FMVs folder; no transcoding or original content is committed.

`cargo run -p ffe-tools -- export-media --game <CD-install-directory> --out FMVs` copies DATA/*.AVI and DATA/*.BMP and writes FMVs/manifest.json with each filename, size and SHA-256. Source filenames are matched without case sensitivity and output uses the original uppercase names. Every copied payload is read back and compared before the staging folder is published. Identical existing outputs are accepted; changed, incomplete or mixed outputs fail rather than silently retaining stale media. A failed staging folder is left for inspection. No source files are renamed or modified.

`ffe-tools build-data --game <CD-install-directory> --out <directory>/data.bin` also creates/verifies `<directory>/FMVs`. The videos stay outside the bounded 64 MiB data.bin cache so playback can stream individual files. Keep data.bin and its FMVs folder together. A floppy installation without DATA needs no media folder. When switching versions, use a fresh output directory so another CD's media cannot be mixed with the selected source.

The sibling ffe-rs extracted CD's DATA folder is the source used for this checkout's root FMVs copy. For an unextracted local MODE1/2352 disc, `python tools/extract-cd.py <disc.bin> <new-install-directory>` extracts the ISO9660 files to a new directory; the supplied disc image is read-only. The CUE must describe MODE1/2352; the helper validates the primary volume descriptor and bounds/filenames and never executes disc contents.

Playback is tracked by content-7 in dashboard/burndown.js: determine the original game-event/actor/filename mapping; decode the actual AVI codecs and audio; draw at the original size/palette/aspect; preserve audio/video timing, input and skip semantics; select matching BMPs when videos are unavailable; retain the floppy-only behaviour. Test representative BBS, station/service and story interactions against the original, missing/corrupt media, skip, and pause/resume. Packaging alone does not enable playback. The in-memory game startup path does not currently load FMVs.

## Loading in the game (2026-11-11)

The game loads the videos when its content was built from CD 1.01 (`ffe-cd-1.01-en`) and `FMVs/manifest.json` is beside the data root (`--root`, the checkout for `play.bat`). `ffe_rs::fmv::Library` lists the manifest's AVI files, checks each is present at its recorded size, and decodes a clip on request with `ffe_convert::avi::decode_avi`. A floppy installation never uses them. The start-up log reports the clip count, or why a folder that does not match its manifest is not used.

Every supplied clip has the same form: RIFF AVI, an uncompressed bottom-up 8-bit DIB stream of 120 x 120 at 15 frames a second (66,666 us) with its 256-colour palette in the format block, and 22,050 Hz 16-bit mono PCM interleaved in `rec ` lists. The decoder accepts only that form and checks every chunk. All 323 clips decode (1,983 seconds; `tests/fmv.rs`, local-only with `FFE_FMV_ROOT`).

**Which clip plays, and when (2026-11-11).** The selection is in the JJFFE 2.8a7 source, ported in `ffe_core::fmv_select`. Four routines set the current clip list (`DATA_002321`, count `DATA_002322`); F120 then plays `list[F922(count)]` when videos are enabled (`DATA_009027`) and forgets the list:

| routine | when | lists (`ffedat.asm`) |
|---|---|---|
| F333 | the services root, once per arrival (`DATA_005315`, set by F320/F1031 and at start) | by allegiance (`player+0x43c`): independent `c1`..`c8`, or on a coin flip `c14`.. (hull below 75%), `c25`.. (fine over 50), `fr2`.. (tech level below 180); Imperial `i1`..`i9` or `i10`.. (`+0x1a0` points from 10,000); Federal `f1`..`f7` or `f31`.. (`+0x1a4` points), `f25`..`f28` (hull below 75%); otherwise `g1`..; then a 3-in-20 chance of `g1`... None at a Thargoid system or station. |
| F382 | the Shipyard opens | `sh1`.. (independent), `ish1`.. (Imperial), `fsh1`.. (Federal); 3-in-20 `sh1`..; `isy21`..`isy24` on a coin flip below 75% hull |
| F384 | Upgrades opens | `isy1`..`isy20` (18) |
| F442 | a ship bought | `isy25`..`isy30` |

The routines draw from the shared F922 stream whether or not videos are present, as the original does; only F120's pick needs them. 135 of the 323 clips are reachable through these lists; JJFFE never selects the bulletin-board adverts (`bb1`..`bb44`), `p`, `st`, `f8`..`f23` or the names beyond each list's count.

**Playback.** `ffe_rs::fmv::Player` shows the picture at (1, 17) of the 320 x 200 screen at the clip's 15 frames a second, with its black background transparent (F121 skips pixels whose colour maps to palette index 0; F115 maps each colour at four bits a channel through `DATA_007770`, where only 0x000 is index 0), its sound through the mixer's stream voice (`ffe_audio`), and holds the game clock while it plays (JJFFE's `DATA_007706 = 0` during F120). Any key or click ends it and still acts, as F120 hands the input on; leaving the screen or the station ends it too. The port keeps one military points count where the original has two (`+0x1a0`, `+0x1a4`), and draws no "Connecting video..." line (F565, `0x98d9`). The picture keeps the clip's own 24-bit colours; the original reduces them to four bits a channel and the game's dynamic palette (F1326-F1331).
