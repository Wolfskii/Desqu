# Opens Screenbound (GPUI) with a temp, PAUSED config (the engine cannot move any real window) and drives
# the "Groups, rules and browser" window: adds a group, adds a rule for it, adds a site rule, then checks
# the saved config. Saves screenshots of that window to -Shot (+ .1 .2 .3 .png) when given.
param(
    [string]$Exe = "$PSScriptRoot\..\..\apps\screenbound-gpui\target\release\screenbound-gpui.exe",
    [string]$Shot = ''
)
$ErrorActionPreference = 'Stop'
$Exe = (Resolve-Path $Exe).Path
Add-Type -AssemblyName System.Windows.Forms, System.Drawing
Add-Type -TypeDefinition @'
using System; using System.Collections.Generic; using System.Runtime.InteropServices;
public static class M {
  delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] static extern bool EnumWindows(EnumProc p, IntPtr l);
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
  public static void Init() { SetProcessDPIAware(); }
  public static List<IntPtr> Big(uint pid) { var l = new List<IntPtr>();
    EnumWindows((h, x) => { uint p; GetWindowThreadProcessId(h, out p); R r;
      if (p == pid && IsWindowVisible(h) && GetWindowRect(h, out r) && r.Rt - r.L > 150 && r.B - r.T > 100) l.Add(h); return true; }, IntPtr.Zero); return l; }
  public static int[] Rect(IntPtr h) { R r; GetWindowRect(h, out r); return new[] { r.L, r.T, r.Rt, r.B }; }
  public static void Key(IntPtr h, int vk, char ch) { PostMessage(h, 0x0100, (IntPtr)vk, IntPtr.Zero); if (ch != ' ') PostMessage(h, 0x0102, (IntPtr)ch, IntPtr.Zero); PostMessage(h, 0x0101, (IntPtr)vk, IntPtr.Zero); }
  public static void Msg(IntPtr h, uint m, int w, int x, int y) { PostMessage(h, m, (IntPtr)w, (IntPtr)((y << 16) | (x & 0xFFFF))); }
}
'@
[M]::Init()
$res = [ordered]@{}
$dir = Join-Path $env:TEMP "screenbound-manage-$([guid]::NewGuid().ToString('N').Substring(0,6))"
New-Item -ItemType Directory $dir | Out-Null
$cfg = Join-Path $dir 'config.json'
@{ version = 1; enabled = $false; zones = @(@{ id = 'z'; name = 'Zone'; rect = @{ x = 0; y = 0; width = 0.75; height = 1 } }); rules = @(); browser = @{ integrationEnabled = $false } } | ConvertTo-Json -Depth 6 | Set-Content $cfg -Encoding utf8
function Cfg { Get-Content $cfg -Raw | ConvertFrom-Json }
$script:win = $null
function Click($x, $y) { [M]::Msg($script:win, 0x0201, 1, $x, $y); Start-Sleep -Milliseconds 100; [M]::Msg($script:win, 0x0202, 0, $x, $y); Start-Sleep -Milliseconds 700 }
function TypeText($text) { foreach ($ch in $text.ToCharArray()) { [M]::Key($script:win, [int][char]::ToUpper($ch), $ch); Start-Sleep -Milliseconds 80 } }
function Snap($n) {
    if (-not $Shot) { return }
    $r = [M]::Rect($script:win)
    $bmp = [System.Drawing.Bitmap]::new($r[2] - $r[0], $r[3] - $r[1])
    [System.Drawing.Graphics]::FromImage($bmp).CopyFromScreen($r[0], $r[1], 0, 0, $bmp.Size)
    $bmp.Save("$Shot.$n.png", [System.Drawing.Imaging.ImageFormat]::Png)
}
$p = $null
try {
    $p = Start-Process $Exe -ArgumentList '--config-dir', "`"$dir`"" -PassThru
    Start-Sleep 5
    $script:win = [M]::Big($p.Id) | Select-Object -First 1
    Click 100 602   # "Groups, rules and browser..." in the main window (client 720 x 900)
    $wins = @([M]::Big($p.Id))
    $res.secondWindowOpened = $wins.Count -ge 2
    $script:win = $wins | Where-Object { ([M]::Rect($_)[2] - [M]::Rect($_)[0]) -lt 700 } | Select-Object -First 1
    Snap 1
    # Client coordinates (640 x 640): Groups tab is open, name field at the bottom of the (empty) group list.
    Click 160 144; TypeText "Games"; Click 346 144
    Snap 2
    $res.groups = @((Cfg).groups | ForEach-Object { $_.name }) -join ','
    # Rules tab, then the name field and "Add rule" (positions read from the screenshots)
    Click 136 79
    Snap 3
    $res.rulesTab = $true
    Click 238 187; Click 358 248; Click 211 284; Click 160 320; TypeText "gamerule"; Click 339 320
    $rule = @((Cfg).rules) | Where-Object { $_.name -like 'gamerule*' } | Select-Object -First 1
    $res.rule = if ($rule) { "$($rule.scope.type): $(($rule.scope.group_ids) -join [char]44) -> $($rule.actions[0].type) $($rule.actions[0].target.type)" } else { 'none' }
    Snap 4
    Click 208 79
    Snap 5
    Click 160 315; TypeText 'youtube.com'; Click 378 315
    $site = (Cfg).browser.siteRules.rules | Select-Object -First 1
    $res.site = if ($site) { "$($site.pattern.value) -> $($site.mode) $($site.zoneId)" } else { 'none' }
    Click 298 79; Snap 6
}
finally {
    if ($p) { Start-Process $Exe -ArgumentList '--config-dir', "`"$dir`"", '--action=quit' -Wait; $p.WaitForExit(8000) | Out-Null }
    Get-Process screenbound-gpui -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $Exe } | Stop-Process -Force
    Start-Sleep 1
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
}
$res | ConvertTo-Json
