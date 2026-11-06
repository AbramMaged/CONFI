# CONFI — Testing Strategy

**Document Version:** 1.0  
**Date:** 2026-10-31

---

## 1. Testing Principles

A feature is **complete** only when:

1. The interface works.
2. The processing logic produces valid output.
3. The output is validated — reopened and structure-checked where possible.
4. Error cases are handled and reported correctly.
5. Automated tests pass.
6. The design system is followed.
7. Original files remain unchanged.

**Successful compilation is not proof of correctness.**

---

## 2. Testing Levels

### 2.1 Unit Tests

Scope: Individual Rust functions and modules in `core/`.

Location: `core/src/` inline tests and `tests/unit/`

Each unit test must:
- Test a single, focused behavior.
- Not touch the filesystem where avoidable (use in-memory bytes).
- Run in under 1 second.
- Be deterministic across environments.

**Priority functions to unit test:**

| Module | Test targets |
|---|---|
| `file_engine::detection` | Magic-byte identification for each supported format |
| `file_engine::validation` | Valid inputs pass; invalid inputs return correct error codes |
| `common::models` | Serialization/deserialization of Job, FileInfo, OperationResult |
| `pdf_engine::split` | Page-range parsing; edge cases (empty range, out-of-bounds) |
| `image_engine::resize` | Correct dimension calculation; aspect ratio preservation |
| `job_manager::queue` | Job lifecycle transitions; invalid transitions |

---

### 2.2 Integration Tests

Scope: Complete processing workflows using real test fixture files.

Location: `tests/integration/`

Integration tests use the fixture files in `tests/fixtures/` and verify:
- The output file exists.
- The output file is a valid document of the expected format.
- The output matches the expected result (page count, dimensions, etc.).
- The original input file is unchanged.

**PDF integration tests:**

| Test ID | Scenario | Input Fixtures | Validation |
|---|---|---|---|
| IT-PDF-01 | Merge two PDFs | `fixture_a.pdf` (3 pages), `fixture_b.pdf` (2 pages) | Output has 5 pages; valid PDF |
| IT-PDF-02 | Merge maintains order | Same | Pages 1–3 from A, 4–5 from B |
| IT-PDF-03 | Split by range 1–2 | `fixture_a.pdf` | Output has 2 pages; valid |
| IT-PDF-04 | Split all pages | `fixture_a.pdf` | 3 separate PDFs; each 1 page |
| IT-PDF-05 | Rotate page 1 by 90° | `fixture_a.pdf` | Page 1 rotated; other pages unchanged |
| IT-PDF-06 | Delete page 2 | `fixture_a.pdf` | Output has 2 pages; content correct |
| IT-PDF-07 | Reorder pages [3, 1, 2] | `fixture_a.pdf` | Output page order matches |
| IT-PDF-08 | Compress (balanced preset) | `fixture_large.pdf` | Output ≤ input size; valid PDF |
| IT-PDF-09 | PDF to JPG (all pages) | `fixture_a.pdf` | 3 valid JPEG files produced |
| IT-PDF-10 | JPG to PDF | `fixture_img1.jpg`, `fixture_img2.jpg` | Valid PDF with 2 pages |

**Image integration tests:**

| Test ID | Scenario | Input Fixtures | Validation |
|---|---|---|---|
| IT-IMG-01 | PNG to JPG | `fixture_image.png` | Valid JPEG output |
| IT-IMG-02 | JPG to WEBP | `fixture_photo.jpg` | Valid WEBP output |
| IT-IMG-03 | WEBP to PNG | `fixture_image.webp` | Valid PNG output |
| IT-IMG-04 | JPG to PNG | `fixture_photo.jpg` | Valid PNG output |
| IT-IMG-05 | BMP to JPG | `fixture_image.bmp` | Valid JPEG output |
| IT-IMG-06 | Resize to 800×600 | `fixture_photo.jpg` | Output is 800×600px |
| IT-IMG-07 | Resize to 50% | `fixture_photo.jpg` (1000×800) | Output is 500×400px |
| IT-IMG-08 | Resize preserving aspect | `fixture_photo.jpg` with 800×600 max | Shorter dimension matches max |
| IT-IMG-09 | Compress JPG at quality 50 | `fixture_photo.jpg` | Output smaller than input |
| IT-IMG-10 | Compress PNG (lossless) | `fixture_image.png` | Valid PNG; optimized |
| IT-IMG-11 | Batch conversion | 5 mixed-format images | All 5 outputs produced |
| IT-IMG-12 | Unsupported format | `fixture.tiff` | UNSUPPORTED_FORMAT error; no crash |

---

### 2.3 File Safety Tests

These tests verify the core reliability promise: original files are never modified.

| Test ID | Scenario | Expected |
|---|---|---|
| FS-01 | Any PDF operation | Input file byte-identical after operation |
| FS-02 | Any image operation | Input file byte-identical after operation |
| FS-03 | Failed operation | No output file exists; temp cleaned up |
| FS-04 | Cancelled operation | No partial output file; temp cleaned up |
| FS-05 | Overwrite protection | Existing output file not overwritten when flag is false |
| FS-06 | Invalid PDF input | Error returned; no temp file left |
| FS-07 | Batch partial failure | Failed files report error; other files complete normally |

