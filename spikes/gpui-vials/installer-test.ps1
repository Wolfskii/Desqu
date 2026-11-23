# Installs the built installer silently into a temp folder, starts the installed app (temp config),
# then uninstalls silently and checks that everything it created is gone and the running copy was
# closed. Refuses to run if a Vials (GPUI) install already exists, so it can never touch a real one.
param([string]$Setup = (Get-ChildItem "$PSScriptRoot\..\..\apps\vials-gpui\dist\Vials-GPUI_*_x64-setup.exe" | Sort-Object LastWriteTime | Select-Object -Last 1).FullName)
$ErrorActionPreference = 'Stop'
$uninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\VialsGpui'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Vials.lnk'
if ((Test-Path $uninstallKey) -or (Test-Path $shortcut)) { throw 'Vials (GPUI) seems to be installed already: not touching it.' }
$res = [ordered]@{}
$dir = Join-Path $env:TEMP "vials-install-test-$([guid]::NewGuid().ToString('N').Substring(0,6))"
$cfgDir = Join-Path $env:TEMP "vials-install-cfg-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $cfgDir | Out-Null
$cfg = Join-Path $cfgDir 'config.json'
@{ version = 1; contextMenu = $false; createOnDrag = $false; vials = @(@{ id = 'vial-1'; name = 'I'; monitor = $null; rect = @{ x = 0.4; y = 0.3; width = 0.1; height = 0.2 }; items = @() }) } | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding utf8
$exe = Join-Path $dir 'vials-gpui.exe'
try {
    # /D must be the last argument and unquoted.
    Start-Process $Setup -ArgumentList '/S', "/D=$dir" -Wait
    $res.exeInstalled = Test-Path $exe
    $res.uninstallerInstalled = Test-Path (Join-Path $dir 'uninstall.exe')
    $res.addRemoveEntry = Test-Path $uninstallKey
    $res.displayVersion = (Get-ItemProperty $uninstallKey -ErrorAction SilentlyContinue).DisplayVersion
    $res.startMenuShortcut = Test-Path $shortcut
    $res.installedExeHasIcon = ((Get-Item $exe).VersionInfo.ProductName -eq 'Vials')

    $app = Start-Process $exe -ArgumentList '--live', '--config', "`"$cfg`"" -PassThru
    Start-Sleep 4
    $res.installedAppRuns = -not $app.HasExited

    Start-Process (Join-Path $dir 'uninstall.exe') -ArgumentList '/S'   # copies itself away and returns at once
    for ($i = 0; $i -lt 30 -and (Test-Path $exe); $i++) { Start-Sleep 1 }
    Start-Sleep 2
    $res.runningCopyClosed = $app.HasExited
    $res.exeRemoved = -not (Test-Path $exe)
    $res.addRemoveEntryRemoved = -not (Test-Path $uninstallKey)
    $res.startMenuShortcutRemoved = -not (Test-Path $shortcut)
    $res.configUntouched = Test-Path $cfg
}
finally {
    Get-Process vials-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$dir*" } | Stop-Process -Force
    Remove-Item $uninstallKey -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item "HKCU:\Software\VialsGpui" -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $shortcut -Force -ErrorAction SilentlyContinue
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    Remove-Item $cfgDir -Recurse -Force -ErrorAction SilentlyContinue
}
$res | ConvertTo-Json
