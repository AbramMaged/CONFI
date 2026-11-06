// CONFI — Integration tests for image conversion
//
// These tests create real image files, convert them, and verify the outputs
// by re-decoding them. Not synthetic — actual encoding/decoding roundtrips.

use confi_core::common::errors::ErrorCode;
use confi_core::common::models::ImageOutputFormat;
use confi_core::image_engine::conversion;
use tempfile::TempDir;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Create a real 10×10 solid-color PNG file.
fn create_test_png(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let img = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([255, 0, 0]));
    img.save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path
}

/// Create a real 10×10 JPEG file.
fn create_test_jpeg(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let img = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([0, 128, 255]));
    img.save_with_format(&path, image::ImageFormat::Jpeg)
        .unwrap();
    path
}

/// Create a 10×10 RGBA PNG with partial transparency.
fn create_transparent_png(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let img = image::RgbaImage::from_fn(10, 10, |x, _| {
        if x < 5 {
            image::Rgba([255, 0, 0, 128]) // semi-transparent red
        } else {
            image::Rgba([0, 0, 255, 0]) // fully transparent blue
        }
    });
    img.save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path
}

/// Create a 10×10 BMP file.
fn create_test_bmp(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let img = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([0, 255, 0]));
    img.save_with_format(&path, image::ImageFormat::Bmp)
        .unwrap();
    path
}

// ── Conversion roundtrip tests ───────────────────────────────────────────────

#[test]
fn png_to_jpeg_roundtrip() {
    let dir = TempDir::new().unwrap();
    let input = create_test_png(&dir, "input.png");
    let output = dir.path().join("output.jpg");

    let r = conversion::convert_image(&input, ImageOutputFormat::Jpeg, Some(85), &output).unwrap();
    assert!(output.exists());
    assert!(r.output_size_bytes > 0);

    // Verify output is valid JPEG by re-decoding.
    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 10);
    assert_eq!(decoded.height(), 10);
}

#[test]
fn jpeg_to_png_roundtrip() {
    let dir = TempDir::new().unwrap();
    let input = create_test_jpeg(&dir, "input.jpg");
    let output = dir.path().join("output.png");

    let r = conversion::convert_image(&input, ImageOutputFormat::Png, None, &output).unwrap();
    assert!(output.exists());
    assert!(r.output_size_bytes > 0);

    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 10);
    assert_eq!(decoded.height(), 10);
}

#[test]
fn png_to_webp_roundtrip() {
    let dir = TempDir::new().unwrap();
    let input = create_test_png(&dir, "input.png");
    let output = dir.path().join("output.webp");

    let r = conversion::convert_image(&input, ImageOutputFormat::Webp, None, &output).unwrap();
    assert!(output.exists());
    assert!(r.output_size_bytes > 0);

    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 10);
    assert_eq!(decoded.height(), 10);
}

#[test]
fn png_to_bmp_roundtrip() {
    let dir = TempDir::new().unwrap();
    let input = create_test_png(&dir, "input.png");
    let output = dir.path().join("output.bmp");

    let r = conversion::convert_image(&input, ImageOutputFormat::Bmp, None, &output).unwrap();
    assert!(output.exists());
    assert!(r.output_size_bytes > 0);

    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 10);
    assert_eq!(decoded.height(), 10);
}

#[test]
fn bmp_to_jpeg_roundtrip() {
    let dir = TempDir::new().unwrap();
    let input = create_test_bmp(&dir, "input.bmp");
    let output = dir.path().join("output.jpg");

    let r = conversion::convert_image(&input, ImageOutputFormat::Jpeg, Some(90), &output).unwrap();
    assert!(output.exists());
    assert!(r.output_size_bytes > 0);

    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 10);
    assert_eq!(decoded.height(), 10);
}

// ── Transparency handling ────────────────────────────────────────────────────

