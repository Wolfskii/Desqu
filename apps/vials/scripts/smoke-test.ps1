# End-to-end smoke test of the real Vials app on the real desktop.
#
# It creates ONE temporary file on the desktop, points a throwaway Vials config at it, starts
# vials.exe, and checks that the item is hidden, a vial window exists with the right styles,
# `--quit` exits cleanly, and the item comes back. Nothing else on the desktop is touched; an
# existing Vials config is backed up and restored.
#
#   task vials:dev builds nothing here: build first with
#   cd apps/vials; node ../../node_modules/@tauri-apps/cli/tauri.js build --debug --no-bundle
#   cargo build -p desqu-desktop --example peek
#   pwsh apps/vials/scripts/smoke-test.ps1
param([string]$Exe)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
if (-not $Exe) { $Exe = Join-Path $root 'target/debug/vials.exe' }
$peek = Join-Path $root 'target/debug/examples/peek.exe'
foreach ($f in $Exe, $peek) { if (-not (Test-Path $f)) { throw "Missing $f - see the build steps at the top of this script" } }
if (Get-Process -Name vials -ErrorAction SilentlyContinue) { throw 'vials.exe is already running; quit it first' }

Add-Type -TypeDefinition @"
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;
public static class Win {
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowText(IntPtr h, StringBuilder s, int n);
    [DllImport("user32.dll", EntryPoint = "GetWindowLongPtrW")] static extern IntPtr GetWindowLongPtr(IntPtr h, int i);
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    public class Info { public string Title; public bool Visible; public int L, T, R, B; public long ExStyle; }
    public static List<Info> ForProcess(uint pid) {
        var list = new List<Info>();
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            if (p != pid) return true;
            var sb = new StringBuilder(256); GetWindowText(h, sb, 256);
            RECT r; GetWindowRect(h, out r);
            list.Add(new Info { Title = sb.ToString(), Visible = IsWindowVisible(h), L = r.L, T = r.T, R = r.R, B = r.B,
                                ExStyle = GetWindowLongPtr(h, -20).ToInt64() });
            return true;
        }, IntPtr.Zero);
        return list;
    }
}
"@

$failures = New-Object System.Collections.Generic.List[string]
function Check([bool]$ok, [string]$what) {
    if ($ok) { Write-Host "  ok   $what" } else { Write-Host "  FAIL $what"; $failures.Add($what) }
}
function Peek([string]$id) {
    $out = (& $peek $id 2>&1 | Out-String).Trim()
    if ($out -eq 'none') { return $null }
    $parts = $out -split ' '
    if ($parts.Count -ne 2) { throw "peek failed: $out" }
    return [pscustomobject]@{ X = [int]$parts[0]; Y = [int]$parts[1] }
}
function IsMarkedHidden([string]$path) {
    return ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::Hidden) -ne 0
}
function WaitFor([scriptblock]$cond, [int]$seconds) {
    $end = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $end) { $v = & $cond; if ($v) { return $v }; Start-Sleep -Milliseconds 300 }
    return $null
}

$desktop = [Environment]::GetFolderPath('Desktop')
$temp = Join-Path $desktop ("desqu-vials-smoke-{0}.txt" -f $PID)
$configDir = Join-Path $env:APPDATA 'io.github.wolfskii.vials'
$configPath = Join-Path $configDir 'config.json'
$backup = "$configPath.smoke-backup"
$proc = $null

