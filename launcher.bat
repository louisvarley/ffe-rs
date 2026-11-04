@echo off
rem Build ffe-rs, its tools and the launcher, then open the launcher. The
rem launcher verifies your game files, holds the settings and starts the game,
rem which reads its content straight from your game folder. Requires the Rust toolchain (cargo).
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo cargo was not found. Install Rust from https://rustup.rs and try again.
    goto failed
)

echo === Building the game and launcher - release ===
cargo build --release --locked -p ffe-rs -p ffe-tools -p ffe-launcher
if errorlevel 1 goto failed

rem Pack your own images (the assets folder) into assets.bin for Modern graphics:
rem they override FFED3D's art, which overrides the original's.
target\release\ffe-tools.exe build-assets
if errorlevel 1 goto failed

start "" "target\release\ffe-launcher.exe"
exit /b 0

:failed
echo.
echo Something went wrong - see the messages above.
pause
exit /b 1