---

### 2.4 Error Handling Tests

| Test ID | Scenario | Expected Error Code |
|---|---|---|
| EH-01 | Missing input file | INVALID_INPUT |
| EH-02 | Empty file | INVALID_INPUT |
| EH-03 | Unsupported format (TIFF) | UNSUPPORTED_FORMAT |
| EH-04 | Corrupted PDF | CORRUPTED_FILE |
| EH-05 | Password-protected PDF | PASSWORD_REQUIRED |
| EH-06 | Read-only output directory | OUTPUT_ERROR |
| EH-07 | Disk full (simulated) | INSUFFICIENT_SPACE |
| EH-08 | Cancel a running job | OPERATION_CANCELLED; clean state |
| EH-09 | Invalid page range (out of bounds) | INVALID_INPUT |
| EH-10 | PDF operation on image file | INVALID_OPERATION |

---

### 2.5 Platform Tests

#### Windows

| Test ID | Scenario | Expected |
|---|---|---|
| PT-WIN-01 | File picker imports files | FileInfo returned for each file |
| PT-WIN-02 | Drag-and-drop onto app window | Files added to queue |
| PT-WIN-03 | Context menu > Convert to JPG | File sent to converter; job created |
| PT-WIN-04 | Output folder selection | Output written to selected directory |
| PT-WIN-05 | Window resize | UI adapts; no layout breaks |
| PT-WIN-06 | Multiple concurrent jobs | All jobs run and report independently |

#### Android

| Test ID | Scenario | Expected |
|---|---|---|
| PT-AND-01 | File picker (SAF) | Content URI read correctly |
| PT-AND-02 | Share from Files app | File received and queued |
| PT-AND-03 | Share multiple files | All files queued |
| PT-AND-04 | App backgrounded mid-job | Job continues via WorkManager |
| PT-AND-05 | App killed mid-job | Job status persisted; recovery on relaunch |
| PT-AND-06 | Content URI permission expired | FILE_ACCESS_DENIED error |

---

### 2.6 Stress Tests

Run these periodically, not in every CI build:

| Test | Input |
|---|---|
| Large PDF merge | 50 PDFs, each 100 pages |
| Single large PDF | 500-page PDF; split and rotate |
| High-resolution image batch | 20 images, each 4000×3000px |
| Concurrent batch | 10 jobs running simultaneously |
| Repeated runs | Same operation 100 times; no resource leak |
| Simulated corrupt inputs | Truncated, random bytes, zero-byte files |

---

## 3. Test Fixtures

All fixture files live in `tests/fixtures/`.

Required fixtures for CONFI Core:

| File | Description |
|---|---|
| `fixture_a.pdf` | Valid 3-page PDF with varied content |
| `fixture_b.pdf` | Valid 2-page PDF |
| `fixture_large.pdf` | PDF with embedded images suitable for compression testing |
| `fixture_encrypted.pdf` | Password-protected PDF (password: `test`) |
| `fixture_corrupted.pdf` | Truncated/damaged PDF |
| `fixture_single_page.pdf` | 1-page PDF |
| `fixture_photo.jpg` | 1920×1080 JPEG photograph |
| `fixture_image.png` | 800×600 PNG with transparency |
| `fixture_image.webp` | 800×600 WEBP |
| `fixture_image.bmp` | 400×300 BMP |
| `fixture_img1.jpg` | Portrait-orientation JPEG |
| `fixture_img2.jpg` | Landscape-orientation JPEG |

Fixtures must be:
- Small enough to keep the repository lightweight.
- Representative of real-world files, not trivially simple synthetic files.
- Free of copyright — generated or clearly licensed for test use.

---

## 4. Validation Methods

### PDF Validation

After every PDF-producing operation:
1. Attempt to open the output with the PDF library.
2. Verify the page count matches expectations.
3. Verify no parsing errors occur.
4. For merges: verify page content on first and last page is correct.

### Image Validation

After every image-producing operation:
1. Decode the output image.
2. Verify dimensions match expectations.
3. Verify the file signature matches the expected format.
4. For compression: verify output size ≤ input size (fail otherwise).

---

## 5. CI/CD Integration

Target pipeline (to be configured when the repository is set up):

1. `cargo test` — all Rust unit and integration tests
2. `cargo clippy` — Rust lints
3. `cargo fmt --check` — formatting
4. `npm run lint` — TypeScript lints (desktop app)
5. `npm run build` — frontend build check

Pull requests must not merge if any step fails.

---

## 6. Acceptance Test Execution

The acceptance tests in the SRS (AT-PDF-*, AT-IMG-*, AT-PLAT-*) are to be run manually before each release candidate.

They should be documented in a test run report:
- Date of run
- Build version
- Platform
- Each test: Pass / Fail / Blocked
- Notes on any failures

A release candidate is blocked if any P0 acceptance test fails.

---

## 7. Regression Testing

Every supported operation must have at least one representative fixture and integration test.

When a library version is updated:
1. Run the full integration test suite.
2. Compare output files to stored baseline outputs where possible.
3. Document any behavioral differences.
4. Do not merge library updates that silently break previously passing tests.
