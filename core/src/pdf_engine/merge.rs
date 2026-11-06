// CONFI — PDF merge
//
// Page order follows the order of the input files.
//
// lopdf 0.34 has no `Document::merge`, so the documents are combined by hand:
// renumber each source document's object ids above the running document's
// highest id (which also rewrites every internal reference), append its
// objects, then splice its page ids into the running page tree.
//
// ponytail: the source documents' own page-tree nodes are left behind as
// unreferenced objects. They are dropped by any later `prune_objects()` pass;
// adding one here would cost more than the few hundred dead bytes.

use std::path::Path;

use lopdf::{Document, Object, ObjectId};

use crate::common::errors::{ConfiError, ConfiResult};
use crate::common::fs::require_absent;
use crate::pdf_engine::{finish, load_pdf, pages_root_id, reopen_pdf, save_tmp, PdfResult};

/// Merge two or more PDFs into one document containing every page, in input order.
pub fn merge_pdfs(input_paths: &[&Path], output_path: &Path) -> ConfiResult<PdfResult> {
    if input_paths.len() < 2 {
        return Err(ConfiError::invalid_input(
            "Merge requires at least two PDF files.",
        ));
    }
    require_absent(output_path)?;

    let docs = input_paths
        .iter()
        .map(|p| load_pdf(p))
        .collect::<ConfiResult<Vec<_>>>()?;

    let expected_pages: u32 = docs.iter().map(|d| d.get_pages().len() as u32).sum();
    if expected_pages == 0 {
        return Err(ConfiError::corrupted_file(
            "The selected PDFs contain no pages to merge.",
        ));
    }

    let merged = merge_documents(docs)?;

    let tmp = save_tmp(merged, output_path)?;
    reopen_pdf(&tmp, expected_pages)?;
    finish(&tmp, output_path, expected_pages)
}

/// Combine already-loaded documents into the first one.
fn merge_documents(docs: Vec<Document>) -> ConfiResult<Document> {
    let mut docs = docs.into_iter();
    let mut base = docs.next().ok_or_else(|| {
        // Unreachable: the caller guarantees at least two documents.
        ConfiError::invalid_input("No documents to merge.")
    })?;

    let base_pages_id = pages_root_id(&base)?;
    let mut all_pages: Vec<ObjectId> = base.get_pages().values().copied().collect();

    for mut doc in docs {
        doc.renumber_objects_with(base.max_id + 1);

        let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
        let new_max_id = doc.max_id;

        base.objects.extend(doc.objects);
        base.max_id = new_max_id;

        // Re-parent: the pages now belong to this document's page tree.
        for page_id in &page_ids {
            if let Ok(page) = base.get_dictionary_mut(*page_id) {
                page.set("Parent", Object::Reference(base_pages_id));
            }
        }
        all_pages.extend(page_ids);
    }

    let pages = base.get_dictionary_mut(base_pages_id).map_err(|e| {
        ConfiError::processing_error_detail("Merged page tree is unreadable", e.to_string())
    })?;
    pages.set(
        "Kids",
        Object::Array(all_pages.iter().map(|id| Object::Reference(*id)).collect()),
    );
    pages.set("Count", Object::Integer(all_pages.len() as i64));

    Ok(base)
}
