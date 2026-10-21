param()
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$exe = Join-Path $root 'target\release\ffe-rs.exe'
if (!(Test-Path -LiteralPath $exe)) { throw 'Build first: cargo build --release --locked -p ffe-rs' }
& $exe --root $root
if ($LASTEXITCODE -ne 0) { throw "Playable slice exited with code $LASTEXITCODE. See the error above." }
