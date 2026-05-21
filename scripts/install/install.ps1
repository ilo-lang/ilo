#!/usr/bin/env pwsh
# ilo installer (Windows) — downloads the latest release binary from GitHub and
# verifies its sha256 against the checksums file published with the same release
# before placing it on $PATH. Aborts on mismatch.
#
# Canonical source: https://github.com/ilo-lang/ilo/blob/main/scripts/install/install.ps1
# Served from:      https://ilo-lang.ai/install.ps1
$ErrorActionPreference = 'Stop'

$Repo = "ilo-lang/ilo"
$Target = "x86_64-pc-windows-msvc"
$Asset = "ilo-$Target.exe"
$ReleaseBase = "https://github.com/$Repo/releases/latest/download"
$BinUrl  = "$ReleaseBase/$Asset"
$SumsUrl = "$ReleaseBase/checksums-sha256.txt"

$InstallDir = "$env:USERPROFILE\.ilo\bin"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$TmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ("ilo-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $TmpDir | Out-Null

try {
    $BinPath  = Join-Path $TmpDir $Asset
    $SumsPath = Join-Path $TmpDir "checksums-sha256.txt"

    Write-Host "Downloading ilo for $Target..."
    Invoke-WebRequest -Uri $BinUrl  -OutFile $BinPath  -UseBasicParsing
    Invoke-WebRequest -Uri $SumsUrl -OutFile $SumsPath -UseBasicParsing

    # Pull the expected hash for this asset out of the combined checksum file.
    $line = Select-String -Path $SumsPath -Pattern ("\s" + [regex]::Escape($Asset) + "$") | Select-Object -First 1
    if (-not $line) {
        throw "Could not find $Asset in checksums-sha256.txt; aborting."
    }
    $expected = ($line.Line -split '\s+')[0].ToLower()

    $actual = (Get-FileHash -Path $BinPath -Algorithm SHA256).Hash.ToLower()

    if ($expected -ne $actual) {
        Write-Host "Checksum mismatch for ${Asset}:" -ForegroundColor Red
        Write-Host "  expected: $expected" -ForegroundColor Red
        Write-Host "  actual:   $actual"   -ForegroundColor Red
        throw "Refusing to install. The binary may be corrupted or tampered with."
    }

    Write-Host "Checksum OK ($actual)."

    $Dest = Join-Path $InstallDir "ilo.exe"
    Move-Item -Force -Path $BinPath -Destination $Dest

    $Version = & $Dest --version 2>$null
    if ($Version) { Write-Host "Installed $Version to $Dest" }
    else { Write-Host "Installed ilo to $Dest" }

    # Add to PATH for current user if not already there
    $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
    if ($UserPath -notlike "*$InstallDir*") {
        [Environment]::SetEnvironmentVariable('Path', "$UserPath;$InstallDir", 'User')
        $env:Path = "$env:Path;$InstallDir"
        Write-Host "Added $InstallDir to your PATH"
    }

    Write-Host "Run 'ilo --version' to verify (restart your terminal if needed)"
}
finally {
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $TmpDir
}
