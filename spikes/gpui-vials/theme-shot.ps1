# Screenshots two vials in the given theme (read-only viewer, temp config) in the wallpaper-only strip
# at the right edge of the primary monitor, so the look can be judged against the real wallpaper.
#   pwsh theme-shot.ps1 -Theme liquid -Out liquid.png
param(
    [Parameter(Mandatory)][string]$Theme,
    [Parameter(Mandatory)][string]$Out,
    [int]$Tint = 20,
    [string]$Exe = "$PSScriptRoot\..\..\apps\vials-gpui\target\release\vials-gpui.exe"
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class T {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
  public static void Init() { SetProcessDPIAware(); }
  public static List<IntPtr> Big(uint pid) { var l = new List<IntPtr>();
    EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); R r;
      if (p == pid && IsWindowVisible(h) && GetWindowRect(h, out r) && r.Rt - r.L > 150 && r.B - r.T > 100) l.Add(h); return true; }, IntPtr.Zero); return l; }
  public static int[] Rect(IntPtr h) { R r; GetWindowRect(h, out r); return new[] { r.L, r.T, r.Rt, r.B }; }
  public static void MoveTop(IntPtr h, int x, int y) { SetWindowPos(h, (IntPtr)(-1), x, y, 0, 0, 0x0001 | 0x0010); }
}
'@
[T]::Init()
$dir = Join-Path $env:TEMP "vials-theme-shot-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $dir | Out-Null
$cfg = Join-Path $dir 'config.json'
$screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
# Two vials side by side near the right edge, as fractions of the primary monitor.
$vials = @(
    @{ id = 'vial-1'; name = 'Work'; monitor = $null; rect = @{ x = 0.80; y = 0.08; width = 0.09; height = 0.30 }; items = @() },
    @{ id = 'vial-2'; name = 'Media'; monitor = $null; rect = @{ x = 0.80; y = 0.45; width = 0.09; height = 0.40 }; items = @() }
)
@{ version = 1; theme = $Theme; glassTint = $Tint; vials = $vials } | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding utf8
$p = Start-Process $Exe -ArgumentList '--config', "`"$cfg`"" -PassThru
try {
    Start-Sleep 5   # the glass is drawn in the background after the windows appear
    $wins = @([T]::Big($p.Id))
    foreach ($w in $wins) { $r = [T]::Rect($w); [T]::MoveTop($w, $r[0], $r[1]) }   # raise: vials are normally below everything
    Start-Sleep 2
    $x = [int]($screen.Width * 0.80) - 40; $y = [int]($screen.Height * 0.08) - 30
    $rect = [System.Drawing.Rectangle]::new($x, $y, [int]($screen.Width * 0.09) + 80, [int]($screen.Height * 0.85) + 60)
    $bmp = [System.Drawing.Bitmap]::new($rect.Width, $rect.Height)
    [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($rect.Location, [System.Drawing.Point]::Empty, $rect.Size)
    $bmp.Save($Out, [System.Drawing.Imaging.ImageFormat]::Png)
    "windows: $($wins.Count)"
}
finally {
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
}
