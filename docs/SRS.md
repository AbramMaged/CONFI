# CONFI — Software Requirements Specification

**Document Version:** 1.0  
**Status:** Approved baseline  
**Date:** 2026-10-31  
**Platforms:** Windows 10/11 and Android 10+  
**Processing model:** Offline-first, local-only  
**Architecture:** Shared Rust Core + Tauri/React (Windows) + Kotlin/Compose (Android)

---

## 1. Product Overview

### 1.1 Vision

CONFI is an offline-first, privacy-respecting file utility that combines a comprehensive PDF toolkit with universal file conversion, compression, and optimization capabilities.

Users should be able to perform the most common file operations — merge PDFs, split PDFs, compress images, convert formats — without visiting an online service, without creating an account, and without their files leaving their device.

**Tagline:** Every file. One place.

### 1.2 Product Pillars

1. **PDF Toolkit** — Manipulation, organization, conversion, and compression of PDF documents.
2. **Universal Converter** — Image and file format conversion, compression, and resizing.

### 1.3 Target Users

- Students handling academic documents and submissions.
- Office employees working regularly with PDFs.
- Designers and content creators managing image formats.
- Privacy-conscious users who avoid online converters.
- Users who work with many files and need a reliable local tool.

### 1.4 Core Principles

| Principle | Requirement |
|---|---|
| Offline | Core operations require no internet connection |
| Privacy | User files remain on the local device; nothing is uploaded |
| Reliability | Original files are never overwritten or silently lost |
| Simplicity | Common operations complete in minimal steps |
| Consistency | Both platforms share the same core capabilities and design language |
| Performance | UI never blocks; large files are processed efficiently |
| Transparency | Errors, limitations, and output quality are clearly communicated |

---

## 2. First Release: CONFI Core

### 2.1 Included Features

#### PDF Toolkit

| ID | Feature | Description | Priority |
|---|---|---|---|
| PDF-01 | Merge PDF | Combine two or more PDFs into one document | P0 |
| PDF-02 | Split PDF | Extract selected pages or divide into individual-page PDFs | P0 |
| PDF-03 | Compress PDF | Reduce PDF file size with preset quality levels | P0 |
| PDF-04 | Rotate PDF | Rotate individual pages or entire document (90°/180°/270°) | P0 |
| PDF-05 | Organize PDF | Reorder, delete, and duplicate pages visually | P0 |
| PDF-06 | PDF to JPG | Export PDF pages as JPG images at configurable resolution | P0 |
| PDF-07 | JPG to PDF | Create a PDF from one or more images | P0 |
| PDF-08 | Page Preview | Display page thumbnails for selection and ordering | P0 |
| PDF-09 | Page Selection | Select individual pages and ranges | P0 |
| PDF-10 | Metadata Display | Show document properties and file size | P1 |

#### Image Toolkit

| ID | Feature | Description | Priority |
|---|---|---|---|
| IMG-01 | Image Conversion | Convert between PNG, JPG, WEBP, BMP | P0 |
| IMG-02 | Image Compression | Reduce image file size with quality control | P0 |
| IMG-03 | Image Resizing | Resize by dimensions or percentage | P0 |
| IMG-04 | Quality Control | Adjust output quality where the format supports it | P0 |
| IMG-05 | Batch Conversion | Process multiple images in a single queue | P0 |

#### General File Operations

| ID | Feature | Description | Priority |
|---|---|---|---|
| SYS-01 | File Detection | Detect actual format using file signatures | P0 |
| SYS-02 | Drag and Drop | Import files via drag-and-drop on Windows | P0 |
| SYS-03 | File Picker | Select files through native OS file pickers | P0 |
| SYS-04 | Batch Processing | Process multiple files in one managed queue | P0 |
| SYS-05 | Output Management | Choose destination directory and output naming | P0 |
| SYS-06 | Progress Tracking | Display processing progress and job status | P0 |
| SYS-07 | Error Handling | Provide human-readable error messages | P0 |
| SYS-08 | Conversion History | Store operation history locally | P1 |
| SYS-09 | Context Menu | Windows Explorer shell integration — core Windows requirement | P0 |
| SYS-10 | Android Sharing | Receive files through Android Share intents | P0 |

