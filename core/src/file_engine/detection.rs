// CONFI — File format detection
//
// Detection is ALWAYS from magic bytes (file signatures), never from file
// extension alone. This satisfies AGENTS.md §3 and SRS SYS-01.
//
// Strategy:
//   1. Read up to 16 bytes of the file (the minimum needed for all our signatures).
//   2. Use the `infer` crate to identify the format from those bytes.
//   3. Map the result to our FileFormat enum.
//   4. Extensions are checked separately and mismatches are logged as warnings,
//      but never used to override magic-byte detection.
//
// Important distinction (from M1 feedback):
//   detection  = "what format is this file?" (magic bytes only)
//   validation = "is this file structurally intact?" (see validation.rs)
//
// These are separate steps. Detection can succeed on a truncated file;
// validation will then correctly catch the truncation.

use std::path::Path;

use crate::common::{
    errors::{ConfiError, ConfiResult},
    models::FileFormat,
};

// ── Public API ───────────────────────────────────────────────────────────────

/// Detect the format of a file by reading its magic bytes.
///
/// Returns:
/// - `Ok(FileFormat::Pdf | Jpeg | Png | Webp | Bmp)` for supported formats.
/// - `Ok(FileFormat::Unsupported(mime))` for recognisable but unsupported types.
/// - `Ok(FileFormat::Unknown)` when the format cannot be determined.
/// - `Err(ConfiError)` for access errors or an empty file.
pub fn detect_from_path(path: &Path) -> ConfiResult<FileFormat> {
    // Guard: path must exist.
    if !path.exists() {
        return Err(ConfiError::invalid_input(format!(
            "File not found: {}",
            path.display()
        )));
    }

    // Guard: must be a file (not a directory or symlink to one).
    if !path.is_file() {
        return Err(ConfiError::invalid_input(format!(
            "Path is not a regular file: {}",
            path.display()
        )));
    }

    // Read enough bytes for detection. `infer` only needs the first few bytes.
    let bytes = read_header_bytes(path)?;

    if bytes.is_empty() {
        return Err(ConfiError::invalid_input("File is empty."));
    }

    detect_from_bytes(&bytes)
}

/// Detect the format from raw bytes (e.g., from an Android content URI that
/// has already been read into memory).
pub fn detect_from_bytes(bytes: &[u8]) -> ConfiResult<FileFormat> {
    if bytes.is_empty() {
        return Err(ConfiError::invalid_input("Input bytes are empty."));
    }

    // Use `infer` for magic-byte identification.
    if let Some(kind) = infer::get(bytes) {
        return Ok(map_infer_type(kind));
    }

    // `infer` returned None — bytes are present but no signature matched.
    Ok(FileFormat::Unknown)
}

/// Read the first 16 bytes from a file — enough for all signatures we care about.
fn read_header_bytes(path: &Path) -> ConfiResult<Vec<u8>> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|e| {
        ConfiError::file_access_denied(format!("Cannot open file '{}': {e}", path.display()))
    })?;

    // 16 bytes is sufficient for all CONFI magic signatures.
    // PDF: %PDF (4 bytes at offset 0)
    // JPEG: FF D8 FF (3 bytes)
    // PNG: 89 50 4E 47 0D 0A 1A 0A (8 bytes)
    // WEBP: RIFF????WEBP (12 bytes)
    // BMP: 42 4D (2 bytes)
    let mut buf = vec![0u8; 16];
    let n = file.read(&mut buf).map_err(|e| {
        ConfiError::file_access_denied(format!("Cannot read file '{}': {e}", path.display()))
    })?;
    buf.truncate(n);
    Ok(buf)
}

/// Map an `infer::Type` result to our `FileFormat`.
fn map_infer_type(kind: infer::Type) -> FileFormat {
    match kind.mime_type() {
        "application/pdf" => FileFormat::Pdf,
        "image/jpeg" => FileFormat::Jpeg,
        "image/png" => FileFormat::Png,
        "image/webp" => FileFormat::Webp,
        "image/bmp" => FileFormat::Bmp,
        mime => FileFormat::Unsupported(mime.to_string()),
    }
}

// ── Extension mismatch check (informational, not authoritative) ─────────────

