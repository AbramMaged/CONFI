// CONFI — File format detection
//
// Detection is ALWAYS from magic bytes (file signatures), never from file
// extension. This satisfies AGENTS.md §3 and SRS SYS-01.
//
// Strategy:
//   1. Read up to 16 bytes of the file (enough for all our signatures).
//   2. Use the `infer` crate to identify the format from those bytes.
//   3. Map the result to our FileFormat enum.
//
// Important distinction:
//   detection  = "what format is this file?" (magic bytes only)
//   validation = "is this file structurally intact?" (see validation.rs)
//
// Detection can succeed on a truncated file; validation then catches the
// truncation.

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
    if !path.exists() {
        return Err(ConfiError::invalid_input(format!(
            "File not found: {}",
            path.display()
        )));
    }

    if !path.is_file() {
        return Err(ConfiError::invalid_input(format!(
            "Path is not a regular file: {}",
            path.display()
        )));
    }

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

    if let Some(kind) = infer::get(bytes) {
        return Ok(map_infer_type(kind));
    }

    // Bytes are present but no signature matched.
    Ok(FileFormat::Unknown)
}

/// Read the first 16 bytes from a file — enough for all signatures we care about.
fn read_header_bytes(path: &Path) -> ConfiResult<Vec<u8>> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).map_err(|e| {
        ConfiError::file_access_denied(format!("Cannot open file '{}': {e}", path.display()))
    })?;

    // PDF: %PDF (4 bytes)   JPEG: FF D8 FF (3)   PNG: 89 50 4E 47 (8)
    // WEBP: RIFF????WEBP (12)   BMP: 42 4D (2)
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
