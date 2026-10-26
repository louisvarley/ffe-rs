@echo off
rem Replay a recorded session exactly: replay.bat recordings\20261026-153000.ffe-input
rem The window plays the recorded keys, clicks and timings. Add --replay-exit to close
rem at the end:  replay.bat recordings\file.ffe-input --replay-exit
setlocal
cd /d "%~dp0"
if "%~1"=="" (
    echo Usage: replay.bat recordings\FILE.ffe-input [--replay-exit]
    pause
    exit /b 1
)
target\release\ffe-rs.exe --replay "%~1" %2
if errorlevel 1 (
    pause
    exit /b 1
)
exit /b 0
