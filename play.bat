@echo off
rem Build ffe-rs, create or update data.bin from your own game files when it is
rem missing or older than this build needs, then launch the game. Run it by
rem double-clicking; on any error the window stays open so you can read it.
rem Requires the Rust toolchain (cargo) and your game files in .\game.
setlocal
cd /d "%~dp0"

rem Cache version this build expects. Update together with ffe-tools build-data.
set CACHE=v29

where cargo >nul 2>nul
if errorlevel 1 (
    echo cargo was not found. Install Rust from https://rustup.rs and try again.
    goto failed
)

echo === Building the game - release ===
cargo build --release --locked -p ffe-rs -p ffe-tools
if errorlevel 1 goto failed

if not exist "game\FIRSTENC.EXE" (
    echo Your game files were not found. Put the Frontier: First Encounters files in:
    echo   %CD%\game
    goto failed
)

set REBUILD=1
if exist data.bin (
    target\release\ffe-tools.exe inspect-data --data data.bin > "%TEMP%\ffe-rs-inspect.txt" 2>&1
    findstr /c:"content cache %CACHE%:" "%TEMP%\ffe-rs-inspect.txt" >nul
    if not errorlevel 1 set REBUILD=0
)
if "%REBUILD%"=="1" (
    echo === Building data.bin, cache %CACHE%, from your game files ===
    if exist data-new.bin del data-new.bin
    target\release\ffe-tools.exe build-data --game game --out data-new.bin
    if errorlevel 1 goto failed
    move /y data-new.bin data.bin >nul
    if errorlevel 1 goto failed
)

echo === Starting the game ===
rem Every run records its keyboard and mouse to recordings\<date-time>.ffe-input, so a
rem problem can be replayed exactly (replay.bat recordings\<file>) or sent for a fix.
if not exist recordings mkdir recordings
for /f %%i in ('powershell -NoProfile -Command "Get-Date -Format yyyyMMdd-HHmmss"') do set STAMP=%%i
echo Recording this session to recordings\%STAMP%.ffe-input
target\release\ffe-rs.exe --record "recordings\%STAMP%.ffe-input"
if errorlevel 1 goto failed
exit /b 0

:failed
echo.
echo Something went wrong - see the messages above.
pause
exit /b 1
