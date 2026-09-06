[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$expectedExecutable = [System.IO.Path]::GetFullPath(
    (Join-Path $PSScriptRoot "ctab-web-companion.exe")
)
$matches = @(
    Get-CimInstance -ClassName Win32_Process -Filter "Name = 'ctab-web-companion.exe'" |
        Where-Object {
            -not [string]::IsNullOrWhiteSpace($_.ExecutablePath) -and
            [System.IO.Path]::GetFullPath($_.ExecutablePath).Equals(
                $expectedExecutable,
                [StringComparison]::OrdinalIgnoreCase
            )
        }
)

foreach ($process in $matches) {
    Stop-Process -Id $process.ProcessId -Force
}

Write-Host "Stopped $($matches.Count) cTab Web Companion test process(es)."
