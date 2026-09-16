# Run bin\build.bat from PowerShell in the current window.
# Usage (repo root or anywhere):
#   .\bin\build.ps1
#   $env:QMAKE = 'C:\Qt\6.8.0\msvc2022_64\bin\qmake.exe'; .\bin\build.ps1

$ErrorActionPreference = 'Stop'
$buildBat = Join-Path $PSScriptRoot 'build.bat'
if (-not (Test-Path -LiteralPath $buildBat)) {
    Write-Error "build.bat not found: $buildBat"
    exit 1
}

$command = '"{0}"' -f $buildBat
if ($args.Count -gt 0) {
    $quoted = $args | ForEach-Object {
        if ($_ -match '[\s"]') {
            '"{0}"' -f (($_ -replace '\\', '\\') -replace '"', '\"')
        } else {
            $_
        }
    }
    $command = "$command $($quoted -join ' ')"
}

$ErrorActionPreference = 'Continue'
cmd.exe /c $command
exit $LASTEXITCODE
