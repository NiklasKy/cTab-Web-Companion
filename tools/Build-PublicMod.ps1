[CmdletBinding()]
param(
    [ValidatePattern('^[0-9]+\.[0-9]+\.[0-9]+$')]
    [string]$Version = "1.0.1",
    [string]$PrivateKeyPath = "",
    [string]$SigningToolsDirectory = "",
    [switch]$SkipValidation
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
        throw "Required release input is missing: $Path"
    }
}

function Resolve-SigningTool {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [string]$Directory
    )

    $candidates = @()
    if (-not [string]::IsNullOrWhiteSpace($Directory)) {
        $candidates += Join-Path $Directory "$Name.exe"
    }
    $commands = @(Get-Command "$Name.exe" -ErrorAction SilentlyContinue)
    if ($commands.Count -gt 0) {
        $candidates += $commands[0].Source
    }
    $programFilesX86 = [Environment]::GetFolderPath('ProgramFilesX86')
    if (-not [string]::IsNullOrWhiteSpace($programFilesX86)) {
        $candidates += Join-Path $programFilesX86 "Steam\steamapps\common\Arma 3 Tools\DSSignFile\$Name.exe"
    }

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            return (Resolve-Path -LiteralPath $candidate).Path
        }
    }
    throw "Unable to locate $Name.exe. Pass -SigningToolsDirectory with the Arma 3 DSSignFile directory."
}

$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$buildRoot = Join-Path $repositoryRoot "build"
$releaseRoot = Join-Path $buildRoot (Join-Path "public" $Version)
$stagePath = Join-Path $releaseRoot "@cTab Web Companion"
$archivePath = Join-Path $releaseRoot "cTab-Web-Companion-$Version.zip"
$resolvedBuildRoot = [System.IO.Path]::GetFullPath($buildRoot).TrimEnd('\')
$resolvedReleaseRoot = [System.IO.Path]::GetFullPath($releaseRoot)
$resolvedStagePath = [System.IO.Path]::GetFullPath($stagePath)
$resolvedArchivePath = [System.IO.Path]::GetFullPath($archivePath)

foreach ($target in @($resolvedReleaseRoot, $resolvedStagePath, $resolvedArchivePath)) {
    if (-not $target.StartsWith($resolvedBuildRoot + "\", [StringComparison]::OrdinalIgnoreCase)) {
        throw "A public-release output path is outside the repository build directory: $target"
    }
}

if ([string]::IsNullOrWhiteSpace($PrivateKeyPath)) {
    $PrivateKeyPath = Join-Path (Split-Path -Parent $repositoryRoot) "Arma 3 Mod BiKey\cweb_1_0_0.biprivatekey"
}
$resolvedPrivateKeyPath = [System.IO.Path]::GetFullPath($PrivateKeyPath)
$resolvedRepositoryRoot = [System.IO.Path]::GetFullPath($repositoryRoot).TrimEnd('\')
if ($resolvedPrivateKeyPath.StartsWith(
    $resolvedRepositoryRoot + "\",
    [StringComparison]::OrdinalIgnoreCase
)) {
    throw "The private signing key must remain outside the repository."
}
$resolvedPublicKeyPath = [System.IO.Path]::ChangeExtension($resolvedPrivateKeyPath, ".bikey")
Assert-FileExists -Path $resolvedPrivateKeyPath
Assert-FileExists -Path $resolvedPublicKeyPath

$signFile = Resolve-SigningTool -Name "DSSignFile" -Directory $SigningToolsDirectory
$checkSignatures = Resolve-SigningTool -Name "DSCheckSignatures" -Directory $SigningToolsDirectory

Push-Location -LiteralPath $repositoryRoot
try {
    if (-not $SkipValidation) {
        & (Join-Path $PSScriptRoot "Test-Project.ps1")
    }

    $sources = [ordered]@{
        "CHANGELOG.md" = "CHANGELOG.md"
        "addons\ctab_web_main.pbo" = ".hemttout\build\addons\ctab_web_main.pbo"
        "ctab_web_bridge_x64.dll" = "target\x86_64-pc-windows-msvc\release\ctab_web_bridge.dll"
        "ctab-web-companion.exe" = "target\x86_64-pc-windows-msvc\release\ctab-web-companion.exe"
        "mod.cpp" = "distribution\PUBLIC_MOD.cpp"
        "meta.cpp" = "distribution\PUBLIC_META.cpp"
        "mod.paa" = "data\ctab companion Logo.paa"
        "README.md" = "distribution\PUBLIC_README.md"
    }
    foreach ($source in $sources.Values) {
        Assert-FileExists -Path (Join-Path $repositoryRoot $source)
    }

    if (-not (Test-Path -LiteralPath $releaseRoot)) {
        $null = New-Item -ItemType Directory -Path $releaseRoot
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

    $addonsPath = Join-Path $stagePath "addons"
    $pboPath = Join-Path $addonsPath "ctab_web_main.pbo"
    & $signFile $resolvedPrivateKeyPath $pboPath
    Assert-LastExitCode -Operation "PBO signing"

    $signatures = @(
        Get-ChildItem -LiteralPath $addonsPath -File -Filter "ctab_web_main.pbo.*.bisign"
    )
    if ($signatures.Count -ne 1) {
        throw "Expected exactly one PBO signature, found $($signatures.Count)."
    }

    $keysPath = Join-Path $stagePath "keys"
    $null = New-Item -ItemType Directory -Path $keysPath
    $publicKeyName = Split-Path -Leaf $resolvedPublicKeyPath
    Copy-Item -LiteralPath $resolvedPublicKeyPath -Destination (Join-Path $keysPath $publicKeyName)

    $privateFiles = @(Get-ChildItem -LiteralPath $stagePath -Recurse -File -Filter "*.biprivatekey")
    if ($privateFiles.Count -ne 0) {
        throw "A private signing key was copied into the public release."
    }

    & $checkSignatures -deep $stagePath $keysPath
    Assert-LastExitCode -Operation "PBO signature verification"

    $expectedFiles = @(
        @($sources.Keys) +
        "addons\$($signatures[0].Name)" +
        "keys\$publicKeyName"
    )
    $actualFiles = @(
        Get-ChildItem -LiteralPath $stagePath -File -Recurse |
            ForEach-Object { $_.FullName.Substring($resolvedStagePath.Length + 1) }
    )
    $unexpectedFiles = @($actualFiles | Where-Object { $_ -notin $expectedFiles })
    $missingFiles = @($expectedFiles | Where-Object { $_ -notin $actualFiles })
    if ($unexpectedFiles.Count -ne 0 -or $missingFiles.Count -ne 0) {
        throw "Unexpected public-release contents. Missing: $($missingFiles -join ', '); unexpected: $($unexpectedFiles -join ', ')"
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
            throw "The public archive contains $($archiveEntries.Count) files; expected $($expectedFiles.Count)."
        }
    }
    finally {
        $archive.Dispose()
    }

    [pscustomobject]@{
        PublisherFolder = $stagePath
        Archive = $archivePath
        ArchiveBytes = (Get-Item -LiteralPath $archivePath).Length
        ArchiveSha256 = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash
        Signature = $signatures[0].Name
        PublicKey = $publicKeyName
        Files = $expectedFiles.Count
        BattlEyeApproved = $false
    } | Format-List
}
finally {
    Pop-Location
}
