# CONFI — Supported Formats

**Document Version:** 1.0  
**Date:** 2026-10-31  
**Status:** First release (CONFI Core)

---

## Format Support Legend

| Symbol | Meaning |
|---|---|
| ✓ | Supported in CONFI Core |
| ~ | Partial support |
| ✗ | Not supported |
| Future | Planned for a later release |

---

## 1. Image Formats

### Read Support

| Format | Extension(s) | MIME Type | Supported | Notes |
|---|---|---|---|---|
| JPEG | .jpg, .jpeg | image/jpeg | ✓ | All standard JFIF and Exif variants |
| PNG | .png | image/png | ✓ | Including transparency |
| WEBP | .webp | image/webp | ✓ | Lossy and lossless |
| BMP | .bmp | image/bmp | ✓ | Standard Windows BMP |
| GIF | .gif | image/gif | ✗ | Future |
| TIFF | .tif, .tiff | image/tiff | ✗ | Future |
| AVIF | .avif | image/avif | ✗ | Future |
| HEIC / HEIF | .heic, .heif | image/heic | ✗ | Future — licensing concerns |
| SVG | .svg | image/svg+xml | ✗ | Future |
| ICO | .ico | image/x-icon | ✗ | Future |

### Write Support

| Format | Supported | Compression | Notes |
|---|---|---|---|
| JPEG | ✓ | Yes (lossy) | Quality 1–100; default 85 |
| PNG | ✓ | Yes (lossless) | Optimized via oxipng |
| WEBP | ✓ | Yes (lossy/lossless) | Quality 1–100 |
| BMP | ✓ | Limited | No compression; always 24-bit |
| GIF | ✗ | — | Future |
| TIFF | ✗ | — | Future |

### Image Operations Matrix

| Operation | JPEG | PNG | WEBP | BMP |
|---|---|---|---|---|
| Read | ✓ | ✓ | ✓ | ✓ |
| Write | ✓ | ✓ | ✓ | ✓ |
| Compress | ✓ | ✓ | ✓ | ✗ |
| Resize | ✓ | ✓ | ✓ | ✓ |
| Quality control | ✓ | — | ✓ | — |

---

## 2. PDF

### Core PDF Support

| Capability | Supported | Notes |
|---|---|---|
| Read PDF | ✓ | PDF 1.4–1.7 and PDF 2.0 (best effort) |
| Write PDF | ✓ | Produces PDF 1.7 output |
| Merge | ✓ | Any number of input PDFs |
| Split | ✓ | By page range or to individual pages |
| Rotate | ✓ | 90°, 180°, 270° per page or all pages |
| Organize (reorder/delete/duplicate) | ✓ | Visual page management |
| Compress | ✓ | Three presets: Maximum Quality, Balanced, Smaller File |
| PDF to JPG | ✓ | All pages or selected; configurable DPI |
| PDF to PNG | ~ | Future — JPG is the required first-release target |
| JPG to PDF | ✓ | Multiple images, configurable layout |
| PNG to PDF | ~ | Future |
| Page thumbnails | ✓ | Used in UI for page selection and preview |
| Metadata display | ✓ | Title, author, page count, size |
| PDF/A compliance | ✗ | Future |
| Encrypted / password-protected PDF | ✗ (graceful error) | Reports PASSWORD_REQUIRED error |
| Digital signatures | ✗ | Future |
| Forms | ✗ | Future |
| Watermarks | ✗ | Future |
| Page numbering | ✗ | Future |
| OCR | ✗ | Future |
| Redaction | ✗ | Future |
| Repair | ✗ | Future |

### PDF Compression Presets

| Preset | Behavior |
|---|---|
| Maximum Quality | Structural optimization only; image quality preserved |
| Balanced | Moderate image re-compression; maintains readability |
| Smaller File | Aggressive image compression; trades quality for size |

Compression will not report success if the output file is larger than the input.

---

## 3. Document Formats

All document formats are out of scope for CONFI Core.

| Format | Extension | Status |
|---|---|---|
| Word | .docx, .doc | Future (v1.2) |
| Excel | .xlsx, .xls | Future (v1.2) |
| PowerPoint | .pptx, .ppt | Future (v1.2) |
| OpenDocument Text | .odt | Future |
| OpenDocument Spreadsheet | .ods | Future |
| OpenDocument Presentation | .odp | Future |
| HTML | .html, .htm | Future (v1.2) |
| Markdown | .md | Future |
| Plain Text | .txt | Future |

> **Important:** PDF-to-Office reconstruction is inherently limited because PDFs do not preserve original document structure. When this is implemented in a future release, it will use text extraction and heuristics — not AI — and will clearly communicate its limitations.

---

## 4. Archive Formats

All archive formats are out of scope for CONFI Core.

| Format | Status |
|---|---|
| ZIP | Future (v1.3) |
| 7Z | Future (v1.3) |
| TAR | Future (v1.3) |
| GZ / TGZ | Future (v1.3) |
| RAR | Future — licensing evaluation required |

---

## 5. Audio Formats

All audio formats are out of scope for CONFI Core.

| Format | Status |
|---|---|
| MP3 | Future (v1.3) |
| WAV | Future (v1.3) |
| AAC | Future (v1.3) |
| FLAC | Future (v1.3) |
| OGG | Future (v1.3) |

---

## 6. Video Formats

All video formats are out of scope for CONFI Core.

| Format | Status |
|---|---|
| MP4 | Future (v1.3) |
| MKV | Future (v1.3) |
| MOV | Future (v1.3) |
| WEBM | Future (v1.3) |
| AVI | Future (v1.3) |

---

## 7. File Detection

CONFI uses **magic-byte (file signature) detection** rather than relying on file extensions.

This means:
- A `.pdf` file with corrupted headers is detected as corrupted, not assumed to be a valid PDF.
- A `.jpg` file renamed to `.png` is detected as JPEG, not PNG.
- Unsupported formats are clearly identified as such.

**Detection library:** `infer` (Rust crate, MIT license)

Detection occurs before any operation begins. Files that fail detection produce an `INVALID_INPUT` or `CORRUPTED_FILE` error before any processing is attempted.

---

## 8. Conversion Matrix

Valid first-release conversions:

| From \ To | JPG | PNG | WEBP | BMP | PDF |
|---|---|---|---|---|---|
| JPG | — | ✓ | ✓ | ✓ | ✓ (JPG to PDF) |
| PNG | ✓ | — | ✓ | ✓ | Future |
| WEBP | ✓ | ✓ | — | ✓ | — |
| BMP | ✓ | ✓ | ✓ | — | — |
| PDF | ✓ (PDF to JPG) | ~ | — | — | — |

`—` = not applicable or not supported in CONFI Core.

---

## 9. Platform Format Notes

### Windows
All formats above are supported on Windows 10/11 x64.

### Android
- HEIC/HEIF: May be readable on newer Android devices via platform codecs, but CONFI Core will not implement this in the first release due to licensing complexity.
- Content URI inputs: The format detected from the content URI MIME type is used as a hint; magic-byte verification is still performed.

---

## 10. Format Addition Process

Adding support for a new format requires:

1. Updating `SUPPORTED_FORMATS.md` (this document).
2. Adding a detection rule to `file_engine::detection`.
3. Adding encoding/decoding to the relevant engine module.
4. Adding test fixtures to `tests/fixtures/`.
5. Adding unit and integration tests.
6. Documenting the dependency in `LICENSES/INVENTORY.md`.
7. Updating the format matrix in the UI.
