# Checks Screenbound (GPUI) serves the browser bridge pipe while "browser integration" is on and stops
# serving it when the app quits. The app is copied to a temp folder, so no native host exe is next to it
# and no browser registry entry is written; the config is temp and PAUSED, so no window is moved.
# It never switches integration off (that would remove the real browser entries). Skips if the pipe is
# already served (the Tauri Screenbound is running).
param([string]$Exe = "$PSScriptRoot\..\..\apps\screenbound-gpui\target\release\screenbound-gpui.exe")
$ErrorActionPreference = 'Stop'
$pipe = '\\.\pipe\desqu-browser-bridge'
$res = [ordered]@{}
if (Test-Path $pipe) { Write-Host 'The bridge pipe is already served by another program; stop it first.'; exit 0 }
$dir = Join-Path $env:TEMP "screenbound-bridge-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $dir | Out-Null
Copy-Item (Resolve-Path $Exe).Path (Join-Path $dir 'screenbound-gpui.exe')
$copy = Join-Path $dir 'screenbound-gpui.exe'
@{ version = 1; enabled = $false; zones = @(@{ id = 'z'; name = 'Zone'; rect = @{ x = 0; y = 0; width = 0.75; height = 1 } }); rules = @(); browser = @{ integrationEnabled = $true } } | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $dir 'config.json') -Encoding utf8
$p = $null
try {
    $p = Start-Process $copy -ArgumentList '--autostart', '--config-dir', "`"$dir`"" -PassThru
    Start-Sleep 4
    $res.pipeServedWhileOn = Test-Path $pipe
    Start-Process $copy -ArgumentList '--config-dir', "`"$dir`"", '--action=quit' -Wait
    $p.WaitForExit(8000) | Out-Null
    Start-Sleep 1
    $res.pipeGoneAfterQuit = -not (Test-Path $pipe)
}
finally {
    Get-Process screenbound-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $copy } | Stop-Process -Force
    Start-Sleep 1
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
}
$res | ConvertTo-Json
