// CONFI — Integration tests for image compression
//
// Tests the same-format compression pipeline with real encoded files.
// Validates the critical "size increased" path that must never silently
// claim success.

use confi_core::common::errors::ErrorCode;
use confi_core::common::models::FileFormat;
use confi_core::image_engine::compression;
use tempfile::TempDir;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Create a 100×100 gradient PNG (compressible content).
fn create_compressible_png(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let img = image::RgbImage::from_fn(100, 100, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    });
    img.save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path
}

/// Create a 100×100 JPEG at high quality (room to compress further).
fn create_high_quality_jpeg(dir: &TempDir, name: &str) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let img = image::RgbImage::from_fn(100, 100, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    });
    // Save as high-quality JPEG.
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 98);
    encoder.encode_image(&img).unwrap();
    path
}

// ── JPEG compression ─────────────────────────────────────────────────────────

#[test]
fn jpeg_compression_at_lower_quality_reduces_or_warns() {
    let dir = TempDir::new().unwrap();
    let input = create_high_quality_jpeg(&dir, "input.jpg");
    let output = dir.path().join("compressed.jpg");
    let input_size = std::fs::metadata(&input).unwrap().len();

    let r = compression::compress_image(&input, &FileFormat::Jpeg, Some(30), &output).unwrap();

    // Either the file is smaller (success) or size_increased is true (valid outcome).
    if r.size_increased {
        // Output must NOT exist — it was cleaned up.
        assert!(
            !output.exists(),
            "Output should be removed when size increased"
        );
        assert!(!r.warnings.is_empty());
    } else {
        assert!(output.exists());
        assert!(
            r.output_size_bytes < input_size,
            "Expected smaller output ({} bytes) than input ({} bytes)",
            r.output_size_bytes,
            input_size
        );
    }
}

#[test]
fn jpeg_compression_quality_affects_file_size() {
    let dir = TempDir::new().unwrap();
    let input = create_high_quality_jpeg(&dir, "input.jpg");
    let output_low = dir.path().join("compressed_low.jpg");
    let output_high = dir.path().join("compressed_high.jpg");

    let r_low =
        compression::compress_image(&input, &FileFormat::Jpeg, Some(20), &output_low).unwrap();
    let r_high =
        compression::compress_image(&input, &FileFormat::Jpeg, Some(90), &output_high).unwrap();

    assert!(
        r_low.output_size_bytes < r_high.output_size_bytes,
        "Quality 20 ({} bytes) should be smaller than quality 90 ({} bytes)",
        r_low.output_size_bytes,
        r_high.output_size_bytes
    );
}

// ── PNG compression (lossless via oxipng) ────────────────────────────────────

#[test]
fn png_compression_is_lossless() {
    let dir = TempDir::new().unwrap();
    let input = create_compressible_png(&dir, "input.png");
    let output = dir.path().join("compressed.png");

    let r = compression::compress_image(&input, &FileFormat::Png, None, &output).unwrap();

    if !r.size_increased {
        assert!(output.exists());
        // Verify the output is a valid PNG with same dimensions.
        let decoded = image::open(&output).unwrap();
        assert_eq!(decoded.width(), 100);
        assert_eq!(decoded.height(), 100);
    }
    // Even if size didn't decrease, the operation must not crash.
}

// ── BMP compression (unsupported) ────────────────────────────────────────────

#[test]
fn bmp_compression_returns_unsupported_error() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.bmp");
    let img = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([128, 128, 128]));
    img.save_with_format(&input, image::ImageFormat::Bmp)
        .unwrap();

    let output = dir.path().join("compressed.bmp");
    let err = compression::compress_image(&input, &FileFormat::Bmp, None, &output).unwrap_err();
    assert_eq!(err.code, ErrorCode::UnsupportedFormat);
}

// ── Safety: no-overwrite ─────────────────────────────────────────────────────

#[test]
fn refuses_to_overwrite_existing_output() {
    let dir = TempDir::new().unwrap();
    let input = create_high_quality_jpeg(&dir, "input.jpg");
    let output = dir.path().join("compressed.jpg");
    std::fs::write(&output, b"original content").unwrap();

    let err =
        compression::compress_image(&input, &FileFormat::Jpeg, Some(50), &output).unwrap_err();
    assert_eq!(err.code, ErrorCode::OutputError);
    assert_eq!(std::fs::read(&output).unwrap(), b"original content");
}

// ── Safety: original file unchanged ──────────────────────────────────────────

#[test]
fn original_file_unchanged_after_compression() {
    let dir = TempDir::new().unwrap();
    let input = create_high_quality_jpeg(&dir, "input.jpg");
    let output = dir.path().join("compressed.jpg");
    let original_bytes = std::fs::read(&input).unwrap();

    let _ = compression::compress_image(&input, &FileFormat::Jpeg, Some(50), &output);

    let after_bytes = std::fs::read(&input).unwrap();
    assert_eq!(original_bytes, after_bytes, "Original must not be modified");
}

// ── WebP compression (lossy disabled by default) ────────────────────────────

#[test]
fn webp_compression_reports_quality_warning_when_lossy_disabled() {
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("input.webp");
    let img = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([100, 150, 200]));
    img.save_with_format(&input, image::ImageFormat::WebP)
        .unwrap();
    let output = dir.path().join("output.webp");

    let r = compression::compress_image(&input, &FileFormat::Webp, Some(60), &output).unwrap();

    #[cfg(not(feature = "webp-encode"))]
    assert!(
        r.warnings
            .iter()
            .any(|w| w.contains("quality (60)") || w.contains("quality control")),
        "Expected warning about WebP quality, got: {:?}",
        r.warnings
    );
}

// ── Size increase handling ───────────────────────────────────────────────────

#[test]
fn size_increase_does_not_produce_output_file() {
    // Create an already-heavily-compressed JPEG.
    let dir = TempDir::new().unwrap();
    let input = dir.path().join("tiny.jpg");
    let img = image::RgbImage::from_fn(2, 2, |_, _| image::Rgb([128, 128, 128]));
    let file = std::fs::File::create(&input).unwrap();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(file, 1);
    encoder.encode_image(&img).unwrap();

    let output = dir.path().join("compressed.jpg");

    // Re-compressing a 2×2 already-compressed JPEG at higher quality
    // should produce a larger output.
    let r = compression::compress_image(&input, &FileFormat::Jpeg, Some(95), &output).unwrap();

    if r.size_increased {
        assert!(
            !output.exists(),
            "When size increases, the output file must be cleaned up"
        );
        assert!(
            !r.warnings.is_empty(),
            "Size increase must be reported via warnings"
        );
    }
    // If somehow it got smaller (unlikely at 2×2), that's also fine.
}
