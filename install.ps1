#!/usr/bin/env pwsh
# install.ps1 — thin bootstrap: acquire the axon binary, then hand off to `axon setup`.
# Windows mirror of install.sh. All prerequisite checks happen inside `axon setup`.
#
# Environment controls:
#   AXON_INSTALL_REPO        GitHub repo (default: jmagar/axon)
#   AXON_VERSION             Release version tag, e.g. v5.8.1 (default: latest)
#   AXON_INSTALL_PREFIX      Install prefix (default: $HOME\.local)
#   AXON_INSTALL_DRY_RUN     Set to 1 to print what would happen without doing it
#   AXON_INSTALL_SKIP_SETUP  Set to 1 to skip running `axon setup` after install
#
# Usage:
#   irm https://raw.githubusercontent.com/jmagar/axon/main/install.ps1 | iex
#
# Pinned version:
#   $env:AXON_VERSION='v5.8.1'; irm .../install.ps1 | iex
#
# Skip setup (install binary only):
#   $env:AXON_INSTALL_SKIP_SETUP='1'; irm .../install.ps1 | iex

[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'

$Repo      = if ($env:AXON_INSTALL_REPO)   { $env:AXON_INSTALL_REPO }   else { 'jmagar/axon' }
$Version   = if ($env:AXON_VERSION)        { $env:AXON_VERSION }        else { 'latest' }
$Prefix    = if ($env:AXON_INSTALL_PREFIX) { $env:AXON_INSTALL_PREFIX } else { Join-Path $HOME '.local' }
$BinDir    = Join-Path $Prefix 'bin'
$Exe       = Join-Path $BinDir 'axon.exe'
$DryRun    = $env:AXON_INSTALL_DRY_RUN    -eq '1'
$SkipSetup = $env:AXON_INSTALL_SKIP_SETUP -eq '1'

function Say { param([string]$Msg) Write-Host $Msg }
function Fail { param([string]$Msg) Write-Error "axon install: $Msg"; exit 1 }

function Get-AssetUrl {
    param([string]$Target, [string]$Ext)
    if ($Version -eq 'latest') {
        "https://github.com/$Repo/releases/latest/download/axon-$Target.$Ext"
    } else {
        "https://github.com/$Repo/releases/download/$Version/axon-$Target.$Ext"
    }
}

if ($DryRun) {
    Say "Dry run OK: target=windows-x86_64 prefix=$Prefix repo=$Repo version=$Version"
    exit 0
}

$Target    = 'windows-x86_64'
$ZipUrl    = Get-AssetUrl $Target 'zip'
$Sha256Url = "$ZipUrl.sha256"

$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) "axon-install-$(Get-Random)"
New-Item -ItemType Directory -Path $TmpDir | Out-Null

try {
    $ZipPath    = Join-Path $TmpDir 'axon.zip'
    $Sha256Path = Join-Path $TmpDir 'axon.zip.sha256'

    Say "Downloading $ZipUrl"
    Invoke-WebRequest -Uri $ZipUrl -OutFile $ZipPath -UseBasicParsing
    Say "Downloading $Sha256Url"
    Invoke-WebRequest -Uri $Sha256Url -OutFile $Sha256Path -UseBasicParsing

    # Verify checksum (release.yml writes lowercase hex; Get-FileHash returns uppercase)
    $Expected = ((Get-Content $Sha256Path -Raw).Trim() -split '\s+')[0].ToLower()
    if ([string]::IsNullOrEmpty($Expected)) { Fail 'checksum file is empty' }
    $Actual = (Get-FileHash $ZipPath -Algorithm SHA256).Hash.ToLower()
    if ($Expected -ne $Actual) { Fail "checksum mismatch — expected $Expected, got $Actual" }

    # Extract axon.exe from the zip
    Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force
    $ExtractedExe = Join-Path $TmpDir 'axon.exe'
    if (-not (Test-Path $ExtractedExe)) { Fail 'release archive did not contain axon.exe' }

    # Install
    New-Item -ItemType Directory -Path $BinDir -Force | Out-Null
    Copy-Item -Path $ExtractedExe -Destination $Exe -Force
    Say "Installed $Exe"

    # Add BinDir to user PATH if not already present
    $UserPath  = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not $UserPath) { $UserPath = '' }
    $PathParts = $UserPath -split ';' | Where-Object { $_ -ne '' }
    if ($BinDir -notin $PathParts) {
        $NewPath = ($PathParts + $BinDir) -join ';'
        [Environment]::SetEnvironmentVariable('Path', $NewPath, 'User')
        Say "Added $BinDir to your user PATH (restart your shell to pick it up)"
        $env:Path = "$env:Path;$BinDir"
    }

    if (-not $SkipSetup) {
        Say ''
        Say 'Running axon setup...'
        & $Exe setup
    }
} finally {
    Remove-Item -Recurse -Force $TmpDir -ErrorAction SilentlyContinue
}
