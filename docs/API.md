# CONFI — API Reference

**Document Version:** 1.0  
**Date:** 2026-10-31  
**Scope:** Shared Rust core API consumed by both the Windows (Tauri) and Android (UniFFI) layers.

The contracts below define the logical interface. The Tauri IPC layer serializes these to JSON. The UniFFI layer maps them to Kotlin types. Both use the same semantic behavior.

---

## 1. Core Data Models

### FileInfo

Returned after a file is detected and validated.

```json
{
  "file_id": "file_001",
  "name": "document.pdf",
  "detected_format": "pdf",
  "mime_type": "application/pdf",
  "size_bytes": 2048000,
  "is_valid": true,
  "validation_error": null
}
```

| Field | Type | Description |
|---|---|---|
| `file_id` | string | Unique identifier for this file in the session |
| `name` | string | Original filename |
| `detected_format` | string | Format detected via magic bytes, not extension |
| `mime_type` | string | MIME type |
| `size_bytes` | integer | File size in bytes |
| `is_valid` | boolean | Whether the file passed validation |
| `validation_error` | ErrorDetail or null | Validation failure details |

---

### Job

Represents a processing job.

```json
{
  "job_id": "job_001",
  "operation": "merge_pdfs",
  "input_files": ["file_001", "file_002"],
  "output_config": {
    "destination": "user_selected_path",
    "filename_template": "merged_{timestamp}",
    "format": "pdf"
  },
  "status": "processing",
  "progress_percentage": 45,
  "current_operation": "copying_pages",
  "created_at": "2026-10-31T10:00:00Z",
  "completed_at": null,
  "error": null
}
```

**Status values:** `pending` | `queued` | `processing` | `completed` | `failed` | `cancelled`

---

### OperationResult

Returned when an operation completes.

```json
{
  "job_id": "job_001",
  "status": "completed",
  "output": {
    "name": "merged_document.pdf",
    "size_bytes": 3500000,
    "path": "user_selected_destination"
  },
  "original_size_bytes": 4096000,
  "reduction_percentage": 14.5,
  "error": null
}
```

---

### ErrorDetail

```json
{
  "code": "CORRUPTED_FILE",
  "message": "This PDF appears to be damaged and cannot be opened.",
  "detail": "PDF header missing; expected %PDF at offset 0"
}
```

`detail` is for internal diagnostics only and must not be displayed to users in production builds.

---

### Progress

```json
{
  "job_id": "job_001",
  "progress_percentage": 60,
  "current_operation": "rendering_page_3_of_5",
  "is_indeterminate": false
}
```

When `is_indeterminate` is `true`, `progress_percentage` should be ignored by the UI (show a spinner, not a bar).

---

## 2. File Engine

### detect_file

Identify a file's format, MIME type, and validity.

```
detect_file(input: InputSource) -> FileInfo
```

**Parameters:**
- `input` — file path (Windows) or content URI resolved to bytes (Android)

**Returns:** `FileInfo`

**Errors:**
- `FILE_ACCESS_DENIED` — cannot read the file
- `INVALID_INPUT` — file is empty or unreadable

---

### validate_file

Check if a file is valid for a specific operation.

```
validate_file(file_id: String, operation: OperationType) -> ValidationResult
```

---

## 3. PDF Engine

### merge_pdfs

Combine two or more PDF files into one document.

```
merge_pdfs(
    input_files: Vec<InputSource>,
    output: OutputConfig
) -> OperationResult
```

**Requirements:**
- At least two input files.
- Files must all be detected as PDF.
- Page order follows the order of `input_files`.

**Errors:** `INVALID_INPUT`, `CORRUPTED_FILE`, `PASSWORD_REQUIRED`, `OUTPUT_ERROR`, `INSUFFICIENT_SPACE`

---

### split_pdf

Split a PDF by page ranges or into individual pages.

```
split_pdf(
    input: InputSource,
    page_ranges: Vec<PageRange>,
    output: OutputConfig
) -> OperationResult
```

**PageRange:**
```json
{ "start": 1, "end": 5 }
```

Use `{ "start": N, "end": N }` for a single page.  
Use `null` for `page_ranges` to split every page into its own file.

**Output:** Multiple output files named according to `output.filename_template`.

---

### rotate_pdf

Rotate selected pages.

```
rotate_pdf(
    input: InputSource,
    page_selection: PageSelection,
    rotation_degrees: RotationDegrees,
    output: OutputConfig
) -> OperationResult
```

**PageSelection:**
```json
{ "all": true }
// or
{ "pages": [1, 3, 5] }
// or
{ "range": { "start": 2, "end": 6 } }
```

**RotationDegrees:** `90` | `180` | `270`

---

### organize_pdf

Reorder, delete, or duplicate pages.

```
organize_pdf(
    input: InputSource,
    operations: Vec<PageOperation>,
    output: OutputConfig
) -> OperationResult
```

**PageOperation:**
```json
{ "type": "reorder", "new_order": [3, 1, 2, 4] }
{ "type": "delete", "pages": [2, 5] }
{ "type": "duplicate", "page": 3, "insert_after": 3 }
```

Operations are applied in sequence.

---

### compress_pdf

Reduce PDF file size.

```
compress_pdf(
    input: InputSource,
    preset: CompressionPreset,
    output: OutputConfig
) -> OperationResult
```

**CompressionPreset:** `maximum_quality` | `balanced` | `smaller_file`

The engine must not report success if the output file is larger than the input.

---

### pdf_to_images

Export PDF pages as images.

