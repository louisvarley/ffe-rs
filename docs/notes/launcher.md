# Launcher (`crates/ffe-launcher`)

A port-only front end, built with egui to the approved mockup
(`crates/ffe-launcher/assets/README.md`). `launcher.bat` builds the release
game, tools and launcher and opens it; the launcher itself is
`target/release/ffe-launcher(.exe)` and expects `ffe-rs(.exe)` beside it.

- **Play**: the original game files (required) and the FFED3D resources
  (optional), each with a status, an open-folder button and Browse. The game
  row runs `ffe_tools::inspect_game` (the English floppy 1.02 / CD 1.01
  fingerprints) and, once the files pass, builds `data.bin` when it is
  missing or older than v29 (`ffe_tools::cache::build_data` into
  `data-new.bin`, then renamed). The FFED3D row checks `resources/` for the
  skybox, star texture, `texNNN` textures, the `.X` models and `ffewin.cfg`.
  LAUNCH starts `ffe-rs --root <data folder> --start N [--skip-intro]
  [--record recordings/<utc>.ffe-input]`. Clicking a status opens About.
- **Settings**: Port options (combat aids, traffic, police, pirates, Advanced
  traffic, docking approach), Display, Game options (controls, the original's
  other and map options), Sound, and Files & start (start system, intro,
  session recording, typed folder paths). The game's settings are read and
  written with `ffe_rs::settings` (`saves/config.cfg`), so the in-game pages
  and the launcher share one file; `bind.*` remaps are kept. Editing is
  disabled while the game runs, and the file is re-read when it closes.
- **About**: the full verification reports, Rebuild data.bin and Open data
  folder.

The launcher's own choices are `saves/launcher.cfg` (game and FFED3D
folders, start, intro, recording). The data folder is found as the game does:
beside the executable, else the nearest of four parent folders with a
`data.bin`. FFED3D's files are not loaded by the game yet; the launcher only
records and checks the folder. Folder pickers are native on Windows and macOS
(`rfd`); Linux uses zenity or kdialog when installed, with the typed path on
Settings > Files & start as the fallback.
