// CONFI — PDF page rotation
//
// Rotation is stored per page as the PDF `/Rotate` key. Values accumulate: a
// page already at 90 rotated by 90 becomes 180, never 90 again.
//
// Only 90/180/270 are accepted. A page at 0 stays visually upright, and 360
// would be a no-op the user would report as "nothing happened", so both are
// rejected rather than silently doing nothing.

use std::path::Path;

use crate::common::errors::{ConfiError, ConfiResult};
use crate::common::fs::require_absent;
use crate::pdf_engine::{
    add_rotation, finish, load_pdf, reopen_pdf, rotation_of, save_tmp, PdfResult,
};

/// Rotate the selected pages clockwise by `degrees` (90, 180 or 270).
pub fn rotate_pdf(
    input_path: &Path,
    pages: &[u32],
    degrees: u16,
    output_path: &Path,
) -> ConfiResult<PdfResult> {
    if !matches!(degrees, 90 | 180 | 270) {
        return Err(ConfiError::invalid_input(format!(
            "Rotation must be 90, 180 or 270 degrees, not {degrees}."
        )));
    }
    if pages.is_empty() {
        return Err(ConfiError::invalid_input(
            "Page selection is empty. Specify at least one page.",
        ));
    }
    require_absent(output_path)?;

    let mut doc = load_pdf(input_path)?;
    let page_map = doc.get_pages();
    let total = page_map.len() as u32;

    let mut applied: Vec<(u32, i64)> = Vec::with_capacity(pages.len());
    for &page in pages {
        let page_id = *page_map.get(&page).ok_or_else(|| {
            ConfiError::invalid_input(format!(
                "Page {page} is out of range. This PDF has {total} page(s)."
            ))
        })?;
        let value = add_rotation(&mut doc, page_id, degrees as i64)?;
        applied.push((page, value));
    }

    let tmp = save_tmp(doc, output_path)?;
    let written = reopen_pdf(&tmp, total)?;

    // Structure check: the rotation must survive the write/read round trip.
    let written_pages = written.get_pages();
    for (page, expected) in &applied {
        let actual = written_pages
            .get(page)
            .map(|id| rotation_of(&written, *id))
            .unwrap_or(0);
        if actual != *expected {
            let _ = std::fs::remove_file(&tmp);
            return Err(ConfiError::processing_error(format!(
                "Output validation failed: page {page} should be rotated to {expected}°, found {actual}°. \
                 The output has been discarded."
            )));
        }
    }
    finish(&tmp, output_path, total)
}
