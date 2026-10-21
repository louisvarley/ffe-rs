param([string]$CaptureDirectory)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exe = Join-Path $root 'target\release\ffe-rs.exe'
if (!(Test-Path -LiteralPath $exe)) { throw 'Build the release ffe-rs executable first.' }
$cache = Join-Path $root 'data.bin'
if (!(Test-Path -LiteralPath $cache)) { throw 'Build data.bin first.' }

if (-not ('PlayableWindowTest' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class PlayableWindowTest {
    [DllImport("user32.dll")] public static extern IntPtr SendMessage(IntPtr window, uint message, IntPtr wp, IntPtr lp);
    [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint code, uint type);
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("kernel32.dll")] private static extern uint GetCurrentThreadId();
    [DllImport("user32.dll")] private static extern bool AttachThreadInput(uint source, uint target, bool attach);
    [DllImport("user32.dll")] private static extern IntPtr SetFocus(IntPtr window);
    [DllImport("user32.dll")] private static extern IntPtr SetActiveWindow(IntPtr window);
    [DllImport("user32.dll")] private static extern bool GetGUIThreadInfo(uint thread, ref GuiInfo info);
    [StructLayout(LayoutKind.Sequential)] private struct GuiInfo {
        public uint Size, Flags;
        public IntPtr Active, Focus, Capture, MenuOwner, MoveSize, Caret;
        public Rect CaretRect;
    }
    public static bool HasFocus(IntPtr window) {
        uint process;
        uint thread = GetWindowThreadProcessId(window, out process);
        var info = new GuiInfo();
        info.Size = (uint)Marshal.SizeOf(typeof(GuiInfo));
        return thread != 0 && GetGUIThreadInfo(thread, ref info) &&
            GetForegroundWindow() == window && info.Active == window && info.Focus == window;
    }
    public static bool FocusWindow(IntPtr window) {
        if (HasFocus(window)) return true;
        if (SetForegroundWindow(window) && HasFocus(window)) return true;
        uint process;
        uint foreground = GetWindowThreadProcessId(GetForegroundWindow(), out process);
        uint target = GetWindowThreadProcessId(window, out process);
        uint current = GetCurrentThreadId();
        bool attachedForeground = false, attachedTarget = false;
        try {
            if (foreground == 0 || target == 0) return false;
            if (foreground != current) {
                attachedForeground = AttachThreadInput(current, foreground, true);
                if (!attachedForeground) return false;
            }
            if (target != current && target != foreground) {
                attachedTarget = AttachThreadInput(current, target, true);
                if (!attachedTarget) return false;
            }
            SetForegroundWindow(window);
            SetActiveWindow(window);
            SetFocus(window);
        } finally {
            if (attachedTarget) AttachThreadInput(current, target, false);
            if (attachedForeground) AttachThreadInput(current, foreground, false);
        }
        return HasFocus(window);
    }
    [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr window, IntPtr dc, uint flags);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr window, out Rect rect);
    [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left, Top, Right, Bottom; }
}
'@
}

