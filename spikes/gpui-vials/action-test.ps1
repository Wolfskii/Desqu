# Single instance, the tray window, the desktop's "Create vial here" entry and `--action=...`:
#   registers the entry and the tray, a second launch with --action=create-vial adds a vial in the
#   first copy and exits, a plain second launch does not start another copy, --action=quit ends the
#   first copy and removes the entry. Uses a temp config; the registry entry is removed afterwards
#   even if something fails.
param([string]$Exe = "$PSScriptRoot\..\..\apps\vials-gpui\target\release\vials-gpui.exe")
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public static class A { [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t); }
'@
$key = 'HKCU:\Software\Classes\DesktopBackground\Shell\DesquVials'
if (Test-Path $key) { throw "The Vials right-click entry already exists (is another Vials running?): not touching it." }
$res = [ordered]@{}
$dir = Join-Path $env:TEMP "vials-action-test-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $dir | Out-Null
$cfg = Join-Path $dir 'config.json'
@{ version = 1; contextMenu = $true; createOnDrag = $false; vials = @(@{ id = 'vial-1'; name = 'A'; monitor = $null
   rect = @{ x = 0.4; y = 0.3; width = 0.1; height = 0.2 }; items = @() }) } | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding utf8
function Launch($extra) { Start-Process $Exe -ArgumentList (@('--live', '--config', "`"$cfg`"") + $extra) -PassThru }
function Vials { @((Get-Content $cfg -Raw | ConvertFrom-Json).vials).Count }
$p = $null
try {
    $p = Launch @(); Start-Sleep 4
    $res.entryRegistered = Test-Path $key
    $res.entryCommand = if (Test-Path "$key\command") { (Get-ItemProperty "$key\command").'(default)' } else { $null }
    $res.trayWindow = ([A]::FindWindow('VialsGpuiTrayWindow', [NullString]::Value) -ne [IntPtr]::Zero)
    $res.vialsAtStart = Vials

    $second = Launch @('--action=create-vial'); $second.WaitForExit(8000) | Out-Null
    $res.actionLauncherExited = $second.HasExited
    Start-Sleep 2
    $res.vialsAfterAction = Vials

    $third = Launch @(); $third.WaitForExit(8000) | Out-Null
    $res.plainSecondLaunchExited = $third.HasExited
    $res.copiesRunning = @(Get-Process vials-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe }).Count

    $quit = Launch @('--action=quit'); $quit.WaitForExit(8000) | Out-Null
    $p.WaitForExit(8000) | Out-Null
    $res.firstExitedOnQuit = $p.HasExited
    Start-Sleep 1
    $res.entryRemovedOnQuit = -not (Test-Path $key)
}
finally {
    Get-Process vials-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Force
    if (Test-Path $key) { Remove-Item $key -Recurse -Force }
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
}
$res | ConvertTo-Json
