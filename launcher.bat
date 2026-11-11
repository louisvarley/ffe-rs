@echo off
setlocal
cd /d "%~dp0"
where cargo >nul 2>nul
if errorlevel 1 goto failed
cargo build --release --locked -p ffe-rs -p ffe-launcher
if errorlevel 1 goto failed
start "" "target\release\ffe-launcher.exe"
exit /b 0
:failed
echo Launcher build failed. Check the messages above and that Rust is installed.
pause
exit /b 1
