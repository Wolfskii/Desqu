# Fails when any product's version differs from the shared version, or when a release tag does not
# match it. The shared version is the one in apps/desqu/src-tauri/Cargo.toml.
#   pwsh scripts/check-versions.ps1
#   pwsh scripts/check-versions.ps1 -Tag desqu-v0.3.0
#   pwsh scripts/check-versions.ps1 -Tag browser-v0.3.0
param(
    # Accepted for compatibility with release.yml; every product is always checked.
    [string]$App,
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'versions-lib.ps1')

$files = @()
foreach ($app in Get-Products) { $files += @(Get-VersionFiles -App $app) }
$files += @(Get-ExtensionVersionFiles)

$truth = Get-FileVersion (Get-VersionFiles -App 'desqu' | Select-Object -First 1)
$bad = @()
foreach ($f in $files) {
    $v = Get-FileVersion $f
    $mark = if ($v -eq $truth) { 'ok  ' } else { 'FAIL'; $bad += $f.Path }
    Write-Host ("{0} {1,-48} {2}" -f $mark, $f.Path, $v)
}

if ($Tag) {
    if ($Tag -notmatch '^(?:(?:desqu|screenbound|vials|workspaces|browser)-)?v(?<version>.+)$') {
        $bad += "tag:$Tag"
        Write-Host "FAIL tag '$Tag' is not <product>-v<version>"
    } elseif ($Matches['version'] -ne $truth) {
        $bad += "tag:$Tag"
        Write-Host "FAIL tag '$Tag' does not match the shared version $truth"
    } else {
        Write-Host "ok   tag '$Tag' matches $truth"
    }
}

if ($bad.Count -gt 0) {
    Write-Error "Version mismatch (shared version $truth): $($bad -join ', ')"
    exit 1
}
Write-Host "All products are $truth."