$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('ffe-rs-window-' + [guid]::NewGuid().ToString('N'))
$process = $null
$window = [IntPtr]::Zero
function Focus {
    for ($attempt = 0; $attempt -lt 10; $attempt++) {
        if ([PlayableWindowTest]::FocusWindow($window)) {
            Start-Sleep -Milliseconds 100
            if ([PlayableWindowTest]::HasFocus($window)) { return }
        }
        if ($attempt -eq 0) { Write-Host 'Waiting for Windows to grant test-window focus...' }
        Start-Sleep -Milliseconds 100
    }
    throw 'Windows refused test-window focus; no gameplay assertion can be made.'
}
function KeyDown([int]$Code) {
    $scan = [PlayableWindowTest]::MapVirtualKey($Code, 0)
    if ($Code -ge 0x21 -and $Code -le 0x28) { $scan += 256 }
    $down = [IntPtr](1 + ([int64]$scan * 65536))
    [void][PlayableWindowTest]::SendMessage($window, 0x100, [IntPtr]$Code, $down)
}
function KeyUp([int]$Code) {
    $scan = [PlayableWindowTest]::MapVirtualKey($Code, 0)
    if ($Code -ge 0x21 -and $Code -le 0x28) { $scan += 256 }
    $up = [IntPtr](1 + ([int64]$scan * 65536) + 3221225472)
    [void][PlayableWindowTest]::SendMessage($window, 0x101, [IntPtr]$Code, $up)
}
function Key([int]$Code, [int]$HoldMs = 120) {
    Focus
    KeyDown $Code
    Start-Sleep -Milliseconds $HoldMs
    KeyUp $Code
    Start-Sleep -Milliseconds 120
}
function Latest {
    $paths = @(Get-ChildItem -LiteralPath (Join-Path $testRoot 'saves') -Filter 'prototype-*.json' | Sort-Object Name)
    if ($paths.Count -eq 0) { throw 'No snapshot: window/input was not responsive.' }
    return (Get-Content -LiteralPath $paths[-1].FullName -Raw | ConvertFrom-Json).state
}
function Capture([string]$Name) {
    if (!$CaptureDirectory) { return }
    Add-Type -AssemblyName System.Drawing
    [void][IO.Directory]::CreateDirectory($CaptureDirectory)
    $rect = New-Object PlayableWindowTest+Rect
    if (![PlayableWindowTest]::GetWindowRect($window, [ref]$rect)) { throw 'Cannot measure target game window.' }
    $bitmap = New-Object Drawing.Bitmap ($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top)
    $graphics = [Drawing.Graphics]::FromImage($bitmap)
    $dc = $graphics.GetHdc()
    try {
        if (![PlayableWindowTest]::PrintWindow($window, $dc, 0)) { throw 'Cannot capture target game window.' }
    } finally { $graphics.ReleaseHdc($dc); $graphics.Dispose() }
    try { $bitmap.Save((Join-Path $CaptureDirectory $Name), [Drawing.Imaging.ImageFormat]::Png) }
    finally { $bitmap.Dispose() }
}
try {
    [void][IO.Directory]::CreateDirectory($testRoot)
    Copy-Item -LiteralPath $cache -Destination (Join-Path $testRoot 'data.bin')
    $process = Start-Process -FilePath $exe -ArgumentList @('--root', ('"' + $testRoot + '"')) -PassThru
    for ($i = 0; $i -lt 50; $i++) {
        Start-Sleep -Milliseconds 100
        $process.Refresh()
        if ($process.HasExited) { throw "Window process exited early: $($process.ExitCode)" }
        if ($process.MainWindowHandle -ne [IntPtr]::Zero) { $window = $process.MainWindowHandle; break }
    }
    if ($window -eq [IntPtr]::Zero) { throw 'No native window appeared.' }
    Focus
    Start-Sleep -Milliseconds 500
    Capture 'help.png'
    Key 0x70 # F1: help closed
    Key 0x71 # Free-flight scenario, zero initial velocity
    Key 0x33
    Key 0x74
    $labBefore = Latest
    $snapshotsBeforeLab = @(Get-ChildItem -LiteralPath (Join-Path $testRoot 'saves') -Filter 'prototype-*.json').Count
    Key 0x73 # F4: original station model lab, game paused
    Capture 'model-074.png'
    Key 0x09 # Tab: declared-face inspection
    Capture 'model-074-face.png'
    Key 0xbe # Period: next declared face, not flight steering
    Key 0xbc # Comma: previous face
    Key 0x09 # Points
    Capture 'model-074-points.png'
    Key 0x09 # Original normal filter (v5), diagnostic camera
    Capture 'model-074-normals.png'
    Key 0x09 # All declared faces, wire only
    Key 0x32 # Second station, not a scenario reset
    Key 0x27 400 # Inspect yaw
    Key 0x26 300 # Inspect pitch
    Key 0x39 # Maximum diagnostic animation input
    Capture 'model-075.png'
    Key 0x57 500 # Default Modern thrust must be ignored
    Key 0x42 # Buy must be ignored
    Key 0x44 # Dock request must be ignored
    Key 0x74 # Save must be ignored
    Key 0x78 # Load must be ignored
    Start-Sleep -Seconds 2
    if (@(Get-ChildItem -LiteralPath (Join-Path $testRoot 'saves') -Filter 'prototype-*.json').Count -ne $snapshotsBeforeLab) {
        throw 'Model lab wrote a save or did not open.'
    }
    KeyDown 0x57
    Start-Sleep -Milliseconds 120
    Key 0x73 # Return with thrust held: must require a fresh press
    Key 0x74
    KeyUp 0x57
    $labAfter = Latest
    foreach ($field in @('mode','position','velocity','yaw','pitch','credits','cargo','stock','docking_requested')) {
        if ((ConvertTo-Json -InputObject $labBefore.$field -Compress) -ne (ConvertTo-Json -InputObject $labAfter.$field -Compress)) {
            $previous = ConvertTo-Json -InputObject $labBefore.$field -Compress
            $current = ConvertTo-Json -InputObject $labAfter.$field -Compress
            throw "Model lab changed gameplay field ${field}: $previous -> $current."
        }
    }
    if ($labAfter.ticks - $labBefore.ticks -gt 60) { throw 'Model lab allowed simulation/catch-up ticks.' }
    Key 0x71
    Key 0x31 # Restore docked baseline for full flight loop
    Key 0x42 # Buy
    Key 0x74 # F5
    $state = Latest
    if ($state.credits -ne 990 -or $state.cargo -ne 1) { throw 'UI trade/save failed.' }
    Capture 'docked.png'
    Key 0x4c # Launch
    Key 0x57 400 # Default Modern thrust; no F3 toggle
    Key 0x44 # Dock request
    Key 0x74
    $state = Latest
    if ($state.mode -ne 'Flight' -or $state.velocity[2] -le 0 -or !$state.docking_requested) {
        throw 'UI launch/thrust/docking input failed.'
    }
    Capture 'flight.png'
    Write-Host 'Window responsive; launch/thrust/trade/save passed. Coasting to station...'
    for ($coast = 0; $coast -lt 60; $coast++) {
        Key 0x74
        if ((Latest).mode -eq 'Docked') { break }
        Start-Sleep -Seconds 1
    }
    if ((Latest).mode -ne 'Docked') { throw 'UI launched route did not dock.' }
    Key 0x4e # Sell
    Key 0x74
    $state = Latest
    if ($state.credits -ne 999 -or $state.cargo -ne 0) { throw 'UI dock/sell failed.' }
    Key 0x71 # F2
    Key 0x33 # 3 free flight/reset
    Key 0x78 # F9 reload
    Capture 'reloaded.png'
    Key 0x74
    $state = Latest
    if ($state.mode -ne 'Docked' -or $state.credits -ne 999) { throw "UI snapshot reload failed: mode=$($state.mode), credits=$($state.credits), cargo=$($state.cargo)." }
    Key 0x71
    Key 0x33 # Free flight
    Key 0x57 200 # W thrust
    Key 0x27 200 # Right yaw
    Key 0x26 200 # Up pitch
    Key 0x74
    $before = Latest
    if ($before.yaw -le 0 -or $before.pitch -le 0 -or $before.velocity[2] -le 0) {
        throw 'Modern thrust/yaw/pitch failed.'
    }
    Key 0x50 # Pause
    KeyDown 0x57 # Keep thrust held across pause exit
    Start-Sleep -Milliseconds 300
    Key 0x50
    Key 0x74
    KeyUp 0x57
    $after = Latest
    if ($after.velocity[2] -ne $before.velocity[2]) { throw 'Pause allowed held thrust input after resuming.' }
    Key 0x58 400 # X brake
    Key 0x74
    if ((Latest).velocity[2] -ge $after.velocity[2]) { throw 'Braking did not reduce speed.' }
    Key 0x71
    Key 0x33 # Reset before checking negative Modern axes
    Key 0x53 200 # S reverse thrust
    Key 0x25 200 # Left yaw
    Key 0x28 200 # Down pitch
    Key 0x74
    $reverse = Latest
    if ($reverse.velocity[2] -ge 0 -or $reverse.yaw -le [Math]::PI -or $reverse.pitch -ge 0) {
        throw "Default Modern reverse axes failed: z velocity=$($reverse.velocity[2]), yaw=$($reverse.yaw), pitch=$($reverse.pitch)."
    }
    Key 0x71
    Key 0x33 # Reset before checking optional Classic-like preset
    Key 0x72 # F3: Modern -> Classic-like
    Key 0x0d 200 # Enter thrust
    Key 0xbe 200 # Period yaw
    Key 0x41 200 # A pitch
    Key 0x74
    $classic = Latest
    if ($classic.velocity[2] -le 0 -or $classic.yaw -le 0 -or $classic.pitch -le 0) {
        throw 'Optional Classic-like thrust/yaw/pitch failed.'
    }
    Key 0x72 # Restore Modern
    $path = @(Get-ChildItem -LiteralPath (Join-Path $testRoot 'saves') -Filter 'prototype-*.json' | Sort-Object Name)[-1].FullName
    $bad = (Get-Content -LiteralPath $path -Raw).Replace('"version": 1', '"version": 99')
    [IO.File]::WriteAllText($path, $bad)
    Key 0x78 # Bad load must leave current state unchanged
    Capture 'invalid-save.png'
    Key 0x74
    if ((Latest).mode -ne 'Flight' -or (Latest).credits -ne 1000) { throw 'Invalid load mutated live state.' }
    Key 0x71
    Key 0x31 # Docked reset
    Key 0x42
    Key 0x4e # One trade cycle costs one credit
    Key 0x74
    [void][PlayableWindowTest]::SendMessage($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)
    if (!$process.WaitForExit(5000)) { throw 'Window did not close.' }
    if ($process.ExitCode -ne 0) { throw "Window exit code $($process.ExitCode)" }
    $process = Start-Process -FilePath $exe -ArgumentList @('--root', ('"' + $testRoot + '"')) -PassThru
    Start-Sleep -Seconds 1
    $process.Refresh()
    $window = $process.MainWindowHandle
    if ($window -eq [IntPtr]::Zero) { throw 'No window on relaunch.' }
    Focus
    Key 0x70
    Key 0x78
    Key 0x74
    if ((Latest).credits -ne 999) { throw 'Cross-process persistence failed.' }
    Write-Host 'PASS: station model lab isolation -> trade -> launch -> default Modern thrust -> dock -> sell -> save/reset/load -> steering/braking/held-pause isolation -> optional Classic-like controls -> corrupt-load rejection -> relaunch/load.'
} finally {
    if ($process -and !$process.HasExited) {
        [void][PlayableWindowTest]::SendMessage($window, 0x10, [IntPtr]::Zero, [IntPtr]::Zero)
        if (!$process.WaitForExit(3000)) { Stop-Process -Id $process.Id }
    }
    if (Test-Path -LiteralPath $testRoot) { Remove-Item -LiteralPath $testRoot -Recurse -Force }
}
