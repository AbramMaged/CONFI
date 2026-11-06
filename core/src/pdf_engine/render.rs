// CONFI — PDF rasterisation via PDFium
//
// This module is the ONLY place PDFium is used. The structural engine
// (merge/split/rotate) runs on lopdf and does not touch these functions, so a
// machine without the PDFium native library can still reorganise PDFs.
//
// Binding model: dynamic. `pdfium-render` loads the native library at runtime
// through `libloading`; nothing is linked at build time and no environment
// variable is required, because the caller passes the directory holding the
// library. `bind_pdfium` is the single entry point for that.
//
// Threading: PDFium is not thread-safe and a `Pdfium` instance must not be
// shared across threads. The core never spawns threads (AGENTS.md §5), so the
// host application owns the instance and serialises access to it.
//
// Feature-gated behind `render` so the default build stays dependency-free.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pdfium_render::prelude::{
    PdfRenderConfig, PdfPageIndex, Pdfium, PdfiumError, PdfiumInternalError,
};

use crate::common::errors::{ConfiError, ConfiResult, ErrorCode};
use crate::common::fs::{finalize, require_absent, temp_path_for};

/// Result of rasterising one page.
#[derive(Debug)]
pub struct RenderResult {
    pub output_path: PathBuf,
    /// 1-indexed page number, as the user sees it.
    pub page_number: u32,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub size_bytes: u64,
    /// Time spent inside PDFium/encoding for this page only.
    pub render_time: Duration,
}

/// Bind the PDFium native library from `library_dir`.
///
/// `library_dir` is a directory: the platform library name (`pdfium.dll`,
/// `libpdfium.so`) is appended. Every render call takes the returned `Pdfium`
/// so binding happens once per process, not once per page.
pub fn bind_pdfium(library_dir: &Path) -> ConfiResult<Pdfium> {
    let library = Pdfium::pdfium_platform_library_name_at_path(library_dir);

    if !library.exists() {
        return Err(ConfiError::new(
            ErrorCode::InvalidInput,
            format!(
                "The PDFium library was not found at '{}'. \
                 Rendering is unavailable; folder and image tools are unaffected.",
                library.display()
            ),
        ));
    }

    let bindings = Pdfium::bind_to_library(&library).map_err(|e| {
        ConfiError::processing_error_detail(
            format!(
                "The PDFium library at '{}' could not be loaded.",
                library.display()
            ),
            e.to_string(),
        )
    })?;

    Ok(Pdfium::new(bindings))
}

/// Render one page (1-indexed) to a PNG file at `dpi`.
pub fn render_page_to_png(
    pdfium: &Pdfium,
    input_path: &Path,
    page_number: u32,
    dpi: u32,
    output_path: &Path,
) -> ConfiResult<RenderResult> {
    if !(72..=600).contains(&dpi) {
        return Err(ConfiError::invalid_input(format!(
            "DPI must be between 72 and 600, not {dpi}."
        )));
    }
    require_absent(output_path)?;

    let bitmap = render_page_rgba(pdfium, input_path, page_number, dpi)?;
    let (width, height) = (bitmap.width, bitmap.height);

    let image = image::RgbaImage::from_raw(width, height, bitmap.rgba).ok_or_else(|| {
        ConfiError::processing_error("PDFium returned a bitmap with an unexpected buffer size.")
    })?;

    let tmp_path = temp_path_for(output_path);
    image.save_with_format(&tmp_path, image::ImageFormat::Png).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        ConfiError::processing_error_detail("Failed to encode the rendered page", e.to_string())
    })?;

    finalize(&tmp_path, output_path)?;

    Ok(RenderResult {
        output_path: output_path.to_owned(),
        page_number,
        pixel_width: width,
        pixel_height: height,
        size_bytes: std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0),
        render_time: bitmap.elapsed,
    })
}

