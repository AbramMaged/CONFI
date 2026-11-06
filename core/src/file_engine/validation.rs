// CONFI — File validation
//
// Validation is SEPARATE from detection (see detection.rs).
//
// Three-stage model (as per M1 feedback):
//   Stage 1 — Detection:  magic bytes → FileFormat (detection.rs)
//   Stage 2 — Structural: can we parse the container? (this file)
//   Stage 3 — Operational: does this file work for a specific operation?
//             (handled inside each engine operation before processing starts)
//
// This module handles Stage 2: given a detected format, attempt a lightweight
// structural parse to confirm the file is not truncated or internally corrupt
// before any processing begins.
//
// Error code mapping:
//   File missing / unreadable     → INVALID_INPUT
//   Valid format, corrupt content → CORRUPTED_FILE
//   Extension mismatch            → logged warning, not an error (detection wins)
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
    /// For PDFs: page count if parseable.
    pub page_count: Option<u32>,
    /// For images: pixel dimensions if parseable.
    pub dimensions: Option<(u32, u32)>,
}

/// Validate that a file is structurally sound for the given detected format.
///
/// This goes one step further than detection:
/// - For images: decode the image header (not the full pixel data) to check dimensions.
/// - For PDFs: check that the header is present and the cross-reference is reachable.
///   (Full lopdf parse is done in the pdf_poc module if feature is enabled.)
///
/// A file that passes here is still not guaranteed to be fully intact (only a
/// complete decode/parse can guarantee that), but it catches the common cases:
/// truncated files, zero-byte files, and files with valid magic bytes but
/// immediately corrupted content.
pub fn validate_file(path: &Path, detected_format: &FileFormat) -> ConfiResult<ValidationResult> {
    let metadata = std::fs::metadata(path).map_err(|e| {
        ConfiError::file_access_denied(format!("Cannot stat '{}': {e}", path.display()))
    })?;

    let size_bytes = metadata.len();

    if size_bytes == 0 {
        return Err(ConfiError::invalid_input("File is empty."));
    }

    match detected_format {
        FileFormat::Jpeg => validate_jpeg(path, size_bytes),
        FileFormat::Png => validate_png(path, size_bytes),
        FileFormat::Webp => validate_webp(path, size_bytes),
        FileFormat::Bmp => validate_bmp(path, size_bytes),
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

// ── Image structural checks ──────────────────────────────────────────────────
//
// These use the `image` crate's reader in "header-only" mode where possible,
// so we do not load the full pixel data just to validate the structure.

fn validate_jpeg(path: &Path, size_bytes: u64) -> ConfiResult<ValidationResult> {
    let reader = open_image_reader(path, image::ImageFormat::Jpeg)?;

    // Verify JPEG-specific: must have valid SOI marker and at least one other segment.
    // `into_dimensions()` reads the header without decoding pixels.
    let (width, height) = reader.into_dimensions().map_err(|e| {
        ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            "This JPEG file appears to be damaged. It cannot be opened.",
            e.to_string(),
        )
    })?;

    if width == 0 || height == 0 {
        return Err(ConfiError::corrupted_file(
            "This JPEG file reports zero dimensions and cannot be processed.",
        ));
    }

    Ok(ValidationResult {
        format: FileFormat::Jpeg,
        size_bytes,
        page_count: None,
        dimensions: Some((width, height)),
    })
}

fn validate_png(path: &Path, size_bytes: u64) -> ConfiResult<ValidationResult> {
    let reader = open_image_reader(path, image::ImageFormat::Png)?;
    let (width, height) = reader.into_dimensions().map_err(|e| {
        ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            "This PNG file appears to be damaged. It cannot be opened.",
            e.to_string(),
        )
    })?;

    if width == 0 || height == 0 {
        return Err(ConfiError::corrupted_file(
            "This PNG file reports zero dimensions.",
        ));
    }

    Ok(ValidationResult {
        format: FileFormat::Png,
        size_bytes,
        page_count: None,
        dimensions: Some((width, height)),
    })
}

fn validate_webp(path: &Path, size_bytes: u64) -> ConfiResult<ValidationResult> {
    // The `image` crate can decode WebP but its dimension reading may not be
    // as lightweight as for JPEG/PNG. We do a full header check here.
    let reader = open_image_reader(path, image::ImageFormat::WebP)?;
    let (width, height) = reader.into_dimensions().map_err(|e| {
        ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            "This WebP file appears to be damaged.",
            e.to_string(),
        )
    })?;

    Ok(ValidationResult {
        format: FileFormat::Webp,
        size_bytes,
        page_count: None,
        dimensions: Some((width, height)),
    })
}