/// Check whether the file extension matches the detected format.
/// Returns `false` if they disagree — useful for logging a warning to the user.
/// Never used to override magic-byte detection.
pub fn extension_matches_format(path: &Path, detected: &FileFormat) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase());

    let expected = detected.extension();

    match (ext.as_deref(), expected) {
        (Some("jpg") | Some("jpeg"), Some("jpg")) => true,
        (Some(ext), Some(expected)) => ext == expected,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // ── Magic byte helpers ───────────────────────────────────────────────────

    fn make_temp(bytes: &[u8], extension: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(&format!(".{extension}"))
            .tempfile()
            .unwrap();
        f.write_all(bytes).unwrap();
        f
    }

    const PDF_HEADER: &[u8] = b"%PDF-1.7\n";
    const JPEG_HEADER: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0];
    const PNG_HEADER: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    const WEBP_HEADER: &[u8] = b"RIFF\x00\x00\x00\x00WEBP";
    const BMP_HEADER: &[u8] = &[0x42, 0x4D, 0x36, 0x00];

    // ── detect_from_bytes tests ──────────────────────────────────────────────

    #[test]
    fn detects_pdf_from_bytes() {
        assert_eq!(detect_from_bytes(PDF_HEADER).unwrap(), FileFormat::Pdf);
    }

    #[test]
    fn detects_jpeg_from_bytes() {
        assert_eq!(detect_from_bytes(JPEG_HEADER).unwrap(), FileFormat::Jpeg);
    }

    #[test]
    fn detects_png_from_bytes() {
        assert_eq!(detect_from_bytes(PNG_HEADER).unwrap(), FileFormat::Png);
    }

    #[test]
    fn detects_webp_from_bytes() {
        assert_eq!(detect_from_bytes(WEBP_HEADER).unwrap(), FileFormat::Webp);
    }

    #[test]
    fn detects_bmp_from_bytes() {
        assert_eq!(detect_from_bytes(BMP_HEADER).unwrap(), FileFormat::Bmp);
    }

    #[test]
    fn empty_bytes_returns_error() {
        let result = detect_from_bytes(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn random_bytes_returns_unknown() {
        let garbage = [0u8, 1, 2, 3, 4, 5, 6, 7, 8, 9];
        let result = detect_from_bytes(&garbage).unwrap();
        // Random bytes should not match any known format.
        assert!(matches!(
            result,
            FileFormat::Unknown | FileFormat::Unsupported(_)
        ));
    }

    // ── detect_from_path tests ───────────────────────────────────────────────

    #[test]
    fn detects_pdf_from_path_regardless_of_extension() {
        // PDF magic bytes written to a file named with a .txt extension.
        // Detection must use magic bytes, not the extension.
        let f = make_temp(PDF_HEADER, "txt");
        let result = detect_from_path(f.path()).unwrap();
        assert_eq!(result, FileFormat::Pdf);
    }

    #[test]
    fn detects_jpeg_from_path() {
        let f = make_temp(JPEG_HEADER, "jpg");
        assert_eq!(detect_from_path(f.path()).unwrap(), FileFormat::Jpeg);
    }

    #[test]
    fn missing_file_returns_invalid_input_error() {
        let result = detect_from_path(Path::new("does_not_exist.pdf"));
        assert!(result.is_err());
        let e = result.unwrap_err();
        assert_eq!(e.code, crate::common::errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn empty_file_returns_invalid_input_error() {
        let f = make_temp(&[], "pdf");
        let result = detect_from_path(f.path());
        assert!(result.is_err());
    }

    // ── Extension mismatch ───────────────────────────────────────────────────

    #[test]
    fn jpeg_extension_matches_jpeg_format() {
        let path = Path::new("photo.jpg");
        assert!(extension_matches_format(path, &FileFormat::Jpeg));
    }

    #[test]
    fn jpeg_extension_does_not_match_png_format() {
        let path = Path::new("photo.jpg");
        assert!(!extension_matches_format(path, &FileFormat::Png));
    }

    #[test]
    fn jpeg_alternative_extension_matches() {
        assert!(extension_matches_format(
            Path::new("photo.jpeg"),
            &FileFormat::Jpeg
        ));
    }
}
