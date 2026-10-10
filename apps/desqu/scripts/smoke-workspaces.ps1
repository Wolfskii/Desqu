# End-to-end smoke test of Desqu hosting the Workspaces module on the real desktop.
#
# It creates ONE temporary file on the desktop, points a throwaway Vials config at it, starts
# vials.exe, and checks that the item is hidden, a vial window exists with the right styles,
# `--quit` exits cleanly, and the item comes back. Nothing else on the desktop is touched; an
# existing Vials config is backed up and restored.
#
#   build first with: cd apps/desqu; node ../../node_modules/@tauri-apps/cli/tauri.js build --debug --no-bundle
#   cargo build -p desqu-desktop --example peek
#   pwsh apps/vials/scripts/smoke-test.ps1
param([string]$Exe)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
if (-not $Exe) { $Exe = Join-Path $root 'target/debug/desqu.exe' }
$peek = Join-Path $root 'target/debug/examples/peek.exe'
foreach ($f in $Exe, $peek) { if (-not (Test-Path $f)) { throw "Missing $f - see the build steps at the top of this script" } }
if (Get-Process -Name desqu -ErrorAction SilentlyContinue) { throw 'desqu.exe is already running; quit it first' }

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

$vd = Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\VirtualDesktops'
function DesktopGuid([byte[]]$b) { return ('{' + ([guid]::new($b)).ToString().ToUpper() + '}') }
$current = DesktopGuid ([byte[]]$vd.CurrentVirtualDesktop[0..15])
$ids = @(); for ($i = 0; $i + 16 -le $vd.VirtualDesktopIDs.Length; $i += 16) { $ids += DesktopGuid ([byte[]]$vd.VirtualDesktopIDs[$i..($i + 15)]) }
$other = $ids | Where-Object { $_ -ne $current } | Select-Object -First 1
if (-not $other) { throw 'needs at least two virtual desktops' }

$desktop = [Environment]::GetFolderPath('Desktop')
$temp = Join-Path $desktop ("desqu-host-ws-smoke-{0}.txt" -f $PID)
$wsDir = Join-Path $env:APPDATA 'io.github.wolfskii.workspaces'
$desquDir = Join-Path $env:APPDATA 'io.github.wolfskii.desqu'
$wsCfg = Join-Path $wsDir 'config.json'
$vialsDir = Join-Path $env:APPDATA 'io.github.wolfskii.vials'
$vialsCfg = Join-Path $vialsDir 'config.json'
$temp2 = Join-Path $desktop ("desqu-host-ws-smoke2-{0}.txt" -f $PID)
$modulesCfg = Join-Path $desquDir 'modules.json'
$saved = @{}
foreach ($f in $wsCfg, $modulesCfg, $vialsCfg) { if (Test-Path $f) { $saved[$f] = "$f.smoke-backup"; Move-Item -Force $f $saved[$f] } }
$madeWsDir = -not (Test-Path $wsDir)
$proc = $null
try {
    New-Item -ItemType Directory -Force $wsDir, $desquDir | Out-Null
    Set-Content -Path $temp -Value 'temporary file created by the Desqu smoke test; safe to delete'
    $before = WaitFor { Peek $temp } 30
    Check ($null -ne $before) 'the temp item shows up in the desktop view'
    if (-not $before) { throw 'item never appeared' }

    '{"modules":{"workspaces":{"enabled":true,"startWithSystem":false}}}' | Set-Content -Path $modulesCfg -Encoding UTF8
    $ws = @{ desktopId = $other; name = 'Hosted'; items = @() }
    $ws.items = [string[]]@($temp)
    @{ version = 1; workspaces = @($ws); autoAssignNew = $false } | ConvertTo-Json -Depth 6 | Set-Content -Path $wsCfg -Encoding UTF8

    $proc = Start-Process -FilePath $Exe -PassThru
    $parked = WaitFor { $p = Peek $temp; if ($p -and $p.X -le -2000) { $p } } 45
    Check ($null -ne $parked) 'Desqu started Workspaces and the item tied to another desktop is hidden here'

    $q = Start-Process -FilePath $Exe -ArgumentList '--quit' -PassThru -WindowStyle Hidden
    Check ($q.WaitForExit(20000)) 'the --quit launch finishes'
    Check ($proc.WaitForExit(20000)) 'Desqu exits'
    $restored = WaitFor { $p = Peek $temp; if ($p -and $p.X -gt -2000) { $p } } 20
    Check ($null -ne $restored) 'the item is back on the desktop'
    if ($restored) { Check (($restored.X -eq $before.X) -and ($restored.Y -eq $before.Y)) 'at its original place' }

    # Both modules on: Vials holds item 1, Workspaces ties items 1 and 2 to another desktop. They share the layer.
    Write-Host ''
    Write-Host 'Both Vials and Workspaces on, sharing one item'
    Set-Content -Path $temp2 -Value 'second temporary file; safe to delete'
    $before2 = WaitFor { Peek $temp2 } 30
    Check ($null -ne $before2) 'the second temp item shows up in the desktop view'
    New-Item -ItemType Directory -Force $vialsDir | Out-Null
    '{"modules":{"workspaces":{"enabled":true,"startWithSystem":false},"vials":{"enabled":true,"startWithSystem":false}}}' | Set-Content -Path $modulesCfg -Encoding UTF8
    $ws.items = [string[]]@($temp, $temp2)
    @{ version = 1; workspaces = @($ws); autoAssignNew = $false } | ConvertTo-Json -Depth 6 | Set-Content -Path $wsCfg -Encoding UTF8
    @{ version = 1; vials = @(@{ id = 'host-vial'; name = 'Hosted vial'; monitor = $null
         rect = @{ x = 0.45; y = 0.55; width = 0.15; height = 0.2 }; items = [string[]]@($temp); collapsed = $false })
       rules = @(); autoSort = $false; hideOnDesktop = $true } | ConvertTo-Json -Depth 6 | Set-Content -Path $vialsCfg -Encoding UTF8
    $proc = Start-Process -FilePath $Exe -PassThru
    $p1 = WaitFor { $p = Peek $temp; if ($p -and $p.X -le -2000) { $p } } 45
    $p2 = WaitFor { $p = Peek $temp2; if ($p -and $p.X -le -2000) { $p } } 45
    Check ($null -ne $p1 -and $null -ne $p2) 'both items are hidden'
    $q = Start-Process -FilePath $Exe -ArgumentList '--quit' -PassThru -WindowStyle Hidden
    Check ($proc.WaitForExit(30000)) 'Desqu exits'
    $r1 = WaitFor { $p = Peek $temp; if ($p -and $p.X -gt -2000) { $p } } 20
    $r2 = WaitFor { $p = Peek $temp2; if ($p -and $p.X -gt -2000) { $p } } 20
    Check ($null -ne $r1 -and $null -ne $r2) 'both items are back'
    if ($r1 -and $r2) { Check (($r1.X -eq $before.X) -and ($r1.Y -eq $before.Y) -and ($r2.X -eq $before2.X) -and ($r2.Y -eq $before2.Y)) 'each at its original place' }
}
finally {
    if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
    Remove-Item -Force -ErrorAction SilentlyContinue $temp, $temp2, $wsCfg, $modulesCfg, $vialsCfg
    foreach ($f in $saved.Keys) { Move-Item -Force $saved[$f] $f }
    if ($madeWsDir -and -not (Get-ChildItem -Force $wsDir)) { Remove-Item -Force $wsDir }
}

Write-Host ''
if ($failures.Count -gt 0) { Write-Host ("{0} check(s) FAILED" -f $failures.Count); exit 1 }
Write-Host 'All checks passed.'
