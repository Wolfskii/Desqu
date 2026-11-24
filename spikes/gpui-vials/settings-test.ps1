# Opens the settings window of `vials-gpui --live` (`--action=settings`), clicks a theme button, a
# toggle and the glass tint slider with posted window messages, and checks what reached the config.
# Uses a temp config. Saves a screenshot of the window to -Shot when given.
param(
    [string]$Exe = "$PSScriptRoot\..\..\apps\vials-gpui\target\release\vials-gpui.exe",
    [string]$Shot = ''
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class S {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h, out R r);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
  public static void Init() { SetProcessDPIAware(); }
  public static List<IntPtr> Big(uint pid) { var l = new List<IntPtr>();
    EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); R r;
      if (p == pid && IsWindowVisible(h) && GetWindowRect(h, out r) && r.Rt - r.L > 150 && r.B - r.T > 100) l.Add(h); return true; }, IntPtr.Zero); return l; }
  public static int[] Rect(IntPtr h) { R r; GetWindowRect(h, out r); return new[] { r.L, r.T, r.Rt, r.B }; }
  public static int[] Client(IntPtr h) { R r; GetClientRect(h, out r); return new[] { r.Rt, r.B }; }
  public static void Top(IntPtr h, int x, int y) { SetWindowPos(h, (IntPtr)(-1), x, y, 0, 0, 0x0001 | 0x0010); }
  public static void Msg(IntPtr h, uint m, int w, int x, int y) { PostMessage(h, m, (IntPtr)w, (IntPtr)((y << 16) | (x & 0xFFFF))); }
}
'@
[S]::Init()
$res = [ordered]@{}
$dir = Join-Path $env:TEMP "vials-settings-test-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $dir | Out-Null
$cfg = Join-Path $dir 'config.json'
@{ version = 1; contextMenu = $false; createOnDrag = $false; theme = 'glass'; glassTint = 20; autoSort = $true; vials = @(@{ id = 'vial-1'; name = 'A'; monitor = $null
   rect = @{ x = 0.4; y = 0.3; width = 0.1; height = 0.2 }; items = @() }) } | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding utf8
function Cfg { Get-Content $cfg -Raw | ConvertFrom-Json }
function Launch($extra) { Start-Process $Exe -ArgumentList (@('--live', '--config', "`"$cfg`"") + $extra) -PassThru }
$p = $null
try {
    $p = Launch @(); Start-Sleep 4
    $null = Launch @('--action=settings') ; Start-Sleep 3
    $vialWin = [S]::Big($p.Id) | Sort-Object { ([S]::Rect($_))[2] - ([S]::Rect($_))[0] } | Select-Object -First 1
    $win = [S]::Big($p.Id) | Sort-Object { ([S]::Rect($_))[2] - ([S]::Rect($_))[0] } | Select-Object -Last 1
    $res.settingsWindowOpened = ($win -ne $vialWin)
    $c = [S]::Client($win); $res.clientSize = "$($c[0])x$($c[1])"
    if ($Shot) {
        $r = [S]::Rect($win)
        $bmp = [System.Drawing.Bitmap]::new($r[2] - $r[0], $r[3] - $r[1])
        [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($r[0], $r[1], 0, 0, $bmp.Size)
        $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    # Client coordinates (the window is 560 x 400 at 100 %): the "Classic" button, the "Create vials by
    # dragging a box" toggle, and the middle of the tint slider.
    $scale = $c[0] / 560.0
    function Click($x, $y) { [S]::Msg($win, 0x0201, 1, [int]($x * $scale), [int]($y * $scale)); Start-Sleep -Milliseconds 100; [S]::Msg($win, 0x0202, 0, [int]($x * $scale), [int]($y * $scale)); Start-Sleep 1 }
    Click 437 75;  $res.theme = (Cfg).theme
    Click 297 274; $res.createOnDrag = (Cfg).createOnDrag
    Click 384 142; $res.glassTint = (Cfg).glassTint
    Click 297 187; $res.autoSort = (Cfg).autoSort
}
finally {
    if ($p) { Start-Process $Exe -ArgumentList '--live', '--config', "`"$cfg`"", '--action=quit' -Wait; $p.WaitForExit(5000) | Out-Null }
    Get-Process vials-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Force
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
}
$res | ConvertTo-Json