P0 = required for first release. P1 = included only if schedule permits.

### 2.2 Explicitly Excluded Features

The following features are out of scope for CONFI Core and must not be partially implemented:

- Any AI feature (summarization, translation, analysis, workflows)
- PDF-to-Word, PDF-to-Excel, PDF-to-PowerPoint reconstruction
- OCR and document scanning
- Electronic signatures
- PDF forms creation
- Watermarks
- PDF encryption and password removal
- PDF redaction, repair, comparison
- Video and audio conversion
- Cloud storage and synchronization
- User accounts or authentication
- Subscriptions or payment infrastructure

---

## 3. Functional Requirements

### 3.1 File Input

The application must:
- Allow multiple file selection via native file pickers.
- Support drag-and-drop input on Windows.
- Accept files shared from other applications on Android.
- Validate input files before processing begins.
- Detect unsupported and corrupted files.
- Display filename, detected format, and file size for all queued files.

### 3.2 Conversion

The application must:
- Allow users to select a target format.
- Reject unsupported conversion combinations with a clear message.
- Preserve original input files at all times.
- Allow users to select an output directory.
- Generate output files with correct extensions.
- Validate output files after generation.
- Report successful and unsuccessful conversions separately.

### 3.3 Compression

- Provide configurable compression levels where the format supports it.
- Display original file size before and after compression.
- Calculate and display percentage reduction.
- Never report success when the output file is larger than the input.
- PDF compression presets: Maximum Quality, Balanced, Smaller File.
- Compression must be format-aware; it must not blindly recompress every object in a PDF.

### 3.4 PDF Operations

**Merge**
- Accept two or more PDF files.
- Allow ordering before merging.
- Preserve page orientation and dimensions.
- Produce a single valid output document.

**Split**
- Support page-range extraction.
- Support extracting individual pages.
- Support splitting into individual-page PDFs.
- Allow output naming configuration.

**Rotate**
- Rotate individual or all pages.
- Rotation angles: 90°, 180°, 270°.
- Preserve page content and metadata.

**Organize**
- Display page thumbnails.
- Allow reordering by drag-and-drop.
- Allow deletion of individual pages.
- Allow duplication of individual pages.
- Save the result as a new PDF.

**PDF to JPG**
- Render selected or all pages as JPG files.
- Allow resolution selection.

**JPG to PDF**
- Accept multiple images.
- Allow page ordering.
- Support portrait and landscape layouts.
- Support configurable margins and image fitting.

### 3.5 Job Management

Every operation must be represented as a processing job.

**Required job fields:**
- `job_id` — unique identifier
- `operation` — operation type
- `input_files` — list of inputs
- `output_config` — destination and options
- `status` — Pending | Queued | Processing | Completed | Failed | Cancelled
- `progress_percentage` — integer 0–100, or null for indeterminate
- `created_at`, `completed_at` — timestamps
- `error` — null or ErrorDetail

**Job rules:**
- Jobs may be cancelled when the operation supports it.
- Batch jobs must track per-file results independently.
- One file failure must not cancel unrelated jobs in the same batch.

### 3.6 Windows Explorer Context-Menu Integration

Context-menu integration is a **core Windows release requirement**. It is a defining feature of the product — CONFI should be part of the user's everyday Windows file workflow, not just another application they manually open.

**Actions by selection type:**

| Selection | Actions |
|---|---|
| Single PDF | Open with CONFI · Compress · Convert to Image · Split PDF |
| Single image | Open with CONFI · Convert · Compress · Resize |
| Multiple PDFs | Merge with CONFI · Batch Compress |
| Multiple images | Batch Convert · Batch Compress |
| Mixed file types | Open with CONFI |

**Behavioral requirements:**
- File-type-aware: only expose operations valid for the selected file type(s).
- Deep-link: invoking an action opens CONFI directly in the relevant operation with files pre-loaded.
- Instance-aware: if CONFI is already running, send the request to the existing instance; otherwise launch.
- Support filenames with spaces, non-ASCII characters, and Unicode.
- Handle files that were moved or deleted between the right-click and the action execution.
- Prevent duplicate processing of the same request.

**Windows 10 implementation:**
Registry-based shell verbs registered at the user level (`HKCU\Software\Classes`). No administrator permission required.

