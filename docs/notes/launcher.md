# Classic launcher

Run `launcher.bat` to build the Classic game and launcher and open it. The launcher uses the artwork from the sibling ffe-rs launcher. The game executable must sit beside the launcher.

Play contains the original game folder, Browse and Verify, verification status, optional CD video presence, Gateway/Ross 154 starting location, Skip intro and a full-width rounded Launch button matching the original olive gradient. Browsing an extracted release folder detects FIRSTENC.EXE within three levels. Verification uses ffe_tools::inspect_game; video status means AVI files were found in DATA, not that they were decoded or authenticated.

Settings offers Display (window size, fit, nearest/linear, pixel aspect), and Game options (Classic/Modern controls, other options and galaxy map). Settings use the same saves/config.cfg as the game and preserve key remaps. Editing is disabled while the game runs and preferences reload when it closes. No rendering engine, FFED3D, sound, shape detail, space colour or font controls are exposed.

Launcher choices use saves/launcher.cfg. About shows the verification report and links to saves and the last game log. No game content is copied or bundled.

## Elite Club shareware identification

The supplied 2001 English shareware release (floppy 1.06) is identified by exact size and SHA-256. Browse accepts its outer folder and finds `game/FIRSTENC.DAT`, or the same executable named `FIRSTENC.EXE`. Inspection never renames files. The supplied installation passes all 48 file checks. The launcher shows Experimental and permits a launch attempt for shareware. Its data locations are now mapped (`tools/reference/map-layouts.py`) and every table decodes except the system tables: the shareware is floppy 1.06, whose patch added ten star systems (339 against the 329 of floppy 1.02, CD 1.01 and JJFFE 2.8a7), so the importer refuses it with that reason and the launcher reports it. Supporting it needs the galaxy tables to take a variable record count, and its other 1.06 changes checked against the JJFFE baseline.

Changing the selected game installation removes `assets.bin` from the launcher data root before saving the new folder. Re-selecting the same resolved directory leaves it alone. If deletion fails, the folder change is rejected and the error is shown.
