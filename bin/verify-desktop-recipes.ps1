#Requires -Version 5.1
# Run every feature-map desktop-recipe against the Rust client under
# control-omairc-desktop.ps1 (Windows). Evidence stays in
# test-artifacts/verify-desktop/. Cleanup between features so unread / drafts
# do not poison the next fence.
#
# Prefer pwsh; also loadable under Windows PowerShell 5.1.
# Usage (from repo root or anywhere):
#   pwsh bin\verify-desktop-recipes.ps1
#   powershell -NoProfile -File bin\verify-desktop-recipes.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$Ctrl = Join-Path $Root '.cursor\skills\verify-omairc-desktop\control-omairc-desktop.ps1'
$FeaturesDir = Join-Path $Root '.cursor\skills\verify-omairc\features'

if (-not (Test-Path -LiteralPath $Ctrl)) {
    [Console]::Error.WriteLine("verify-desktop-recipes: missing $Ctrl")
    exit 1
}
if (-not (Test-Path -LiteralPath $FeaturesDir)) {
    [Console]::Error.WriteLine("verify-desktop-recipes: missing $FeaturesDir")
    exit 1
}

# Invoke the driver in a child shell so Die/exit in the driver does not kill
# this loop. Prefer pwsh when available.
$ShellExe = $null
if (Get-Command pwsh -ErrorAction SilentlyContinue) {
    $ShellExe = (Get-Command pwsh).Source
} elseif (Get-Command powershell -ErrorAction SilentlyContinue) {
    $ShellExe = (Get-Command powershell).Source
} else {
    [Console]::Error.WriteLine('verify-desktop-recipes: need pwsh or powershell on PATH')
    exit 1
}

# Prefer pwsh child so Die/exit in the driver does not kill this loop.
# Do not assign the invocation's pipeline to a variable: child Write-Output
# would mix with any returned exit code and break -eq checks. Record the
# exit code in a script variable instead. Disable native-command EAP so a
# non-zero child exit is not turned into a terminating error under PS 7.
$script:LastControlExit = 0
function Invoke-Control {
    param(
        [Parameter(Mandatory = $true)][string[]]$ControlArgs,
        [switch]$Quiet
    )
    $prevEap = $ErrorActionPreference
    $prevNative = $null
    if (Test-Path Variable:PSNativeCommandUseErrorActionPreference) {
        $prevNative = $PSNativeCommandUseErrorActionPreference
        $PSNativeCommandUseErrorActionPreference = $false
    }
    $ErrorActionPreference = 'Continue'
    try {
        if ($Quiet) {
            & $ShellExe -NoProfile -File $Ctrl @ControlArgs 1>$null 2>$null
        } else {
            & $ShellExe -NoProfile -File $Ctrl @ControlArgs
        }
        if ($null -eq $LASTEXITCODE) {
            $script:LastControlExit = 1
        } else {
            $script:LastControlExit = [int]$LASTEXITCODE
        }
    } finally {
        $ErrorActionPreference = $prevEap
        if ($null -ne $prevNative) {
            $PSNativeCommandUseErrorActionPreference = $prevNative
        }
    }
}

Set-Location -LiteralPath $Root
$failed = 0
$ran = 0

$featureFiles = @(Get-ChildItem -LiteralPath $FeaturesDir -Filter '*.md' | Sort-Object Name)
foreach ($featureFile in $featureFiles) {
    $feature = [System.IO.Path]::GetFileNameWithoutExtension($featureFile.Name)
    if ($feature -eq 'README') { continue }
    if (-not (Select-String -LiteralPath $featureFile.FullName -Pattern '^```desktop-recipe' -Quiet)) {
        continue
    }

    $ran++
    [Console]::Error.WriteLine("verify-desktop-recipes: run $feature")
    Invoke-Control -ControlArgs @('cleanup') -Quiet
    Invoke-Control -ControlArgs @('run', $feature)
    if ($script:LastControlExit -eq 0) {
        [Console]::Error.WriteLine("verify-desktop-recipes: ok $feature")
    } else {
        [Console]::Error.WriteLine("verify-desktop-recipes: FAIL $feature")
        $failed = 1
    }
    Invoke-Control -ControlArgs @('cleanup') -Quiet
}

if ($ran -eq 0) {
    [Console]::Error.WriteLine('verify-desktop-recipes: no desktop-recipe fences found')
    exit 1
}

if ($failed -ne 0) {
    [Console]::Error.WriteLine("verify-desktop-recipes: $ran feature(s) attempted; at least one failed")
    exit 1
}

[Console]::Error.WriteLine("verify-desktop-recipes: all $ran feature(s) ok")
Write-Output ("evidence=" + (Join-Path $Root 'test-artifacts\verify-desktop'))
exit 0
