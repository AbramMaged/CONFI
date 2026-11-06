// CONFI — Image resizing
//
// Supports two resize modes:
//   - Pixels: target width and/or height. If only one is given, the other is
//     computed to preserve the original aspect ratio.
//   - Percentage: scale by a percentage of the original dimensions.
//
// Upscaling is permitted but flagged with a warning.
// Output validation: re-read the output header after writing.
// No silent overwrites.

use std::path::{Path, PathBuf};

use image::imageops::FilterType;

use crate::common::{
    errors::{ConfiError, ConfiResult},
    fs::{finalize, require_absent, temp_path_for},
    models::ResizeDimensions,
};

// ── Public API ───────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct ResizeResult {
    pub output_path: PathBuf,
    pub original_dimensions: (u32, u32),
    pub output_dimensions: (u32, u32),
    pub output_size_bytes: u64,
    pub warnings: Vec<String>,
}

/// Resize an image.
///
/// - `dimensions`: target dimensions (pixels or percentage).
/// - `output_path`: must not already exist.
pub fn resize_image(
    input_path: &Path,
    dimensions: ResizeDimensions,
    output_path: &Path,
) -> ConfiResult<ResizeResult> {
    require_absent(output_path)?;

    let img = image::open(input_path).map_err(|e| {
        ConfiError::corrupted_file(format!("Cannot decode image for resizing: {e}"))
    })?;

    let (orig_w, orig_h) = (img.width(), img.height());
    let mut warnings = Vec::new();

    // A 0% request resolves to a 1x1 image, which is never what the user meant.
    if matches!(dimensions, ResizeDimensions::Percentage { percentage: 0 }) {
        return Err(ConfiError::invalid_input(
            "Percentage must be at least 1. A 0% resize would produce a 1×1 image.",
        ));
    }

    let (target_w, target_h) = resolve_dimensions(orig_w, orig_h, &dimensions);

    if target_w == 0 || target_h == 0 {
        return Err(ConfiError::invalid_input(
            "Target dimensions must be greater than zero.",
        ));
    }

    // Refuse sizes that no encoder can honour or that would exhaust memory,
    // instead of failing deep inside a huge allocation.
    // ponytail: fixed budget; raise it if large-format (CAD/poster) work needs it.
    const MAX_DIMENSION: u32 = 65_535;
    const MAX_PIXELS: u64 = 100_000_000;

    if target_w > MAX_DIMENSION || target_h > MAX_DIMENSION {
        return Err(ConfiError::invalid_input(format!(
            "Requested size {target_w}×{target_h} exceeds the {MAX_DIMENSION} pixel per-side limit."
        )));
    }
    if (target_w as u64) * (target_h as u64) > MAX_PIXELS {
        return Err(ConfiError::invalid_input(format!(
            "Requested size {target_w}×{target_h} exceeds the {MAX_PIXELS} pixel budget."
        )));
    }

    if target_w > orig_w || target_h > orig_h {
        warnings.push(format!(
            "The requested size ({target_w}×{target_h}) is larger than the original ({orig_w}×{orig_h}). \
             Upscaling will reduce image quality."
        ));
    }

    // Use Lanczos3 for high-quality downscaling; acceptable for upscaling too.
    let resized = img.resize(target_w, target_h, FilterType::Lanczos3);

    let tmp_path = temp_path_for(output_path);

    // Determine output format from the output path extension.
    let fmt = image_format_from_path(output_path)?;
    resized.save_with_format(&tmp_path, fmt).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        ConfiError::processing_error_detail("Failed to encode resized image", e.to_string())
    })?;

    // Validate output.
    let reader = image::ImageReader::open(&tmp_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        ConfiError::processing_error_detail("Output validation failed", e.to_string())
    })?;
    let (out_w, out_h) = reader.into_dimensions().map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        ConfiError::processing_error_detail(
            "Output validation: cannot read dimensions",
            e.to_string(),
        )
    })?;

    // Move temp → final.
    finalize(&tmp_path, output_path)?;

    let output_size_bytes = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);

    Ok(ResizeResult {
        output_path: output_path.to_owned(),
        original_dimensions: (orig_w, orig_h),
        output_dimensions: (out_w, out_h),
        output_size_bytes,
        warnings,
    })
}

