// CONFI — PDF split
//
// Two entry points, one implementation:
//   extract_pages  — one output file containing the selected pages
//   split_to_pages — one output file per page
//
// Page numbers are 1-indexed, matching what the user sees.
//
// The document is loaded once per call. `split_to_pages` reuses that single
// parse for every output instead of re-reading the source per page.

use std::collections::BTreeSet;
use std::path::Path;

use lopdf::Document;

use crate::common::errors::{ConfiError, ConfiResult};
use crate::common::fs::require_absent;
use crate::pdf_engine::{finish, load_pdf, reopen_pdf, save_tmp, PdfResult};

/// Write a new PDF containing only `pages`, in the order given.
///
/// Duplicate page numbers are collapsed: asking for page 2 twice yields one page.
pub fn extract_pages(
    input_path: &Path,
    pages: &[u32],
    output_path: &Path,
) -> ConfiResult<PdfResult> {
    let doc = load_pdf(input_path)?;
    extract_from(&doc, pages, output_path)
}

/// Split a PDF into one file per page, named `{stem}-page-{n}.pdf`.
///
/// If any page fails, the files this call already produced are removed so a
/// partial split is never left behind.
pub fn split_to_pages(input_path: &Path, output_dir: &Path) -> ConfiResult<Vec<PdfResult>> {
    let doc = load_pdf(input_path)?;
    let total = doc.get_pages().len() as u32;

    if total == 0 {
        return Err(ConfiError::corrupted_file("This PDF has no pages."));
    }

    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("document");

    let mut results: Vec<PdfResult> = Vec::with_capacity(total as usize);

    for page in 1..=total {
        let output_path = output_dir.join(format!("{stem}-page-{page}.pdf"));
        match extract_from(&doc, &[page], &output_path) {
            Ok(result) => results.push(result),
            Err(e) => {
                for done in &results {
                    let _ = std::fs::remove_file(&done.output_path);
                }
                return Err(e);
            }
        }
    }

    Ok(results)
}

/// Extract from an already-loaded document.
fn extract_from(doc: &Document, pages: &[u32], output_path: &Path) -> ConfiResult<PdfResult> {
    require_absent(output_path)?;

    let total = doc.get_pages().len() as u32;
    if pages.is_empty() {
        return Err(ConfiError::invalid_input(
            "Page selection is empty. Specify at least one page.",
        ));
    }
    for &page in pages {
        if page == 0 || page > total {
            return Err(ConfiError::invalid_input(format!(
                "Page {page} is out of range. This PDF has {total} page(s)."
            )));
        }
    }

    let keep: BTreeSet<u32> = pages.iter().copied().collect();
    let to_delete: Vec<u32> = (1..=total).filter(|p| !keep.contains(p)).collect();

    let mut out = doc.clone();
    out.delete_pages(&to_delete);

    let tmp = save_tmp(out, output_path)?;
    reopen_pdf(&tmp, keep.len() as u32)?;
    finish(&tmp, output_path, keep.len() as u32)
}
