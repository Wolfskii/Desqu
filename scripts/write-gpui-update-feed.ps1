# Writes the update manifest the GPUI apps read ("Check now"): apps/<app>-gpui/dist/feed/latest.json.
# The GPUI apps only point at the download (they never install anything), so no signature is needed.
# Publish the file at https://wolfskii.github.io/Desqu/updates/<app>-gpui/latest.json (the same place the
# Tauri feeds go) and attach the installer from `package-gpui-app.ps1` to the release named by -Tag.
#   pwsh scripts/write-gpui-update-feed.ps1 -App vials -Tag gpui-v0.4.0 -Notes "First GPUI build"
param(
    [Parameter(Mandatory)][ValidateSet('vials', 'workspaces', 'screenbound', 'desqu')][string]$App,
    [Parameter(Mandatory)][string]$Tag,
    [string]$Notes = '',
    [string]$Repo = 'Wolfskii/Desqu'
)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$dir = Join-Path $root "apps/$App-gpui"
$display = (Get-Culture).TextInfo.ToTitleCase($App)
$version = (Select-String -Path (Join-Path $dir 'Cargo.toml') -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
$setup = Join-Path $dir "dist/$display-GPUI_${version}_x64-setup.exe"
if (-not (Test-Path $setup)) { throw "No installer at ${setup}: run scripts/package-gpui-app.ps1 -App $App first." }

$feed = [ordered]@{
    version   = $version
    notes     = $Notes
    pub_date  = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    platforms = [ordered]@{
        'windows-x86_64' = [ordered]@{ url = "https://github.com/$Repo/releases/download/$Tag/$(Split-Path $setup -Leaf)" }
    }
}
$feedDir = Join-Path $dir 'dist/feed'
New-Item -ItemType Directory -Force $feedDir | Out-Null
$path = Join-Path $feedDir 'latest.json'
[System.IO.File]::WriteAllText($path, ($feed | ConvertTo-Json -Depth 6), (New-Object System.Text.UTF8Encoding($false)))
Write-Host "Wrote $path"
