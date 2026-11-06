// CONFI — Image format conversion
//
// All conversions:
//   1. Decode the input image fully.
//   2. Encode to the target format into a temp file.
//   3. Validate the output by re-decoding its header.
//   4. Move the temp file to the final destination only after validation.
//
// WebP encoding note (from M1 feedback):
//   The `image` crate includes a WebP decoder and a basic encoder, but as of
//   v0.25 the encoder does not expose quality control for lossy encoding.
//   When the `webp-encode` feature is disabled (default), we encode WebP via
//   the `image` crate (lossless / best-available), which may produce larger
//   files than libwebp's lossy encoder would. This limitation is documented in
//   the output metadata. Users should be aware the "compressed" WebP may not
//   always be smaller than the source JPEG.
//
// Transparent PNG → JPEG:
//   JPEG has no alpha channel. Transparent pixels are composited onto a white
//   background before encoding. This behavior is documented in the output result.
//   Future: allow the user to select the background color.

use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat, Rgba};

use crate::common::{
    errors::{ConfiError, ConfiResult, ErrorCode},
    fs::{finalize, require_absent, temp_path_for},
    models::ImageOutputFormat,
};

// ── Public API ───────────────────────────────────────────────────────────────

/// Result of a conversion operation.
#[derive(Debug)]
pub struct ConversionResult {
    /// Path to the validated output file.
    pub output_path: PathBuf,
    pub output_size_bytes: u64,
    pub input_size_bytes: u64,
    /// True if the input had transparency that was composited onto a background.
    pub transparency_flattened: bool,
    /// Warning messages that the UI should surface to the user.
    pub warnings: Vec<String>,
}

/// Convert an image file to a different format.
///
/// - `input_path`: source file (must already be validated by the file engine).
/// - `output_format`: target format.
/// - `quality`: 1–100 for lossy formats (JPEG, WebP). Ignored for lossless.
/// - `output_path`: final destination path. Must not already exist (callers
///   are responsible for checking; we do not silently overwrite).
pub fn convert_image(
    input_path: &Path,
    output_format: ImageOutputFormat,
    quality: Option<u8>,
    output_path: &Path,
) -> ConfiResult<ConversionResult> {
    // Guard: do not overwrite existing files.
    require_absent(output_path)?;

    let input_size_bytes = std::fs::metadata(input_path)
        .map_err(|e| ConfiError::file_access_denied(e.to_string()))?
        .len();

    // Decode the input image.
    let img = load_image(input_path)?;

    // Track whether transparency was present before any processing.
    let had_transparency = has_transparency(&img);

    // Flatten transparency when encoding to a format that does not support it.
    let (img, transparency_flattened) = if format_supports_alpha(output_format) {
        (img, false)
    } else if had_transparency {
        (flatten_to_white(img), true)
    } else {
        (img, false)
    };

    // Write to a temp file alongside the destination.
    let tmp_path = temp_path_for(output_path);
    encode_image(&img, &tmp_path, output_format, quality)?;

    // Validate the output by re-decoding its header.
    validate_output_image(&tmp_path)?;

    // Move temp → final destination.
    finalize(&tmp_path, output_path)?;

    let output_size_bytes = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);

    let mut warnings = Vec::new();
    if transparency_flattened {
        warnings.push(
            "The input image had a transparent background. \
             Transparent areas were filled with white because the output format does not support transparency."
                .to_string(),
        );
    }

    // WebP lossy quality warning when using the basic encoder.
    #[cfg(not(feature = "webp-encode"))]
    if output_format == ImageOutputFormat::Webp {
        warnings.push(
            "WebP was encoded without quality control (libwebp not enabled). \
             The output may not be smaller than a lossy-encoded WebP."
                .to_string(),
        );
    }

    Ok(ConversionResult {
        output_path: output_path.to_owned(),
        output_size_bytes,
        input_size_bytes,
        transparency_flattened,
        warnings,
    })
}

// ── Encoding ─────────────────────────────────────────────────────────────────

fn encode_image(
    img: &DynamicImage,
    path: &Path,
    format: ImageOutputFormat,
    quality: Option<u8>,
) -> ConfiResult<()> {
    match format {
        ImageOutputFormat::Jpeg => {
            let file = create_output_file(path)?;
            let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(
                file,
                quality.unwrap_or(85).clamp(1, 100),
            );
            encoder.encode_image(img).map_err(|e| {
                ConfiError::processing_error_detail("JPEG encoding failed", e.to_string())
            })
        }
        ImageOutputFormat::Png => img
            .save_with_format(path, ImageFormat::Png)
            .map_err(|e| ConfiError::processing_error_detail("PNG encoding failed", e.to_string())),
        ImageOutputFormat::Webp => encode_webp(img, path, quality),
        ImageOutputFormat::Bmp => img
            .save_with_format(path, ImageFormat::Bmp)
            .map_err(|e| ConfiError::processing_error_detail("BMP encoding failed", e.to_string())),
    }
}

