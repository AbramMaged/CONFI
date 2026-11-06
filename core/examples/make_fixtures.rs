//! Generates the PDF fixtures used by the M4 rendering PoC.
//!
//! Fixtures are generated per run rather than committed: binary PDFs bloat the
//! repository, and the embedded-font fixture carries a font program that must
//! not be redistributed from here.
//!
//! Usage:
//!   cargo run -p confi-core --example make_fixtures -- <outdir> [font.ttf]
//!
//! The font argument may be omitted; the embedded-font fixture is then skipped.
//! LiberationSans-Regular.ttf (SIL OFL) works and ships with Windows.

use std::path::{Path, PathBuf};

use lopdf::{Dictionary, Document, Object, Stream};

fn main() {
    let mut args = std::env::args().skip(1);
    let outdir = args
        .next()
        .map(PathBuf::from)
        .expect("usage: make_fixtures <outdir> [font.ttf]");
    let font_path = args.next().map(PathBuf::from);

    std::fs::create_dir_all(&outdir).expect("cannot create output directory");

    let multipage = write(&outdir, "multipage-5.pdf", build_multipage(5));
    let rotated = write(&outdir, "rotated.pdf", build_rotated());
    let mixed = write(&outdir, "mixed-sizes.pdf", build_mixed_sizes());
    let scanned = write(&outdir, "scanned.pdf", build_scanned());
    let large = write(&outdir, "large-200.pdf", build_multipage(200));

    println!("multipage-5.pdf\t{}", multipage.display());
    println!("rotated.pdf\t{}", rotated.display());
    println!("mixed-sizes.pdf\t{}", mixed.display());
    println!("scanned.pdf\t{}", scanned.display());
    println!("large-200.pdf\t{}", large.display());

    match font_path {
        Some(path) if path.exists() => {
            let ttf = std::fs::read(&path).expect("cannot read font file");
            let out = write(&outdir, "embedded-font.pdf", build_with_embedded_font(&ttf));
            println!("embedded-font.pdf\t{}\tfont={}", out.display(), path.display());
        }
        Some(path) => eprintln!("note: font '{}' not found — skipping embedded-font fixture", path.display()),
        None => eprintln!("note: no font argument — skipping embedded-font fixture"),
    }
}

fn write(dir: &Path, name: &str, mut doc: Document) -> PathBuf {
    let path = dir.join(name);
    doc.save(&path).expect("cannot save fixture");
    path
}

// ── Page model ──────────────────────────────────────────────────────────────

struct Page {
    width: f32,
    height: f32,
    rotate: i64,
    content: Vec<u8>,
    font: Option<Object>,
    xobjects: Vec<(&'static str, Object)>,
}

impl Page {
    fn text(width: f32, height: f32, label: &str, font: Object) -> Self {
        Self {
            width,
            height,
            rotate: 0,
            content: text_content(width, height, label),
            font: Some(font),
            xobjects: Vec::new(),
        }
    }
}

fn text_content(width: f32, height: f32, label: &str) -> Vec<u8> {
    // A filled header band and a border make the page unambiguously non-blank,
    // which the PoC uses to prove pixels were actually drawn.
    format!(
        concat!(
            "0.10 0.45 0.85 rg\n",
            "0 0 {w:.1} 60 re f\n",
            "0.85 0.85 0.85 RG 2 w\n",
            "10 10 {w_minus:.1} {h_minus:.1} re S\n",
            "0 0 0 rg\n",
            "BT /F1 36 Tf 60 {ty:.1} Td ({label}) Tj ET\n",
            "BT /F1 18 Tf 60 {ty2:.1} Td (CONFI render fixture) Tj ET\n"
        ),
        w = width,
        w_minus = width - 20.0,
        h_minus = height - 20.0,
        ty = height - 160.0,
        ty2 = height - 200.0,
        label = label
    )
    .into_bytes()
}

// ── Fonts ───────────────────────────────────────────────────────────────────

/// A standard-14 font: not embedded, PDFium supplies the glyphs.
fn standard_font(doc: &mut Document) -> Object {
    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"Type1".to_vec()));
    font.set("BaseFont", Object::Name(b"Helvetica".to_vec()));
    font.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
    Object::Reference(doc.add_object(font))
}

