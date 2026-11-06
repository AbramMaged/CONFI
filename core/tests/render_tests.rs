// CONFI — Rendering tests (require the PDFium native library)
//
// PDFium is a native binary and is deliberately not vendored in the repository
// (7.8 MB per platform, see vendor/ and tools/fetch-pdfium.ps1). These tests
// therefore run only when CONFI_PDFIUM_DIR points at the directory holding
// `pdfium.dll` / `libpdfium.so`:
//
//   $env:CONFI_PDFIUM_DIR = "C:\work\projects\CONFI\vendor\pdfium\win-x64\bin"
//   cargo test -p confi-core --features render --test render_tests
//
// Without that variable each test prints SKIP and returns, so the suite stays
// green on machines that have no native library — and never silently passes
// while claiming to have rendered something.

#![cfg(feature = "render")]

use std::path::{Path, PathBuf};

use confi_core::common::errors::ErrorCode;
use confi_core::pdf_engine::render;
use lopdf::{Dictionary, Document, Object, Stream};
use tempfile::TempDir;

fn pdfium_dir() -> Option<PathBuf> {
    std::env::var_os("CONFI_PDFIUM_DIR")
        .map(PathBuf::from)
        .filter(|p| {
            let ok = p.exists();
            if !ok {
                eprintln!(
                    "SKIP: CONFI_PDFIUM_DIR is set but does not exist: {}",
                    p.display()
                );
            }
            ok
        })
}

macro_rules! require_pdfium {
    () => {
        match pdfium_dir() {
            Some(dir) => dir,
            None => {
                eprintln!("SKIP: set CONFI_PDFIUM_DIR to run rendering tests");
                return;
            }
        }
    };
}

/// A one-page A4 PDF with a filled black rectangle, so a rendered page is
/// provably not blank.
fn write_fixture(dir: &TempDir) -> PathBuf {
    write_fixture_sized(dir, "one-page.pdf", 595, 842)
}

/// Same, with an arbitrary MediaBox — used to provoke the allocation guard.
fn write_fixture_sized(dir: &TempDir, name: &str, width: i64, height: i64) -> PathBuf {
    let path = dir.path().join(name);
    let mut doc = Document::with_version("1.7");

    let content = Stream::new(
        Dictionary::new(),
        b"0 0 0 rg 100 600 300 100 re f\n".to_vec(),
    );
    let content_id = doc.add_object(content);
    let pages_id = doc.new_object_id();

    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(width),
            Object::Integer(height),
        ]),
    );
    page.set("Contents", Object::Reference(content_id));
    let page_id = doc.add_object(page);

    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Count", Object::Integer(1));
    pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));

    doc.save(&path).unwrap();
    path
}

#[test]
fn renders_page_at_expected_pixel_size() {
    let dir = require_pdfium!();
    let pdfium = render::bind_pdfium(&dir).expect("PDFium must bind");

    let tmp = TempDir::new().unwrap();
    let input = write_fixture(&tmp);
    let output = tmp.path().join("page-1.png");

    // 144 dpi = 2x. A4 at 595x842 pt must land on 1190x1684 px.
    let result = render::render_page_to_png(&pdfium, &input, 1, 144, &output).unwrap();

    assert_eq!((result.pixel_width, result.pixel_height), (1190, 1684));

    let decoded = image::open(&output).unwrap();
    assert_eq!(decoded.width(), 1190);
    assert_eq!(decoded.height(), 1684);

    // The rectangle must actually be drawn.
    let rgb = decoded.to_rgb8();
    let dark = rgb
        .pixels()
        .filter(|p| p[0] < 60 && p[1] < 60 && p[2] < 60)
        .count();
    assert!(
        dark > 1000,
        "expected the filled rectangle, found {dark} dark pixels"
    );
}

