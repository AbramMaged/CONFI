// CONFI — File validation
//
// Validation is SEPARATE from detection (see detection.rs):
//   Stage 1 — Detection:  magic bytes → FileFormat (detection.rs)
//   Stage 2 — Structural: can we parse the container? (this file)
//   Stage 3 — Operational: does this file work for a specific operation?
//             (handled inside each engine operation before processing starts)
//
// Error code mapping:
//   File missing / unreadable     → INVALID_INPUT
//   Valid format, corrupt content → CORRUPTED_FILE
//   Extension mismatch            → ignored; detected format always wins
//   Format not supported          → UNSUPPORTED_FORMAT

use std::path::Path;

use crate::common::{
    errors::{ConfiError, ConfiResult, ErrorCode},
    models::FileFormat,
};

// ── Public API ───────────────────────────────────────────────────────────────

/// Result of a structural validation check.
#[derive(Debug)]
pub struct ValidationResult {
    pub format: FileFormat,
    pub size_bytes: u64,
    /// For images: pixel dimensions if parseable.
    pub dimensions: Option<(u32, u32)>,
}

/// Validate that a file is structurally sound for the given detected format.
///
/// - Images: decode the header only (not the pixel data) to read dimensions.
/// - PDFs: check the header, the end marker and the encryption dictionary.
///
/// A file that passes here is still not guaranteed to be fully intact, but it
/// catches the common cases: truncated files, zero-byte files, and files with
/// valid magic bytes but immediately corrupted content.
pub fn validate_file(path: &Path, detected_format: &FileFormat) -> ConfiResult<ValidationResult> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        ConfiError::file_access_denied(format!("Cannot stat '{}': {e}", path.display()))
    })?;

    let size_bytes = metadata.len();

    if size_bytes == 0 {
        return Err(ConfiError::invalid_input("File is empty."));
    }

    match detected_format {
        FileFormat::Jpeg => validate_image(path, FileFormat::Jpeg, image::ImageFormat::Jpeg, size_bytes),
        FileFormat::Png => validate_image(path, FileFormat::Png, image::ImageFormat::Png, size_bytes),
        FileFormat::Webp => validate_image(path, FileFormat::Webp, image::ImageFormat::WebP, size_bytes),
        FileFormat::Bmp => {
            // Minimum valid BMP: 14-byte file header + 40-byte DIB header.
            const MIN_BMP_SIZE: u64 = 54;
            if size_bytes < MIN_BMP_SIZE {
                return Err(ConfiError::corrupted_file(
                    "This BMP file is too small to be valid.",
                ));
            }
            validate_image(path, FileFormat::Bmp, image::ImageFormat::Bmp, size_bytes)
        }
        FileFormat::Pdf => validate_pdf_header(path, size_bytes),
        FileFormat::Unsupported(mime) => Err(ConfiError::unsupported_format(format!(
            "The file format '{mime}' is not supported in this version of CONFI."
        ))),
        FileFormat::Unknown => Err(ConfiError::with_detail(
            ErrorCode::InvalidInput,
            "The file format could not be determined. The file may be empty, corrupted, or an unrecognised type.",
            "magic-byte detection returned Unknown",
        )),
    }
}

// ── Image structural check ──────────────────────────────────────────────────

/// One implementation for all image formats: `into_dimensions()` reads the
/// header without decoding pixels, so this stays cheap on large files.
fn validate_image(
    path: &Path,
    format: FileFormat,
    image_format: image::ImageFormat,
    size_bytes: u64,
) -> ConfiResult<ValidationResult> {
    let mut reader = image::ImageReader::open(path).map_err(|e| {
        ConfiError::file_access_denied(format!("Cannot open image '{}': {e}", path.display()))
    })?;
    reader.set_format(image_format);

    let (width, height) = reader.into_dimensions().map_err(|e| {
        ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            format!("This {format} file appears to be damaged. It cannot be opened."),
            e.to_string(),
        )
    })?;

    if width == 0 || height == 0 {
        return Err(ConfiError::corrupted_file(format!(
            "This {format} file reports zero dimensions and cannot be processed."
        )));
    }

    Ok(ValidationResult {
        format,
        size_bytes,
        dimensions: Some((width, height)),
    })
}

// ── PDF structural check ─────────────────────────────────────────────────────
//
// Lightweight, without a full parse:
//   1. File begins with %PDF-
//   2. File ends with %%EOF (within the last 1024 bytes)
//   3. File is at minimum a plausible size
//   4. No /Encrypt dictionary in the header region
//
// This catches truncated PDFs, zero-byte files renamed to .pdf, and the most
// common corruption pattern (file cut off before %%EOF). Full structural
// validation happens in the pdf_engine operations.

fn validate_pdf_header(path: &Path, size_bytes: u64) -> ConfiResult<ValidationResult> {
    use std::io::{Read, Seek, SeekFrom};

    const MIN_PDF_SIZE: u64 = 100;
    const PDF_MAGIC: &[u8] = b"%PDF-";

    if size_bytes < MIN_PDF_SIZE {
        return Err(ConfiError::corrupted_file(
            "This file is too small to be a valid PDF.",
        ));
    }

    let mut file = std::fs::File::open(path).map_err(|e| {
        ConfiError::file_access_denied(format!("Cannot open '{}': {e}", path.display()))
    })?;

    let mut header = vec![0u8; PDF_MAGIC.len()];
    file.read_exact(&mut header)
        .map_err(|e| ConfiError::corrupted_file(format!("Cannot read PDF header: {e}")))?;

    if &header != PDF_MAGIC {
        return Err(ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            "This file does not appear to be a valid PDF.",
            format!("Expected %PDF- header, found: {:?}", &header),
        ));
    }

    // Check %%EOF in the tail.
    let tail_start = size_bytes.saturating_sub(1024);
    file.seek(SeekFrom::Start(tail_start)).map_err(|e| {
        ConfiError::processing_error_detail("Seek failed during PDF validation", e.to_string())
    })?;

    let mut tail = Vec::new();
    file.read_to_end(&mut tail)
        .map_err(|e| ConfiError::processing_error_detail("Cannot read PDF tail", e.to_string()))?;

    if !tail.windows(5).any(|w| w == b"%%EOF") {
        return Err(ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            "This PDF appears to be truncated or damaged (missing end marker).",
            "%%EOF not found in the last 1024 bytes",
        ));
    }

    // Check for an encryption dictionary — surface this as PASSWORD_REQUIRED.
    file.seek(SeekFrom::Start(0)).ok();
    let mut head_buf = vec![0u8; 2048.min(size_bytes as usize)];
    let n = file.read(&mut head_buf).unwrap_or(0);
    head_buf.truncate(n);

    if head_buf.windows(8).any(|w| w == b"/Encrypt") {
        return Err(ConfiError::password_required());
    }

    Ok(ValidationResult {
        format: FileFormat::Pdf,
        size_bytes,
        dimensions: None,
    })
}
