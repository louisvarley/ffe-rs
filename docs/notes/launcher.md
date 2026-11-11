# Classic launcher

Run `launcher.bat` to build the Classic game and launcher and open it. The launcher uses the artwork from the sibling ffe-rs launcher. The game executable must sit beside the launcher.

Play contains the original game folder, Browse and Verify, verification status, optional CD video presence, Gateway/Ross 154 starting location, Skip intro and a full-width rounded Launch button matching the original olive gradient. Browsing an extracted release folder detects FIRSTENC.EXE within three levels. Verification uses ffe_tools::inspect_game; video status means AVI files were found in DATA, not that they were decoded or authenticated.

Settings offers Display (window size, fit, nearest/linear, pixel aspect), and Game options (Classic/Modern controls, other options and galaxy map). Settings use the same saves/config.cfg as the game and preserve key remaps. Editing is disabled while the game runs and preferences reload when it closes. No rendering engine, FFED3D, sound, shape detail, space colour or font controls are exposed.

Launcher choices use saves/launcher.cfg. About shows the verification report and links to saves and the last game log. No game content is copied or bundled.

## Elite Club shareware identification

The supplied 2001 English shareware release (floppy 1.06) is identified by exact size and SHA-256. Browse accepts its outer folder and finds `game/FIRSTENC.DAT`, or the same executable named `FIRSTENC.EXE`. Inspection never renames files. The supplied installation passes all 48 file checks. The shareware is floppy 1.06 ("English Release 1.1 (Remastered Version)"). Its data locations are mapped (`tools/reference/map-layouts.py`) and it imports and plays. Its galaxy table has 339 systems against floppy 1.02's 329; the system tables now take any record count, moving the generator tables after a longer galaxy table. Besides the ten systems, 1.06 differs from 1.02 in 35 models (model 89's program now decodes), the ship statistics, the mission scripts and the version strings. JJFFE 2.8a7's own data has the same 339 systems and strings, so this release is the closest data match to the gameplay reference.

Changing the selected game installation removes `assets.bin` from the launcher data root before saving the new folder. Re-selecting the same resolved directory leaves it alone. If deletion fails, the folder change is rejected and the error is shown.