/// A TrueType font embedded as a FontFile2 stream — the glyph program travels
/// inside the PDF, so rendering must come from the embedded file.
fn embedded_font(doc: &mut Document, ttf: &[u8]) -> Object {
    let font_file = doc.add_object(Stream::new(
        lopdf::dictionary! { "Length1" => Object::Integer(ttf.len() as i64) },
        ttf.to_vec(),
    ));

    let mut descriptor = Dictionary::new();
    descriptor.set("Type", Object::Name(b"FontDescriptor".to_vec()));
    descriptor.set("FontName", Object::Name(b"CONFIEmbedded".to_vec()));
    descriptor.set("Flags", Object::Integer(32));
    descriptor.set(
        "FontBBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(-200),
            Object::Integer(1000),
            Object::Integer(900),
        ]),
    );
    descriptor.set("ItalicAngle", Object::Integer(0));
    descriptor.set("Ascent", Object::Integer(750));
    descriptor.set("Descent", Object::Integer(-250));
    descriptor.set("CapHeight", Object::Integer(700));
    descriptor.set("StemV", Object::Integer(80));
    descriptor.set("FontFile2", Object::Reference(font_file));
    let descriptor_id = doc.add_object(descriptor);

    let mut font = Dictionary::new();
    font.set("Type", Object::Name(b"Font".to_vec()));
    font.set("Subtype", Object::Name(b"TrueType".to_vec()));
    font.set("BaseFont", Object::Name(b"CONFIEmbedded".to_vec()));
    font.set("FirstChar", Object::Integer(32));
    font.set("LastChar", Object::Integer(126));
    // Uniform advance widths: enough for a rendering fixture, which is about
    // proving the embedded glyph program is used, not about perfect metrics.
    font.set(
        "Widths",
        Object::Array((32..=126).map(|_| Object::Integer(600)).collect()),
    );
    font.set("FontDescriptor", Object::Reference(descriptor_id));
    font.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));

    Object::Reference(doc.add_object(font))
}

// ── Fixtures ────────────────────────────────────────────────────────────────

const A4: (f32, f32) = (595.0, 842.0);
const LETTER: (f32, f32) = (612.0, 792.0);
const A3: (f32, f32) = (842.0, 1191.0);

fn build_multipage(count: u32) -> Document {
    let mut doc = Document::with_version("1.7");
    let font = standard_font(&mut doc);
    let pages = (1..=count)
        .map(|n| {
            Page::text(
                A4.0,
                A4.1,
                &format!("Page {n} of {count}"),
                font.clone(),
            )
        })
        .collect();
    append_pages(doc, pages)
}

fn build_rotated() -> Document {
    let mut doc = Document::with_version("1.7");
    let font = standard_font(&mut doc);

    let mut upright = Page::text(A4.0, A4.1, "Upright page", font.clone());
    let mut quarter = Page::text(A4.0, A4.1, "Rotated 90", font.clone());
    let mut three_quarter = Page::text(A4.0, A4.1, "Rotated 270", font.clone());
    quarter.rotate = 90;
    three_quarter.rotate = 270;
    upright.rotate = 0;

    append_pages(doc, vec![upright, quarter, three_quarter])
}

fn build_mixed_sizes() -> Document {
    let mut doc = Document::with_version("1.7");
    let font = standard_font(&mut doc);
    let specs = [
        (A4.0, A4.1, "A4 portrait"),
        (LETTER.0, LETTER.1, "US Letter"),
        (A3.0, A3.1, "A3 portrait"),
        (300.0, 400.0, "Small 300x400"),
        (A4.1, A4.0, "A4 landscape"),
    ];
    let pages = specs
        .iter()
        .map(|(w, h, label)| Page::text(*w, *h, label, font.clone()))
        .collect();
    append_pages(doc, pages)
}

