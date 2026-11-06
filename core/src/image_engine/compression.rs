// CONFI — Image compression
//
// Important distinction (from M1 feedback):
//   - "Conversion" = changing the format.
//   - "Compression" = reducing file size while keeping the format.
//
// This module handles same-format compression.
//
// Behavior when output would be larger than input:
//   - Do NOT overwrite the original.
//   - Return a CompressionResult with `size_increased = true`.
//   - Let the platform layer (Tauri/UI) decide whether to show the result.
//   - Never silently discard — the user should know what happened.
//
// PNG: lossless optimization via `oxipng`.
// JPEG: re-encode at the requested quality via `image` crate JpegEncoder.
// WebP: lossless re-encode (lossy requires `webp-encode` feature).
// BMP: no compression available — return UnsupportedFormat for compress ops.

use std::path::{Path, PathBuf};

use crate::common::{
    errors::{ConfiError, ConfiResult, ErrorCode},
    fs::{finalize, require_absent, temp_path_for},
    models::FileFormat,
};

// ── Public API ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct CompressionResult {
    pub output_path: PathBuf,
    pub input_size_bytes: u64,
    pub output_size_bytes: u64,
    /// True if the "compressed" output is larger than the input.
    /// The platform layer must inform the user and not present this as success.
    pub size_increased: bool,
    pub warnings: Vec<String>,
}

/// Compress an image in-format.
///
/// - `quality`: 1–100 for lossy formats. For PNG (lossless), ignored — oxipng
///   determines the optimal compression level automatically.
/// - `output_path`: must not already exist.
pub fn compress_image(
    input_path: &Path,
    format: &FileFormat,
    quality: Option<u8>,
    output_path: &Path,
) -> ConfiResult<CompressionResult> {
    require_absent(output_path)?;

    let input_size_bytes = std::fs::metadata(input_path)
        .map_err(|e| ConfiError::file_access_denied(e.to_string()))?
        .len();

    let tmp_path = temp_path_for(output_path);

    let mut warnings = Vec::new();

    match format {
        FileFormat::Jpeg => compress_jpeg(input_path, &tmp_path, quality.unwrap_or(80))?,
        FileFormat::Png => compress_png(input_path, &tmp_path, &mut warnings)?,
        FileFormat::Webp => compress_webp(input_path, &tmp_path, quality, &mut warnings)?,
        FileFormat::Bmp => {
            return Err(ConfiError::new(
                ErrorCode::UnsupportedFormat,
                "BMP does not support compression. Convert to PNG or JPEG for a smaller file.",
            ));
        }
        _ => {
            return Err(ConfiError::unsupported_format(
                "Compression is not supported for this file format.",
            ));
        }
    }

    let output_size_bytes = std::fs::metadata(&tmp_path).map(|m| m.len()).unwrap_or(0);

    // Critical rule: if output is larger, do NOT move it to the final path.
    // Report the situation to the caller instead.
    let size_increased = output_size_bytes >= input_size_bytes;

    if size_increased {
        // Clean up the temp file.
        let _ = std::fs::remove_file(&tmp_path);
        warnings.push(format!(
            "The compressed output ({} bytes) would be larger than the original ({} bytes). \
             The original file has not been changed. \
             This file may already be well-optimized.",
            output_size_bytes, input_size_bytes
        ));
        return Ok(CompressionResult {
            output_path: output_path.to_owned(),
            input_size_bytes,
            output_size_bytes,
            size_increased: true,
            warnings,
        });
    }

    // Move temp → final.
    finalize(&tmp_path, output_path)?;

    Ok(CompressionResult {
        output_path: output_path.to_owned(),
        input_size_bytes,
        output_size_bytes,
        size_increased: false,
        warnings,
    })
}

// ── Format-specific compressors ───────────────────────────────────────────────

fn compress_jpeg(input: &Path, output: &Path, quality: u8) -> ConfiResult<()> {
    let img = image::open(input).map_err(|e| {
        ConfiError::corrupted_file(format!("Cannot decode JPEG for compression: {e}"))
    })?;

    let file = std::fs::File::create(output)
        .map_err(|e| ConfiError::output_error(format!("Cannot create temp file: {e}")))?;

    let mut encoder =
        image::codecs::jpeg::JpegEncoder::new_with_quality(file, quality.clamp(1, 100));
    encoder
        .encode_image(&img)
        .map_err(|e| ConfiError::processing_error_detail("JPEG re-encoding failed", e.to_string()))
}

fn compress_png(input: &Path, output: &Path, warnings: &mut Vec<String>) -> ConfiResult<()> {
    // Copy to temp first, then run oxipng in-place on the temp copy.
    std::fs::copy(input, output)
        .map_err(|e| ConfiError::output_error(format!("Cannot copy PNG to temp: {e}")))?;

    let options = oxipng::Options {
        // Level 3: good compression speed/ratio trade-off.
        // Level 6 is maximum but significantly slower for large files.
        deflate: oxipng::Deflaters::Libdeflater { compression: 9 },
        ..oxipng::Options::default()
    };

    oxipng::optimize(
        &oxipng::InFile::Path(output.to_owned()),
        &oxipng::OutFile::Path {
            path: Some(output.to_owned()),
            preserve_attrs: true,
        },
        &options,
    )
    .map_err(|e| {
        let _ = std::fs::remove_file(output);
        ConfiError::processing_error_detail("PNG optimization failed", e.to_string())
    })?;

    // oxipng preserves the file even if it can't reduce it further — that is
    // correct behavior. The size_increased check in compress_image handles it.
    warnings.push("PNG compression is lossless. Image quality is preserved exactly.".to_string());

    Ok(())
}

