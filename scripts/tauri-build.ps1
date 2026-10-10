# Runs `tauri build` for one app with the updater signing key in place.
# CI provides TAURI_SIGNING_PRIVATE_KEY from a secret; locally the product's key is read from
# $HOME/.tauri/<app>.key (created once with `tauri signer generate`, never committed).
#   pwsh scripts/tauri-build.ps1 -App screenbound
param([Parameter(Mandatory)][string]$App)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')

if (-not $env:TAURI_SIGNING_PRIVATE_KEY -and -not $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
    $key = Join-Path $HOME ".tauri/$App.key"
    if (Test-Path $key) {
        $env:TAURI_SIGNING_PRIVATE_KEY = $key
        Write-Host "Signing updater artifacts with $key"
    } else {
        Write-Warning "No updater signing key found ($key). The build fails if createUpdaterArtifacts is on."
    }
}
# Keys generated without a password still need the variable to exist.
if ($null -eq $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD) { $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = '' }

Push-Location (Join-Path $root "apps/$App")
try {
    npm run tauri build
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