fn validate_bmp(path: &Path, size_bytes: u64) -> ConfiResult<ValidationResult> {
    // BMP minimum valid size: 54 bytes (14-byte file header + 40-byte DIB header).
    if size_bytes < 54 {
        return Err(ConfiError::corrupted_file(
            "This BMP file is too small to be valid.",
        ));
    }

    let reader = open_image_reader(path, image::ImageFormat::Bmp)?;
    let (width, height) = reader.into_dimensions().map_err(|e| {
        ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            "This BMP file appears to be damaged.",
            e.to_string(),
        )
    })?;

    Ok(ValidationResult {
        format: FileFormat::Bmp,
        size_bytes,
        page_count: None,
        dimensions: Some((width, height)),
    })
}

/// Open an image reader at the given path with the specified format.
fn open_image_reader(
    path: &Path,
    format: image::ImageFormat,
) -> ConfiResult<image::ImageReader<std::io::BufReader<std::fs::File>>> {
    let mut reader = image::ImageReader::open(path).map_err(|e| {
        ConfiError::file_access_denied(format!("Cannot open image '{}': {e}", path.display()))
    })?;
    reader.set_format(format);
    Ok(reader)
}

// ── PDF structural check ─────────────────────────────────────────────────────
//
// Without lopdf in scope, we perform a lightweight manual check:
//   1. File begins with %PDF-
//   2. File ends with %%EOF (within the last 1024 bytes)
//   3. File is at minimum a plausible size (> 100 bytes)
//
// This catches truncated PDFs, zero-byte files renamed to .pdf, and the
// most common corruption pattern (file cut off before %%EOF).
// Full structural validation occurs inside the pdf_engine operations.

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

    // Check PDF header magic.
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

    // Check %%EOF marker in the tail (last 1024 bytes).
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

    // Check for encryption dictionary — surface this as PASSWORD_REQUIRED.
    // We only read the header region to check for /Encrypt, so this is not
    // exhaustive, but catches the most common case.
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
        page_count: None, // Determined by the pdf_engine, not here.
        dimensions: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp(bytes: &[u8], ext: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(&format!(".{ext}"))
            .tempfile()
            .unwrap();
        f.write_all(bytes).unwrap();
        f
    }

    // ── Truncated file tests ─────────────────────────────────────────────────

    #[test]
    fn truncated_file_with_pdf_magic_is_corrupted() {
        // Only the header, no %%EOF — truncated.
        let data = b"%PDF-1.7\n% just the header, nothing else";
        let f = write_temp(data, "pdf");
        let result = validate_file(f.path(), &FileFormat::Pdf);
        assert!(result.is_err());
        let e = result.unwrap_err();
        assert_eq!(e.code, ErrorCode::CorruptedFile);
    }

    #[test]
    fn valid_pdf_skeleton_passes_header_check() {
        // Minimal PDF-like structure with proper markers.
        let mut data = b"%PDF-1.7\n".to_vec();
        data.extend_from_slice(b"1 0 obj\n<< >>\nendobj\n");
        data.extend_from_slice(&vec![b' '; 80]); // padding
        data.extend_from_slice(b"\n%%EOF\n");
        let f = write_temp(&data, "pdf");
        let result = validate_file(f.path(), &FileFormat::Pdf);
        assert!(result.is_ok());
    }

    #[test]
    fn empty_file_always_fails() {
        let f = write_temp(&[], "pdf");
        let result = validate_file(f.path(), &FileFormat::Pdf);
        assert!(result.is_err());
    }

    #[test]
    fn unsupported_format_returns_correct_error_code() {
        let f = write_temp(b"some content", "tiff");
        let result = validate_file(f.path(), &FileFormat::Unsupported("image/tiff".to_string()));
        let e = result.unwrap_err();
        assert_eq!(e.code, ErrorCode::UnsupportedFormat);
    }

    #[test]
    fn unknown_format_returns_invalid_input_error() {
        let f = write_temp(b"\x00\x01\x02\x03", "bin");
        let result = validate_file(f.path(), &FileFormat::Unknown);
        let e = result.unwrap_err();
        assert_eq!(e.code, ErrorCode::InvalidInput);
    }

    #[test]
    fn encrypted_pdf_detected_as_password_required() {
        let mut data = b"%PDF-1.7\n".to_vec();
        data.extend_from_slice(b"1 0 obj\n<< /Encrypt 2 0 R >>\nendobj\n");
        data.extend_from_slice(&vec![b' '; 80]);
        data.extend_from_slice(b"\n%%EOF\n");
        let f = write_temp(&data, "pdf");
        let result = validate_file(f.path(), &FileFormat::Pdf);
        let e = result.unwrap_err();
        assert_eq!(e.code, ErrorCode::PasswordRequired);
    }
}
