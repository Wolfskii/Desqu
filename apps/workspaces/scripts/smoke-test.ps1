# End-to-end smoke test of the real Workspaces app on the real desktop.
#
# It creates ONE temporary file on the desktop, points a throwaway Vials config at it, starts
# vials.exe, and checks that the item is hidden, a vial window exists with the right styles,
# `--quit` exits cleanly, and the item comes back. Nothing else on the desktop is touched; an
# existing Vials config is backed up and restored.
#
#   build first with: cd apps/workspaces; node ../../node_modules/@tauri-apps/cli/tauri.js build --debug --no-bundle
#   cargo build -p desqu-desktop --example peek
#   pwsh apps/vials/scripts/smoke-test.ps1
param([string]$Exe)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
if (-not $Exe) { $Exe = Join-Path $root 'target/debug/workspaces.exe' }
$peek = Join-Path $root 'target/debug/examples/peek.exe'
foreach ($f in $Exe, $peek) { if (-not (Test-Path $f)) { throw "Missing $f - see the build steps at the top of this script" } }
if (Get-Process -Name workspaces -ErrorAction SilentlyContinue) { throw 'workspaces.exe is already running; quit it first' }

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
function WaitFor([scriptblock]$cond, [int]$seconds) {
    $end = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $end) { $v = & $cond; if ($v) { return $v }; Start-Sleep -Milliseconds 300 }
    return $null
}

# Virtual desktop ids as the app sees them: 16 registry bytes -> {GUID}.
function DesktopGuid([byte[]]$b) { return ('{' + ([guid]::new($b)).ToString().ToUpper() + '}') }
$vd = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops'
$ids = @(); for ($i = 0; $i + 16 -le $vd.VirtualDesktopIDs.Length; $i += 16) { $ids += DesktopGuid ([byte[]]$vd.VirtualDesktopIDs[$i..($i + 15)]) }
$current = DesktopGuid ([byte[]]$vd.CurrentVirtualDesktop[0..15])
$other = $ids | Where-Object { $_ -ne $current } | Select-Object -First 1
Write-Host "Desktops: $($ids.Count); current $current; other $other"
if ($ids.Count -lt 2) { throw 'needs at least two virtual desktops to test the "other desktop" case' }

$desktop = [Environment]::GetFolderPath('Desktop')
$temp = Join-Path $desktop ("desqu-workspaces-smoke-{0}.txt" -f $PID)
$temp2 = Join-Path $desktop ("desqu-workspaces-smoke2-{0}.txt" -f $PID)
$configDir = Join-Path $env:APPDATA 'io.github.wolfskii.workspaces'
$configPath = Join-Path $configDir 'config.json'
$journal = Join-Path $configDir 'desktop-journal.json'
$backup = "$configPath.smoke-backup"
$proc = $null

# One workspace that owns the temp item (or nothing, when $owner is empty).
function WriteConfig([string]$owner, [bool]$autoAssign) {
    $workspaces = @()
    if ($owner) {
        $ws = @{ desktopId = $owner; name = 'Smoke'; items = @() }
        # A one-element array would be unrolled to a plain string by ConvertTo-Json without the cast.
        $ws.items = [string[]]@($temp)
        $workspaces = @($ws)
    }
    @{ version = 1; workspaces = $workspaces; autoAssignNew = $autoAssign } | ConvertTo-Json -Depth 6 | Set-Content -Path $configPath -Encoding UTF8
}