**Windows 11 implementation:**
Windows 11's modern context menu requires a packaged application (MSIX or SparsePackage) to appear at the top level. Without packaging, entries appear under "Show more options" via classic shell verbs. The implementation approach (packaged vs. classic) must be evaluated and documented during M7 based on current Microsoft guidance. Stability takes priority over first-level menu placement — do not use unsupported workarounds.

**CLI helper (`confi-shell.exe`):**
A lightweight binary receives `--action <action> --files <path1> [<path2> ...]` arguments from the shell verb, then either launches CONFI with a deep-link URI or communicates with an existing instance via named pipe.

**Installation:** Shell verbs registered during install.  
**Uninstallation:** All CONFI registry entries removed cleanly.  
**Settings:** Enable/disable toggle for context-menu integration.

---

## 4. Non-Functional Requirements

### 4.1 Performance Targets

| Requirement | Target |
|---|---|
| Application startup | Under 3 seconds on reference hardware |
| File detection | Under 500 ms for typical files |
| UI responsiveness | Never blocks during conversion |
| Job feedback | Progress reported throughout |
| Memory usage | Large documents streamed, not fully loaded |

These are engineering targets, not guarantees across all devices.

### 4.2 Privacy and Security

- Local processing only; no file uploads.
- No mandatory user account.
- No telemetry containing filenames, paths, or file contents.
- Temporary files cleaned up on completion or crash recovery.
- Input files validated before processing.
- Output paths validated against path traversal attacks.
- Malformed documents handled defensively.
- Frontend must not have unrestricted filesystem access.

### 4.3 Reliability

- Failed jobs must not corrupt or alter original files.
- Partial outputs must be cleaned up or clearly marked as incomplete.
- Interrupted operations must not report as completed.
- Batch operations must report individual file results.

### 4.4 Platform Compatibility

**Windows:** Windows 10 and Windows 11, x64. ARM64 future evaluation.  
**Android:** Android 10 (API level 29) minimum. Scoped storage and content URIs required.

---

## 5. Acceptance Criteria

### 5.1 PDF Tests

| Test ID | Scenario | Expected Result |
|---|---|---|
| AT-PDF-01 | Merge two valid PDFs | One valid combined PDF |
| AT-PDF-02 | Merge multiple PDFs | Pages preserve specified order |
| AT-PDF-03 | Split by page range | Correct pages extracted |
| AT-PDF-04 | Split to individual pages | Separate valid PDFs for each page |
| AT-PDF-05 | Rotate selected pages | Correct orientation applied |
| AT-PDF-06 | Delete a page | Page absent from output |
| AT-PDF-07 | Reorder pages | New order preserved |
| AT-PDF-08 | Compress PDF | Valid output; size compared to original |
| AT-PDF-09 | PDF to JPG | Selected pages rendered correctly |
| AT-PDF-10 | JPG to PDF | Valid PDF containing all input images |

### 5.2 Image Tests

| Test ID | Scenario | Expected Result |
|---|---|---|
| AT-IMG-01 | PNG to JPG | Valid JPG file |
| AT-IMG-02 | JPG to WEBP | Valid WEBP file |
| AT-IMG-03 | WEBP to PNG | Valid PNG file |
| AT-IMG-04 | Resize image | Requested dimensions respected |
| AT-IMG-05 | Compress image | Output reflects quality setting |
| AT-IMG-06 | Batch conversion | All valid inputs produce outputs |
| AT-IMG-07 | Unsupported input | Clear error message; no crash |

### 5.3 Platform Tests

| Test ID | Scenario | Expected Result |
|---|---|---|
| AT-PLAT-01 | Windows file picker | Files imported correctly |
| AT-PLAT-02 | Windows drag-and-drop | Files added to queue |
| AT-PLAT-04 | Android file picker | Content URI handled correctly |
| AT-PLAT-05 | Android Share | Shared file received and processed |
| AT-PLAT-06 | Android background | Job completes or reports interruption |
| AT-PLAT-07 | Offline execution | Conversion works with no network |
| AT-PLAT-08 | Original preservation | Input file unchanged after operation |
| AT-PLAT-09 | Cancel operation | Job stops; no misleading partial output |
| AT-PLAT-10 | Insufficient storage | User receives clear error |

