# Builds a Desqu GPUI app in release mode and packs it into a per-user installer with NSIS.
#   pwsh scripts/package-gpui-app.ps1 -App vials        -> apps/vials-gpui/dist/Vials-GPUI_<version>_x64-setup.exe
#   pwsh scripts/package-gpui-app.ps1 -App workspaces | screenbound
# NSIS is found on PATH, or in the folder the Tauri bundler downloads it to ($env:LOCALAPPDATA\tauri\NSIS).
param([Parameter(Mandatory)][ValidateSet('vials', 'workspaces', 'screenbound')][string]$App)
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')

# What differs per app: names, how it is started, and what else the uninstaller removes.
$apps = @{
    vials      = @{ Display = 'Vials'; Key = 'VialsGpui'; Exe = 'vials-gpui.exe'; Args = '--live'
                    Menu = 'Software\Classes\DesktopBackground\Shell\DesquVials' }
    workspaces = @{ Display = 'Workspaces'; Key = 'WorkspacesGpui'; Exe = 'workspaces-gpui.exe'; Args = ''; Menu = '' }
    screenbound = @{ Display = 'Screenbound'; Key = 'ScreenboundGpui'; Exe = 'screenbound-gpui.exe'; Args = ''; Menu = '' }
}
$a = $apps[$App]
$dir = Join-Path $root "apps/$App-gpui"

$makensis = (Get-Command makensis -ErrorAction SilentlyContinue)?.Source
if (-not $makensis) { $makensis = Join-Path $env:LOCALAPPDATA 'tauri/NSIS/makensis.exe' }
if (-not (Test-Path $makensis)) { throw 'makensis.exe not found: install NSIS, or run a Tauri build once so it downloads it.' }

$version = (Select-String -Path (Join-Path $dir 'Cargo.toml') -Pattern '^version = "([^"]+)"').Matches[0].Groups[1].Value
Push-Location $dir
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
} finally { Pop-Location }

$dist = Join-Path $dir 'dist'
New-Item -ItemType Directory -Force $dist | Out-Null
$out = Join-Path $dist "$($a.Display)-GPUI_${version}_x64-setup.exe"
$nsisArgs = @('/V2', "/DAPP=$($a.Display)", "/DKEY=$($a.Key)", "/DEXEFILE=$($a.Exe)", "/DVERSION=$version",
    "/DEXE=$(Join-Path $dir "target/release/$($a.Exe)")", "/DOUT=$out")
if ($a.Args) { $nsisArgs += "/DEXTRA_ARGS=$($a.Args)" }
if ($a.Menu) { $nsisArgs += "/DDESKTOP_MENU_KEY=$($a.Menu)" }
& $makensis @nsisArgs (Join-Path $root 'scripts/nsis/gpui-app.nsi')
if ($LASTEXITCODE -ne 0) { throw 'makensis failed' }
Write-Host "Built $out"
