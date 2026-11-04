@echo off
rem Build ffe-rs and launch it on your own game files in .\game (the game
rem reads its content straight from them; there is no separate cache file). Run it by
rem double-clicking; on any error the window stays open so you can read it.
rem Requires the Rust toolchain (cargo) and your game files in .\game.
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo cargo was not found. Install Rust from https://rustup.rs and try again.
    goto failed
)

echo === Building the game - release ===
cargo build --release --locked -p ffe-rs -p ffe-tools
if errorlevel 1 goto failed

rem Pack your own images (the assets folder) into assets.bin for Modern graphics:
rem they override FFED3D's art, which overrides the original's.
target\release\ffe-tools.exe build-assets
if errorlevel 1 goto failed

if not exist "game\FIRSTENC.EXE" (
    echo Your game files were not found. Put the Frontier: First Encounters files in:
    echo   %CD%\game
    goto failed
)

echo === Starting the game ===
rem Every run records its keyboard and mouse to recordings\<date-time>.ffe-input, so a
rem problem can be replayed exactly (replay.bat recordings\<file>) or sent for a fix.
if not exist recordings mkdir recordings
for /f %%i in ('powershell -NoProfile -Command "Get-Date -Format yyyyMMdd-HHmmss"') do set STAMP=%%i
echo Recording this session to recordings\%STAMP%.ffe-input
target\release\ffe-rs.exe --game game --record "recordings\%STAMP%.ffe-input"
if errorlevel 1 goto failed
exit /b 0

:failed
echo.
echo Something went wrong - see the messages above.
pause
exit /b 1