```
pdf_to_images(
    input: InputSource,
    page_selection: PageSelection,
    image_format: ImageFormat,
    resolution_dpi: u32,
    output: OutputConfig
) -> OperationResult
```

**ImageFormat:** `jpg` | `png` (PNG output is a future enhancement for v1.0; JPG is required)  
**resolution_dpi:** default 150; range 72–600

---

### images_to_pdf

Create a PDF from one or more images.

```
images_to_pdf(
    input_files: Vec<InputSource>,
    page_config: PageConfig,
    output: OutputConfig
) -> OperationResult
```

**PageConfig:**
```json
{
  "orientation": "portrait",
  "margin_mm": 10,
  "fit_mode": "fit_to_page"
}
```

**fit_mode:** `fit_to_page` | `fill_page` | `actual_size`

---

### get_pdf_metadata

Return document properties and page count.

```
get_pdf_metadata(input: InputSource) -> PdfMetadata
```

```json
{
  "page_count": 12,
  "title": "Annual Report",
  "author": "",
  "created_at": "2026-01-15",
  "size_bytes": 2048000,
  "is_encrypted": false
}
```

---

### render_page_thumbnail

Render a single PDF page as a low-resolution thumbnail for UI display.

```
render_page_thumbnail(
    input: InputSource,
    page_number: u32,
    width_px: u32
) -> ThumbnailResult
```

**Returns:** Image bytes (JPEG or PNG) suitable for display. Not for export.

---

## 4. Image Engine

### convert_image

Convert an image to a different format.

```
convert_image(
    input: InputSource,
    output_format: ImageFormat,
    quality: Option<u8>,
    output: OutputConfig
) -> OperationResult
```

**ImageFormat:** `jpg` | `png` | `webp` | `bmp`  
**quality:** 1–100, used for lossy formats (JPG, WEBP). Ignored for lossless.

---

### compress_image

Reduce image file size.

```
compress_image(
    input: InputSource,
    quality: u8,
    output: OutputConfig
) -> OperationResult
```

**quality:** 1–100 where 100 = maximum quality, minimum compression.

Behavior:
- JPG: lossy compression at given quality
- PNG: lossless optimization (oxipng)
- WEBP: quality-based compression
- BMP: compression not applicable; return UNSUPPORTED_FORMAT for compression

---

### resize_image

Resize an image.

```
resize_image(
    input: InputSource,
    dimensions: ResizeDimensions,
    preserve_aspect_ratio: bool,
    output: OutputConfig
) -> OperationResult
```

**ResizeDimensions:**
```json
{ "width": 800, "height": 600 }
// or
{ "percentage": 50 }
```

When `preserve_aspect_ratio` is true and both `width` and `height` are provided, the image is fit within the bounding box.

---

## 5. Job Management

### create_job

Submit a new operation job.

```
create_job(request: JobRequest) -> Job
```

---

### get_job

Retrieve the current state of a job.

```
get_job(job_id: String) -> Job
```

**Errors:** Job not found returns `INVALID_INPUT`.

---

### get_job_progress

Get live progress for a running job.

```
get_job_progress(job_id: String) -> Progress
```

---

### cancel_job

Request cancellation of a running job.

```
cancel_job(job_id: String) -> CancelResult
```

```json
{
  "job_id": "job_001",
  "cancelled": true,
  "message": "Job cancelled by user."
}
```

Cancellation is best-effort. Some operations may not support mid-operation cancellation; they will run to their next safe checkpoint and then cancel.

Cancelled jobs must clean up any partial output.

---

### list_jobs

Return all jobs in the current session.

```
list_jobs() -> Vec<Job>
```

---

## 6. Error Code Reference

| Code | Meaning | Typical Cause |
|---|---|---|
| `INVALID_INPUT` | Input is missing or invalid | Missing file, wrong type |
| `UNSUPPORTED_FORMAT` | Format not supported in this release | TIFF, GIF, etc. |
| `INVALID_OPERATION` | Operation incompatible with input | Merging a JPG as PDF |
| `FILE_ACCESS_DENIED` | Insufficient permissions | Locked file, content URI expired |
| `OUTPUT_ERROR` | Failed to write output | Destination not writable |
| `CORRUPTED_FILE` | Input is structurally invalid | Truncated PDF, broken JPEG |
| `PASSWORD_REQUIRED` | Encrypted document | Password-protected PDF |
| `INSUFFICIENT_SPACE` | Not enough storage | Disk full |
| `OPERATION_CANCELLED` | User cancelled | Explicit cancellation |
| `PROCESSING_ERROR` | Unexpected internal failure | Library error, OOM |

---

## 7. Output Configuration

All operations accept an `OutputConfig`:

```json
{
  "destination": "user_selected_path",
  "filename_template": "{name}_compressed",
  "overwrite_existing": false
}
```

**filename_template tokens:**
- `{name}` — original filename without extension
- `{ext}` — output format extension
- `{timestamp}` — Unix timestamp
- `{date}` — ISO date (YYYY-MM-DD)
- `{index}` — numeric index for batch outputs

`overwrite_existing` defaults to `false`. If a file already exists at the destination and `overwrite_existing` is false, the operation returns `OUTPUT_ERROR`.

---

## 8. Progress Reporting Behavior

- Operations that can calculate reliable progress (e.g., page-by-page PDF rendering) must report 0–100.
- Operations that cannot calculate reliable progress must set `is_indeterminate: true`.
- Progress events are emitted at meaningful intervals, not more than ~10 times per second.
- The UI must show a spinner (not a progress bar) when `is_indeterminate` is true.
- Fake or estimated progress is prohibited.
