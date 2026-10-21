@echo off
rem Build debug and release executables of the playable game (ffe-rs) and the
rem tools (ffe-tools). Run from anywhere; outputs go to target\debug and
rem target\release in this checkout. Requires the Rust toolchain (cargo).
setlocal
cd /d "%~dp0"

where cargo >nul 2>nul
if errorlevel 1 (
    echo cargo was not found. Install Rust from https://rustup.rs and try again.
    exit /b 1
)

echo === Debug build ===
cargo build --locked -p ffe-rs -p ffe-tools
if errorlevel 1 goto failed

echo === Release build ===
cargo build --release --locked -p ffe-rs -p ffe-tools
if errorlevel 1 goto failed

echo.
echo Built:
echo   Debug:   %CD%\target\debug\ffe-rs.exe
echo            %CD%\target\debug\ffe-tools.exe
echo   Release: %CD%\target\release\ffe-rs.exe
echo            %CD%\target\release\ffe-tools.exe
echo Play with tools\play.ps1 (uses the release build) or run
echo   target\release\ffe-rs.exe --root .
exit /b 0

:failed
echo.
echo Build failed; see the errors above.
exit /b 1