#[test]
fn transparent_png_to_jpeg_flattens_to_white() {
    let dir = TempDir::new().unwrap();
    let input = create_transparent_png(&dir, "transparent.png");
    let output = dir.path().join("output.jpg");

    let r = conversion::convert_image(&input, ImageOutputFormat::Jpeg, Some(95), &output).unwrap();
    assert!(
        r.transparency_flattened,
        "Should report that transparency was flattened"
    );
    assert!(r.warnings.iter().any(|w| w.contains("transparent")));

    // Verify the output is a valid JPEG.
    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 10);
    assert_eq!(decoded.height(), 10);

    // The fully transparent pixel (right side) should become white.
    let rgb = decoded.to_rgb8();
    let pixel = rgb.get_pixel(9, 0);
    // Should be close to white (255,255,255) since original was fully transparent.
    assert!(
        pixel[0] > 240 && pixel[1] > 240 && pixel[2] > 240,
        "Expected near-white, got {:?}",
        pixel
    );
}

#[test]
fn transparent_png_to_png_preserves_alpha() {
    let dir = TempDir::new().unwrap();
    let input = create_transparent_png(&dir, "transparent.png");
    let output = dir.path().join("output.png");

    let r = conversion::convert_image(&input, ImageOutputFormat::Png, None, &output).unwrap();
    assert!(
        !r.transparency_flattened,
        "PNG to PNG should preserve transparency"
    );
}

// ── JPEG quality control ─────────────────────────────────────────────────────

#[test]
fn jpeg_quality_affects_output_size() {
    let dir = TempDir::new().unwrap();
    // Create a larger image so quality differences are measurable.
    let input = dir.path().join("input.png");
    let img = image::RgbImage::from_fn(100, 100, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    });
    img.save_with_format(&input, image::ImageFormat::Png)
        .unwrap();

    let low_q = dir.path().join("low_quality.jpg");
    let high_q = dir.path().join("high_quality.jpg");

    let r_low =
        conversion::convert_image(&input, ImageOutputFormat::Jpeg, Some(10), &low_q).unwrap();
    let r_high =
        conversion::convert_image(&input, ImageOutputFormat::Jpeg, Some(95), &high_q).unwrap();

    assert!(
        r_low.output_size_bytes < r_high.output_size_bytes,
        "Quality 10 ({} bytes) should produce a smaller file than quality 95 ({} bytes)",
        r_low.output_size_bytes,
        r_high.output_size_bytes
    );
}

// ── Safety: no-overwrite ─────────────────────────────────────────────────────

#[test]
fn refuses_to_overwrite_existing_file() {
    let dir = TempDir::new().unwrap();
    let input = create_test_png(&dir, "input.png");
    let output = dir.path().join("output.jpg");

    // Pre-create with known content.
    std::fs::write(&output, b"original content").unwrap();

    let err =
        conversion::convert_image(&input, ImageOutputFormat::Jpeg, Some(85), &output).unwrap_err();
    assert_eq!(err.code, ErrorCode::OutputError);

    // Verify original content is untouched.
    let content = std::fs::read(&output).unwrap();
    assert_eq!(content, b"original content");
}

// ── Safety: original file unchanged ──────────────────────────────────────────

#[test]
fn original_file_unchanged_after_conversion() {
    let dir = TempDir::new().unwrap();
    let input = create_test_png(&dir, "input.png");
    let output = dir.path().join("output.jpg");

    let original_bytes = std::fs::read(&input).unwrap();

    conversion::convert_image(&input, ImageOutputFormat::Jpeg, Some(85), &output).unwrap();

    let after_bytes = std::fs::read(&input).unwrap();
    assert_eq!(
        original_bytes, after_bytes,
        "Original file must not be modified"
    );
}

// ── Error handling: corrupted input ──────────────────────────────────────────

#[test]
fn corrupted_image_returns_error() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("corrupted.png");
    // Valid PNG header followed by garbage.
    let mut data = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    data.extend_from_slice(&[0xFF; 100]);
    std::fs::write(&input, &data).unwrap();

    let output = dir.path().join("output.jpg");
    let err =
        conversion::convert_image(&input, ImageOutputFormat::Jpeg, Some(85), &output).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptedFile);
    // Output should NOT exist after a failed conversion.
    assert!(!output.exists());
}
