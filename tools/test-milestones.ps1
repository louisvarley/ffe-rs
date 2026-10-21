param(
    [string]$Tool = '',
    [string]$Reference = ''
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if (!$Tool) { $Tool = Join-Path $root 'target\release\ffe-tools.exe' }
if (!$Reference) { $Reference = Join-Path $root '.cache\jjffe-reference\jjffe-smoke.exe' }
$toolPath = (Resolve-Path -LiteralPath $Tool).Path
$referencePath = (Resolve-Path -LiteralPath $Reference).Path
$game = Join-Path $root 'game'
$cache = Join-Path $root 'data.bin'
$env:PYTHONDONTWRITEBYTECODE = '1'

Write-Output '1/3 Inspecting the local partial cache (377 v1 / 378 v2 / 380 v3 / 382 v4 / 384 v5 / 385 v6 assets)...'
& $toolPath inspect-data --data $cache
if ($LASTEXITCODE -ne 0) { throw 'Cache verification failed. See README cache build instructions.' }

Write-Output '2/3 Comparing freshly imported assets against local originals and JJFFE...'
& python (Join-Path $PSScriptRoot 'reference\verify-assets.py') --tool $toolPath --game $game --reference (Join-Path $root 'jjffe-master')
if ($LASTEXITCODE -ne 0) { throw 'Real-asset verification failed.' }

Write-Output '3/3 Running the new reference/core milestone twice in temporary directories...'
& python (Join-Path $PSScriptRoot 'reference\verify-core.py') --tool $toolPath --reference $referencePath
if ($LASTEXITCODE -ne 0) { throw 'Reference/core verification failed. Rebuild the reference if it predates the new modes.' }

Write-Output 'PASS: both technical milestones verified. These checks do not open a window; use tools\play.ps1 for the separate experimental playable slice.'
