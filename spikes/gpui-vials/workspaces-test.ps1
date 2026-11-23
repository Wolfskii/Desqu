# Starts Workspaces (GPUI) with a temp config and checks: the window opens, the tray window exists, a
# second plain launch hands "show" to the first and exits, `--action=quit` ends the first. Saves a
# screenshot of the window to -Shot when given.
param(
    [string]$Exe = "$PSScriptRoot\..\..\apps\workspaces-gpui\target\release\workspaces-gpui.exe",
    [string]$Shot = ''
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class K {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
  public static void Init() { SetProcessDPIAware(); }
  public static List<IntPtr> Big(uint pid) { var l = new List<IntPtr>();
    EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); R r;
      if (p == pid && IsWindowVisible(h) && GetWindowRect(h, out r) && r.Rt - r.L > 150 && r.B - r.T > 100) l.Add(h); return true; }, IntPtr.Zero); return l; }
  public static int[] Rect(IntPtr h) { R r; GetWindowRect(h, out r); return new[] { r.L, r.T, r.Rt, r.B }; }
}
'@
[K]::Init()
$res = [ordered]@{}
$dir = Join-Path $env:TEMP "workspaces-test-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $dir | Out-Null
$cfg = Join-Path $dir 'config.json'
$p = $null
try {
    $p = Start-Process $Exe -ArgumentList '--config', "`"$cfg`"" -PassThru
    Start-Sleep 4
    $win = [K]::Big($p.Id) | Select-Object -First 1
    $res.windowOpened = [bool]$win
    $res.trayWindow = ([K]::FindWindow('WorkspacesGpuiTrayWindow', [NullString]::Value) -ne [IntPtr]::Zero)
    if ($Shot -and $win) {
        $r = [K]::Rect($win)
        $bmp = [System.Drawing.Bitmap]::new($r[2] - $r[0], $r[3] - $r[1])
        [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($r[0], $r[1], 0, 0, $bmp.Size)
        $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    $second = Start-Process $Exe -ArgumentList '--config', "`"$cfg`"" -PassThru
    $second.WaitForExit(8000) | Out-Null
    $res.secondLaunchExited = $second.HasExited
    $res.copiesRunning = @(Get-Process workspaces-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe }).Count
    $quit = Start-Process $Exe -ArgumentList '--config', "`"$cfg`"", '--action=quit' -PassThru
    $quit.WaitForExit(8000) | Out-Null
    $p.WaitForExit(8000) | Out-Null
    $res.exitedOnQuit = $p.HasExited
}
finally {
    Get-Process workspaces-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Force
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
}
$res | ConvertTo-Json
