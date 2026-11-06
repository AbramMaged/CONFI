# CONFI — Shell Integration Prototype Test Script
# 
# PURPOSE: Validate the Windows Explorer context-menu multi-file selection
# delivery mechanism before production implementation.
#
# Prerequisites:
#   cargo build -p confi-shell
#
# Run this script from the repository root:
#   powershell -File integrations\windows\confi-shell\test_prototype.ps1

$ErrorActionPreference = "Stop"

$shellExe = ".\target\debug\confi-shell.exe"

if (-not (Test-Path $shellExe)) {
    Write-Host "ERROR: $shellExe not found. Run 'cargo build -p confi-shell' first." -ForegroundColor Red
    exit 1
}

Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "  CONFI Shell Integration Prototype — Acceptance Tests"
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host ""

$testDir = Join-Path $env:TEMP "confi-shell-test-$(Get-Date -Format 'yyyyMMdd-HHmmss')"
New-Item -ItemType Directory -Path $testDir -Force | Out-Null

$passed = 0
$failed = 0

function Test-Case {
    param(
        [string]$Name,
        [scriptblock]$Test
    )
    Write-Host "  TEST: $Name" -NoNewline
    try {
        & $Test
        Write-Host " ✓" -ForegroundColor Green
        $script:passed++
    } catch {
        Write-Host " ✗ $($_.Exception.Message)" -ForegroundColor Red
        $script:failed++
    }
}

# ── Create test fixtures ──────────────────────────────────────────────────────

# Regular PDF (just the header signature)
$pdfFile = Join-Path $testDir "document.pdf"
[System.IO.File]::WriteAllBytes($pdfFile, [byte[]](0x25, 0x50, 0x44, 0x46, 0x2D, 0x31, 0x2E, 0x37))

# Regular image
$jpgFile = Join-Path $testDir "photo.jpg"
[System.IO.File]::WriteAllBytes($jpgFile, [byte[]](0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10))

# File with spaces in path
$spacesDir = Join-Path $testDir "My Documents"
New-Item -ItemType Directory -Path $spacesDir -Force | Out-Null
$spacesFile = Join-Path $spacesDir "my report.pdf"
[System.IO.File]::WriteAllBytes($spacesFile, [byte[]](0x25, 0x50, 0x44, 0x46))

# File with non-ASCII characters
$unicodeFile = Join-Path $testDir "日本語ドキュメント.pdf"
[System.IO.File]::WriteAllBytes($unicodeFile, [byte[]](0x25, 0x50, 0x44, 0x46))

# Second PDF for merge test
$pdfFile2 = Join-Path $testDir "appendix.pdf"
[System.IO.File]::WriteAllBytes($pdfFile2, [byte[]](0x25, 0x50, 0x44, 0x46))

# Additional PDFs for 5-file test
$pdfFile3 = Join-Path $testDir "chapter3.pdf"
$pdfFile4 = Join-Path $testDir "chapter4.pdf"
$pdfFile5 = Join-Path $testDir "chapter5.pdf"
[System.IO.File]::WriteAllBytes($pdfFile3, [byte[]](0x25, 0x50, 0x44, 0x46))
[System.IO.File]::WriteAllBytes($pdfFile4, [byte[]](0x25, 0x50, 0x44, 0x46))
[System.IO.File]::WriteAllBytes($pdfFile5, [byte[]](0x25, 0x50, 0x44, 0x46))

Write-Host ""
Write-Host "  Test fixtures created in: $testDir"
Write-Host ""

# ── Test 1: Single PDF selection ──────────────────────────────────────────────

Test-Case "1. Single PDF selection" {
    $output = & $shellExe --action compress --files $pdfFile 2>&1
    $json = $output | Where-Object { $_ -match '"operation"' } | Out-String
    if (-not $json.Contains('"compress"')) { throw "Expected operation 'compress'" }
    if (-not $json.Contains("document.pdf")) { throw "Expected file path in output" }
}

# ── Test 2: Two PDF selection ─────────────────────────────────────────────────

Test-Case "2. Two PDF selection (merge)" {
    $output = & $shellExe --action merge_pdf --files $pdfFile $pdfFile2 2>&1
    $json = $output | Where-Object { $_ -match '"operation"' } | Out-String
    if (-not $json.Contains('"merge_pdf"')) { throw "Expected operation 'merge_pdf'" }
    # Both files should be present
    $fileCount = ([regex]::Matches($json, '\.pdf')).Count
    if ($fileCount -lt 2) { throw "Expected 2 file paths, found $fileCount" }
}

