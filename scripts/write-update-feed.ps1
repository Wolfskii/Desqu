# Writes dist/<app>/feed/latest.json, the file the in-app updater reads (Tauri updater format).
# Run after publish-artifacts.ps1 of a *signed* build.
#   pwsh scripts/write-update-feed.ps1 -App screenbound
#   pwsh scripts/write-update-feed.ps1 -App screenbound -NotesFile notes.md
param(
    [Parameter(Mandatory)][string]$App,
    [string]$NotesFile,
    [string]$Repo = 'Wolfskii/Desqu'
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$appDir = Join-Path $root "apps/$App"
$dist = Join-Path $root "dist/$App"

$conf = Get-Content -Raw (Join-Path $appDir 'src-tauri/tauri.conf.json') | ConvertFrom-Json
$version = $conf.version
$product = $conf.productName

$setup = Get-ChildItem $dist -Filter "${product}_${version}_*-setup.exe" | Select-Object -First 1
if (-not $setup) { throw "Setup installer for $product $version not found in $dist (run publish-artifacts.ps1 first)" }
$sigPath = "$($setup.FullName).sig"
if (-not (Test-Path $sigPath)) { throw "Missing signature ${sigPath}: build with TAURI_SIGNING_PRIVATE_KEY set" }
$signature = (Get-Content -Raw $sigPath).Trim()

# Release notes: the CHANGELOG section for this version (without its heading), or an explicit file.
function Get-ChangelogSection([string]$path, [string]$version) {
    if (-not (Test-Path $path)) { return $null }
    $lines = Get-Content $path
    $start = -1
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match "^##\s+\[?v?$([regex]::Escape($version))\]?(\s|$)") { $start = $i + 1; break }
    }
    if ($start -lt 0) { return $null }
    $end = $lines.Count
    for ($j = $start; $j -lt $lines.Count; $j++) { if ($lines[$j] -match '^##\s') { $end = $j; break } }
    ($lines[$start..($end - 1)] -join "`n").Trim()
}

if ($NotesFile) {
    $notes = (Get-Content -Raw $NotesFile).Trim()
} else {
    $notes = Get-ChangelogSection (Join-Path $appDir 'CHANGELOG.md') $version
    if (-not $notes) { throw "No '## $version' section in apps/$App/CHANGELOG.md (or pass -NotesFile)" }
}

$tag = "$App-v$version"
$feed = [ordered]@{
    version   = $version
    notes     = $notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{
            signature = $signature
            url       = "https://github.com/$Repo/releases/download/$tag/$($setup.Name)"
        }
    }
}

$feedDir = Join-Path $dist 'feed'
New-Item -ItemType Directory -Force -Path $feedDir | Out-Null
$json = $feed | ConvertTo-Json -Depth 6
[System.IO.File]::WriteAllText((Join-Path $feedDir 'latest.json'), $json, (New-Object System.Text.UTF8Encoding($false)))

# Release notes next to the assets, for `gh release create --notes-file`.
[System.IO.File]::WriteAllText((Join-Path $dist 'RELEASE_NOTES.md'), $notes + "`n", (New-Object System.Text.UTF8Encoding($false)))

Write-Host "Wrote $feedDir\latest.json for $tag"