// ── Dimension resolution ──────────────────────────────────────────────────────

/// Compute the actual target (width, height) from the requested dimensions and
/// original image size, preserving aspect ratio where a dimension is omitted.
fn resolve_dimensions(orig_w: u32, orig_h: u32, dims: &ResizeDimensions) -> (u32, u32) {
    match dims {
        ResizeDimensions::Pixels {
            width: Some(w),
            height: Some(h),
        } => (*w, *h),

        ResizeDimensions::Pixels {
            width: Some(w),
            height: None,
        } => {
            let h = scale_preserve(*w, orig_w, orig_h);
            (*w, h)
        }

        ResizeDimensions::Pixels {
            width: None,
            height: Some(h),
        } => {
            let w = scale_preserve(*h, orig_h, orig_w);
            (w, *h)
        }

        ResizeDimensions::Pixels {
            width: None,
            height: None,
        } => (orig_w, orig_h),

        ResizeDimensions::Percentage { percentage } => {
            let scale = *percentage as f64 / 100.0;
            let w = ((orig_w as f64 * scale).round() as u32).max(1);
            let h = ((orig_h as f64 * scale).round() as u32).max(1);
            (w, h)
        }
    }
}

/// Scale `dimension_a` proportionally: given that `from_a / from_b` is the
/// aspect ratio, compute the scaled value of `from_b` when `from_a` changes to `to_a`.
fn scale_preserve(to_a: u32, from_a: u32, from_b: u32) -> u32 {
    if from_a == 0 {
        return 1;
    }
    ((to_a as f64 / from_a as f64) * from_b as f64).round() as u32
}

fn image_format_from_path(path: &Path) -> ConfiResult<image::ImageFormat> {
    image::ImageFormat::from_path(path).map_err(|_| {
        ConfiError::invalid_operation(format!(
            "Cannot determine image format for output path: {}. \
             Make sure the file extension is one of: jpg, png, webp, bmp.",
            path.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_both_dimensions_explicit() {
        let (w, h) = resolve_dimensions(
            1920,
            1080,
            &ResizeDimensions::Pixels {
                width: Some(800),
                height: Some(600),
            },
        );
        assert_eq!((w, h), (800, 600));
    }

    #[test]
    fn resolve_width_only_preserves_aspect() {
        // 1920×1080 → width=960 → height should be 540 (ratio preserved).
        let (w, h) = resolve_dimensions(
            1920,
            1080,
            &ResizeDimensions::Pixels {
                width: Some(960),
                height: None,
            },
        );
        assert_eq!(w, 960);
        assert_eq!(h, 540);
    }

    #[test]
    fn resolve_height_only_preserves_aspect() {
        // 1920×1080 → height=540 → width should be 960.
        let (w, h) = resolve_dimensions(
            1920,
            1080,
            &ResizeDimensions::Pixels {
                width: None,
                height: Some(540),
            },
        );
        assert_eq!(w, 960);
        assert_eq!(h, 540);
    }

    #[test]
    fn resolve_percentage_50() {
        let (w, h) = resolve_dimensions(800, 600, &ResizeDimensions::Percentage { percentage: 50 });
        assert_eq!((w, h), (400, 300));
    }

    #[test]
    fn resolve_percentage_100_is_identity() {
        let (w, h) =
            resolve_dimensions(800, 600, &ResizeDimensions::Percentage { percentage: 100 });
        assert_eq!((w, h), (800, 600));
    }

    #[test]
    fn resolve_percentage_zero_clamps_to_1() {
        let (w, h) = resolve_dimensions(800, 600, &ResizeDimensions::Percentage { percentage: 0 });
        // 800 * 0 = 0, clamped to 1.
        assert!(w >= 1 && h >= 1);
    }
}