try {
    New-Item -ItemType Directory -Force $configDir | Out-Null
    if (Test-Path $configPath) { Move-Item -Force $configPath $backup }

    Set-Content -Path $temp -Value 'temporary file created by the Workspaces smoke test; safe to delete'
    $before = WaitFor { Peek $temp } 30
    Check ($null -ne $before) 'the temp item shows up in the desktop view'
    if (-not $before) { throw 'item never appeared; cannot continue' }

    Write-Host ''
    Write-Host 'Scenario 1: the item is tied to ANOTHER desktop'
    WriteConfig $other $false
    $proc = Start-Process -FilePath $Exe -PassThru
    $parked = WaitFor { $p = Peek $temp; if ($p -and $p.X -le -2000) { $p } } 45
    Check ($null -ne $parked) 'the item is hidden on this desktop'
    $q = Start-Process -FilePath $Exe -ArgumentList '--quit' -PassThru -WindowStyle Hidden
    Check ($q.WaitForExit(20000)) 'the --quit launch finishes'
    Check ($proc.WaitForExit(20000)) 'Workspaces exits'
    if ($proc.HasExited) { Check ($proc.ExitCode -eq 0) 'it exits with code 0' }
    $restored = WaitFor { $p = Peek $temp; if ($p -and $p.X -gt -2000) { $p } } 20
    Check ($null -ne $restored) 'after exit the item is back on the desktop'
    if ($restored) { Check (($restored.X -eq $before.X) -and ($restored.Y -eq $before.Y)) ('it returned to its original place ({0}, {1})' -f $restored.X, $restored.Y) }
    Check (-not (Test-Path $journal)) 'the journal is cleared'

    Write-Host ''
    Write-Host 'Scenario 2: the item is tied to the CURRENT desktop'
    WriteConfig $current $false
    $proc = Start-Process -FilePath $Exe -PassThru
    Start-Sleep -Seconds 12
    $p = Peek $temp
    Check ($null -ne $p -and $p.X -gt -2000) 'the item stays visible on its own desktop'
    Check (-not $proc.HasExited) 'the app is running'

    Write-Host ''
    Write-Host 'Scenario 3: a new file joins the desktop you are on'
    WriteConfig '' $true
    Start-Process -FilePath $Exe -ArgumentList '--quit' -WindowStyle Hidden | Out-Null
    Check ($proc.WaitForExit(20000)) 'the app quits'
    $proc = Start-Process -FilePath $Exe -PassThru
    Start-Sleep -Seconds 6
    Set-Content -Path $temp2 -Value 'second temporary file; safe to delete'
    $joined = WaitFor {
        $c = Get-Content $configPath -Raw | ConvertFrom-Json
        $w = $c.workspaces | Where-Object { $_.desktopId -eq $current } | Select-Object -First 1
        if ($w -and ($w.items -contains $temp2)) { $w }
    } 40
    Check ($null -ne $joined) 'the new file is now tied to the current desktop in the config'
    $p2 = Peek $temp2
    Check ($null -ne $p2 -and $p2.X -gt -2000) 'and it is visible here'
    Start-Process -FilePath $Exe -ArgumentList '--quit' -WindowStyle Hidden | Out-Null
    Check ($proc.WaitForExit(20000)) 'the app quits again'

    Write-Host ''
    Write-Host 'Scenario 4: the app is killed while the item is hidden'
    WriteConfig $other $false
    $proc = Start-Process -FilePath $Exe -PassThru
    $parked2 = WaitFor { $p = Peek $temp; if ($p -and $p.X -le -2000) { $p } } 45
    Check ($null -ne $parked2) 'the item is hidden again'
    Stop-Process -Id $proc.Id -Force
    $proc.WaitForExit(10000) | Out-Null
    Get-Process -Name workspaces -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 1
    $still = Peek $temp
    Check ($null -ne $still -and $still.X -le -2000) 'after the kill the item is still hidden (nobody put it back)'
    Check (Test-Path $journal) 'the journal survived the kill'

    # Restart with nothing tied to any desktop: only crash recovery can bring the item back.
    WriteConfig '' $false
    $proc = Start-Process -FilePath $Exe -PassThru
    $back = WaitFor { $p = Peek $temp; if ($p -and $p.X -gt -2000) { $p } } 45
    Check ($null -ne $back) 'the next start restores the item'
    if ($back) { Check (($back.X -eq $before.X) -and ($back.Y -eq $before.Y)) ('...to its original place ({0}, {1})' -f $back.X, $back.Y) }
    Start-Process -FilePath $Exe -ArgumentList '--quit' -WindowStyle Hidden | Out-Null
    Check ($proc.WaitForExit(20000)) 'and the app quits cleanly'
    Check (-not (Test-Path $journal)) 'the journal is cleared after recovery'
}
finally {
    if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
    Remove-Item -Force -ErrorAction SilentlyContinue $temp, $temp2
    Remove-Item -Force -ErrorAction SilentlyContinue $configPath
    if (Test-Path $backup) { Move-Item -Force $backup $configPath }
    # Remove the folder again if this script created it and nothing else lives there.
    if ((Test-Path $configDir) -and -not (Get-ChildItem -Force $configDir)) { Remove-Item -Force $configDir }
}

Write-Host ''
if ($failures.Count -gt 0) {
    Write-Host ("{0} check(s) FAILED" -f $failures.Count)
    exit 1
}
Write-Host 'All checks passed.'
