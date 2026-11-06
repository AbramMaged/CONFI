//! Convert a Word / PowerPoint / Excel document to PDF.
//!
//! Usage:
//!   cargo run -p confi-core --example convert_doc -- <input.docx|.pptx|.xlsx> <output.pdf>

use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        eprintln!("usage: convert_doc <input> <output.pdf>");
        std::process::exit(2);
    }

    let input = Path::new(&args[0]);
    let output = Path::new(&args[1]);

    println!(
        "backend_available={:?}",
        confi_core::office_engine::detect_backend()
    );
    println!("kind={:?}", confi_core::office_engine::kind_of(input));

    match confi_core::office_engine::convert_to_pdf(input, output) {
        Ok(outcome) => {
            println!(
                "OK backend={:?} pages={} bytes={} output={}",
                outcome.backend,
                outcome.page_count,
                outcome.size_bytes,
                outcome.output_path.display()
            );
        }
        Err(error) => {
            println!("FAILED code={} message={}", error.code, error.message);
            if let Some(detail) = error.detail {
                println!("detail={detail}");
            }
            std::process::exit(1);
        }
    }
}
