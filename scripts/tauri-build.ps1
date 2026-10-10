# Runs `tauri build` for one app with the updater signing key in place.
# CI provides TAURI_SIGNING_PRIVATE_KEY from a secret; locally the product's key is read from
# $HOME/.tauri/<app>.key (created once with `tauri signer generate`, never committed).
#   pwsh scripts/tauri-build.ps1 -App screenbound            (signed: needs the product's key)
#   pwsh scripts/tauri-build.ps1 -App screenbound -NoSign    (local build: no updater signature, no
#                                                              key and no password prompt)
param(
    [Parameter(Mandatory)][string]$App,
    [switch]$NoSign
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')

if ($NoSign) {
    # Installers only: updater artifacts are what need the private key.
    $noSignConfig = Join-Path ([IO.Path]::GetTempPath()) "desqu-nosign-$App.json"
    '{"bundle":{"createUpdaterArtifacts":false}}' | Set-Content -Path $noSignConfig -Encoding ASCII
    Write-Host 'Local build: not signing updater artifacts (use the release workflow for real releases).'
} elseif (-not $env:TAURI_SIGNING_PRIVATE_KEY -and -not $env:TAURI_SIGNING_PRIVATE_KEY_PATH) {
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
    if ($NoSign) {
        # Not `npm run`: npm swallows `--config` as one of its own options.
        node (Join-Path $root 'node_modules/@tauri-apps/cli/tauri.js') build --config $noSignConfig
    } elseif ($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD -eq '') {
        # Tauri treats an empty password variable as unset and asks on the console: press Enter there.
        '' | npm run tauri build
    } else {
        npm run tauri build
    }
    exit $LASTEXITCODE
} finally {
    Pop-Location
}
