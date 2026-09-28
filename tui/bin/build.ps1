# Build omairc-tui and control-omairc-tui with the repository version injected.
#
# version.pri stays the single version source. OMAIRC_BUILD_VERSION overrides it
# so CI snapshot versions match the Qt build; both binaries see the same value.
#
# Usage (from anywhere):
#   .\tui\bin\build.ps1
#   $env:OMAIRC_BUILD_VERSION = '1.0.4+master.gdeadbeef'; .\tui\bin\build.ps1

$ErrorActionPreference = 'Stop'

$Root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$Tui = Join-Path $Root 'tui'
$BinDir = Join-Path $Tui 'bin'

function Get-OmaircVersion {
    $versionPri = Join-Path $Root 'version.pri'
    if (-not (Test-Path -LiteralPath $versionPri)) {
        throw "version.pri not found: $versionPri"
    }
    foreach ($line in Get-Content -LiteralPath $versionPri) {
        if ($line -match '^VERSION\s*=\s*(.+)$') {
            $version = $Matches[1].Trim()
            if (-not $version) {
                throw 'version.pri must set VERSION'
            }
            if ($version -match '[\s/:-]') {
                throw "VERSION $version is not a valid Arch pkgver (no hyphens, colons, slashes, or whitespace)."
            }
            if ($version -notmatch '^[0-9]') {
                throw "VERSION $version must start with a digit so package names stay omairc-<ver>-*."
            }
            return $version
        }
    }
    throw 'version.pri must set VERSION'
}

if (-not (Get-Command go -ErrorAction SilentlyContinue)) {
    Write-Error @'
go is required to build omairc-tui.
Install Go 1.25 or newer: winget install GoLang.Go
'@
    exit 1
}

Push-Location $Tui
try {
    $version = Get-OmaircVersion
    $buildVersion = if ($env:OMAIRC_BUILD_VERSION) { $env:OMAIRC_BUILD_VERSION } else { $version }
    $ldflags = "-X github.com/fredimachado/omairc/tui/internal/version.Value=$buildVersion"
    $goexe = (go env GOEXE).Trim()

    if (-not (Test-Path -LiteralPath $BinDir)) {
        New-Item -ItemType Directory -Path $BinDir | Out-Null
    }

    go build -ldflags $ldflags -o (Join-Path $BinDir "omairc-tui$goexe") ./cmd/omairc-tui
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

    go build -ldflags $ldflags -o (Join-Path $BinDir "control-omairc-tui$goexe") ./cmd/control-omairc-tui
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
finally {
    Pop-Location
}