fn build_scanned() -> Document {
    let mut doc = Document::with_version("1.7");
    let mut pages = Vec::new();

    for page_number in 1..=2 {
        let (width, height) = A4;
        let jpeg = scanned_jpeg(1000, 1414, page_number);
        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"XObject".to_vec()));
        dict.set("Subtype", Object::Name(b"Image".to_vec()));
        dict.set("Width", Object::Integer(1000));
        dict.set("Height", Object::Integer(1414));
        dict.set("ColorSpace", Object::Name(b"DeviceRGB".to_vec()));
        dict.set("BitsPerComponent", Object::Integer(8));
        dict.set("Filter", Object::Name(b"DCTDecode".to_vec()));
        let image_id = doc.add_object(Stream::new(dict, jpeg));

        let content = format!("q {width:.1} 0 0 {height:.1} 0 0 cm /Im0 Do Q\n").into_bytes();
        pages.push(Page {
            width,
            height,
            rotate: 0,
            content,
            font: None,
            xobjects: vec![("Im0", Object::Reference(image_id))],
        });
    }

    append_pages(doc, pages)
}

fn build_with_embedded_font(ttf: &[u8]) -> Document {
    let mut doc = Document::with_version("1.7");
    let font = embedded_font(&mut doc, ttf);

    let mut page = Page::text(A4.0, A4.1, "Embedded font: ABC abc 123", font);
    // Extra weight so the embedded-glyph result is unmistakably rendered.
    page.content.extend_from_slice(
        b"BT /F1 72 Tf 60 500 Td (CONFI) Tj ET\n0 0 0 rg 60 400 400 6 re f\n",
    );

    append_pages(doc, vec![page])
}

fn append_pages(doc: Document, pages: Vec<Page>) -> Document {
    // build_document starts from a fresh document, so re-use it and keep the
    // font/xobject objects already registered in `doc`.
    let mut doc = doc;
    let pages_id = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::new();

    for page in pages {
        let content_id = doc.add_object(Stream::new(Dictionary::new(), page.content));

        let mut resources = Dictionary::new();
        if let Some(font) = page.font {
            let mut fonts = Dictionary::new();
            fonts.set("F1", font);
            resources.set("Font", Object::Dictionary(fonts));
        }
        if !page.xobjects.is_empty() {
            let mut xobjects = Dictionary::new();
            for (name, reference) in page.xobjects {
                xobjects.set(name, reference);
            }
            resources.set("XObject", Object::Dictionary(xobjects));
        }

        let mut dict = Dictionary::new();
        dict.set("Type", Object::Name(b"Page".to_vec()));
        dict.set("Parent", Object::Reference(pages_id));
        dict.set(
            "MediaBox",
            Object::Array(vec![
                Object::Real(0.0),
                Object::Real(0.0),
                Object::Real(page.width),
                Object::Real(page.height),
            ]),
        );
        if page.rotate != 0 {
            dict.set("Rotate", Object::Integer(page.rotate));
        }
        dict.set("Contents", Object::Reference(content_id));
        dict.set("Resources", Object::Dictionary(resources));

        kids.push(Object::Reference(doc.add_object(dict)));
    }

    let mut pages_node = Dictionary::new();
    pages_node.set("Type", Object::Name(b"Pages".to_vec()));
    pages_node.set("Count", Object::Integer(kids.len() as i64));
    pages_node.set("Kids", Object::Array(kids));
    doc.objects.insert(pages_id, Object::Dictionary(pages_node));

    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    let catalog_id = doc.add_object(catalog);
    doc.trailer.set("Root", Object::Reference(catalog_id));

    doc
}

/// A synthetic "scan": light paper with noise, dark text lines and a margin.
fn scanned_jpeg(width: u32, height: u32, page_number: u32) -> Vec<u8> {
    let mut img = image::RgbImage::from_fn(width, height, |x, y| {
        let noise = ((x * 37 + y * 91 + page_number * 13) % 14) as u8;
        image::Rgb([232 + noise / 4, 230 + noise / 4, 220 + noise / 4])
    });

    // Text-like dark bars.
    for line in 0..28u32 {
        let y0 = 120 + line * 40;
        if y0 + 12 >= height {
            break;
        }
        let width_jitter = (line * 53) % 200;
        let x_end = (width - 120).min(140 + 520 + width_jitter);
        for y in y0..(y0 + 10) {
            for x in 120..x_end {
                *img.get_pixel_mut(x, y) = image::Rgb([40, 40, 45]);
            }
        }
    }

    let mut buffer = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut buffer, 75);
    encoder
        .encode_image(&image::DynamicImage::ImageRgb8(img))
        .expect("cannot encode scan fixture");
    buffer
}
