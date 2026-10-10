# Fails when an app's version differs between its files, or from a release tag.
#   pwsh scripts/check-versions.ps1 -App screenbound
#   pwsh scripts/check-versions.ps1 -App screenbound -Tag screenbound-v0.2.0
param(
    [Parameter(Mandatory)][string]$App,
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'versions-lib.ps1')

$files = @(Get-VersionFiles -App $App)
$truth = Get-FileVersion $files[0]   # apps/<app>/src-tauri/Cargo.toml
$bad = @()
foreach ($f in $files) {
    $v = Get-FileVersion $f
    $mark = if ($v -eq $truth) { 'ok  ' } else { 'FAIL'; $bad += $f.Path }
    Write-Host ("{0} {1,-48} {2}" -f $mark, $f.Path, $v)
}

if ($Tag) {
    $expected = "$App-v$truth"
    if ($Tag -ne $expected) {
        Write-Host "FAIL tag '$Tag' does not match the version: expected '$expected'"
        $bad += "tag:$Tag"
    } else {
        Write-Host "ok   tag '$Tag' matches"
    }
}

if ($bad.Count -gt 0) {
    Write-Error "Version mismatch for '$App' (source of truth: src-tauri/Cargo.toml = $truth): $($bad -join ', ')"
    exit 1
}
Write-Host "All versions for '$App' are $truth."
