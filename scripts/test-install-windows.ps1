#!/usr/bin/env pwsh
# Exercise the real installer with local download and verifier doubles.
$ErrorActionPreference = 'Stop'
$Root = Split-Path $PSScriptRoot -Parent
$Work = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory $Work | Out-Null
try {
    $env:AXON_TEST_INSTALLER = Join-Path $Root 'install.ps1'
    $env:AXON_TEST_FIXTURE = $Work
    $env:AXON_UPDATE_MINISIGN_PUBKEY = 'independently-trusted-test-key'
    $env:AXON_INSTALL_SKIP_SETUP = '1'
    Remove-Item Env:AXON_INSTALL_DRY_RUN -ErrorAction SilentlyContinue
    Set-Content (Join-Path $Work 'archive') 'fixture archive with a matching checksum'
    Set-Content (Join-Path $Work 'verifier.ps1') 'exit ([int]$env:AXON_TEST_VERIFY_EXIT)'
    $Child = Join-Path $Work 'child.ps1'
    @'
$ErrorActionPreference = 'Stop'
function Get-Command {
    param($Name, $CommandType, $ErrorAction)
    [pscustomobject]@{Source = (Join-Path $env:AXON_TEST_FIXTURE 'verifier.ps1')}
}
function Invoke-WebRequest {
    param($Uri, $OutFile, [switch]$UseBasicParsing)
    if ($Uri.EndsWith('.sha256')) {
        $hash = (Get-FileHash (Join-Path $env:AXON_TEST_FIXTURE 'archive') -Algorithm SHA256).Hash
        Set-Content $OutFile "$hash  axon.zip"
    } elseif ($Uri.EndsWith('.minisig')) {
        Set-Content $OutFile 'signature fixture'
    } else {
        Copy-Item (Join-Path $env:AXON_TEST_FIXTURE 'archive') $OutFile
    }
}
function Expand-Archive {
    param($Path, $DestinationPath, [switch]$Force)
    Set-Content (Join-Path $env:AXON_TEST_FIXTURE 'extracted') 'reached'
    throw 'stop before installation: extraction reached'
}
& $env:AXON_TEST_INSTALLER
'@ | Set-Content $Child
    foreach ($VerifierExit in @(42, 0)) {
        $env:AXON_TEST_VERIFY_EXIT = "$VerifierExit"
        $Output = & (Join-Path $PSHOME 'pwsh') -NoLogo -NoProfile -File $Child 2>&1
        if ($LASTEXITCODE -eq 0) { throw 'fixture must stop before installation' }
        $Reached = Test-Path (Join-Path $Work 'extracted')
        if ($VerifierExit -ne 0) {
            if ($Reached) { throw 'invalid signature reached extraction' }
            if (($Output | Out-String) -notmatch 'release signature verification failed') { throw "wrong failure: $Output" }
        } elseif (-not $Reached) { throw "valid verifier result never reached extraction: $Output" }
    }
    Write-Host 'Windows installer rejects an invalid signature despite matching checksum; valid verification gates extraction.'
} finally {
    Remove-Item -Recurse -Force $Work
}
