# Launcher (`crates/ffe-launcher`)

A port-only front end, built with egui to the approved mockup
(`crates/ffe-launcher/assets/README.md`). `launcher.bat` builds the release
game, tools and launcher and opens it; the launcher itself is
`target/release/ffe-launcher(.exe)` and expects `ffe-rs(.exe)` beside it.

- **Play**: the original game files (required) and the FFED3D resources
  (optional), each with a status, an open-folder button and Browse. The game
  row runs `ffe_tools::inspect_game` (the English floppy 1.02 / CD 1.01
  fingerprints). There is no cache file: the game builds its content from
  the game folder at every start (`ffe_tools::cache::build_bytes`, about a
  quarter of a second). The FFED3D row checks `resources/` for the skybox,
  star texture, `texNNN` textures, the `.X` models and `ffewin.cfg`. LAUNCH
  starts `ffe-rs --root <data folder> --game <game folder> --start N
  [--skip-intro] [--record recordings/<utc>.ffe-input]`. Clicking a status
  opens About.
- **Settings**: Port options (combat aids, traffic, police, pirates, Advanced
  traffic, docking approach), Display, Game options (controls, the original's
  other and map options), Sound, and Files & start (start system, intro,
  session recording, typed folder paths). The game's settings are read and
  written with `ffe_rs::settings` (`saves/config.cfg`), so the in-game pages
  and the launcher share one file; `bind.*` remaps are kept. Editing is
  disabled while the game runs, and the file is re-read when it closes.
- **About**: the full verification reports, Open saves folder and Open last
  game log.

The launcher's own choices are `saves/launcher.cfg` (game and FFED3D
folders, start, intro, recording). The data folder (saves and recordings) is
the nearest of the launcher's folder and four parents with launcher settings,
a `game\FIRSTENC.EXE` or the workspace's `Cargo.toml`. FFED3D's files are not loaded by the game yet; the launcher only
records and checks the folder. Folder pickers are native on Windows and macOS
(`rfd`); Linux uses zenity or kdialog when installed, with the typed path on
Settings > Files & start as the fallback.

## Windows

- `ffe-launcher.exe` is a windowed (GUI-subsystem) program with its own icon
  (`assets/icon.ico`, embedded by `build.rs` through `winresource`; a missing
  resource compiler only loses the icon) and window icon.
- The game is a console program; the launcher starts it with
  `CREATE_NO_WINDOW`, no stdin and its output in `saves/last-run.log`. A
  failed start shows the game's last message on the launcher's footer, and
  About opens the log.
- OpenGL is tried first; where Windows has no OpenGL 2 driver (virtual
  machines, Remote Desktop, basic display adapters) the launcher reopens with
  wgpu (Direct3D 12 / Vulkan).
- A panic or a window that cannot open writes `saves/launcher.log` and shows a
  message box, since a GUI program has no console.
- Fonts come from `%WINDIR%\Fonts` (Segoe UI).
- Checked by cross-building both executables for `x86_64-pc-windows-gnu`
  (the launcher links as a Windows GUI program); not yet run on Windows.