try {
    New-Item -ItemType Directory -Force $configDir | Out-Null
    if (Test-Path $configPath) { Move-Item -Force $configPath $backup }

    Set-Content -Path $temp -Value 'temporary file created by the Vials smoke test; safe to delete'
    Write-Host "Temp item: $temp"
    $before = WaitFor { Peek $temp } 30
    Check ($null -ne $before) 'the temp item shows up in the desktop view'
    if (-not $before) { throw 'item never appeared; cannot continue' }
    Write-Host ("  item is at ({0}, {1})" -f $before.X, $before.Y)

    $config = @{
        version = 1
        vials = @(@{ id = 'smoke-vial'; name = 'Smoke vial'; monitor = $null
                     rect = @{ x = 0.45; y = 0.55; width = 0.15; height = 0.2 }
                     items = @($temp); collapsed = $false })
        rules = @(); autoSort = $false; hideOnDesktop = $true; drawDesktop = $false
    }
    $config | ConvertTo-Json -Depth 6 | Set-Content -Path $configPath -Encoding UTF8

    Write-Host 'Starting vials.exe'
    $env:RUST_LOG = 'info'
    $proc = Start-Process -FilePath $Exe -PassThru
    $parked = WaitFor { if ($null -eq (Peek $temp)) { 'hidden' } } 45
    Check ($null -ne $parked) 'the item is hidden (marked hidden) on the real desktop'
    Check (IsMarkedHidden $temp) 'the file carries the hidden attribute (and nothing else changed about it)'

    # The window is created right after the item is parked and styled a moment later: wait for it.
    $vial = WaitFor {
        $w = [Win]::ForProcess([uint32]$proc.Id) | Where-Object { $_.Title -eq 'Smoke vial' } | Select-Object -First 1
        if ($w -and ($w.ExStyle -band 0x80) -ne 0) { $w }
    } 15
    $wins = [Win]::ForProcess([uint32]$proc.Id)
    if (-not $vial) { $vial = $wins | Where-Object { $_.Title -eq 'Smoke vial' } | Select-Object -First 1 }
    Check ($null -ne $vial) 'a window titled "Smoke vial" exists'
    if ($vial) {
        Check $vial.Visible 'the vial window is visible'
        Check (($vial.ExStyle -band 0x80) -ne 0) 'it is a tool window (not in Alt+Tab)'
        Check (($vial.ExStyle -band 0x40000) -eq 0) 'it does not force a taskbar button (no WS_EX_APPWINDOW)'
        Check (($vial.ExStyle -band 0x08000000) -ne 0) 'it is non-activating (never takes focus)'
        Check ((($vial.R - $vial.L) -gt 100) -and (($vial.B - $vial.T) -gt 60)) ('it has a real size: {0}x{1} at ({2},{3})' -f ($vial.R - $vial.L), ($vial.B - $vial.T), $vial.L, $vial.T)
    }
    $main = $wins | Where-Object { $_.Title -eq 'Vials' } | Select-Object -First 1
    Check ($null -ne $main) 'the main window exists'

    Write-Host 'Sending --quit'
    $q = Start-Process -FilePath $Exe -ArgumentList '--quit' -PassThru -WindowStyle Hidden
    Check ($q.WaitForExit(20000)) 'the --quit launch finishes'
    Check ($proc.WaitForExit(20000)) 'the running Vials exits'
    if ($proc.HasExited) { Check ($proc.ExitCode -eq 0) 'it exits with code 0' }

    $restored = WaitFor { Peek $temp } 20
    Check ($null -ne $restored) 'after exit the item is back on the desktop'
    Check (-not (IsMarkedHidden $temp)) 'the hidden attribute is gone again'
    if ($restored) { Check (($restored.X -eq $before.X) -and ($restored.Y -eq $before.Y)) ('it returned to its original place ({0}, {1})' -f $restored.X, $restored.Y) }
    Check (-not (Test-Path (Join-Path $configDir 'desktop-journal.json'))) 'the journal is cleared'

    # ---- Scenario 2: the app is killed without any chance to clean up ----------------------
    Write-Host ''
    Write-Host 'Scenario 2: kill the app while the item is hidden'
    $journal = Join-Path $configDir 'desktop-journal.json'
    $config | ConvertTo-Json -Depth 6 | Set-Content -Path $configPath -Encoding UTF8
    $proc = Start-Process -FilePath $Exe -PassThru
    $parked2 = WaitFor { if ($null -eq (Peek $temp)) { 'hidden' } } 45
    Check ($null -ne $parked2) 'the item is hidden again'
    Stop-Process -Id $proc.Id -Force
    $proc.WaitForExit(10000) | Out-Null
    Get-Process -Name vials -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 1
    $still = Peek $temp
    Check ($null -eq $still) 'after the kill the item is still hidden (nobody put it back)'
    Check (Test-Path $journal) 'the journal survived the kill'

    # Restart with the item no longer in the vial: only crash recovery can bring it back.
    $config.vials[0].items = @()
    $config | ConvertTo-Json -Depth 6 | Set-Content -Path $configPath -Encoding UTF8
    $proc = Start-Process -FilePath $Exe -PassThru
    $back = WaitFor { Peek $temp } 45
    Check ($null -ne $back) 'the next start restores the item'
    if ($back) { Check (($back.X -eq $before.X) -and ($back.Y -eq $before.Y)) ('...to its original place ({0}, {1})' -f $back.X, $back.Y) }
    Start-Process -FilePath $Exe -ArgumentList '--quit' -WindowStyle Hidden | Out-Null
    Check ($proc.WaitForExit(20000)) 'and the app quits cleanly'
    Check (-not (Test-Path $journal)) 'the journal is cleared after recovery'
}
finally {
    if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
    Remove-Item -Force -ErrorAction SilentlyContinue $temp
    Remove-Item -Force -ErrorAction SilentlyContinue $configPath
    if (Test-Path $backup) { Move-Item -Force $backup $configPath }
}

Write-Host ''
if ($failures.Count -gt 0) {
    Write-Host ("{0} check(s) FAILED" -f $failures.Count)
    exit 1
}
Write-Host 'All checks passed.'
