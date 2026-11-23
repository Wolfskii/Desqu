# End-to-end check of the Desqu shell with its modules, in a temp folder with a temp APPDATA (so no real
# config is read or written; Screenbound's config is pre-written paused):
#   Desqu starts the modules that are on, the window opens on request, switching a module off stops its
#   program, switching it on starts it again, and quitting Desqu quits every module it started.
param([string]$Shot = '')
$ErrorActionPreference = 'Stop'
$apps = "$PSScriptRoot\..\..\apps"
$exes = @{ desqu = "$apps\desqu-gpui\target\release\desqu-gpui.exe"; vials = "$apps\vials-gpui\target\release\vials-gpui.exe"
           workspaces = "$apps\workspaces-gpui\target\release\workspaces-gpui.exe"; screenbound = "$apps\screenbound-gpui\target\release\screenbound-gpui.exe" }
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class D {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
  public static void Init() { SetProcessDPIAware(); }
  public static IntPtr Big(uint pid) { IntPtr found = IntPtr.Zero;
    EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); R r;
      if (p == pid && IsWindowVisible(h) && GetWindowRect(h, out r) && r.Rt - r.L > 150 && r.B - r.T > 100) found = h; return true; }, IntPtr.Zero); return found; }
  public static int[] Rect(IntPtr h) { R r; GetWindowRect(h, out r); return new[] { r.L, r.T, r.Rt, r.B }; }
  public static void Msg(IntPtr h, uint m, int w, int x, int y) { PostMessage(h, m, (IntPtr)w, (IntPtr)((y << 16) | (x & 0xFFFF))); }
}
'@
[D]::Init()
$res = [ordered]@{}
$root = Join-Path $env:TEMP "desqu-test-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $root | Out-Null
foreach ($k in $exes.Keys) { Copy-Item $exes[$k] (Join-Path $root (Split-Path $exes[$k] -Leaf)) }
$env:APPDATA = Join-Path $root 'appdata'
New-Item -ItemType Directory (Join-Path $env:APPDATA 'io.github.wolfskii.desqu'), (Join-Path $env:APPDATA 'io.github.wolfskii.screenbound') | Out-Null
@{ vials = $false; workspaces = $true; screenbound = $true } | ConvertTo-Json | Set-Content (Join-Path $env:APPDATA 'io.github.wolfskii.desqu\gpui-shell.json') -Encoding utf8
@{ version = 1; enabled = $false; zones = @(@{ id = 'z'; name = 'Zone'; rect = @{ x = 0; y = 0; width = 0.75; height = 1 } }); rules = @() } | ConvertTo-Json -Depth 6 | Set-Content (Join-Path $env:APPDATA 'io.github.wolfskii.screenbound\config.json') -Encoding utf8
$desqu = Join-Path $root 'desqu-gpui.exe'
function Running($name) { @(Get-Process $name -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$root*" }).Count }
try {
    $p = Start-Process $desqu -ArgumentList '--autostart' -PassThru
    Start-Sleep 7
    $res.desquRuns = -not $p.HasExited
    $res.workspacesStarted = (Running 'workspaces-gpui') -eq 1
    $res.screenboundStarted = (Running 'screenbound-gpui') -eq 1
    $res.vialsNotStarted = (Running 'vials-gpui') -eq 0

    Start-Process $desqu -Wait   # a plain second launch: show the window
    Start-Sleep 3
    $win = [D]::Big($p.Id)
    $res.windowOpened = ($win -ne [IntPtr]::Zero)
    if ($Shot -and $win -ne [IntPtr]::Zero) {
        $r = [D]::Rect($win)
        $bmp = [System.Drawing.Bitmap]::new($r[2] - $r[0], $r[3] - $r[1])
        [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($r[0], $r[1], 0, 0, $bmp.Size)
        $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    # Client coordinates (620 x 560): the Off/On toggles of the three cards, from the top: Vials, Workspaces, Screenbound.
    function Click($x, $y) { [D]::Msg($win, 0x0201, 1, $x, $y); Start-Sleep -Milliseconds 100; [D]::Msg($win, 0x0202, 0, $x, $y); Start-Sleep 1 }
    if ($true) {
        $t = @(560, 265)   # client x, y of the Workspaces toggle
        Click $t[0] $t[1]; Start-Sleep 8
        $res.workspacesStoppedWhenOff = (Running 'workspaces-gpui') -eq 0
        Click $t[0] $t[1]; Start-Sleep 5
        $res.workspacesStartedWhenOn = (Running 'workspaces-gpui') -eq 1
    }
    Start-Process $desqu -ArgumentList '--action=quit' -Wait
    $p.WaitForExit(15000) | Out-Null
    Start-Sleep 3
    $res.desquExited = $p.HasExited
    $res.modulesQuitWithDesqu = ((Running 'workspaces-gpui') + (Running 'screenbound-gpui')) -eq 0
}
finally {
    Get-Process desqu-gpui, vials-gpui, workspaces-gpui, screenbound-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$root*" } | Stop-Process -Force
    Start-Sleep 1
    Remove-Item $root -Recurse -Force -ErrorAction SilentlyContinue
}
$res | ConvertTo-Json
