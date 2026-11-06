// CONFI — Integration tests for the PDF engine (merge, split, rotate)
//
// Fixtures are synthesized with lopdf so the suite needs no binary files on
// disk. Every output is reopened and checked, matching what the engine itself
// guarantees.

use std::path::{Path, PathBuf};

use confi_core::common::errors::ErrorCode;
use confi_core::pdf_engine;
use lopdf::{Dictionary, Document, Object};
use tempfile::TempDir;

// ── Fixtures ────────────────────────────────────────────────────────────────

/// Write a PDF with `page_count` empty pages at `dir/name`.
fn write_pdf(dir: &Path, name: &str, page_count: u32) -> PathBuf {
    let path = dir.join(name);
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::new();

    for _ in 0..page_count {
        let content_id = doc.add_object(lopdf::Stream::new(Dictionary::new(), b"BT ET".to_vec()));

        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set("Parent", Object::Reference(pages_id));
        page.set(
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(595),
                Object::Integer(842),
            ]),
        );
        page.set("Contents", Object::Reference(content_id));
        kids.push(Object::Reference(doc.add_object(page)));
    }

    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Count", Object::Integer(kids.len() as i64));
    pages.set("Kids", Object::Array(kids));
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));

    doc.save(&path).unwrap();
    path
}

/// Current `/Rotate` of a 1-indexed page, read straight from disk.
fn rotation_on_disk(path: &Path, page: u32) -> i64 {
    let doc = Document::load(path).unwrap();
    let page_id = doc.get_pages()[&page];
    doc.get_dictionary(page_id)
        .unwrap()
        .get(b"Rotate")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(0)
}

fn out(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(name)
}

// ── Merge ───────────────────────────────────────────────────────────────────

#[test]
fn merge_two_one_page_pdfs() {
    let dir = TempDir::new().unwrap();
    let a = write_pdf(dir.path(), "a.pdf", 1);
    let b = write_pdf(dir.path(), "b.pdf", 1);
    let output = out(&dir, "merged.pdf");

    let result = pdf_engine::merge_pdfs(&[a.as_path(), b.as_path()], &output).unwrap();

    assert_eq!(result.page_count, 2);
    assert_eq!(pdf_engine::page_count(&output).unwrap(), 2);
}

#[test]
fn merge_three_pdfs_sums_pages() {
    let dir = TempDir::new().unwrap();
    let a = write_pdf(dir.path(), "a.pdf", 2);
    let b = write_pdf(dir.path(), "b.pdf", 3);
    let c = write_pdf(dir.path(), "c.pdf", 1);
    let output = out(&dir, "merged.pdf");

    let result = pdf_engine::merge_pdfs(&[&a, &b, &c], &output).unwrap();
    assert_eq!(result.page_count, 6);
    assert!(result.size_bytes > 0);
}

#[test]
fn merge_requires_at_least_two_inputs() {
    let dir = TempDir::new().unwrap();
    let a = write_pdf(dir.path(), "a.pdf", 1);
    let output = out(&dir, "merged.pdf");

    let err = pdf_engine::merge_pdfs(&[a.as_path()], &output).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(!output.exists());
}

#[test]
fn merge_refuses_to_overwrite_existing_output() {
    let dir = TempDir::new().unwrap();
    let a = write_pdf(dir.path(), "a.pdf", 1);
    let b = write_pdf(dir.path(), "b.pdf", 1);
    let output = out(&dir, "merged.pdf");
    std::fs::write(&output, b"existing").unwrap();

    let err = pdf_engine::merge_pdfs(&[&a, &b], &output).unwrap_err();
    assert_eq!(err.code, ErrorCode::OutputError);
    assert_eq!(std::fs::read(&output).unwrap(), b"existing");
}

#[test]
fn merge_leaves_inputs_unchanged() {
    let dir = TempDir::new().unwrap();
    let a = write_pdf(dir.path(), "a.pdf", 2);
    let b = write_pdf(dir.path(), "b.pdf", 2);
    let before_a = std::fs::read(&a).unwrap();
    let before_b = std::fs::read(&b).unwrap();

    pdf_engine::merge_pdfs(&[&a, &b], &out(&dir, "merged.pdf")).unwrap();

    assert_eq!(std::fs::read(&a).unwrap(), before_a);
    assert_eq!(std::fs::read(&b).unwrap(), before_b);
}

