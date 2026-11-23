# Starts Screenbound (GPUI) with a temp, PAUSED config (so the engine cannot move any real window),
# and checks: the window opens, the tray window exists, a second plain launch hands "show" over, a drag on
# the monitor preview changes the zone in the config, a preset button changes it, `--action=quit` ends it.
# Saves a screenshot of the window to -Shot when given.
param(
    [string]$Exe = "$PSScriptRoot\..\..\apps\screenbound-gpui\target\release\screenbound-gpui.exe",
    [string]$Shot = ''
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class B {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr FindWindow(string c, string t);
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
  public static void Init() { SetProcessDPIAware(); }
  public static List<IntPtr> Big(uint pid) { var l = new List<IntPtr>();
    EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); R r;
      if (p == pid && IsWindowVisible(h) && GetWindowRect(h, out r) && r.Rt - r.L > 150 && r.B - r.T > 100) l.Add(h); return true; }, IntPtr.Zero); return l; }
  public static int[] Rect(IntPtr h) { R r; GetWindowRect(h, out r); return new[] { r.L, r.T, r.Rt, r.B }; }
  public static void Msg(IntPtr h, uint m, int w, int x, int y) { PostMessage(h, m, (IntPtr)w, (IntPtr)((y << 16) | (x & 0xFFFF))); }
}
'@
[B]::Init()
$res = [ordered]@{}
$dir = Join-Path $env:TEMP "screenbound-test-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $dir | Out-Null
$cfg = Join-Path $dir 'config.json'
@{ version = 1; enabled = $false; zones = @(@{ id = 'z'; name = 'Zone'; rect = @{ x = 0; y = 0; width = 0.75; height = 1 } }); rules = @() } | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding utf8
function Zone { (Get-Content $cfg -Raw | ConvertFrom-Json).zones[0].rect }
$p = $null
try {
    $p = Start-Process $Exe -ArgumentList '--config-dir', "`"$dir`"" -PassThru
    Start-Sleep 5
    $win = [B]::Big($p.Id) | Select-Object -First 1
    $res.windowOpened = [bool]$win
    $res.trayWindow = ([B]::FindWindow('ScreenboundGpuiTrayWindow', [NullString]::Value) -ne [IntPtr]::Zero)
    if ($Shot -and $win) {
        $r = [B]::Rect($win)
        $bmp = [System.Drawing.Bitmap]::new($r[2] - $r[0], $r[3] - $r[1])
        [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($r[0], $r[1], 0, 0, $bmp.Size)
        $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    $second = Start-Process $Exe -ArgumentList '--config-dir', "`"$dir`"" -PassThru
    $second.WaitForExit(8000) | Out-Null
    $res.secondLaunchExited = $second.HasExited
    $res.copiesRunning = @(Get-Process screenbound-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe }).Count

    # Client coordinates (720 x 900 at 100 %). The first monitor is drawn from x 167 to 522, y 127 to 227.
    $res.zoneBefore = "x=$((Zone).x) width=$((Zone).width)"
    [B]::Msg($win, 0x0201, 1, 256, 140); Start-Sleep -Milliseconds 150          # press at 25 % across
    [B]::Msg($win, 0x0200, 1, 350, 180); Start-Sleep -Milliseconds 150
    [B]::Msg($win, 0x0200, 1, 433, 215); Start-Sleep -Milliseconds 150
    [B]::Msg($win, 0x0202, 0, 433, 215); Start-Sleep 2                          # release at 75 % across
    $res.zoneAfterDrag = "x=$((Zone).x) width=$((Zone).width) height=$((Zone).height)"
    # The "Right 50%" preset button.
    [B]::Msg($win, 0x0201, 1, 508, 300); Start-Sleep -Milliseconds 100; [B]::Msg($win, 0x0202, 0, 508, 300); Start-Sleep 2
    $res.zoneAfterPreset = "x=$((Zone).x) width=$((Zone).width)"
}
finally {
    if ($p) {
        Start-Process $Exe -ArgumentList '--config-dir', "`"$dir`"", '--action=quit' -Wait
        $p.WaitForExit(8000) | Out-Null
        $res.exitedOnQuit = $p.HasExited
    }
    Get-Process screenbound-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Force
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
}
$res | ConvertTo-Json