# ── Test 3: Five PDF selection ────────────────────────────────────────────────

Test-Case "3. Five PDF selection" {
    $output = & $shellExe --action merge_pdf --files $pdfFile $pdfFile2 $pdfFile3 $pdfFile4 $pdfFile5 2>&1
    $json = $output | Where-Object { $_ -match '"files"' } | Out-String
    $fileCount = ([regex]::Matches($json, '\.pdf')).Count
    if ($fileCount -lt 5) { throw "Expected 5 file paths, found $fileCount" }
}

# ── Test 4: Paths containing spaces ──────────────────────────────────────────

Test-Case "4. Paths containing spaces" {
    $output = & $shellExe --action compress --files "$spacesFile" 2>&1
    $stderr = ($output | Out-String)
    if (-not $stderr.Contains("my report.pdf")) { throw "Space-containing path not received correctly" }
    if ($stderr.Contains("not found") -or $stderr.Contains("File not found")) { throw "File with spaces was not found" }
}

# ── Test 5: Paths containing non-ASCII characters ────────────────────────────

Test-Case "5. Paths containing non-ASCII characters" {
    $output = & $shellExe --action compress --files "$unicodeFile" 2>&1
    $stderr = ($output | Out-String)
    if ($stderr.Contains("not found") -or $stderr.Contains("File not found")) { throw "Unicode file was not found" }
}

# ── Test 6: No missing or duplicated selected paths ──────────────────────────

Test-Case "6. No duplicated paths" {
    $output = & $shellExe --action merge_pdf --files $pdfFile $pdfFile2 2>&1
    $stdout = $output | Where-Object { $_ -notmatch '═' -and $_ -notmatch '^\s*$' -and $_ -notmatch 'File \d' -and $_ -match '"' } | Out-String
    # Check JSON output for exactly 2 files
    if ($stdout) {
        $req = $stdout | ConvertFrom-Json -ErrorAction SilentlyContinue
        if ($req -and $req.files.Count -ne 2) { throw "Expected exactly 2 files, got $($req.files.Count)" }
    }
}

# ── Test 7: Correct operation identifier ──────────────────────────────────────

Test-Case "7. Correct operation identifier" {
    $operations = @("open", "compress", "convert", "split", "merge_pdf", "resize", "batch_convert", "batch_compress")
    foreach ($op in $operations) {
        $output = & $shellExe --action $op --files $pdfFile 2>&1
        $json = $output | Where-Object { $_ -match '"operation"' } | Out-String
        if (-not $json.Contains("`"$op`"")) { throw "Operation '$op' not preserved in output" }
    }
}

# ── Test 8: Deep-link URI generated when pipe unavailable ─────────────────────

Test-Case "8. Deep-link URI format" {
    $output = & $shellExe --action compress --files $pdfFile 2>&1
    $stderr = ($output | Out-String)
    if (-not $stderr.Contains("confi://action/compress")) { throw "Expected deep-link URI in output" }
}

# ── Test 9: No valid files gracefully handled ─────────────────────────────────

Test-Case "9. No valid files (deleted file)" {
    $deletedFile = Join-Path $testDir "deleted.pdf"
    # Don't create the file — simulate a deleted file.
    $output = & $shellExe --action compress --files $deletedFile 2>&1
    $stderr = ($output | Out-String)
    if (-not $stderr.Contains("not found") -and -not $stderr.Contains("File not found")) { throw "Expected file-not-found message" }
}

# ── Test 10: Mixed valid and invalid files ────────────────────────────────────

Test-Case "10. Mixed valid and invalid files" {
    $deletedFile = Join-Path $testDir "gone.pdf"
    $output = & $shellExe --action merge_pdf --files $pdfFile $deletedFile $pdfFile2 2>&1
    $stderr = ($output | Out-String)
    # Should report the error for the missing file but still process the valid ones.
    if (-not $stderr.Contains("Valid files")) { throw "Expected valid file count report" }
}

# ── Summary ───────────────────────────────────────────────────────────────────

Write-Host ""
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan
Write-Host "  Results: $passed passed, $failed failed" -ForegroundColor $(if ($failed -gt 0) { "Red" } else { "Green" })
Write-Host "═══════════════════════════════════════════════════════════════" -ForegroundColor Cyan

# Cleanup
Remove-Item -Path $testDir -Recurse -Force -ErrorAction SilentlyContinue

if ($failed -gt 0) { exit 1 } else { exit 0 }