/// Render one page to raw RGBA bytes — used for thumbnails that never touch disk.
pub fn render_page_rgba(
    pdfium: &Pdfium,
    input_path: &Path,
    page_number: u32,
    dpi: u32,
) -> ConfiResult<RgbaBitmap> {
    let started = Instant::now();

    let document = pdfium
        .load_pdf_from_file(input_path, None)
        .map_err(|e| map_load_error(e, input_path))?;

    let pages = document.pages();
    let total = pages.len() as u32;

    if page_number == 0 || page_number > total {
        return Err(ConfiError::invalid_input(format!(
            "Page {page_number} is out of range. This PDF has {total} page(s)."
        )));
    }

    let page = pages
        .get((page_number - 1) as PdfPageIndex)
        .map_err(|e| map_load_error(e, input_path))?;

    // 72 pt = 1 inch, so dpi/72 is the scale factor. PDFium applies the page's
    // /Rotate and /CropBox internally, which is why rotated pages come out
    // upright here without any extra handling.
    let config = PdfRenderConfig::new().scale_page_by_factor(dpi as f32 / 72.0);

    let bitmap = page
        .render_with_config(&config)
        .map_err(|e| map_load_error(e, input_path))?;

    let width = bitmap.width() as u32;
    let height = bitmap.height() as u32;
    let rgba = bitmap.as_rgba_bytes();

    let elapsed = started.elapsed();

    if rgba.len() < (width as usize * height as usize * 4) {
        return Err(ConfiError::processing_error(
            "PDFium returned fewer bytes than the reported bitmap size.",
        ));
    }

    Ok(RgbaBitmap {
        width,
        height,
        rgba,
        elapsed,
    })
}

/// Page dimensions in PDF points (1/72 inch), as PDFium reports them —
/// already accounting for /Rotate.
pub fn page_size(pdfium: &Pdfium, input_path: &Path, page_number: u32) -> ConfiResult<(f32, f32)> {
    let document = pdfium
        .load_pdf_from_file(input_path, None)
        .map_err(|e| map_load_error(e, input_path))?;

    let pages = document.pages();
    let page = pages
        .get((page_number.saturating_sub(1)) as PdfPageIndex)
        .map_err(|e| map_load_error(e, input_path))?;

    Ok((page.width().value, page.height().value))
}

/// Number of pages, read through PDFium (independent of lopdf).
pub fn page_count(pdfium: &Pdfium, input_path: &Path) -> ConfiResult<u32> {
    let document = pdfium
        .load_pdf_from_file(input_path, None)
        .map_err(|e| map_load_error(e, input_path))?;
    Ok(document.pages().len() as u32)
}

/// A rendered page held in memory.
#[derive(Debug)]
pub struct RgbaBitmap {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub elapsed: Duration,
}

impl RgbaBitmap {
    /// Fraction of pixels that are not pure white — a cheap "did anything draw?"
    /// check used by the PoC to prove a page is not blank.
    pub fn non_white_ratio(&self) -> f64 {
        let pixels = (self.width as usize * self.height as usize).max(1);
        let non_white = self
            .rgba
            .chunks_exact(4)
            .filter(|p| p[0] < 250 || p[1] < 250 || p[2] < 250)
            .count();
        non_white as f64 / pixels as f64
    }

    /// Number of distinct 24-bit RGB values, capped for speed.
    pub fn distinct_colors(&self, cap: usize) -> usize {
        let mut seen = std::collections::BTreeSet::new();
        for pixel in self.rgba.chunks_exact(4) {
            seen.insert((pixel[0], pixel[1], pixel[2]));
            if seen.len() >= cap {
                break;
            }
        }
        seen.len()
    }
}

/// Translate a PDFium failure into a CONFI error with the right code.
fn map_load_error(error: PdfiumError, path: &Path) -> ConfiError {
    match error {
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::PasswordError) => {
            ConfiError::password_required()
        }
        PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FileError)
        | PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::FormatError) => {
            ConfiError::with_detail(
                ErrorCode::CorruptedFile,
                format!(
                    "'{}' could not be opened. The file may be damaged or not a PDF.",
                    path.display()
                ),
                error.to_string(),
            )
        }
        PdfiumError::IoError(ref io) if io.kind() == std::io::ErrorKind::NotFound => {
            ConfiError::invalid_input(format!("File not found: {}", path.display()))
        }
        other => ConfiError::with_detail(
            ErrorCode::ProcessingError,
            "The PDF could not be rendered.",
            other.to_string(),
        ),
    }
}
