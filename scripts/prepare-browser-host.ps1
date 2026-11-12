# Builds the native messaging host and copies it where Tauri bundles it for Desqu and Screenbound
# (bundle.externalBin). Tauri refuses to build an app whose sidecar file is missing, so every build
# path runs this first.
#   pwsh scripts/prepare-browser-host.ps1                    (debug build, for dev and tests)
#   pwsh scripts/prepare-browser-host.ps1 -Profile release   (what installers ship)
param(
    [ValidateSet('debug', 'release')][string]$Profile = 'debug'
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $root
try {
    $cargoArgs = @('build', '-p', 'desqu-browser-host')
    if ($Profile -eq 'release') { $cargoArgs += '--release' }
    cargo @cargoArgs
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

$built = Join-Path $root "target/$Profile/desqu-browser-host.exe"
if (-not (Test-Path $built)) { throw "Host not built: $built" }

# Tauri names sidecars after the target triple and strips it at install time.
foreach ($app in 'screenbound', 'desqu') {
    $dir = Join-Path $root "apps/$app/src-tauri/binaries"
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Copy-Item -Force $built (Join-Path $dir 'desqu-browser-host-x86_64-pc-windows-msvc.exe')
}
Write-Host "Browser host ($Profile) copied for screenbound and desqu."
