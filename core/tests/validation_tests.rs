// CONFI — Integration tests for file validation
//
// Tests the three-stage validation model:
//   Stage 1: Detection (magic bytes) — tested in detection_tests.rs
//   Stage 2: Structural validation — tested here
//   Stage 3: Operational validation — tested by individual engine tests
//
// Per M1 feedback: distinguishes between detection and validation errors.
// A file with valid magic bytes but corrupted internal structure must
// return CORRUPTED_FILE, not UNSUPPORTED_FORMAT.

use confi_core::common::errors::ErrorCode;
use confi_core::common::models::FileFormat;
use confi_core::file_engine::validation;
use tempfile::TempDir;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn write_temp(dir: &TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

// ── Valid files ───────────────────────────────────────────────────────────────

#[test]
fn valid_jpeg_passes_validation() {
    let dir = TempDir::new().unwrap();
    // Create a real valid JPEG.
    let path = dir.path().join("valid.jpg");
    let img = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([255, 0, 0]));
    img.save_with_format(&path, image::ImageFormat::Jpeg)
        .unwrap();

    let result = validation::validate_file(&path, &FileFormat::Jpeg);
    assert!(result.is_ok(), "Valid JPEG should pass: {:?}", result.err());
    let r = result.unwrap();
    assert!(r.dimensions.is_some());
    let (w, h) = r.dimensions.unwrap();
    assert_eq!(w, 10);
    assert_eq!(h, 10);
}

#[test]
fn valid_png_passes_validation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("valid.png");
    let img = image::RgbImage::from_fn(20, 15, |_, _| image::Rgb([0, 255, 0]));
    img.save_with_format(&path, image::ImageFormat::Png)
        .unwrap();

    let r = validation::validate_file(&path, &FileFormat::Png).unwrap();
    assert_eq!(r.dimensions, Some((20, 15)));
}

#[test]
fn valid_bmp_passes_validation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("valid.bmp");
    let img = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([0, 0, 255]));
    img.save_with_format(&path, image::ImageFormat::Bmp)
        .unwrap();

    let r = validation::validate_file(&path, &FileFormat::Bmp).unwrap();
    assert!(r.dimensions.is_some());
}

#[test]
fn valid_pdf_skeleton_passes_validation() {
    let dir = TempDir::new().unwrap();
    let mut data = b"%PDF-1.7\n".to_vec();
    data.extend_from_slice(b"1 0 obj\n<< >>\nendobj\n");
    data.extend_from_slice(&vec![b' '; 80]);
    data.extend_from_slice(b"\n%%EOF\n");
    let path = write_temp(&dir, "valid.pdf", &data);

    let r = validation::validate_file(&path, &FileFormat::Pdf).unwrap();
    assert_eq!(r.format, FileFormat::Pdf);
}

// ── Truncated files ──────────────────────────────────────────────────────────

#[test]
fn truncated_jpeg_fails_validation() {
    let dir = TempDir::new().unwrap();
    // JPEG header but truncated immediately after.
    let path = write_temp(&dir, "truncated.jpg", &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10]);

    let err = validation::validate_file(&path, &FileFormat::Jpeg).unwrap_err();
    assert_eq!(
        err.code,
        ErrorCode::CorruptedFile,
        "Truncated JPEG should return CORRUPTED_FILE, got {:?}",
        err.code
    );
}

#[test]
fn truncated_png_fails_validation() {
    let dir = TempDir::new().unwrap();
    // PNG header but no IHDR or IDAT chunks.
    let path = write_temp(
        &dir,
        "truncated.png",
        &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
    );

    let err = validation::validate_file(&path, &FileFormat::Png).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptedFile);
}

#[test]
fn truncated_pdf_no_eof_fails_validation() {
    let dir = TempDir::new().unwrap();
    let mut data = b"%PDF-1.7\n".to_vec();
    data.extend_from_slice(&vec![b' '; 200]); // No %%EOF marker.
    let path = write_temp(&dir, "truncated.pdf", &data);

    let err = validation::validate_file(&path, &FileFormat::Pdf).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptedFile);
}

// ── Valid header but corrupted content ────────────────────────────────────────

#[test]
fn jpeg_header_with_garbage_body_fails() {
    let dir = TempDir::new().unwrap();
    let mut data = vec![0xFF, 0xD8, 0xFF, 0xE0]; // JPEG SOI + APP0
    data.extend_from_slice(&[0xDE; 200]); // garbage
    let path = write_temp(&dir, "garbage_body.jpg", &data);

    let err = validation::validate_file(&path, &FileFormat::Jpeg).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptedFile);
}

// ── Incorrect file extensions ────────────────────────────────────────────────
// Validation operates on the detected format, not the extension.

#[test]
fn jpeg_validated_with_correct_detected_format_regardless_of_extension() {
    let dir = TempDir::new().unwrap();
    // Create a valid JPEG but name it .png.
    let path = dir.path().join("actually_jpeg.png");
    let img = image::RgbImage::from_fn(5, 5, |_, _| image::Rgb([128, 128, 128]));
    img.save_with_format(&path, image::ImageFormat::Jpeg)
        .unwrap();

    // Validate as JPEG (detected format, not extension).
    let r = validation::validate_file(&path, &FileFormat::Jpeg);
    assert!(
        r.is_ok(),
        "Should validate based on detected format, not extension"
    );
}

// ── Unsupported formats ──────────────────────────────────────────────────────

#[test]
fn unsupported_format_returns_correct_error() {
    let dir = TempDir::new().unwrap();
    let path = write_temp(&dir, "file.tiff", b"some tiff-like content");

    let err = validation::validate_file(&path, &FileFormat::Unsupported("image/tiff".to_string()))
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedFormat);
}

#[test]
fn unknown_format_returns_invalid_input() {
    let dir = TempDir::new().unwrap();
    let path = write_temp(&dir, "file.bin", &[0x00, 0x01, 0x02]);

    let err = validation::validate_file(&path, &FileFormat::Unknown).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

// ── Empty files ──────────────────────────────────────────────────────────────

#[test]
fn empty_file_returns_invalid_input() {
    let dir = TempDir::new().unwrap();
    let path = write_temp(&dir, "empty.pdf", &[]);

    let err = validation::validate_file(&path, &FileFormat::Pdf).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

// ── Tiny BMP (below minimum valid size) ──────────────────────────────────────

#[test]
fn tiny_bmp_below_minimum_fails() {
    let dir = TempDir::new().unwrap();
    // BMP header is 14 bytes, DIB header is 40 bytes. Below 54 is invalid.
    let path = write_temp(&dir, "tiny.bmp", &[0x42, 0x4D, 0x10, 0x00]);

    let err = validation::validate_file(&path, &FileFormat::Bmp).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptedFile);
}

// ── Encrypted PDF ────────────────────────────────────────────────────────────

#[test]
fn encrypted_pdf_returns_password_required() {
    let dir = TempDir::new().unwrap();
    let mut data = b"%PDF-1.7\n".to_vec();
    data.extend_from_slice(b"1 0 obj\n<< /Encrypt 2 0 R >>\nendobj\n");
    data.extend_from_slice(&vec![b' '; 80]);
    data.extend_from_slice(b"\n%%EOF\n");
    let path = write_temp(&dir, "encrypted.pdf", &data);

    let err = validation::validate_file(&path, &FileFormat::Pdf).unwrap_err();
    assert_eq!(err.code, ErrorCode::PasswordRequired);
}
