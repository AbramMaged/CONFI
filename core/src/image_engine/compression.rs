// CONFI — Image compression
//
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
        // Deflate level 9: best ratio; oxipng's own multicore pass keeps it usable.
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

    // oxipng preserves the file even if it cannot reduce it further — that is
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
        Ok(())
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
