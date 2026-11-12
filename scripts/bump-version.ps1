# Sets one version on every product in the Desqu family: Desqu, Screenbound, Vials, Workspaces (their
# apps, modules and UI packages) and the browser extension. Everything moves together.
#   pwsh scripts/bump-version.ps1 -Version 0.3.0
# Each app's CHANGELOG gets its "## Unreleased" notes moved under the new version.
param(
    [Parameter(Mandatory)][string]$Version
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'versions-lib.ps1')

if (-not (Test-SemVer $Version)) { throw "'$Version' is not a semantic version (e.g. 1.2.3 or 1.2.3-rc.1)" }

foreach ($app in Get-Products) {
    foreach ($f in @(Get-VersionFiles -App $app)) {
        $old = Get-FileVersion $f
        Set-FileVersion $f $Version
        Write-Host ("{0,-48} {1} -> {2}" -f $f.Path, $old, $Version)
    }
    Set-ChangelogRelease -App $app -Version $Version
}

foreach ($f in @(Get-ExtensionVersionFiles)) {
    $old = Get-FileVersion $f
    Set-FileVersion $f $Version
    Write-Host ("{0,-48} {1} -> {2}" -f $f.Path, $old, $Version)
}

Set-LockWorkspaceVersions

# Cargo.lock records workspace crate versions too; update only those, not dependencies.
Push-Location $script:RepoRoot
try { cargo update --workspace 2>&1 | Out-Host } finally { Pop-Location }

& (Join-Path $PSScriptRoot 'check-versions.ps1')
Write-Host ''
Write-Host "Next: review the CHANGELOG sections, commit, then tag each product at $Version (see docs/releasing.md)."
