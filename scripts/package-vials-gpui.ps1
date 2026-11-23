# Builds Vials (GPUI) in release mode and packs it into a per-user installer with NSIS.
#   pwsh scripts/package-vials-gpui.ps1            -> dist/Vials-GPUI_<version>_x64-setup.exe
# NSIS is found on PATH, or in the folder the Tauri bundler downloads it to ($env:LOCALAPPDATA\tauri\NSIS).
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$app = Join-Path $root 'apps/vials-gpui'

$makensis = (Get-Command makensis -ErrorAction SilentlyContinue)?.Source
if (-not $makensis) { $makensis = Join-Path $env:LOCALAPPDATA 'tauri/NSIS/makensis.exe' }
if (-not (Test-Path $makensis)) { throw 'makensis.exe not found: install NSIS, or run a Tauri build once so it downloads it.' }

$version = (Select-String -Path (Join-Path $app 'Cargo.toml') -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
Push-Location $app
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
} finally { Pop-Location }

$dist = Join-Path $app 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$out = Join-Path $dist "Vials-GPUI_${version}_x64-setup.exe"
& $makensis /V2 "/DVERSION=$version" "/DEXE=$(Join-Path $app 'target/release/vials-gpui.exe')" "/DOUT=$out" (Join-Path $app 'installer/vials.nsi')
if ($LASTEXITCODE -ne 0) { throw 'makensis failed' }
Write-Host "Built $out"
