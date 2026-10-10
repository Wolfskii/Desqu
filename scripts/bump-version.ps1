# Sets an app's version in every file that carries it, then refreshes Cargo.lock.
#   pwsh scripts/bump-version.ps1 -App screenbound -Version 0.2.0
param(
    [Parameter(Mandatory)][string]$App,
    [Parameter(Mandatory)][string]$Version
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'versions-lib.ps1')

if (-not (Test-SemVer $Version)) { throw "'$Version' is not a semantic version (e.g. 1.2.3 or 1.2.3-rc.1)" }

$files = @(Get-VersionFiles -App $App)
foreach ($f in $files) {
    $old = Get-FileVersion $f
    Set-FileVersion $f $Version
    Write-Host ("{0,-48} {1} -> {2}" -f $f.Path, $old, $Version)
}

# Cargo.lock records workspace crate versions too; update only those, not dependencies.
Push-Location $script:RepoRoot
try { cargo update --workspace 2>&1 | Out-Host } finally { Pop-Location }

& (Join-Path $PSScriptRoot 'check-versions.ps1') -App $App
Write-Host ''
Write-Host "Next: add a CHANGELOG entry, commit, then tag $App-v$Version (see docs/releasing.md)."
