# End-to-end check of `vials-gpui --live` with a throw-away file and a temp config (never your real ones):
#   hide on start, save a window move, survive a hard kill, recover on the next start, restore on close.
param([string]$Exe = "$PSScriptRoot\..\..\apps\vials-gpui\target\release\vials-gpui.exe")
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
Add-Type -TypeDefinition @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class T {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  public static void Dpi() { SetProcessDPIAware(); }
  public static List<IntPtr> Of(uint pid) { var l = new List<IntPtr>();
    EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h)) l.Add(h); return true; }, IntPtr.Zero); return l; }
  public static void Move(IntPtr h, int x, int y) { SetWindowPos(h, IntPtr.Zero, x, y, 0, 0, 0x0001 | 0x0004 | 0x0010); }
  public static void Close(IntPtr h) { PostMessage(h, 0x0010, IntPtr.Zero, IntPtr.Zero); }
}
'@
[T]::Dpi()
$results = [ordered]@{}
$dir = Join-Path $env:TEMP "vials-live-test-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $dir | Out-Null
$file = Join-Path ([Environment]::GetFolderPath('Desktop')) 'zz-vials-gpui-test.txt'
Set-Content $file 'throw-away file for the vials-gpui live test'
$cfg = Join-Path $dir 'config.json'
@{ version = 1; hideOnDesktop = $true; vials = @(@{ id = 'vial-1'; name = 'Live test'; monitor = $null
   rect = @{ x = 0.4; y = 0.3; width = 0.1; height = 0.2 }; items = @($file) }) } | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding utf8
function Hidden { ((Get-Item -LiteralPath $file -Force).Attributes -band [IO.FileAttributes]::Hidden) -ne 0 }
function Start-App { Start-Process $Exe -ArgumentList '--live', '--config', "`"$cfg`"" -PassThru -RedirectStandardError (Join-Path $dir "err-$([guid]::NewGuid().ToString('N').Substring(0,4)).txt") }
try {
    $p = Start-App; Start-Sleep 4
    $results.hiddenOnStart = Hidden
    $w = [T]::Of($p.Id) | Select-Object -First 1
    $results.windowShown = [bool]$w
    $before = (Get-Content $cfg -Raw | ConvertFrom-Json).vials[0].rect.x
    $all = @([T]::Of($p.Id)); $results.windowsOfProcess = $all.Count
    foreach ($h in $all) { [T]::Move($h, 700, 500) }; Start-Sleep 2
    $after = (Get-Content $cfg -Raw | ConvertFrom-Json).vials[0].rect.x
    $results.moveSaved = ($before -ne $after)
    $results.journalWritten = Test-Path (Join-Path $dir 'desktop-journal.json')
    Stop-Process -Id $p.Id -Force; Start-Sleep 1
    $results.stillHiddenAfterKill = Hidden            # a crash leaves it hidden...
    $p = Start-App; Start-Sleep 4                     # ...and the next start recovers, then hides again
    $results.hiddenAfterRestart = Hidden
    [T]::Of($p.Id) | ForEach-Object { [T]::Close($_) }
    $p.WaitForExit(8000) | Out-Null
    $results.exitedOnClose = $p.HasExited
    Start-Sleep 1
    $results.restoredOnClose = -not (Hidden)
}
finally {
    Get-ChildItem $dir -Filter err-*.txt -ErrorAction SilentlyContinue | ForEach-Object { "--- $($_.Name)"; Get-Content $_.FullName }
    Get-Process vials-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Force
    if (Test-Path -LiteralPath $file) { $i = Get-Item -LiteralPath $file -Force; $i.Attributes = $i.Attributes -band (-bnot [IO.FileAttributes]::Hidden); Remove-Item -LiteralPath $file -Force }
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
}
$results | ConvertTo-Json
