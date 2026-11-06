# CONFI — fetch the pinned PDFium native binaries
#
# PDFium is not vendored in this repository: the Windows DLL is 7.5 MiB and the
# Android .so is 6.5 MiB. This script downloads the exact release that M4 was
# validated against and verifies its published SHA-256 before extracting.
#
# Usage:
#   pwsh -File tools/fetch-pdfium.ps1              # Windows x64 only
#   pwsh -File tools/fetch-pdfium.ps1 -Android     # Windows x64 + Android arm64
#
# The resulting layout is:
#   vendor/pdfium/win-x64/{bin/pdfium.dll, include/, licenses/}
#   vendor/pdfium/android-arm64/lib/libpdfium.so
#
# Anything that renders PDFs must be pointed at the platform directory:
#   Windows:  vendor/pdfium/win-x64/bin
#   Android:  the .so goes into app/src/main/jniLibs/arm64-v8a/

param(
    [string]$Dest = (Join-Path $PSScriptRoot '..\vendor\pdfium'),
    [switch]$Android
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# PDFium 159.0.8138.0, built 2026-11-04 by github.com/bblanchon/pdfium-binaries
# from PDFium branch chromium/8138. Config: no V8, no XFA, standalone release.
$tag = 'chromium/8138'
$base = "https://github.com/bblanchon/pdfium-binaries/releases/download/$tag"

$assets = [ordered]@{
    'pdfium-win-x64.tgz' = @{
        Sha256 = '8d48973e3eb472dda0f5f2435e595ac043f9f2219dd71883392f549445afa8a5'
        Dir    = 'win-x64'
    }
}

if ($Android) {
    $assets['pdfium-android-arm64.tgz'] = @{
        Sha256 = 'fc73c8020d7204e52f7a9d8153489b3cac2e9c5dc5d70d3a37a976a7398c1a86'
        Dir    = 'android-arm64'
    }
}

New-Item -ItemType Directory -Force -Path $Dest | Out-Null

foreach ($name in $assets.Keys) {
    $info = $assets[$name]
    $archive = Join-Path $Dest $name

    if (-not (Test-Path $archive)) {
        Write-Host "downloading $name"
        Invoke-WebRequest -Uri "$base/$name" -OutFile $archive -UseBasicParsing
    }

    $actual = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actual -ne $info.Sha256) {
        throw "SHA-256 mismatch for ${name}: expected $($info.Sha256), got $actual"
    }
    Write-Host "verified $name"

    $target = Join-Path $Dest $info.Dir
    New-Item -ItemType Directory -Force -Path $target | Out-Null
    tar -xzf $archive -C $target
    Write-Host "extracted to $target"
}

Write-Host ''
Write-Host 'PDFium ready. Windows render tests:'
Write-Host '  $env:CONFI_PDFIUM_DIR = (Resolve-Path vendor\pdfium\win-x64\bin).Path'
Write-Host '  cargo test -p confi-core --features render --test render_tests'
