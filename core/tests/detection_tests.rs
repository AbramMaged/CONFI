// CONFI — Integration tests for file detection
//
// These tests validate the full detection pipeline from disk files,
// including edge cases: wrong extensions, truncated files, corrupted content.

use confi_core::common::errors::ErrorCode;
use confi_core::common::models::FileFormat;
use confi_core::file_engine::detection;
use std::io::Write;
use tempfile::NamedTempFile;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn make_temp(bytes: &[u8], ext: &str) -> NamedTempFile {
    let mut f = tempfile::Builder::new()
        .suffix(&format!(".{ext}"))
        .tempfile()
        .unwrap();
    f.write_all(bytes).unwrap();
    f
}

// ── Magic byte signatures ────────────────────────────────────────────────────

const PDF_HEADER: &[u8] = b"%PDF-1.7\n";
const JPEG_HEADER: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0];
const PNG_HEADER: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
const WEBP_HEADER: &[u8] = b"RIFF\x00\x00\x00\x00WEBP";
const BMP_HEADER: &[u8] = &[0x42, 0x4D, 0x36, 0x00];

// ── Correct detection tests ──────────────────────────────────────────────────

#[test]
fn detect_pdf_with_correct_extension() {
    let f = make_temp(PDF_HEADER, "pdf");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Pdf
    );
}

#[test]
fn detect_jpeg_with_correct_extension() {
    let f = make_temp(JPEG_HEADER, "jpg");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Jpeg
    );
}

#[test]
fn detect_png_with_correct_extension() {
    let f = make_temp(PNG_HEADER, "png");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Png
    );
}

#[test]
fn detect_webp_with_correct_extension() {
    let f = make_temp(WEBP_HEADER, "webp");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Webp
    );
}

#[test]
fn detect_bmp_with_correct_extension() {
    let f = make_temp(BMP_HEADER, "bmp");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Bmp
    );
}

// ── Wrong extension tests (detection MUST use magic bytes, not extension) ────

#[test]
fn detect_pdf_with_txt_extension() {
    let f = make_temp(PDF_HEADER, "txt");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Pdf
    );
}

#[test]
fn detect_jpeg_with_png_extension() {
    let f = make_temp(JPEG_HEADER, "png");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Jpeg
    );
}

#[test]
fn detect_png_with_jpg_extension() {
    let f = make_temp(PNG_HEADER, "jpg");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Png
    );
}

// ── Extension mismatch ──────────────────────────────────────────────────────

#[test]
fn extension_mismatch_detected_but_not_blocking() {
    let f = make_temp(PDF_HEADER, "jpg");
    // Detection succeeds: magic bytes win over the extension.
    let format = detection::detect_from_path(f.path()).unwrap();
    assert_eq!(format, FileFormat::Pdf);
}

// ── Error cases ──────────────────────────────────────────────────────────────

#[test]
fn empty_file_returns_error() {
    let f = make_temp(&[], "pdf");
    let err = detection::detect_from_path(f.path()).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[test]
fn missing_file_returns_error() {
    let err =
        detection::detect_from_path(std::path::Path::new("nonexistent_file.pdf")).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[test]
fn random_bytes_return_unknown() {
    let f = make_temp(&[0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07], "bin");
    let result = detection::detect_from_path(f.path()).unwrap();
    assert!(
        matches!(result, FileFormat::Unknown | FileFormat::Unsupported(_)),
        "Random bytes should not match a supported format"
    );
}

// ── Truncated files with valid magic bytes ───────────────────────────────────
// These test that detection succeeds even on truncated files — it is the
// validation stage's job to catch truncation, not detection.

#[test]
fn truncated_pdf_still_detected_as_pdf() {
    // Only the magic header, no content.
    let f = make_temp(PDF_HEADER, "pdf");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Pdf
    );
}

#[test]
fn truncated_jpeg_still_detected_as_jpeg() {
    let f = make_temp(JPEG_HEADER, "jpg");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Jpeg
    );
}

#[test]
fn truncated_png_still_detected_as_png() {
    let f = make_temp(PNG_HEADER, "png");
    assert_eq!(
        detection::detect_from_path(f.path()).unwrap(),
        FileFormat::Png
    );
}

// ── Unsupported but recognisable formats ─────────────────────────────────────

#[test]
fn gif_detected_as_unsupported() {
    let gif_header = b"GIF89a";
    let f = make_temp(gif_header, "gif");
    let result = detection::detect_from_path(f.path()).unwrap();
    assert!(matches!(result, FileFormat::Unsupported(_)));
}

#[test]
fn tiff_detected_as_unsupported() {
    // Big-endian TIFF header (requires > 9 bytes for infer to distinguish from CR2)
    let tiff_header = &[0x4D, 0x4D, 0x00, 0x2A, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00];
    let f = make_temp(tiff_header, "tiff");
    let result = detection::detect_from_path(f.path()).unwrap();
    assert!(matches!(result, FileFormat::Unsupported(_)));
}