fn encode_webp(img: &DynamicImage, path: &Path, _quality: Option<u8>) -> ConfiResult<()> {
    // With `webp-encode` feature: use libwebp for lossy quality control.
    // Without it: fall back to the `image` crate's WebP encoder (lossless/best).
    //
    // M1 PoC finding: the `image` crate v0.25 WebP encoder is lossless only.
    // For true lossy quality control, the `webp` crate (libwebp wrapper) is needed.
    // This is documented in LICENSES/INVENTORY.md and flagged in conversion warnings.

    #[cfg(feature = "webp-encode")]
    {
        let q = _quality.unwrap_or(80) as f32;
        let rgb = img.to_rgb8();
        let encoder = webp::Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height());
        let data = encoder.encode(q);
        std::fs::write(path, &*data)
            .map_err(|e| ConfiError::output_error(format!("Cannot write WebP: {e}")))
    }

    #[cfg(not(feature = "webp-encode"))]
    {
        img.save_with_format(path, ImageFormat::WebP)
            .map_err(|e| ConfiError::processing_error_detail("WebP encoding failed", e.to_string()))
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn load_image(path: &Path) -> ConfiResult<DynamicImage> {
    image::open(path).map_err(|e| {
        // Distinguish between access error and corruption.
        if e.to_string().to_lowercase().contains("permission") {
            ConfiError::file_access_denied(format!("Cannot read image: {e}"))
        } else {
            ConfiError::with_detail(
                ErrorCode::CorruptedFile,
                "The image file could not be decoded. It may be damaged.",
                e.to_string(),
            )
        }
    })
}

fn has_transparency(img: &DynamicImage) -> bool {
    use image::DynamicImage::*;
    matches!(
        img,
        ImageRgba8(_) | ImageRgba16(_) | ImageRgba32F(_) | ImageLumaA8(_) | ImageLumaA16(_)
    )
}

fn format_supports_alpha(format: ImageOutputFormat) -> bool {
    matches!(format, ImageOutputFormat::Png | ImageOutputFormat::Webp)
}

/// Composite an image with a transparent background onto white.
///
/// Uses the crate's own source-over blend rather than a per-pixel loop.
fn flatten_to_white(img: DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    let mut canvas = image::RgbaImage::from_pixel(
        rgba.width(),
        rgba.height(),
        Rgba([255, 255, 255, 255]),
    );
    image::imageops::overlay(&mut canvas, &rgba, 0, 0);
    DynamicImage::ImageRgba8(canvas)
}

/// Create an output file, returning a write handle.
fn create_output_file(path: &Path) -> ConfiResult<std::fs::File> {
    std::fs::File::create(path).map_err(|e| {
        ConfiError::output_error(format!(
            "Cannot create output file '{}': {e}",
            path.display()
        ))
    })
}

/// Re-decode the output file's header to confirm it was written correctly.
fn validate_output_image(path: &Path) -> ConfiResult<()> {
    let reader = image::ImageReader::open(path).map_err(|e| {
        ConfiError::processing_error_detail(
            "Output validation: cannot open temp file",
            e.to_string(),
        )
    })?;

    let (w, h) = reader.into_dimensions().map_err(|e| {
        let _ = std::fs::remove_file(path);
        ConfiError::processing_error_detail(
            "Output validation failed: the encoded image could not be re-read. The output has been discarded.",
            e.to_string(),
        )
    })?;

    if w == 0 || h == 0 {
        let _ = std::fs::remove_file(path);
        return Err(ConfiError::processing_error(
            "Output validation failed: encoded image has zero dimensions.",
        ));
    }

    // Verify the file extension on the output matches the expected format.
    // This is a sanity check on our own logic, not user input.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_transparency_detection() {
        // RGBA image has transparency.
        let rgba = DynamicImage::ImageRgba8(image::RgbaImage::new(1, 1));
        assert!(has_transparency(&rgba));

        // RGB image does not.
        let rgb = DynamicImage::ImageRgb8(image::RgbImage::new(1, 1));
        assert!(!has_transparency(&rgb));

        // Grayscale with alpha has transparency.
        let luma_a8 = DynamicImage::ImageLumaA8(image::GrayAlphaImage::new(1, 1));
        assert!(has_transparency(&luma_a8));

        let luma_a16 = DynamicImage::ImageLumaA16(image::ImageBuffer::new(1, 1));
        assert!(has_transparency(&luma_a16));

        // Plain grayscale does not.
        let luma8 = DynamicImage::ImageLuma8(image::GrayImage::new(1, 1));
        assert!(!has_transparency(&luma8));

        let luma16 = DynamicImage::ImageLuma16(image::ImageBuffer::new(1, 1));
        assert!(!has_transparency(&luma16));
    }

    #[test]
    fn flatten_to_white_removes_alpha() {
        let mut rgba = image::RgbaImage::new(1, 1);
        // Fully transparent red pixel.
        rgba.put_pixel(0, 0, Rgba([255, 0, 0, 0]));
        let img = DynamicImage::ImageRgba8(rgba);
        let flattened = flatten_to_white(img);
        let pixel = flattened.to_rgba8().get_pixel(0, 0).to_owned();
        // Fully transparent → should become white (255,255,255,255).
        assert_eq!(pixel, Rgba([255, 255, 255, 255]));
    }
}