### 5.4 Context-Menu Acceptance Tests (Windows — all required for release)

| Test ID | Scenario | Expected Result |
|---|---|---|
| AT-CTX-01 | Right-click one PDF | Open with CONFI · Compress · Convert to Image · Split PDF appear |
| AT-CTX-02 | Right-click one image | Open with CONFI · Convert · Compress · Resize appear |
| AT-CTX-03 | Select multiple PDFs, right-click | Merge with CONFI · Batch Compress appear |
| AT-CTX-04 | Select multiple images, right-click | Batch Convert · Batch Compress appear |
| AT-CTX-05 | Select mixed file types | Only Open with CONFI appears |
| AT-CTX-06 | Invoke action, CONFI closed | CONFI launches directly to correct operation with files loaded |
| AT-CTX-07 | Invoke action, CONFI already open | Request routed to existing instance; correct operation opens with files loaded |
| AT-CTX-08 | Filename with spaces | File loaded correctly |
| AT-CTX-09 | Filename with Unicode characters | File loaded correctly |
| AT-CTX-10 | File deleted before action invoked | Graceful error; no crash |
| AT-CTX-11 | Uninstall CONFI | No CONFI entries remain in Explorer context menu |
| AT-CTX-12 | Disable in Settings | Menu entries removed; re-enabling restores them |
| AT-CTX-13 | Windows 10 — right-click PDF | Actions appear in classic context menu |
| AT-CTX-14 | Windows 11 — right-click PDF | Actions appear (top-level if packaged; under Show more options if not) |


### 5.5 Release Acceptance Conditions

The first release is ready when:
- All P0 features pass their acceptance tests.
- Core operations work on both platforms.
- Output files pass format-specific validation.
- Original files remain unchanged in all tested scenarios.
- Unsupported formats are handled without crashes.
- Failed operations do not produce success messages.
- No network connection is required for core functionality.
- All distributed dependencies have documented licenses.
- Windows Explorer context-menu integration works on Windows 10 and Windows 11.
- Windows installer and Android APK/AAB are ready.
- User-facing documentation is available.

---

## 6. Development Milestones

| Milestone | Work | Estimated Duration | Status |
|---|---|---|---|
| M0 | Repository, architecture, documentation, dependency audit | 1 week | ✅ Completed |
| M1 | Rust foundation: file detection, error types, models | 1 week | ✅ Completed |
| M2 | Image engine: conversion, compression, resizing | 1–2 weeks | ✅ Completed |
| M3 | PDF engine: merge, split, rotate (PoC validates library choice first) | 2 weeks | ✅ Completed |
| M4 | PDF engine: rendering, organize, page thumbnails | 1–2 weeks | 🟡 Rendering PoC only (Windows x64 validated; Android arm64 builds, runtime untested). Organize + thumbnails pending |
| M5 | PDF engine: compress, JPG conversion | 1–2 weeks | ⬜ |
| M6 | Windows app: Tauri + React, navigation, design system, core operations | 2 weeks | 🟡 Started: window runs, 6 operations wired to the core; batch/jobs/thumbnails pending |
| M7 | Windows Explorer context-menu integration (core Windows requirement) | 1–2 weeks | ⬜ |
| M8 | Android app: Kotlin/Compose + UniFFI integration | 2–3 weeks | ⬜ |
| M9 | Cross-platform testing, optimization, release prep | 2 weeks | ⬜ |

**Estimated remaining:** 10–13 weeks (part-time, one developer).

**M3 note:** the library decision is settled for manipulation only — `lopdf`
handles merge/split/rotate. Rendering and thumbnails still need a separate
PDFium evaluation in M4.

---

## 7. Future Roadmap

### v1.1 — Advanced PDF Tools
Page numbering, watermarks, cropping, metadata editing, PDF comparison, password protection.

### v1.2 — Document Processing
Word/Excel/PowerPoint to PDF, HTML to PDF, OCR, searchable PDFs, PDF forms.

### v1.3 — Advanced Conversions
Audio conversion, video conversion, archive management, additional image formats.

### v2.0 — Full PDF Toolkit
PDF-to-Office reconstruction, PDF/A, signatures, redaction, repair.

All future features remain subject to offline processing and licensing requirements.
