# Shared helpers for bump-version.ps1 and check-versions.ps1.
# The version of an app is duplicated in several files; apps/<app>/src-tauri/Cargo.toml is the
# source of truth and the others must match it.

$script:RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..')

function Get-VersionFiles {
    param([Parameter(Mandatory)][string]$App)
    $appDir = Join-Path $script:RepoRoot "apps/$App"
    if (-not (Test-Path $appDir)) { throw "Unknown app '$App': $appDir does not exist" }

    $files = @(
        @{ Path = "apps/$App/src-tauri/Cargo.toml"; Kind = 'cargo' }
        @{ Path = "apps/$App/src-tauri/tauri.conf.json"; Kind = 'json' }
        @{ Path = "apps/$App/package.json"; Kind = 'json' }
        @{ Path = "apps/$App/module/Cargo.toml"; Kind = 'cargo'; Optional = $true }
        @{ Path = "apps/$App/ui/package.json"; Kind = 'json'; Optional = $true }
    )
    foreach ($f in $files) {
        $full = Join-Path $script:RepoRoot $f.Path
        if (Test-Path $full) {
            [pscustomobject]@{ Path = $f.Path; Full = $full; Kind = $f.Kind }
        } elseif (-not $f.Optional) {
            throw "Missing required file: $($f.Path)"
        }
    }
}

# First `version = "x"` inside [package], or the first top-level "version" of a JSON file.
function Get-FileVersion {
    param([Parameter(Mandatory)]$File)
    $text = [System.IO.File]::ReadAllText($File.Full)
    if ($File.Kind -eq 'cargo') {
        $m = [regex]::Match($text, '(?ms)^\[package\]\s*$.*?^version\s*=\s*"([^"]+)"')
    } else {
        $m = [regex]::Match($text, '"version"\s*:\s*"([^"]+)"')
    }
    if (-not $m.Success) { throw "No version found in $($File.Path)" }
    $m.Groups[1].Value
}

function Set-FileVersion {
    param([Parameter(Mandatory)]$File, [Parameter(Mandatory)][string]$Version)
    $text = [System.IO.File]::ReadAllText($File.Full)
    if ($File.Kind -eq 'cargo') {
        $re = [regex]'(?ms)(^\[package\]\s*$.*?^version\s*=\s*")[^"]+(")'
    } else {
        $re = [regex]'("version"\s*:\s*")[^"]+(")'
    }
    if (-not $re.IsMatch($text)) { throw "No version found in $($File.Path)" }
    # Replace only the first match so dependency versions further down are never touched.
    $new = $re.Replace($text, { param($m) $m.Groups[1].Value + $Version + $m.Groups[2].Value }, 1)
    # No BOM, original line endings kept (we only swapped the version text).
    [System.IO.File]::WriteAllText($File.Full, $new, (New-Object System.Text.UTF8Encoding($false)))
}

function Test-SemVer {
    param([string]$Version)
    $Version -match '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$'
}
