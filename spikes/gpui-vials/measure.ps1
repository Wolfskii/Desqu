# Measures one build of "three vials on the desktop": startup, idle, and a scripted drag.
#   pwsh measure.ps1 -Kind tauri -Exe ..\..\..\Desqu\target\release\vials.exe -Out tauri-1.json
#   pwsh measure.ps1 -Kind gpui  -Exe target\release\gpui-vials-spike.exe     -Out gpui-1.json
# The Tauri run uses a throw-away APPDATA/LOCALAPPDATA with three empty vials, so it never touches
# the real Vials config or hides any of the real desktop's icons.
param(
    [Parameter(Mandatory)][ValidateSet('tauri', 'gpui')][string]$Kind,
    [Parameter(Mandatory)][string]$Exe,
    [Parameter(Mandatory)][string]$Out,
    [string]$Shot = '',
    [int]$IdleSeconds = 20,
    [int]$DragSeconds = 10
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path

Add-Type -ReferencedAssemblies System.Collections, System.Linq -TypeDefinition @'
using System; using System.Linq; using System.Collections.Generic; using System.Runtime.InteropServices; using System.Text;
public static class W {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  public static void Top(long h) { SetWindowPos((IntPtr)h, (IntPtr)(-1), 0, 0, 0, 0, 0x0001 | 0x0002 | 0x0010); }
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
  public static void Dpi() { SetProcessDPIAware(); }
  // Visible top-level windows of the given processes that are big enough to be a vial.
  public static List<long[]> Windows(HashSet<uint> pids) {
    var list = new List<long[]>();
    EnumWindows((h, l) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      R r;
      if (pids.Contains(pid) && IsWindowVisible(h) && GetWindowRect(h, out r) && r.Rt - r.L > 150 && r.B - r.T > 100)
        list.Add(new long[] { (long)h, r.L, r.T, r.Rt - r.L, r.B - r.T });
      return true; }, IntPtr.Zero);
    return list;
  }
  public static void Move(long h, int x, int y) { SetWindowPos((IntPtr)h, IntPtr.Zero, x, y, 0, 0, 0x0001 | 0x0004 | 0x0010); }
}
'@
[W]::Dpi()

function Get-Tree([int]$RootPid) {
    $all = Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId
    $set = [System.Collections.Generic.HashSet[int]]::new()
    [void]$set.Add($RootPid)
    do {
        $added = 0
        foreach ($p in $all) { if ($set.Contains([int]$p.ParentProcessId) -and $set.Add([int]$p.ProcessId)) { $added++ } }
    } while ($added -gt 0)
    return ,$set
}

function Get-Snapshot($pids) {
    $perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process | Where-Object { $pids.Contains([int]$_.IDProcess) }
    $procs = Get-Process -Id ([int[]]$pids) -ErrorAction SilentlyContinue
    $gpuMem = 0.0
    $gpuUtil = 0.0
    try {
        $m = Get-Counter '\GPU Process Memory(*)\Local Usage' -ErrorAction Stop
        foreach ($s in $m.CounterSamples) { if ($s.InstanceName -match '^pid_(\d+)_' -and $pids.Contains([int]$Matches[1])) { $gpuMem += $s.CookedValue } }
        $u = Get-Counter '\GPU Engine(*)\Utilization Percentage' -ErrorAction Stop
        foreach ($s in $u.CounterSamples) { if ($s.InstanceName -match '^pid_(\d+)_' -and $pids.Contains([int]$Matches[1])) { $gpuUtil += $s.CookedValue } }
    } catch { }
    [pscustomobject]@{
        processes   = @($procs).Count
        privateMB   = [math]::Round((($perf | Measure-Object WorkingSetPrivate -Sum).Sum) / 1MB, 1)
        workingMB   = [math]::Round((($procs | Measure-Object WorkingSet64 -Sum).Sum) / 1MB, 1)
        commitMB    = [math]::Round((($procs | Measure-Object PrivateMemorySize64 -Sum).Sum) / 1MB, 1)
        threads     = ($perf | Measure-Object ThreadCount -Sum).Sum
        handles     = ($perf | Measure-Object HandleCount -Sum).Sum
        gpuMemMB    = [math]::Round($gpuMem / 1MB, 1)
        gpuUtilPct  = [math]::Round($gpuUtil, 2)
        perProcess  = @($perf | ForEach-Object { "$($_.Name)#$($_.IDProcess): $([math]::Round($_.WorkingSetPrivate/1MB,1)) MB" })
        cpuSeconds  = [math]::Round((($procs | ForEach-Object { $_.TotalProcessorTime.TotalSeconds } | Measure-Object -Sum).Sum), 3)
    }
}

