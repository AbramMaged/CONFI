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
// Threading: PDFium's C API is not thread-safe, but this crate does not expose
// that hazard to callers. `pdfium-render`'s default `thread_safe` feature wraps
// every FPDF_* call in a mutex marshaller (`ThreadSafePdfiumBindings`) and
// declares `Pdfium: Send + Sync`, so concurrent calls are serialised inside the
// crate and a `Pdfium` handle may be shared. The cost is that parallel renders
// do not overlap: they queue on that one lock. Host applications therefore do
// not need their own locking, but must not expect parallel speed-up from
// sharing a single handle. Disabling `thread_safe` to get per-thread parallelism
// is unsafe with a single library instance and is not done here.
//
// Library lifecycle: `pdfium-render` keeps the bindings in a process-global
// `OnceCell`; `Pdfium::new` initialises the library once and later binds are
// ignored (the path argument only takes effect on the first successful bind in
// the process). Binding is therefore process-wide by construction, not merely
// by convention. Tests that bind repeatedly rely on this documented behaviour.
//
// Cost note: every call re-opens the document, so PDFium re-parses its xref per
// page. Measured share is in docs/PDFIUM-POC.md; a document-handle API is
// deferred to M4-full so the PoC interface stays path-based like the rest of
// the engine.
//
// Feature-gated behind `render` so the default build stays dependency-free.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use pdfium_render::prelude::{
    PdfPageIndex, PdfRenderConfig, Pdfium, PdfiumError, PdfiumInternalError,
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
    require_absent(output_path)?;

    let bitmap = render_page_rgba(pdfium, input_path, page_number, dpi)?;
    let (width, height) = (bitmap.width, bitmap.height);

    let image = image::RgbaImage::from_raw(width, height, bitmap.rgba).ok_or_else(|| {
        ConfiError::processing_error("PDFium returned a bitmap with an unexpected buffer size.")
    })?;

    let tmp_path = temp_path_for(output_path);
    image
        .save_with_format(&tmp_path, image::ImageFormat::Png)
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            ConfiError::processing_error_detail("Failed to encode the rendered page", e.to_string())
        })?;

    // Validate before the file reaches the destination: reopen the written PNG's
    // header and confirm the pixels survived encoding. Same rule as the image
    // engine — a failed validation deletes the temp file and reports an error.
    let (written_w, written_h) = image::image_dimensions(&tmp_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp_path);
        ConfiError::processing_error_detail(
            "Output validation failed: the rendered PNG could not be re-read. The output has been discarded.",
            e.to_string(),
        )
    })?;
    if (written_w, written_h) != (width, height) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(ConfiError::processing_error(format!(
            "Output validation failed: expected {width}×{height} pixels, the PNG reports \
             {written_w}×{written_h}. The output has been discarded."
        )));
    }

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
    // Validated here, at the single shared entry point, so a direct caller of
    // render_page_rgba cannot ask for a 576,000 dpi bitmap.
    if !(72..=600).contains(&dpi) {
        return Err(ConfiError::invalid_input(format!(
            "DPI must be between 72 and 600, not {dpi}."
        )));
    }

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

    // Refuse a request that would allocate an unreasonable bitmap *before*
    // PDFium allocates it. The page size already accounts for /Rotate, so this
    // measures the same rectangle PDFium is about to fill.
    // ponytail: fixed 100 Mpixel budget (~400 MB RGBA); raise it if poster-size
    // output is ever required.
    const MAX_PIXELS: u64 = 100_000_000;
    let scale = dpi as f64 / 72.0;
    let wanted_w = (page.width().value as f64 * scale).ceil().max(0.0) as u64;
    let wanted_h = (page.height().value as f64 * scale).ceil().max(0.0) as u64;
    if wanted_w.saturating_mul(wanted_h) > MAX_PIXELS {
        return Err(ConfiError::invalid_input(format!(
            "Rendering page {page_number} at {dpi} dpi would produce {wanted_w}×{wanted_h} pixels, \
             above the {MAX_PIXELS} pixel budget. Lower the DPI."
        )));
    }

    // 72 pt = 1 inch, so dpi/72 is the scale factor. PDFium applies the page's
    // /Rotate and /CropBox internally, which is why rotated pages come out
    // upright here without any extra handling.
    let config = PdfRenderConfig::new().scale_page_by_factor(scale as f32);

    let bitmap = page
        .render_with_config(&config)
        .map_err(|e| map_load_error(e, input_path))?;

    // `Pixels` is i32; a negative value would wrap under `as u32`.
    let width = u32::try_from(bitmap.width())
        .map_err(|_| ConfiError::processing_error("PDFium reported a negative bitmap width."))?;
    let height = u32::try_from(bitmap.height())
        .map_err(|_| ConfiError::processing_error("PDFium reported a negative bitmap height."))?;
    let rgba = bitmap.as_rgba_bytes();

    let elapsed = started.elapsed();

    // Overflow-checked: on a 32-bit target `width * height * 4` can exceed usize.
    let expected_bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| {
            ConfiError::processing_error(format!(
                "Rendered bitmap size {width}×{height} overflows this platform's address space."
            ))
        })?;

    if rgba.len() < expected_bytes {
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
    if page_number == 0 {
        return Err(ConfiError::invalid_input("Page numbers start at 1."));
    }

    let document = pdfium
        .load_pdf_from_file(input_path, None)
        .map_err(|e| map_load_error(e, input_path))?;

    let pages = document.pages();
    let page = pages
        .get((page_number - 1) as PdfPageIndex)
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
