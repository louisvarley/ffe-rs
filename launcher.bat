@echo off
rem Build ffe-rs, its tools and the launcher, then open the launcher. The
rem launcher verifies your game files, builds data.bin when needed, holds the
rem settings and starts the game. Requires the Rust toolchain (cargo).
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

start "" "target\release\ffe-launcher.exe"
exit /b 0

:failed
echo.
echo Something went wrong - see the messages above.
pause
exit /b 1
