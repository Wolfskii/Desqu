# Enforces the dependency rules from AGENTS.md:
#   - a standalone app (anything under apps/ except `desqu`) never depends on the collection app
#   - module crates never depend on other module crates (shared code goes in crates/)
#   - no package.json of a standalone app depends on `desqu-app`
#   pwsh scripts/check-standalone-deps.ps1
$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
Push-Location $root
$failures = @()

function Get-CargoPackageName([string]$tomlPath) {
    $inPackage = $false
    foreach ($line in Get-Content $tomlPath) {
        if ($line -match '^\[') { $inPackage = ($line.Trim() -eq '[package]'); continue }
        if ($inPackage -and $line -match '^name\s*=\s*"([^"]+)"') { return $Matches[1] }
    }
    throw "No [package] name in $tomlPath"
}

try {
    $apps = Get-ChildItem (Join-Path $root 'apps') -Directory | Where-Object { $_.Name -ne 'desqu' }
    $modulePackages = @()
    foreach ($app in $apps) {
        $mod = Join-Path $app.FullName 'module/Cargo.toml'
        if (Test-Path $mod) { $modulePackages += (Get-CargoPackageName $mod) }
    }

    foreach ($app in $apps) {
        $shellToml = Join-Path $app.FullName 'src-tauri/Cargo.toml'
        if (Test-Path $shellToml) {
            $pkg = Get-CargoPackageName $shellToml
            $tree = cargo tree -p $pkg -e normal --prefix none 2>&1
            if ($LASTEXITCODE -ne 0) { throw "cargo tree failed for $pkg`n$tree" }
            # `desqu v0.1.0 (...)` is the collection app; `desqu-shell` etc. do not match.
            if ($tree | Where-Object { $_ -match '^desqu v' }) {
                $failures += "$pkg depends on the collection app (desqu)"
            }
            Write-Host "ok   $pkg does not depend on desqu"
        }

        foreach ($pj in @('package.json', 'ui/package.json')) {
            $path = Join-Path $app.FullName $pj
            if (-not (Test-Path $path)) { continue }
            $json = Get-Content $path -Raw | ConvertFrom-Json
            $deps = @()
            foreach ($section in 'dependencies', 'devDependencies', 'peerDependencies') {
                if ($json.$section) { $deps += $json.$section.PSObject.Properties.Name }
            }
            if ($deps -contains 'desqu-app') { $failures += "apps/$($app.Name)/$pj depends on desqu-app" }
            else { Write-Host "ok   apps/$($app.Name)/$pj does not depend on desqu-app" }
        }

        $modToml = Join-Path $app.FullName 'module/Cargo.toml'
        if (Test-Path $modToml) {
            $modPkg = Get-CargoPackageName $modToml
            $tree = cargo tree -p $modPkg -e normal --prefix none 2>&1
            if ($LASTEXITCODE -ne 0) { throw "cargo tree failed for $modPkg`n$tree" }
            $others = $modulePackages | Where-Object { $_ -ne $modPkg }
            foreach ($other in $others) {
                if ($tree | Where-Object { $_ -match "^$([regex]::Escape($other)) v" }) {
                    $failures += "$modPkg depends on another module crate ($other)"
                }
            }
            Write-Host "ok   $modPkg depends on no other module crate"
        }
    }
} finally {
    Pop-Location
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Host "FAIL $_" }
    Write-Error "Dependency rules violated: $($failures.Count) problem(s)."
    exit 1
}
Write-Host 'Dependency rules hold.'