function Measure-Phase([string]$Name, [int]$Seconds, $pids, [scriptblock]$Work) {
    $before = Get-Snapshot $pids
    $t0 = Get-Date
    $gpuSamples = @()
    while (((Get-Date) - $t0).TotalSeconds -lt $Seconds) {
        if ($Work) { & $Work ((Get-Date) - $t0).TotalSeconds } else { Start-Sleep -Milliseconds 500 }
        $gpuSamples += (Get-Snapshot $pids).gpuUtilPct
    }
    $elapsed = ((Get-Date) - $t0).TotalSeconds
    $after = Get-Snapshot $pids
    [pscustomobject]@{
        phase        = $Name
        seconds      = [math]::Round($elapsed, 1)
        cpuPctOfOneCore = [math]::Round((($after.cpuSeconds - $before.cpuSeconds) / $elapsed) * 100, 2)
        gpuUtilAvgPct = [math]::Round((($gpuSamples | Measure-Object -Average).Average), 2)
        snapshot     = $after
    }
}

# --- launch -----------------------------------------------------------------------------------
$root = Join-Path $env:TEMP "vials-measure-$Kind-$([guid]::NewGuid().ToString('N').Substring(0,6))"
$runKeyBefore = (Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run).PSObject.Properties.Name
$psi = [System.Diagnostics.ProcessStartInfo]::new($Exe)
$psi.UseShellExecute = $false
if ($Kind -eq 'tauri') {
    # Tauri finds its config folder through the Windows known-folder API, which ignores APPDATA, so a
    # throw-away environment does not work. Instead the real Vials folder is backed up, replaced by the
    # seeded one for the run, and put back (byte for byte) in `finally`.
    $cfg = Join-Path $env:APPDATA 'io.github.wolfskii.vials'
    $backup = Join-Path $root 'real-config-backup'
    New-Item -ItemType Directory -Force $root | Out-Null
    if (Test-Path $cfg) { Copy-Item $cfg $backup -Recurse -Force; Remove-Item (Join-Path $cfg '*') -Recurse -Force }
    else { New-Item -ItemType Directory -Force $cfg | Out-Null }
    $vials = @(@(0.10, 0.10), @(0.30, 0.20), @(0.50, 0.30)) | ForEach-Object -Begin { $i = 0 } -Process {
        $i++
        @{ id = "vial-$i"; name = "Vial $i"; monitor = $null; rect = @{ x = $_[0]; y = $_[1]; width = 0.16; height = 0.34 }
           items = @(); collapsed = $false; inbox = $false; sortKey = $null; sortDesc = $false; foldersFirst = $false }
    }
    @{ version = 1; vials = @($vials); rules = @(); autoSort = $false; hideOnDesktop = $true; drawDesktop = $false
       createOnDrag = $false; contextMenu = $false; theme = 'glass'; glassBlur = 'frosted'; glassTint = 20; declined = @() } |
        ConvertTo-Json -Depth 6 | Set-Content (Join-Path $cfg 'config.json') -Encoding utf8
    $psi.Arguments = '--autostart'   # as at sign-in: the settings window stays hidden
}
$sw = [System.Diagnostics.Stopwatch]::StartNew()
$proc = [System.Diagnostics.Process]::Start($psi)
$result = [ordered]@{ kind = $Kind; exe = $Exe; cores = [Environment]::ProcessorCount }
try {
    # Startup: until three vial-sized windows are visible.
    $wins = @()
    while ($sw.Elapsed.TotalSeconds -lt 60) {
        $tree = Get-Tree $proc.Id
        $pidSet = [System.Collections.Generic.HashSet[uint32]]::new()
        foreach ($p in $tree) { [void]$pidSet.Add([uint32]$p) }
        $wins = [W]::Windows($pidSet)
        if ($wins.Count -ge 3) { break }
        Start-Sleep -Milliseconds 100
    }
    $result.startupSeconds = [math]::Round($sw.Elapsed.TotalSeconds, 2)
    $result.vialWindows = $wins.Count
    Start-Sleep -Seconds 8   # let the first paint and the webviews settle

    $tree = Get-Tree $proc.Id
    $result.processTree = @(Get-Process -Id ([int[]]$tree) -ErrorAction SilentlyContinue | ForEach-Object { "$($_.ProcessName)#$($_.Id)" })
    $startup = Get-Snapshot $tree
    $result.afterStartup = $startup

    if ($Shot) {
        # Vials normally sit below every window and VS Code & co. cover most of the screen, so for the
        # picture the three vials are stacked, topmost, in the wallpaper-only strip at the right edge.
        Add-Type -AssemblyName System.Windows.Forms, System.Drawing
        $ws = @([W]::Windows($pidSet))
        $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
        $x = $b.Right - 1180; $y = 0; $orig = @{}
        foreach ($w in $ws) { $orig[$w[0]] = @($w[1], $w[2]); [W]::Top($w[0]); [W]::Move($w[0], $x, $y); $y += [int]$w[4] - 10 }
        Start-Sleep -Milliseconds 2000
        $rect = [System.Drawing.Rectangle]::new($x - 40, 0, 1180 + 40, [math]::Min($b.Height, $y + 40))
        $bmp = [System.Drawing.Bitmap]::new($rect.Width, $rect.Height)
        [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($rect.Location, [System.Drawing.Point]::Empty, $rect.Size)
        $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
        foreach ($w in $ws) { [W]::Move($w[0], [int]$orig[$w[0]][0], [int]$orig[$w[0]][1]) }
        $result.screenshot = $Shot
    }

    $result.idle = Measure-Phase 'idle' $IdleSeconds $tree $null

    # Drag: every vial window is moved around at ~60 Hz, as if it were being dragged.
    $pidSet = [System.Collections.Generic.HashSet[uint32]]::new()
    foreach ($p in (Get-Tree $proc.Id)) { [void]$pidSet.Add([uint32]$p) }
    $wins = [W]::Windows($pidSet)
    $result.drag = Measure-Phase 'drag' $DragSeconds $tree {
        param($t)
        foreach ($w in $wins) {
            [W]::Move($w[0], [int]($w[1] + 140 * [math]::Sin($t * 3)), [int]($w[2] + 90 * [math]::Cos($t * 3)))
        }
        Start-Sleep -Milliseconds 12
    }
    foreach ($w in $wins) { [W]::Move($w[0], [int]$w[1], [int]$w[2]) }
}
finally {
    taskkill /PID $proc.Id /T /F 2>&1 | Out-Null
    $runKeyAfter = (Get-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run).PSObject.Properties.Name
    $added = $runKeyAfter | Where-Object { $_ -notin $runKeyBefore }
    foreach ($a in $added) { Remove-ItemProperty HKCU:\Software\Microsoft\Windows\CurrentVersion\Run -Name $a; Write-Warning "removed startup entry the test app added: $a" }
    if ($Kind -eq 'tauri') {
        Start-Sleep 2
        if (Test-Path $backup) {
            Remove-Item (Join-Path $cfg '*') -Recurse -Force -ErrorAction SilentlyContinue
            Copy-Item (Join-Path $backup '*') $cfg -Recurse -Force
        }
        Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
    }
}
$result | ConvertTo-Json -Depth 8 | Set-Content $Out -Encoding utf8
$result | ConvertTo-Json -Depth 8