#[test]
fn merge_rejects_missing_input() {
    let dir = TempDir::new().unwrap();
    let a = write_pdf(dir.path(), "a.pdf", 1);
    let err = pdf_engine::merge_pdfs(
        &[a.as_path(), Path::new("does_not_exist.pdf")],
        &out(&dir, "merged.pdf"),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

// ── Extract ─────────────────────────────────────────────────────────────────

#[test]
fn extract_single_page() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 3);
    let output = out(&dir, "page2.pdf");

    let result = pdf_engine::extract_pages(&input, &[2], &output).unwrap();
    assert_eq!(result.page_count, 1);
    assert_eq!(pdf_engine::page_count(&output).unwrap(), 1);
}

#[test]
fn extract_multiple_pages() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 5);
    let output = out(&dir, "selected.pdf");

    let result = pdf_engine::extract_pages(&input, &[1, 3, 5], &output).unwrap();
    assert_eq!(result.page_count, 3);
}

#[test]
fn extract_out_of_range_page_fails() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 3);
    let output = out(&dir, "extract.pdf");

    let err = pdf_engine::extract_pages(&input, &[99], &output).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(!output.exists());
}

#[test]
fn extract_empty_selection_fails() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 3);

    let err = pdf_engine::extract_pages(&input, &[], &out(&dir, "extract.pdf")).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

// ── Split ───────────────────────────────────────────────────────────────────

#[test]
fn split_produces_one_file_per_page() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "document.pdf", 3);

    let results = pdf_engine::split_to_pages(&input, dir.path()).unwrap();

    assert_eq!(results.len(), 3);
    for (index, result) in results.iter().enumerate() {
        assert_eq!(result.page_count, 1);
        assert!(result.output_path.exists());
        assert!(result
            .output_path
            .ends_with(format!("document-page-{}.pdf", index + 1)));
        assert_eq!(pdf_engine::page_count(&result.output_path).unwrap(), 1);
    }
    // Source must still be intact.
    assert_eq!(pdf_engine::page_count(&input).unwrap(), 3);
}

#[test]
fn split_refuses_when_output_names_taken() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "document.pdf", 2);
    // Pre-create the second output so the run fails halfway through.
    std::fs::write(dir.path().join("document-page-2.pdf"), b"taken").unwrap();

    let err = pdf_engine::split_to_pages(&input, dir.path()).unwrap_err();
    assert_eq!(err.code, ErrorCode::OutputError);
    // The page-1 file this run produced must be cleaned up again.
    assert!(!dir.path().join("document-page-1.pdf").exists());
    // The pre-existing file is untouched.
    assert_eq!(
        std::fs::read(dir.path().join("document-page-2.pdf")).unwrap(),
        b"taken"
    );
}

// ── Rotate ──────────────────────────────────────────────────────────────────

#[test]
fn rotate_single_page_by_90() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 3);
    let output = out(&dir, "rotated.pdf");

    let result = pdf_engine::rotate_pdf(&input, &[1], 90, &output).unwrap();

    assert_eq!(result.page_count, 3);
    assert_eq!(rotation_on_disk(&output, 1), 90);
    // Untouched pages keep no rotation.
    assert_eq!(rotation_on_disk(&output, 2), 0);
    assert_eq!(rotation_on_disk(&output, 3), 0);
}

#[test]
fn rotate_all_pages() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 2);
    let output = out(&dir, "rotated.pdf");

    pdf_engine::rotate_pdf(&input, &[1, 2], 180, &output).unwrap();

    assert_eq!(rotation_on_disk(&output, 1), 180);
    assert_eq!(rotation_on_disk(&output, 2), 180);
}

#[test]
fn rotation_accumulates_across_runs() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 1);
    let first = out(&dir, "first.pdf");
    let second = out(&dir, "second.pdf");

    pdf_engine::rotate_pdf(&input, &[1], 90, &first).unwrap();
    pdf_engine::rotate_pdf(&first, &[1], 90, &second).unwrap();

    assert_eq!(rotation_on_disk(&second, 1), 180);
}

#[test]
fn rotation_wraps_at_360() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 1);
    let first = out(&dir, "first.pdf");
    let second = out(&dir, "second.pdf");

    pdf_engine::rotate_pdf(&input, &[1], 270, &first).unwrap();
    pdf_engine::rotate_pdf(&first, &[1], 270, &second).unwrap();

    // 270 + 270 = 540, which normalizes to 180.
    assert_eq!(rotation_on_disk(&second, 1), 180);
}

#[test]
fn rotate_rejects_invalid_degrees() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 1);

    let err = pdf_engine::rotate_pdf(&input, &[1], 45, &out(&dir, "rotated.pdf")).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[test]
fn rotate_out_of_range_page_fails() {
    let dir = TempDir::new().unwrap();
    let input = write_pdf(dir.path(), "input.pdf", 2);

    let err = pdf_engine::rotate_pdf(&input, &[9], 90, &out(&dir, "rotated.pdf")).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}
