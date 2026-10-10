# Collect release outputs into dist/<app>/ with clear installer vs portable names.
param([Parameter(Mandatory)][string]$App)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$appDir = Join-Path $root "apps/$App"
$arch = 'x64'

$conf = Get-Content -Raw (Join-Path $appDir 'src-tauri/tauri.conf.json') | ConvertFrom-Json
$version = $conf.version
$product = $conf.productName

# Binary name comes from the crate's [package] name (the exe is named after it).
$cargoToml = Get-Content (Join-Path $appDir 'src-tauri/Cargo.toml')
$inPackage = $false
$binName = $null
foreach ($line in $cargoToml) {
    if ($line -match '^\[') { $inPackage = ($line -eq '[package]'); continue }
    if ($inPackage -and $line -match '^name\s*=\s*"([^"]+)"') { $binName = $Matches[1]; break }
}
if (-not $binName) { throw "No [package] name in $appDir/src-tauri/Cargo.toml" }

# The Cargo workspace puts build output at the repo root.
$releaseDir = Join-Path $root 'target/release'
$nsisDir = Join-Path $releaseDir 'bundle/nsis'
$msiDir = Join-Path $releaseDir 'bundle/msi'
$dist = Join-Path $root "dist/$App"

$exe = Join-Path $releaseDir "$binName.exe"
if (-not (Test-Path $exe)) {
    throw "Release binary not found: $exe"
}

$nsis = Get-ChildItem -Path $nsisDir -Filter '*-setup.exe' -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $nsis) {
    throw "NSIS installer not found under $nsisDir"
}

$msi = Get-ChildItem -Path $msiDir -Filter '*.msi' -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
if (-not $msi) {
    throw "MSI installer not found under $msiDir"
}

# Clear only this app's folder so other apps' artifacts survive.
if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force -Path $dist | Out-Null

$nsisOut = Join-Path $dist ("{0}_{1}_{2}-setup.exe" -f $product, $version, $arch)
$msiOut = Join-Path $dist ("{0}_{1}_{2}.msi" -f $product, $version, $arch)
$portableOut = Join-Path $dist ("{0}_{1}_{2}-portable.exe" -f $product, $version, $arch)

Copy-Item -Force $nsis.FullName $nsisOut
Copy-Item -Force $msi.FullName $msiOut
Copy-Item -Force $exe $portableOut

Write-Host ''
Write-Host "Release artifacts in $dist`:"
Write-Host ("  NSIS installer : {0} ({1:N2} MiB)" -f $nsisOut, ((Get-Item $nsisOut).Length / 1MB))
Write-Host ("  MSI installer  : {0} ({1:N2} MiB)" -f $msiOut, ((Get-Item $msiOut).Length / 1MB))
Write-Host ("  Portable exe   : {0} ({1:N2} MiB)" -f $portableOut, ((Get-Item $portableOut).Length / 1MB))
Write-Host ''
Write-Host 'Note: *-setup.exe and *.msi install the app. *-portable.exe is a no-install single binary (needs WebView2).'
