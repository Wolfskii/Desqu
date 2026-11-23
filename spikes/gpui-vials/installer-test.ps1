# Installs an app's installer silently into a temp folder, starts the installed app (temp config), then
# uninstalls silently and checks that everything it created is gone and the running copy was closed.
# Refuses to run if that app is already installed, so it can never touch a real install.
#   pwsh installer-test.ps1 -App vials | workspaces | screenbound
param([ValidateSet('vials', 'workspaces', 'screenbound')][string]$App = 'vials')
$ErrorActionPreference = 'Stop'
$a = @{
    vials      = @{ Display = 'Vials'; Key = 'VialsGpui'; Exe = 'vials-gpui.exe'; Args = @('--live') }
    workspaces = @{ Display = 'Workspaces'; Key = 'WorkspacesGpui'; Exe = 'workspaces-gpui.exe'; Args = @() }
    screenbound = @{ Display = 'Screenbound'; Key = 'ScreenboundGpui'; Exe = 'screenbound-gpui.exe'; Args = @() }
}[$App]
$setup = (Get-ChildItem "$PSScriptRoot\..\..\apps\$App-gpui\dist\$($a.Display)-GPUI_*_x64-setup.exe" | Sort-Object LastWriteTime | Select-Object -Last 1).FullName
$uninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\$($a.Key)"
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) "$($a.Display).lnk"
if ((Test-Path $uninstallKey) -or (Test-Path $shortcut)) { throw "$($a.Display) (GPUI) seems to be installed already: not touching it." }
$res = [ordered]@{}
$dir = Join-Path $env:TEMP "gpui-install-test-$([guid]::NewGuid().ToString('N').Substring(0,6))"
$cfgDir = Join-Path $env:TEMP "gpui-install-cfg-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $cfgDir | Out-Null
$cfg = Join-Path $cfgDir 'config.json'
if ($App -eq 'vials') {
    @{ version = 1; contextMenu = $false; createOnDrag = $false; vials = @(@{ id = 'vial-1'; name = 'I'; monitor = $null; rect = @{ x = 0.4; y = 0.3; width = 0.1; height = 0.2 }; items = @() }) } | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding utf8
}
if ($App -eq 'screenbound') {
    # Paused, so the engine cannot move any real window while the installed copy runs.
    @{ version = 1; enabled = $false; zones = @(@{ id = 'z'; name = 'Zone'; rect = @{ x = 0; y = 0; width = 0.75; height = 1 } }); rules = @() } | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding utf8
}
$configArgs = if ($App -eq 'screenbound') { @('--config-dir', "`"$cfgDir`"") } else { @('--config', "`"$cfg`"") }
$exe = Join-Path $dir $a.Exe
try {
    # /D must be the last argument and unquoted.
    Start-Process $setup -ArgumentList '/S', "/D=$dir" -Wait
    $res.exeInstalled = Test-Path $exe
    $res.uninstallerInstalled = Test-Path (Join-Path $dir 'uninstall.exe')
    $res.addRemoveEntry = Test-Path $uninstallKey
    $res.displayVersion = (Get-ItemProperty $uninstallKey -ErrorAction SilentlyContinue).DisplayVersion
    $res.startMenuShortcut = Test-Path $shortcut
    $res.installedExeHasIcon = ((Get-Item $exe).VersionInfo.ProductName -eq $a.Display)

    $running = Start-Process $exe -ArgumentList (@($a.Args) + $configArgs) -PassThru
    Start-Sleep 4
    $res.installedAppRuns = -not $running.HasExited

    Start-Process (Join-Path $dir 'uninstall.exe') -ArgumentList '/S'   # copies itself away and returns at once
    for ($i = 0; $i -lt 30 -and (Test-Path $exe); $i++) { Start-Sleep 1 }
    Start-Sleep 2
    $res.runningCopyClosed = $running.HasExited
    $res.exeRemoved = -not (Test-Path $exe)
    $res.addRemoveEntryRemoved = -not (Test-Path $uninstallKey)
    $res.startMenuShortcutRemoved = -not (Test-Path $shortcut)
}
finally {
    Get-Process ($a.Exe -replace '\.exe$', '') -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$dir*" } | Stop-Process -Force
    Remove-Item $uninstallKey -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item "HKCU:\Software\$($a.Key)" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $shortcut -Force -ErrorAction SilentlyContinue
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $cfgDir -Recurse -Force -ErrorAction SilentlyContinue
}
$res | ConvertTo-Json