#[test]
fn out_of_range_page_returns_invalid_input() {
    let dir = require_pdfium!();
    let pdfium = render::bind_pdfium(&dir).unwrap();

    let tmp = TempDir::new().unwrap();
    let input = write_fixture(&tmp);
    let err =
        render::render_page_to_png(&pdfium, &input, 2, 96, &tmp.path().join("x.png")).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[test]
fn dpi_outside_supported_range_returns_invalid_input() {
    let dir = require_pdfium!();
    let pdfium = render::bind_pdfium(&dir).unwrap();

    let tmp = TempDir::new().unwrap();
    let input = write_fixture(&tmp);

    for dpi in [0u32, 71, 601] {
        let err = render::render_page_to_png(&pdfium, &input, 1, dpi, &tmp.path().join("x.png"))
            .unwrap_err();
        assert_eq!(
            err.code,
            ErrorCode::InvalidInput,
            "dpi {dpi} should be rejected"
        );
    }
}

#[test]
fn missing_library_directory_returns_invalid_input() {
    let tmp = TempDir::new().unwrap();
    let err = render::bind_pdfium(&tmp.path().join("no-such-library-dir")).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(err.message.contains("PDFium library was not found"));
}

#[test]
fn corrupt_pdf_returns_corrupted_file() {
    let dir = require_pdfium!();
    let pdfium = render::bind_pdfium(&dir).unwrap();

    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("broken.pdf");
    std::fs::write(&input, b"not a pdf at all").unwrap();

    let err =
        render::render_page_to_png(&pdfium, &input, 1, 96, &tmp.path().join("x.png")).unwrap_err();
    assert_eq!(err.code, ErrorCode::CorruptedFile);
}

#[test]
fn render_page_rgba_rejects_bad_dpi_without_a_second_check() {
    // render_page_rgba is public; its own validation must not depend on the
    // caller having gone through render_page_to_png first.
    let dir = require_pdfium!();
    let pdfium = render::bind_pdfium(&dir).unwrap();

    let tmp = TempDir::new().unwrap();
    let input = write_fixture(&tmp);
    let err = render::render_page_rgba(&pdfium, &input, 1, 10_000).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[test]
fn page_zero_is_rejected() {
    let dir = require_pdfium!();
    let pdfium = render::bind_pdfium(&dir).unwrap();

    let tmp = TempDir::new().unwrap();
    let input = write_fixture(&tmp);

    let err =
        render::render_page_rgba(&pdfium, &input, 0, 96).expect_err("page 0 must be rejected");
    assert_eq!(err.code, ErrorCode::InvalidInput);

    let err = render::page_size(&pdfium, &input, 0).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}

#[test]
fn allocation_budget_rejects_oversized_page_before_rendering() {
    let dir = require_pdfium!();
    let pdfium = render::bind_pdfium(&dir).unwrap();

    let tmp = TempDir::new().unwrap();
    // 20,000 x 20,000 pt at 72 dpi = 400 Mpixels, above the 100 Mpixel budget.
    let input = write_fixture_sized(&tmp, "huge.pdf", 20_000, 20_000);
    let output = tmp.path().join("huge.png");

    let err = render::render_page_to_png(&pdfium, &input, 1, 72, &output).unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
    assert!(
        err.message.contains("pixel budget"),
        "expected the allocation budget to be cited, got: {}",
        err.message
    );
    assert!(
        !output.exists(),
        "no output may be written when the guard fires"
    );
}

#[test]
fn failed_render_leaves_no_output_file() {
    let dir = require_pdfium!();
    let pdfium = render::bind_pdfium(&dir).unwrap();

    let tmp = TempDir::new().unwrap();
    let input = tmp.path().join("broken.pdf");
    std::fs::write(&input, b"not a pdf at all").unwrap();
    let output = tmp.path().join("out.png");

    let _ = render::render_page_to_png(&pdfium, &input, 1, 96, &output);

    assert!(
        !output.exists(),
        "a failed render must not leave an output file"
    );
    // No leftover temp files in the destination directory either.
    let leftovers: Vec<_> = std::fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains("confi-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp files leaked: {leftovers:?}");
}

#[test]
fn missing_input_file_returns_invalid_input() {
    let dir = require_pdfium!();
    let pdfium = render::bind_pdfium(&dir).unwrap();

    let tmp = TempDir::new().unwrap();
    let err = render::render_page_to_png(
        &pdfium,
        Path::new("does-not-exist.pdf"),
        1,
        96,
        &tmp.path().join("x.png"),
    )
    .unwrap_err();
    assert_eq!(err.code, ErrorCode::InvalidInput);
}
