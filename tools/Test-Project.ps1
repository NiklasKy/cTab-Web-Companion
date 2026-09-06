[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-LastExitCode {
    param([Parameter(Mandatory = $true)][string]$Operation)
    if ($LASTEXITCODE -ne 0) {
        throw "$Operation failed with exit code $LASTEXITCODE."
    }
}

function Get-DumpbinPath {
    $vswherePath = "C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"
    $visualStudioRoot = (& $vswherePath -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath).Trim()
    $path = Get-ChildItem -LiteralPath (Join-Path $visualStudioRoot "VC\Tools\MSVC") -Filter "dumpbin.exe" -Recurse |
        Where-Object { $_.FullName -match "Hostx64\\x64" } |
        Sort-Object -Property FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
    if ([string]::IsNullOrWhiteSpace($path)) {
        throw "dumpbin.exe was not found."
    }
    return $path
}

$repositoryRoot = Split-Path -Parent $PSScriptRoot
Push-Location -LiteralPath $repositoryRoot
try {
    & npm ci --ignore-scripts
    Assert-LastExitCode -Operation "Frontend lockfile install"
    & npm run typecheck
    Assert-LastExitCode -Operation "Frontend typecheck"
    & npm test
    Assert-LastExitCode -Operation "Frontend tests"
    & npm run build
    Assert-LastExitCode -Operation "Frontend production build"
    & npm audit --audit-level=high
    Assert-LastExitCode -Operation "Frontend dependency audit"

    & cargo fmt --all -- --check
    Assert-LastExitCode -Operation "Rust formatting check"
    & cargo clippy --workspace --all-targets -- -D warnings
    Assert-LastExitCode -Operation "Rust workspace lint"
    & cargo test --workspace
    Assert-LastExitCode -Operation "Rust workspace tests"
    & cargo test -p ctab-web-bridge enqueue_path_stays_below_the_five_millisecond_p99_budget -- --nocapture
    Assert-LastExitCode -Operation "Bridge latency test"
    & cargo clippy --target i686-pc-windows-msvc -p ctab-web-bridge --all-targets -- -D warnings
    Assert-LastExitCode -Operation "32-bit bridge lint"

    & cargo build --release --target x86_64-pc-windows-msvc -p ctab-web-bridge -p ctab-web-companion
    Assert-LastExitCode -Operation "64-bit release build"
    & cargo build --release --target i686-pc-windows-msvc -p ctab-web-bridge
    Assert-LastExitCode -Operation "32-bit bridge release build"

    $dumpbinPath = Get-DumpbinPath
    $x64Bridge = "target\x86_64-pc-windows-msvc\release\ctab_web_bridge.dll"
    $x86Bridge = "target\i686-pc-windows-msvc\release\ctab_web_bridge.dll"
    $companion = "target\x86_64-pc-windows-msvc\release\ctab-web-companion.exe"
    $x64Exports = (& $dumpbinPath /nologo /exports $x64Bridge) -join "`n"
    Assert-LastExitCode -Operation "64-bit export inspection"
    $x86Exports = (& $dumpbinPath /nologo /exports $x86Bridge) -join "`n"
    Assert-LastExitCode -Operation "32-bit export inspection"
    foreach ($export in @("RVExtension", "RVExtensionArgs", "RVExtensionVersion")) {
        if ($x64Exports -notmatch "\s$export\s") {
            throw "Missing 64-bit export: $export"
        }
    }
    foreach ($export in @("_RVExtension@12", "_RVExtensionArgs@20", "_RVExtensionVersion@8")) {
        if (-not $x86Exports.Contains($export)) {
            throw "Missing 32-bit export: $export"
        }
    }

    & hemtt check --no-color
    Assert-LastExitCode -Operation "HEMTT check"
    & hemtt build --no-color
    Assert-LastExitCode -Operation "HEMTT build"

    $forbiddenDomMatches = @(rg -n "innerHTML|outerHTML|document\.write|eval\(|new Function" web/src)
    if ($LASTEXITCODE -notin @(0, 1)) {
        throw "Frontend source scan failed."
    }
    if ($forbiddenDomMatches.Count -ne 0) {
        throw "Forbidden dynamic DOM or code execution API found: $($forbiddenDomMatches -join '; ')"
    }
    $global:LASTEXITCODE = 0

    $pbo = ".hemttout\build\addons\ctab_web_main.pbo"
    @($x64Bridge, $x86Bridge, $companion, $pbo) | ForEach-Object {
        [pscustomobject]@{
            File = $_
            Bytes = (Get-Item -LiteralPath $_).Length
            Sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_).Hash
        }
    } | Format-Table -AutoSize
}
finally {
    Pop-Location
}
