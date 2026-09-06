[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateSet("build", "dev", "test")]
    [string]$Action
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

# Vite treats '#' in a Windows filesystem path as a URL fragment. This project
# may live below a path containing '#', so frontend tools use an isolated source copy.
$repositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$systemTemporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$workingCopy = Join-Path $systemTemporaryRoot ("ctab-web-companion-" + [Guid]::NewGuid().ToString("N"))
$exitCode = 1

try {
    $null = New-Item -ItemType Directory -Path $workingCopy
    foreach ($file in @(
        "package.json",
        "package-lock.json",
        "tsconfig.json",
        "vite.config.ts",
        "vitest.config.ts"
    )) {
        Copy-Item -LiteralPath (Join-Path $repositoryRoot $file) -Destination $workingCopy
    }
    Copy-Item -LiteralPath (Join-Path $repositoryRoot "web") -Destination $workingCopy -Recurse

    Push-Location -LiteralPath $workingCopy
    try {
        & npm ci --ignore-scripts --no-audit
        if ($LASTEXITCODE -ne 0) {
            throw "The isolated frontend dependency install failed."
        }

        switch ($Action) {
            "build" { & npm exec -- vite build }
            "dev" { & npm exec -- vite --host 127.0.0.1 }
            "test" { & npm exec -- vitest run --config vitest.config.ts }
        }
        $exitCode = $LASTEXITCODE
    }
    finally {
        Pop-Location
    }

    if ($Action -eq "build" -and $exitCode -eq 0) {
        $builtAssets = Join-Path $workingCopy "web\dist"
        $destination = Join-Path $repositoryRoot "web\dist"
        if (-not (Test-Path -LiteralPath $builtAssets)) {
            throw "The frontend build did not produce web\dist."
        }
        if (Test-Path -LiteralPath $destination) {
            [System.IO.Directory]::Delete($destination, $true)
        }
        Copy-Item -LiteralPath $builtAssets -Destination $destination -Recurse
    }
}
finally {
    if (Test-Path -LiteralPath $workingCopy) {
        $resolvedWorkingCopy = [System.IO.Path]::GetFullPath($workingCopy)
        if (-not $resolvedWorkingCopy.StartsWith($systemTemporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove a frontend working copy outside the system temp directory."
        }
        [System.IO.Directory]::Delete($resolvedWorkingCopy, $true)
    }
}

exit $exitCode