fn compress_webp(
    input: &Path,
    output: &Path,
    quality: Option<u8>,
    warnings: &mut Vec<String>,
) -> ConfiResult<()> {
    #[cfg(feature = "webp-encode")]
    {
        let img = image::open(input)
            .map_err(|e| ConfiError::corrupted_file(format!("Cannot decode WebP: {e}")))?;
        let q = quality.unwrap_or(80).clamp(1, 100) as f32;
        let rgb = img.to_rgb8();
        let encoder = webp::Encoder::from_rgb(rgb.as_raw(), rgb.width(), rgb.height());
        let data = encoder.encode(q);
        std::fs::write(output, &*data)
            .map_err(|e| ConfiError::output_error(format!("Cannot write WebP: {e}")))?;
        return Ok(());
    }

    #[cfg(not(feature = "webp-encode"))]
    {
        // Fall back to image crate re-encode (lossless).
        let img = image::open(input)
            .map_err(|e| ConfiError::corrupted_file(format!("Cannot decode WebP: {e}")))?;
        img.save_with_format(output, image::ImageFormat::WebP)
            .map_err(|e| {
                ConfiError::processing_error_detail("WebP re-encode failed", e.to_string())
            })?;
        if let Some(q) = quality {
            warnings.push(format!(
                "Requested WebP quality ({q}) could not be applied because lossy encoding is disabled (libwebp not enabled). \
                 The image was re-encoded losslessly instead."
            ));
        } else {
            warnings.push(
                "WebP was re-encoded without lossy quality control (libwebp not enabled). \
                 Enable the `webp-encode` feature for quality-controlled lossy compression."
                    .to_string(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Minimal valid JPEG (2×2 pixel, quality 95).
    /// Generated offline with: convert -size 2x2 xc:red out.jpg (then base64).
    fn minimal_jpeg_bytes() -> Vec<u8> {
        // A minimal JFIF JPEG — 2×2 red pixel image.
        vec![
            0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00,
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06,
            0x07, 0x06, 0x05, 0x08, 0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D,
            0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12, 0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D,
            0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20, 0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28,
            0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27, 0x39, 0x3D, 0x38, 0x32,
            0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x02, 0x00, 0x02,
            0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00, 0x00, 0x01, 0x05, 0x01, 0x01,
            0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02,
            0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4, 0x00, 0xB5, 0x10,
            0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04, 0x00, 0x00,
            0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06,
            0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42,
            0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16,
            0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37,
            0x38, 0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55,
            0x56, 0x57, 0x58, 0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73,
            0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
            0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
            0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA,
            0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6,
            0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA,
            0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9, 0xFA, 0xFF, 0xDA, 0x00, 0x08,
            0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0xFB, 0xD3, 0xFF, 0xD9,
        ]
    }

    #[test]
    fn jpeg_compression_produces_smaller_or_warns() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.jpg");
        std::fs::write(&input, minimal_jpeg_bytes()).unwrap();
        let output = dir.path().join("output.jpg");

        let result = compress_image(&input, &FileFormat::Jpeg, Some(50), &output);
        assert!(result.is_ok());
        let r = result.unwrap();

        // Either a compressed file was produced, or the tool correctly reported
        // that size increased (for a tiny 2×2 test image, this is expected).
        if r.size_increased {
            assert!(!output.exists(), "Output must not exist if size increased");
            assert!(!r.warnings.is_empty());
        } else {
            assert!(output.exists());
            assert!(r.output_size_bytes > 0);
        }
    }

    #[test]
    fn bmp_compression_returns_unsupported() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.bmp");
        // BMP header (minimal)
        std::fs::write(&input, &[0x42, 0x4D, 0x36, 0x00, 0x00, 0x00]).unwrap();
        let output = dir.path().join("output.bmp");

        let result = compress_image(&input, &FileFormat::Bmp, None, &output);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::UnsupportedFormat);
    }

    #[test]
    fn no_overwrite_of_existing_output() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.jpg");
        std::fs::write(&input, minimal_jpeg_bytes()).unwrap();
        let output = dir.path().join("output.jpg");
        std::fs::write(&output, b"already here").unwrap();

        let result = compress_image(&input, &FileFormat::Jpeg, Some(50), &output);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::OutputError);
        // Original preserved.
        assert_eq!(std::fs::read(&output).unwrap(), b"already here");
    }

    #[test]
    fn webp_compression_reports_quality_warning_when_lossy_disabled() {
        let dir = TempDir::new().unwrap();
        let input = dir.path().join("input.webp");
        let img = image::RgbImage::from_fn(10, 10, |_, _| image::Rgb([100, 150, 200]));
        img.save_with_format(&input, image::ImageFormat::WebP)
            .unwrap();
        let output = dir.path().join("output.webp");

        let result = compress_image(&input, &FileFormat::Webp, Some(60), &output);
        assert!(result.is_ok());
        let r = result.unwrap();
        #[cfg(not(feature = "webp-encode"))]
        {
            assert!(
                r.warnings
                    .iter()
                    .any(|w| w.contains("quality (60)") || w.contains("quality control")),
                "Expected warning about WebP quality, got: {:?}",
                r.warnings
            );
        }
    }
}
