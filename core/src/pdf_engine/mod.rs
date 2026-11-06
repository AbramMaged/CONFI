// CONFI — PDF engine
//
// Merge, split and rotate, built on `lopdf` (pure Rust, MIT).
//
// Every operation follows the same path: load → operate → save to a temp file →
// reopen the temp file and check its structure → rename into place. An output
// that fails the reopen check is discarded, never reported as a success.
//
// Not here yet: rendering and thumbnails (needs PDFium, M4), organize,
// compress and PDF-to-image conversion (M4/M5).

pub mod merge;
pub mod rotate;
pub mod split;

pub use merge::merge_pdfs;
pub use rotate::rotate_pdf;
pub use split::{extract_pages, split_to_pages};

use std::path::{Path, PathBuf};

use lopdf::{Document, Object, ObjectId};

use crate::common::errors::{ConfiError, ConfiResult, ErrorCode};
use crate::common::fs::{finalize, temp_path_for};

/// What a completed PDF operation produced.
#[derive(Debug)]
pub struct PdfResult {
    pub output_path: PathBuf,
    pub page_count: u32,
    pub size_bytes: u64,
}

// ── Loading ─────────────────────────────────────────────────────────────────

/// Load a PDF, mapping library failures onto CONFI error codes.
pub(crate) fn load_pdf(path: &Path) -> ConfiResult<Document> {
    if !path.exists() {
        return Err(ConfiError::invalid_input(format!(
            "File not found: {}",
            path.display()
        )));
    }

    let doc = Document::load(path).map_err(|e| match e {
        lopdf::Error::IO(ref io) if io.kind() == std::io::ErrorKind::NotFound => {
            ConfiError::invalid_input(format!("File not found: {}", path.display()))
        }
        lopdf::Error::IO(ref io) if io.kind() == std::io::ErrorKind::PermissionDenied => {
            ConfiError::file_access_denied(format!(
                "Cannot read '{}': access denied.",
                path.display()
            ))
        }
        lopdf::Error::Decryption(_) => ConfiError::password_required(),
        other => ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            format!(
                "'{}' could not be opened. The file may be damaged.",
                path.display()
            ),
            other.to_string(),
        ),
    })?;

    // lopdf loads an encrypted document's structure but its content streams stay
    // encrypted, so every later operation would produce garbage. Refuse here.
    if doc.is_encrypted() {
        return Err(ConfiError::password_required());
    }

    Ok(doc)
}

/// Page count of a PDF on disk.
pub fn page_count(path: &Path) -> ConfiResult<u32> {
    Ok(load_pdf(path)?.get_pages().len() as u32)
}

/// The object id of a document's page-tree root (`Catalog /Pages`).
pub(crate) fn pages_root_id(doc: &Document) -> ConfiResult<ObjectId> {
    let missing = || {
        ConfiError::with_detail(
            ErrorCode::CorruptedFile,
            "This PDF has no document catalog and cannot be processed.",
            "trailer /Root or catalog /Pages is missing",
        )
    };

    let root = doc.trailer.get(b"Root").map_err(|_| missing())?;
    let root_id = root.as_reference().map_err(|_| missing())?;
    let catalog = doc.get_dictionary(root_id).map_err(|_| missing())?;
    catalog
        .get(b"Pages")
        .map_err(|_| missing())?
        .as_reference()
        .map_err(|_| missing())
}

// ── Validated writing ───────────────────────────────────────────────────────

/// Save `doc` to a temp file beside `output`.
pub(crate) fn save_tmp(mut doc: Document, output: &Path) -> ConfiResult<PathBuf> {
    let tmp = temp_path_for(output);
    doc.save(&tmp).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        ConfiError::output_error(format!(
            "Cannot write PDF to '{}': {e}",
            tmp.display()
        ))
    })?;
    Ok(tmp)
}

/// Reopen a written PDF and confirm it has the expected page count.
/// Returns the reopened document so callers can make further structural checks.
pub(crate) fn reopen_pdf(tmp: &Path, expected_pages: u32) -> ConfiResult<Document> {
    let doc = load_pdf(tmp).map_err(|e| {
        let _ = std::fs::remove_file(tmp);
        ConfiError::processing_error_detail(
            "Output validation failed: the generated PDF could not be reopened. The output has been discarded.",
            format!("{e} ({e:?})"),
        )
    })?;

    let actual = doc.get_pages().len() as u32;
    if actual != expected_pages {
        let _ = std::fs::remove_file(tmp);
        return Err(ConfiError::processing_error(format!(
            "Output validation failed: expected {expected_pages} page(s), found {actual}. \
             The output has been discarded."
        )));
    }

    Ok(doc)
}

/// Move a validated temp file into place and describe the result.
pub(crate) fn finish(tmp: &Path, output: &Path, page_count: u32) -> ConfiResult<PdfResult> {
    let size_bytes = std::fs::metadata(tmp).map(|m| m.len()).unwrap_or(0);
    finalize(tmp, output)?;
    Ok(PdfResult {
        output_path: output.to_owned(),
        page_count,
        size_bytes,
    })
}

/// Set a page's `/Rotate` value, accumulating with any rotation already there.
pub(crate) fn add_rotation(doc: &mut Document, page_id: ObjectId, degrees: i64) -> ConfiResult<i64> {
    let page = doc.get_dictionary_mut(page_id).map_err(|e| {
        ConfiError::processing_error_detail("Page dictionary is unreadable", e.to_string())
    })?;

    let current = page
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0);
    let next = (current + degrees).rem_euclid(360);

    page.set("Rotate", Object::Integer(next));
    Ok(next)
}

/// Read a page's current `/Rotate` value (0 when absent).
pub(crate) fn rotation_of(doc: &Document, page_id: ObjectId) -> i64 {
    doc.get_dictionary(page_id)
        .ok()
        .and_then(|p| p.get(b"Rotate").ok().and_then(|o| o.as_i64().ok()))
        .unwrap_or(0)
}
