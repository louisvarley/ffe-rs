param(
    [Parameter(Mandatory = $true)][string]$Nasm,
    [string]$VcVars = 'C:\Program Files\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars32.bat'
)

$ErrorActionPreference = 'Stop'

# NASM and cl write warnings to stderr; only a non-zero exit code is a failure.
function Invoke-Native {
    param([string]$Message, [scriptblock]$Step)
    $previous = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { & $Step 2>&1 | ForEach-Object { Write-Output ([string]$_) } }
    finally { $ErrorActionPreference = $previous }
    if ($LASTEXITCODE -ne 0) { throw $Message }
}

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$source = Join-Path $root 'jjffe-master'
$output = Join-Path $root '.cache\jjffe-reference'
$nasmPath = (Resolve-Path -LiteralPath $Nasm).Path
$vcPath = (Resolve-Path -LiteralPath $VcVars).Path
$exports = Join-Path $PSScriptRoot 'exports.inc'
New-Item -ItemType Directory -Path $output -Force | Out-Null

Push-Location $source
try {
    Invoke-Native 'JJFFE assembly failed.' { & $nasmPath -f win32 -p $exports -o (Join-Path $output 'ffe.obj') 'ffe.asm' }
    Invoke-Native 'Bitmap assembly failed.' { & $nasmPath -f win32 -o (Join-Path $output 'ffebmp.obj') 'ffebmp.asm' }
} finally {
    Pop-Location
}

$harness = Join-Path $PSScriptRoot 'headless.c'
$command = 'call "{0}" >nul && cd /d "{1}" && cl /nologo /TC /W3 /O2 /MT /DWIN32 /D__FFEWIN__ /D_CRT_SECURE_NO_WARNINGS /I"{2}" "{3}" "{2}\ffecfg.c" "{2}\ffemisc.c" /Fe:jjffe-smoke.exe /link /SAFESEH:NO ffe.obj ffebmp.obj legacy_stdio_definitions.lib' -f $vcPath, $output, $source, $harness
Invoke-Native 'JJFFE smoke-adapter compile/link failed.' { & $env:ComSpec /d /c $command }

# Build a separate, narrowly instrumented oracle; leave both supplied assembly and
# the ordinary smoke executable unmodified. Capture primitive arguments before rasterization.
$probeSource = Join-Path $output 'ffe-command-probe.asm'
$assembly = [IO.File]::ReadAllText((Join-Path $source 'ffe.asm'))
foreach ($pair in @(@('FUNC_001708', '_trace_triangle'), @('FUNC_001712', '_trace_quad'), @('FUNC_001699', '_trace_point_args'), @('FUNC_001703', '_trace_line_args'))) {
    $pattern = '(?m)^(\s*call )' + $pair[0] + '(\s*)$'
    if ([regex]::Matches($assembly, $pattern).Count -ne 2) { throw "Unexpected primitive hook sites: $($pair[0])" }
    $assembly = [regex]::Replace($assembly, $pattern, ('${1}' + $pair[1] + '${2}'))
}
[IO.File]::WriteAllText($probeSource, "EXTERN _trace_triangle`r`nEXTERN _trace_quad`r`nEXTERN _trace_point_args`r`nEXTERN _trace_line_args`r`n" + $assembly, [Text.Encoding]::ASCII)
Push-Location $source
try {
    Invoke-Native 'JJFFE command-probe assembly failed.' { & $nasmPath -f win32 -p $exports -o (Join-Path $output 'ffe-command-probe.obj') $probeSource }
} finally { Pop-Location }
$probeCommand = $command.Replace('/DWIN32', '/DWIN32 /DFFE_COMMAND_PROBE').Replace('/Fe:jjffe-smoke.exe', '/Fe:jjffe-commands.exe').Replace(' ffe.obj ', ' ffe-command-probe.obj ')
Invoke-Native 'JJFFE command-probe compile/link failed.' { & $env:ComSpec /d /c $probeCommand }

$paths = @(
    (Join-Path $source 'ffe.asm'),
    (Join-Path $source 'ffedat.asm'),
    (Join-Path $source 'ffebmp.asm'),
    (Join-Path $source 'ffecfg.c'),
    (Join-Path $source 'ffemisc.c'),
    (Join-Path $source 'ffeapi.h'),
    (Join-Path $source 'ffecfg.h'),
    $harness,
    $exports,
    $PSCommandPath,
    $vcPath,
    $nasmPath
)
$hashes = @($paths | ForEach-Object {
    $file = Get-Item -LiteralPath $_
    [ordered]@{ name = $file.Name; size = $file.Length; sha256 = (Get-FileHash -LiteralPath $_ -Algorithm SHA256).Hash.ToLowerInvariant() }
})
[ordered]@{
    reference = 'jjffe-2.8a7'
    adapter = 'bounded-headless-smoke-v1'
    state_adapter = 'bounded-new-game-v1'
    numeric_adapter = 'rng-timer-v1'
    font_adapter = 'font-v1'
    blit_adapter = 'blit-v1'
    model_adapter = 'model-vertices-v1'
    normal_adapter = 'normals-v1'
    transform_adapter = 'transforms-v1'
    rotation_operand_adapter = 'rotation-operands-v1'
    visibility_adapter = 'visibility-v1'
    execution_adapter = 'execution-v1'
    depth_adapter = 'depth-v1'
    depth_command_adapter = 'depth-commands-v1'
    order_adapter = 'order-v1'
    composed_order_adapter = 'composed-order-v1'
    projection_adapter = 'projection-v1'
    live_vertex_adapter = 'live-vertices-v1'
    command_adapter = 'commands-v1'
    command_probe_hooks = @('Two calls F1708 -> trace_triangle', 'Two calls F1712 -> trace_quad', 'Two calls F1699 -> trace_point_args', 'Two calls F1703 -> trace_line_args')
    command_probe_command = $probeCommand
    command_probe_sha256 = (Get-FileHash -LiteralPath (Join-Path $output 'jjffe-commands.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    command_probe_source_sha256 = (Get-FileHash -LiteralPath $probeSource -Algorithm SHA256).Hash.ToLowerInvariant()
    source_modified = $false
    command = $command
    assembly_commands = @(
        ('"{0}" -f win32 -p "{1}" -o "{2}" ffe.asm' -f $nasmPath, $exports, (Join-Path $output 'ffe.obj')),
        ('"{0}" -f win32 -o "{1}" ffebmp.asm' -f $nasmPath, (Join-Path $output 'ffebmp.obj'))
    )
    nasm_version = (& $nasmPath -v)
    binary_sha256 = (Get-FileHash -LiteralPath (Join-Path $output 'jjffe-smoke.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
    limitations = @('No gameplay oracle', 'No save/load', 'No audio or video device', 'Synthetic clock', 'Scripted Y acceptance only')
    state_limitations = @('Selected numeric state only', 'Scripted Y then intro choice 1', 'No flight/docking/trading/save validation', 'Not full game-state serialization')
    inputs = $hashes
} | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $output 'build-manifest.json') -Encoding UTF8
Write-Output "Built local-only reference: $output\jjffe-smoke.exe"
Write-Output 'Run from a disposable working directory; capture stdout as JSONL. No original game data is embedded in this adapter source, but the linked binary contains legacy data and must not be distributed.'
