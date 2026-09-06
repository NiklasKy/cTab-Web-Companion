[CmdletBinding()]
param(
    [switch]$SkipValidation,
    [ValidatePattern('^@cTab Web Companion Test(?: v[0-9]+)?$')]
    [string]$StageFolderName = "@cTab Web Companion Test v35",
    [ValidatePattern('^cTab-Web-Companion-Test(?:-v[0-9]+)?\.zip$')]
    [string]$ArchiveFileName = "cTab-Web-Companion-Test-v35.zip"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-LastExitCode {
    param([Parameter(Mandatory = $true)][string]$Operation)
    if ($LASTEXITCODE -ne 0) {
        throw "$Operation failed with exit code $LASTEXITCODE."
    }
}

function Assert-FileExists {
    param([Parameter(Mandatory = $true)][string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Required build artifact is missing: $Path"
    }
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$buildRoot = Join-Path $repositoryRoot "build"
$stagePath = Join-Path $buildRoot $StageFolderName
$archivePath = Join-Path $buildRoot $ArchiveFileName
$resolvedBuildRoot = [System.IO.Path]::GetFullPath($buildRoot).TrimEnd('\')
$resolvedStagePath = [System.IO.Path]::GetFullPath($stagePath)
$resolvedArchivePath = [System.IO.Path]::GetFullPath($archivePath)

if (-not $resolvedStagePath.StartsWith(
    $resolvedBuildRoot + "\",
    [StringComparison]::OrdinalIgnoreCase
)) {
    throw "The test-mod staging path is outside the repository build directory."
}
if (-not $resolvedArchivePath.StartsWith(
    $resolvedBuildRoot + "\",
    [StringComparison]::OrdinalIgnoreCase
)) {
    throw "The test-mod archive path is outside the repository build directory."
}

Push-Location -LiteralPath $repositoryRoot
try {
    if (-not $SkipValidation) {
        & (Join-Path $PSScriptRoot "Test-Project.ps1")
    }

    $sources = [ordered]@{
        "addons\ctab_web_main.pbo" = ".hemttout\build\addons\ctab_web_main.pbo"
        "ctab_web_bridge.dll" = "target\i686-pc-windows-msvc\release\ctab_web_bridge.dll"
        "ctab_web_bridge_x64.dll" = "target\x86_64-pc-windows-msvc\release\ctab_web_bridge.dll"
        "ctab-web-companion.exe" = "target\x86_64-pc-windows-msvc\release\ctab-web-companion.exe"
        "mod.cpp" = "distribution\mod.cpp"
        "mod.paa" = "data\ctab companion Logo.paa"
        "README_TESTING.md" = "distribution\TESTING.md"
        "Stop-TestCompanion.ps1" = "distribution\Stop-TestCompanion.ps1"
    }
    foreach ($source in $sources.Values) {
        Assert-FileExists -Path (Join-Path $repositoryRoot $source)
    }

    if (-not (Test-Path -LiteralPath $buildRoot)) {
        $null = New-Item -ItemType Directory -Path $buildRoot
    }
    if (Test-Path -LiteralPath $stagePath) {
        [System.IO.Directory]::Delete($resolvedStagePath, $true)
    }
    $null = New-Item -ItemType Directory -Path $stagePath

    foreach ($entry in $sources.GetEnumerator()) {
        $destination = Join-Path $stagePath $entry.Key
        $destinationDirectory = Split-Path -Parent $destination
        if (-not (Test-Path -LiteralPath $destinationDirectory)) {
            $null = New-Item -ItemType Directory -Path $destinationDirectory
        }
        Copy-Item -LiteralPath (Join-Path $repositoryRoot $entry.Value) -Destination $destination
    }

    $manifestFiles = @(
        Get-ChildItem -LiteralPath $stagePath -File -Recurse |
            Sort-Object -Property FullName |
            ForEach-Object {
                [pscustomobject]@{
                    Path = $_.FullName.Substring($resolvedStagePath.Length + 1).Replace('\', '/')
                    Bytes = $_.Length
                    Sha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $_.FullName).Hash
                }
            }
    )
    $manifest = [ordered]@{
        Product = "cTab Web Companion"
        Author = "[GRP9] NiklasKy"
        Version = "1.0.0-test-v35"
        GeneratedAtUtc = [DateTime]::UtcNow.ToString("o")
        Signed = $false
        BattlEyeApproved = $false
        Files = $manifestFiles
    }
    $manifestPath = Join-Path $stagePath "build-manifest.json"
    $manifest | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $manifestPath -Encoding UTF8

    $expectedFiles = @($sources.Keys) + "build-manifest.json"
    $actualFiles = @(
        Get-ChildItem -LiteralPath $stagePath -File -Recurse |
            ForEach-Object { $_.FullName.Substring($resolvedStagePath.Length + 1) }
    )
    $unexpectedFiles = @($actualFiles | Where-Object { $_ -notin $expectedFiles })
    $missingFiles = @($expectedFiles | Where-Object { $_ -notin $actualFiles })
    if ($unexpectedFiles.Count -ne 0 -or $missingFiles.Count -ne 0) {
        throw "Unexpected staging contents. Missing: $($missingFiles -join ', '); unexpected: $($unexpectedFiles -join ', ')"
    }

    if (Test-Path -LiteralPath $archivePath) {
        [System.IO.File]::Delete($resolvedArchivePath)
    }
    Compress-Archive -LiteralPath $stagePath -DestinationPath $archivePath -CompressionLevel Optimal
    Assert-FileExists -Path $archivePath

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [System.IO.Compression.ZipFile]::OpenRead($archivePath)
    try {
        $archiveEntries = @($archive.Entries | Where-Object { -not [string]::IsNullOrEmpty($_.Name) })
        if ($archiveEntries.Count -ne $expectedFiles.Count) {
            throw "The archive contains $($archiveEntries.Count) files; expected $($expectedFiles.Count)."
        }
    }
    finally {
        $archive.Dispose()
    }

    [pscustomobject]@{
        TestMod = $stagePath
        Archive = $archivePath
        ArchiveBytes = (Get-Item -LiteralPath $archivePath).Length
        ArchiveSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash
        Files = $expectedFiles.Count
    } | Format-List
}
finally {
    Pop-Location
}
