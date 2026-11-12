# Shared helpers for bump-version.ps1 and check-versions.ps1.
# One version covers the whole Desqu family: every app (with its module and UI package) and the browser
# extension. Each app's version is duplicated in several files; apps/desqu/src-tauri/Cargo.toml is the
# source of truth and every other file must match it.

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

# Every app in apps/ that is a Tauri product (Desqu, Screenbound, Vials, Workspaces).
function Get-Products {
    Get-ChildItem (Join-Path $script:RepoRoot 'apps') -Directory |
        Where-Object { Test-Path (Join-Path $_.FullName 'src-tauri/tauri.conf.json') } |
        ForEach-Object { $_.Name }
}

# The browser extension has one version file.
function Get-ExtensionVersionFiles {
    $path = 'apps/browser-extension/package.json'
    $full = Join-Path $script:RepoRoot $path
    if (-not (Test-Path $full)) { throw "Missing required file: $path" }
    [pscustomobject]@{ Path = $path; Full = $full; Kind = 'json' }
}

# Moves an app's "## Unreleased" notes under "## <Version>" and leaves Unreleased empty. Apps with no
# notes get one line so the release has something to show. Apps without a CHANGELOG are skipped.
function Set-ChangelogRelease {
    param([Parameter(Mandatory)][string]$App, [Parameter(Mandatory)][string]$Version)
    $path = Join-Path $script:RepoRoot "apps/$App/CHANGELOG.md"
    if (-not (Test-Path $path)) { return }
    $text = [System.IO.File]::ReadAllText($path)
    if ([regex]::IsMatch($text, "(?m)^## $([regex]::Escape($Version))[ 	]*?$")) {
        throw "apps/$App/CHANGELOG.md already has a section for $Version"
    }
    $eol = if ($text.Contains("`r`n")) { "`r`n" } else { "`n" }
    $heading = [regex]::Match($text, '(?m)^## Unreleased[ 	]*?$')
    if (-not $heading.Success) { throw "No '## Unreleased' heading in apps/$App/CHANGELOG.md" }
    $bodyStart = $heading.Index + $heading.Length
    $next = [regex]::Match($text.Substring($bodyStart), '(?m)^## ')
    $bodyEnd = if ($next.Success) { $bodyStart + $next.Index } else { $text.Length }
    $body = $text.Substring($bodyStart, $bodyEnd - $bodyStart).Trim()
    if (-not $body) { $body = '- Version aligned with the rest of the Desqu family.' }
    $body = $body -replace "`r?`n", $eol
    $section = "## Unreleased$eol$eol## $Version$eol$eol$body$eol$eol"
    $new = $text.Substring(0, $heading.Index) + $section + $text.Substring($bodyEnd)
    [System.IO.File]::WriteAllText($path, $new, (New-Object System.Text.UTF8Encoding($false)))
    Write-Host ("apps/{0}/CHANGELOG.md: notes moved under {1}" -f $App, $Version)
}

# The root package-lock.json records each workspace package's version. npm ci fails when those disagree
# with the package.json files, so the bump updates them too.
function Set-LockWorkspaceVersions {
    Push-Location $script:RepoRoot
    try {
        & node (Join-Path $script:RepoRoot 'scripts/sync-lock-versions.js')
        if ($LASTEXITCODE -ne 0) { throw 'Could not update package-lock.json' }
    } finally { Pop-Location }
}
