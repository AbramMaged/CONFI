// CONFI — Integration tests for image resizing
//
// Real image files, real resize operations, output validated by re-decoding.

use confi_core::common::models::ResizeDimensions;
use confi_core::image_engine::resize;
use tempfile::TempDir;

// ── Helpers ──────────────────────────────────────────────────────────────────

fn create_test_image(dir: &TempDir, name: &str, w: u32, h: u32) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        image::Rgb([(x % 256) as u8, (y % 256) as u8, 128])
    });
    img.save_with_format(&path, image::ImageFormat::Png)
        .unwrap();
    path
}

// ── Pixel-based resize tests ─────────────────────────────────────────────────

#[test]
fn resize_both_dimensions_explicit() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 200, 100);
    let output = dir.path().join("resized.png");

    let r = resize::resize_image(
        &input,
        ResizeDimensions::Pixels {
            width: Some(80),
            height: Some(40),
        },
        &output,
    )
    .unwrap();

    assert!(output.exists());
    assert_eq!(r.original_dimensions, (200, 100));
    // image::resize uses Lanczos3 which preserves aspect within the bounding box.
    // With both dims explicit, the output should fit within 80×40.
    assert!(r.output_dimensions.0 <= 80);
    assert!(r.output_dimensions.1 <= 40);

    // Verify by re-decoding.
    let decoded = image::open(&output).unwrap();
    assert!(decoded.width() <= 80);
    assert!(decoded.height() <= 40);
}

#[test]
fn resize_width_only_preserves_aspect_ratio() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 200, 100);
    let output = dir.path().join("resized.png");

    let r = resize::resize_image(
        &input,
        ResizeDimensions::Pixels {
            width: Some(100),
            height: None,
        },
        &output,
    )
    .unwrap();
    assert!(r.output_size_bytes > 0);

    // 200×100 → width=100 → height should be ~50 (aspect preserved).
    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 100);
    assert!(
        decoded.height() >= 49 && decoded.height() <= 51,
        "Expected height ~50, got {}",
        decoded.height()
    );
}

#[test]
fn resize_height_only_preserves_aspect_ratio() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 200, 100);
    let output = dir.path().join("resized.png");

    let r = resize::resize_image(
        &input,
        ResizeDimensions::Pixels {
            width: None,
            height: Some(50),
        },
        &output,
    )
    .unwrap();
    assert!(r.output_size_bytes > 0);

    let decoded = image::open(&output).unwrap();
    assert!(
        decoded.width() >= 99 && decoded.width() <= 101,
        "Expected width ~100, got {}",
        decoded.width()
    );
    assert_eq!(decoded.height(), 50);
}

// ── Percentage-based resize tests ────────────────────────────────────────────

#[test]
fn resize_50_percent() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 200, 100);
    let output = dir.path().join("resized.png");

    let r = resize::resize_image(
        &input,
        ResizeDimensions::Percentage { percentage: 50 },
        &output,
    )
    .unwrap();
    assert!(r.output_size_bytes > 0);

    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 100);
    assert_eq!(decoded.height(), 50);
}

#[test]
fn resize_100_percent_is_identity() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 200, 100);
    let output = dir.path().join("resized.png");

    resize::resize_image(
        &input,
        ResizeDimensions::Percentage { percentage: 100 },
        &output,
    )
    .unwrap();

    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 200);
    assert_eq!(decoded.height(), 100);
}

// ── Upscaling warning ────────────────────────────────────────────────────────

#[test]
fn upscaling_produces_warning() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 50, 50);
    let output = dir.path().join("resized.png");

    let r = resize::resize_image(
        &input,
        ResizeDimensions::Pixels {
            width: Some(200),
            height: Some(200),
        },
        &output,
    )
    .unwrap();

    assert!(!r.warnings.is_empty(), "Upscaling should produce a warning");
    assert!(
        r.warnings[0].contains("larger"),
        "Warning should mention upscaling"
    );
}

// ── Safety ───────────────────────────────────────────────────────────────────

#[test]
fn original_file_unchanged_after_resize() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 200, 100);
    let output = dir.path().join("resized.png");

    let original_bytes = std::fs::read(&input).unwrap();

    resize::resize_image(
        &input,
        ResizeDimensions::Percentage { percentage: 50 },
        &output,
    )
    .unwrap();

    let after_bytes = std::fs::read(&input).unwrap();
    assert_eq!(original_bytes, after_bytes, "Original must not be modified");
}

#[test]
fn refuses_to_overwrite_existing_output() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 100, 100);
    let output = dir.path().join("output.png");
    std::fs::write(&output, b"existing").unwrap();

    let err = resize::resize_image(
        &input,
        ResizeDimensions::Percentage { percentage: 50 },
        &output,
    )
    .unwrap_err();

    assert_eq!(err.code, confi_core::common::errors::ErrorCode::OutputError);
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");
}

// ── Invalid input must error, never silently "succeed" ──────────────────────

#[test]
fn zero_percentage_is_rejected() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 100, 100);
    let output = dir.path().join("out.png");

    let err = resize::resize_image(
        &input,
        ResizeDimensions::Percentage { percentage: 0 },
        &output,
    )
    .unwrap_err();

    assert_eq!(
        err.code,
        confi_core::common::errors::ErrorCode::InvalidInput
    );
    assert!(
        !output.exists(),
        "a rejected resize must not write an output file"
    );
}

#[test]
fn excessive_percentage_is_rejected() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 100, 100);
    let output = dir.path().join("out.png");

    // 100000% of 100px = 100000px, beyond the 65535 per-side limit.
    let err = resize::resize_image(
        &input,
        ResizeDimensions::Percentage {
            percentage: 100_000,
        },
        &output,
    )
    .unwrap_err();

    assert_eq!(
        err.code,
        confi_core::common::errors::ErrorCode::InvalidInput
    );
    assert!(!output.exists());
}

#[test]
fn excessive_width_is_rejected() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 100, 100);
    let output = dir.path().join("out.png");

    let err = resize::resize_image(
        &input,
        ResizeDimensions::Pixels {
            width: Some(100_000),
            height: None,
        },
        &output,
    )
    .unwrap_err();

    assert_eq!(
        err.code,
        confi_core::common::errors::ErrorCode::InvalidInput
    );
}

#[test]
fn malformed_json_shapes_are_rejected_before_resizing() {
    // Negative, unknown-field, both-forms and empty requests all fail to
    // deserialize, so they can never reach the engine as a silent no-op.
    for json in [
        r#"{"percentage": -50}"#,
        r#"{"width": -800, "height": 600}"#,
        r#"{"width": 800, "heigth": 600}"#,
        r#"{"percentage": 50, "width": 800}"#,
        "{}",
    ] {
        let parsed = serde_json::from_str::<ResizeDimensions>(json);
        assert!(parsed.is_err(), "expected rejection for: {json}");
    }
}

// ── Output format matches extension ──────────────────────────────────────────

#[test]
fn resize_to_jpeg_output() {
    let dir = TempDir::new().unwrap();
    let input = create_test_image(&dir, "input.png", 100, 100);
    let output = dir.path().join("resized.jpg");

    resize::resize_image(
        &input,
        ResizeDimensions::Percentage { percentage: 50 },
        &output,
    )
    .unwrap();

    // Verify the output is a valid JPEG.
    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 50);
    assert_eq!(decoded.height(), 50);
}
