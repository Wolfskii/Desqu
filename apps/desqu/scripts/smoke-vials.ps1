# End-to-end smoke test of Desqu hosting the Vials module on the real desktop.
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

$desktop = [Environment]::GetFolderPath('Desktop')
$temp = Join-Path $desktop ("desqu-host-smoke-{0}.txt" -f $PID)
$vialsDir = Join-Path $env:APPDATA 'io.github.wolfskii.vials'
$desquDir = Join-Path $env:APPDATA 'io.github.wolfskii.desqu'
$vialsCfg = Join-Path $vialsDir 'config.json'
$modulesCfg = Join-Path $desquDir 'modules.json'
$saved = @{}
foreach ($f in $vialsCfg, $modulesCfg) { if (Test-Path $f) { $saved[$f] = "$f.smoke-backup"; Move-Item -Force $f $saved[$f] } }
$proc = $null
try {
    New-Item -ItemType Directory -Force $vialsDir, $desquDir | Out-Null
    Set-Content -Path $temp -Value 'temporary file created by the Desqu smoke test; safe to delete'
    $before = WaitFor { Peek $temp } 30
    Check ($null -ne $before) 'the temp item shows up in the desktop view'
    if (-not $before) { throw 'item never appeared' }

    '{"modules":{"vials":{"enabled":true,"startWithSystem":false}}}' | Set-Content -Path $modulesCfg -Encoding UTF8
    @{ version = 1; vials = @(@{ id = 'host-vial'; name = 'Hosted vial'; monitor = $null
         rect = @{ x = 0.45; y = 0.55; width = 0.15; height = 0.2 }; items = @($temp); collapsed = $false })
       rules = @(); autoSort = $false; hideOnDesktop = $true } | ConvertTo-Json -Depth 6 | Set-Content -Path $vialsCfg -Encoding UTF8

    $proc = Start-Process -FilePath $Exe -PassThru
    $parked = WaitFor { $p = Peek $temp; if ($p -and $p.X -le -2000) { $p } } 45
    Check ($null -ne $parked) 'Desqu started Vials and the item is parked'
    $vial = WaitFor {
        $w = [Win]::ForProcess([uint32]$proc.Id) | Where-Object { $_.Title -eq 'Hosted vial' } | Select-Object -First 1
        if ($w -and ($w.ExStyle -band 0x80) -ne 0) { $w }
    } 15
    Check ($null -ne $vial) 'the vial window exists and is a tool window'
    if ($vial) { Check $vial.Visible 'the vial window is visible' }

    $q = Start-Process -FilePath $Exe -ArgumentList '--quit' -PassThru -WindowStyle Hidden
    Check ($q.WaitForExit(20000)) 'the --quit launch finishes'
    Check ($proc.WaitForExit(20000)) 'Desqu exits'
    $restored = WaitFor { $p = Peek $temp; if ($p -and $p.X -gt -2000) { $p } } 20
    Check ($null -ne $restored) 'the item is back on the desktop'
    if ($restored) { Check (($restored.X -eq $before.X) -and ($restored.Y -eq $before.Y)) 'at its original place' }
}
finally {
    if ($proc -and -not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
    Remove-Item -Force -ErrorAction SilentlyContinue $temp, $vialsCfg, $modulesCfg
    foreach ($f in $saved.Keys) { Move-Item -Force $saved[$f] $f }
}

Write-Host ''
if ($failures.Count -gt 0) { Write-Host ("{0} check(s) FAILED" -f $failures.Count); exit 1 }
Write-Host 'All checks passed.'
