# Installs the newest NSIS setup of an app from dist/<app> for the current user (silent).
# Used by `task <app>:deploy`; a script file because Task expands `$vars` in inline commands.
#   pwsh scripts/install-local.ps1 -App desqu
param([Parameter(Mandatory)][string]$App)

$ErrorActionPreference = 'Stop'
$dist = Join-Path (Resolve-Path (Join-Path $PSScriptRoot '..')) "dist/$App"
$setup = Get-ChildItem -Path $dist -Filter '*-setup.exe' -ErrorAction SilentlyContinue |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $setup) { throw "NSIS installer not found under $dist" }

Write-Host "Installing $($setup.FullName)"
$p = Start-Process -FilePath $setup.FullName -ArgumentList '/S' -Wait -PassThru
if ($p.ExitCode -ne 0) { exit $p.ExitCode }
