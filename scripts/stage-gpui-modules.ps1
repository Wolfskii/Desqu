# Copies the Vials, Workspaces and Screenbound GPUI programs next to desqu-gpui.exe, where Desqu looks for
# them. Build output only (ignored by git). Used by `task gpui:desqu`.
$ErrorActionPreference = 'Stop'
$apps = Join-Path $PSScriptRoot '..\apps'
foreach ($name in 'vials', 'workspaces', 'screenbound') {
    Copy-Item (Join-Path $apps "$name-gpui\target\release\$name-gpui.exe") (Join-Path $apps 'desqu-gpui\target\release') -Force
}
